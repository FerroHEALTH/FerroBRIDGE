// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an element definition says about the resource a reference points
//! at: the aggregation its types allow (elementdefinition.html §2.30.0.4.4,
//! "the target of the reference SHALL be aggregated as defined") and the
//! profiles the target SHALL conform to
//! (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.type.targetProfile>),
//! both checked on the resource the reference resolves to.

use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::Reason;
use crate::set::ResolveError;
use crate::validate::CORE;
use crate::validate::ValidateError;
use crate::validate::resolver::Resolution;
use crate::validate::resolver::Via;
use crate::validate::structure::Structure;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;

/// Whether a resource conforms to one target profile.
#[derive(Debug, Clone)]
pub(crate) enum Verdict {
    /// It conforms.
    Conforms,
    /// It does not, with the first error found, as message and location.
    Fails(String),
    /// The profile could not be validated against, for the reason given.
    Unknown(Reason),
}

/// The state of one target check: a resource against one profile.
#[derive(Debug, Clone)]
pub(crate) enum Memo {
    /// The check is under way further up the walk: a reference cycle.
    InProgress,
    /// The check is done.
    Done(Verdict),
}

/// The literal `reference` of the `Reference` `value`.
pub(crate) fn literal(value: &Value) -> Option<&str> {
    value.get("reference").and_then(Value::as_str)
}

/// The address `value` sits at, the key a resource has during one
/// validation.
pub(crate) fn address(value: &Value) -> usize {
    std::ptr::from_ref(value).addr()
}

impl<'i> Walk<'_, '_, 'i> {
    /// Resolves the reference `value` holds once per occurrence: an
    /// ambiguous match is an error, and an entry of the Bundle it resolves
    /// to is recorded for the document rules.
    pub(crate) fn reference(&mut self, value: &'i Value, location: &str) {
        let Some(reference) = literal(value) else {
            return;
        };
        match self.validator().resolver().resolve(reference, self.scope()) {
            // NOTE: bundle.html §2.36.4.1: "If multiple matches are found, it is ambiguous
            // which is correct. Applications MAY return an error"; this validator does.
            Resolution::Ambiguous => self.out().issue(
                Severity::Error,
                IssueType::MultipleMatches,
                MessageId::AmbiguousReference,
                location,
                "the reference matches more than one entry of the Bundle".to_owned(),
            ),
            Resolution::Found {
                target,
                via: Via::Entry { from },
            } => self.link(from, target),
            Resolution::Found {
                via: Via::Contained,
                ..
            }
            | Resolution::Missing
            | Resolution::OutOfScope => {}
        }
    }

    /// Checks the aggregation and the target profiles the types of
    /// `element` coded `code` declare on the reference `occurrence` holds.
    ///
    /// A type of another code that names target profiles, a `canonical`,
    /// is listed as unevaluated: the validator resolves no canonical
    /// instance.
    pub(crate) fn targets(
        &mut self,
        structure: &Structure,
        id: &str,
        element: &ElementDefinition,
        occurrence: &Occurrence<'i>,
        code: Option<&str>,
    ) -> Result<(), ValidateError> {
        let mut profiles = Vec::new();
        let mut modes = Vec::new();
        for kind in element
            .r#type
            .iter()
            .filter(|kind| code.is_some() && kind.code.value.as_deref() == code)
        {
            let targets = kind
                .target_profile
                .iter()
                .filter_map(|profile| profile.value.clone());
            if code == Some("Reference") {
                profiles.extend(targets);
                modes.extend(
                    kind.aggregation
                        .iter()
                        .filter_map(|mode| mode.value.clone()),
                );
            } else {
                let listed: Vec<String> = targets.collect();
                if !listed.is_empty() {
                    self.out().unevaluated(
                        structure.url(),
                        id,
                        Constraint::TargetProfile { profiles: listed },
                        &occurrence.location,
                        Reason::CanonicalTarget,
                    );
                }
            }
        }
        let Some(value) = occurrence.value.filter(|value| value.as_object().is_some()) else {
            return Ok(());
        };
        if profiles.is_empty() && modes.is_empty() {
            return Ok(());
        }
        let reference = literal(value);
        let resolution =
            reference.map(|reference| self.validator().resolver().resolve(reference, self.scope()));
        let location = &occurrence.location;
        if !modes.is_empty() {
            let local = reference.is_some_and(|reference| reference.starts_with('#'));
            self.aggregation(structure, id, &modes, (local, resolution), location);
        }
        if !profiles.is_empty() {
            self.target_profiles(structure, id, &profiles, resolution, location)?;
        }
        Ok(())
    }

    /// Holds a reference to the aggregation modes `modes` allow, any of
    /// them.
    fn aggregation(
        &mut self,
        structure: &Structure,
        id: &str,
        modes: &[String],
        (local, resolution): (bool, Option<Resolution<'i>>),
        location: &str,
    ) {
        let allows = |mode: &str| modes.iter().any(|allowed| allowed == mode);
        // NOTE: R4 codesystem-resource-aggregation-mode: `bundled` is a child of `referenced`,
        // so `referenced` admits a reference into the Bundle as well as one outside it.
        let fault = match resolution {
            None => Some("it carries no literal reference, which an aggregation requires"),
            Some(Resolution::Ambiguous) => None,
            Some(Resolution::Found {
                via: Via::Contained,
                ..
            }) => (!allows("contained")).then_some("it names a contained resource"),
            Some(Resolution::Missing) if local => Some(if allows("contained") {
                "it names no contained resource"
            } else {
                "it is a local reference"
            }),
            Some(Resolution::Found {
                via: Via::Entry { .. },
                ..
            }) => (!allows("referenced") && !allows("bundled"))
                .then_some("it names a resource outside the resource that holds it"),
            Some(Resolution::Missing | Resolution::OutOfScope) => {
                if allows("referenced") {
                    None
                } else if !allows("bundled") {
                    Some("it names a resource outside the resource that holds it")
                } else if self.validator().resolver().bundle(self.scope()).is_some() {
                    Some("it names no entry of the Bundle around it")
                } else {
                    self.out().unevaluated(
                        structure.url(),
                        id,
                        Constraint::Aggregation {
                            modes: modes.to_vec(),
                        },
                        location,
                        Reason::NoBundle,
                    );
                    None
                }
            }
        };
        if let Some(fault) = fault {
            self.out().issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::AggregationMismatch,
                location,
                format!(
                    "{id} allows the aggregation {}, and {fault}",
                    modes.join(", ")
                ),
            );
        }
    }

    /// Validates the resource `resolution` found against `profiles`,
    /// passing when it conforms to one.
    ///
    /// A target the walk cannot reach, and a profile the set does not carry,
    /// are listed as unevaluated.
    fn target_profiles(
        &mut self,
        structure: &Structure,
        id: &str,
        profiles: &[String],
        resolution: Option<Resolution<'i>>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let missing = match resolution {
            Some(Resolution::Found { target, .. }) => Err(target),
            None => Ok(Reason::NoLiteralReference),
            Some(Resolution::Missing) => Ok(Reason::UnresolvedReference),
            Some(Resolution::OutOfScope) => Ok(Reason::ReferenceOutOfScope),
            Some(Resolution::Ambiguous) => Ok(Reason::AmbiguousReference),
        };
        let target = match missing {
            Ok(reason) => {
                self.out().unevaluated(
                    structure.url(),
                    id,
                    Constraint::TargetProfile {
                        profiles: profiles.to_vec(),
                    },
                    location,
                    reason,
                );
                return Ok(());
            }
            Err(target) => target,
        };
        let mut failed = Vec::new();
        let mut unknown = false;
        for profile in profiles {
            match self.target_conformance(target, profile, structure, location)? {
                Verdict::Conforms => return Ok(()),
                Verdict::Fails(first) => failed.push((profile, first)),
                Verdict::Unknown(reason) => {
                    unknown = true;
                    self.out().unevaluated(
                        structure.url(),
                        id,
                        Constraint::TargetProfile {
                            profiles: vec![profile.clone()],
                        },
                        location,
                        reason,
                    );
                }
            }
        }
        // NOTE: ElementDefinition.type.targetProfile: "the content must conform to at least
        // one of them", so a target failing the profiles checked may conform to an unknown one.
        if unknown {
            return Ok(());
        }
        let found: Vec<String> = failed
            .iter()
            .map(|(profile, first)| format!("against {profile}: {first}"))
            .collect();
        self.out().issue(
            Severity::Error,
            IssueType::Structure,
            MessageId::TargetProfileMismatch,
            location,
            format!(
                "the target of {id} conforms to none of {}; {}",
                profiles.join(", "),
                found.join("; ")
            ),
        );
        Ok(())
    }

    /// Validates `target` against `profile`, as the definition `from` names
    /// it, once per validation.
    fn target_conformance(
        &mut self,
        target: &'i Value,
        profile: &str,
        from: &Structure,
        location: &str,
    ) -> Result<Verdict, ValidateError> {
        let structure = match self.validator().structure(profile, Some(from)) {
            Ok(structure) => structure,
            Err(ValidateError::Resolve(
                ResolveError::NotFound { .. } | ResolveError::Fragment { .. },
            )) => return Ok(Verdict::Unknown(Reason::ProfileNotFound)),
            Err(error) => return Err(error),
        };
        if matches!(structure.type_name(), "Resource" | "DomainResource") {
            // NOTE: no specification governs this reading: our own design until the abstract
            // base types land (#458); the core Resource and DomainResource admit any resource.
            return Ok(if structure.url().starts_with(CORE) {
                Verdict::Conforms
            } else {
                // TODO(#458): validate a target against a profile on an abstract base type.
                Verdict::Unknown(Reason::AbstractProfile)
            });
        }
        let key = (address(target), Arc::as_ptr(&structure).addr());
        match self.memo(key) {
            // NOTE: no specification governs a reference cycle: our own design; a target
            // whose check is under way further up the walk is taken to conform there.
            Some(Memo::InProgress) => return Ok(Verdict::Conforms),
            Some(Memo::Done(verdict)) => return Ok(verdict),
            None => {}
        }
        self.remember(key, Memo::InProgress);
        let mut fork = self.fork_unclaimed();
        fork.resource(
            target,
            Some(&format!("{location}.resolve()")),
            Some(structure),
        )?;
        let found = fork.into_out();
        let verdict = found.first_error().map_or(Verdict::Conforms, |issue| {
            Verdict::Fails(format!("{} at {}", issue.message, issue.expression))
        });
        self.remember(key, Memo::Done(verdict.clone()));
        Ok(verdict)
    }
}
