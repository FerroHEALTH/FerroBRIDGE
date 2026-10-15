// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One discriminator evaluated for one slice: the nodes the path selects
//! from the item against what the slice sets at the path
//! (<https://hl7.org/fhir/R4/profiling.html#discriminator>, §5.1.0.11).

use std::sync::Arc;

use fhir_types::r4::element_definition::ElementDefinition;

use crate::report::Constraint;
use crate::report::Reason;
use crate::set::ResolveError;
use crate::validate::ValidateError;
use crate::validate::matching::contains_pattern;
use crate::validate::matching::equals_fixed;
use crate::validate::matching::value_of;
use crate::validate::profiles::Conformance;
use crate::validate::profiles::Subject;
use crate::validate::slicing::Discriminator;
use crate::validate::slicing::Unknown;
use crate::validate::slicing::Verdict;
use crate::validate::slicing::definition::Located;
use crate::validate::slicing::definition::locate;
use crate::validate::slicing::instance::Node;
use crate::validate::slicing::instance::NodeKind;
use crate::validate::structure::Structure;
use crate::validate::terminology::Coded;
use crate::validate::terminology::Membership;
use crate::validate::walk::Walk;

impl<'i> Walk<'_, '_, 'i> {
    /// Evaluates `discriminator` for the slice `slice_id` over `nodes`, the
    /// nodes its path selects from the item.
    pub(crate) fn discriminate(
        &mut self,
        structure: &Arc<Structure>,
        slice_id: &str,
        discriminator: &Discriminator,
        nodes: &[Node<'i>],
        location: &str,
    ) -> Result<Verdict, ValidateError> {
        let steps = discriminator.steps.as_deref().unwrap_or_default();
        let located = locate(self.validator(), structure, slice_id, steps)?;
        let unknown = |reason| {
            Verdict::Unknown(vec![Unknown {
                constraint: discriminator.constraint(),
                reason,
            }])
        };
        let verdict = |matches: bool| {
            if matches {
                Verdict::Match
            } else {
                Verdict::NoMatch
            }
        };
        match discriminator.kind.as_str() {
            "value" | "pattern" => {
                let Some(element) = located.element() else {
                    return Ok(unknown(Reason::NoDiscriminatorValue));
                };
                self.value(&located, element, discriminator, nodes)
            }
            "exists" => {
                let Some(element) = located.element() else {
                    return Ok(unknown(Reason::NoDiscriminatorValue));
                };
                let min = element.min.as_ref().and_then(|min| min.value).unwrap_or(0);
                let max = element.max.as_ref().and_then(|max| max.value.as_deref());
                Ok(if min > 0 {
                    verdict(!nodes.is_empty())
                } else if max == Some("0") {
                    verdict(nodes.is_empty())
                } else {
                    unknown(Reason::NoDiscriminatorValue)
                })
            }
            "type" => {
                let allowed = match self.allowed_types(&located, structure)? {
                    Ok(allowed) if !allowed.is_empty() => allowed,
                    Ok(_) => return Ok(unknown(Reason::NoDiscriminatorValue)),
                    Err(reason) => return Ok(unknown(reason)),
                };
                Ok(verdict(nodes.iter().any(|node| {
                    allowed.iter().any(|name| {
                        node.type_name() == name
                            || (matches!(node.kind, NodeKind::Resource(_))
                                && (name == "Resource" || name == "DomainResource"))
                    })
                })))
            }
            "profile" => {
                let profiles: Vec<String> = match &located {
                    Located::Targets(targets) => targets.clone(),
                    Located::Element { .. } => located
                        .types()
                        .into_iter()
                        .flat_map(|(_, profiles)| profiles)
                        .collect(),
                    Located::Missing => Vec::new(),
                };
                if profiles.is_empty() {
                    return Ok(unknown(Reason::NoDiscriminatorValue));
                }
                let from = located.structure().unwrap_or(structure);
                self.conforms_to_any(nodes, (&profiles, from), location, discriminator)
            }
            _ => Ok(unknown(Reason::DiscriminatorType)),
        }
    }

    /// Evaluates a `value` or `pattern` discriminator against the `fixed[x]`,
    /// `pattern[x]` or required binding over an enumerated value set that
    /// `element` sets.
    fn value(
        &self,
        located: &Located,
        element: &ElementDefinition,
        discriminator: &Discriminator,
        nodes: &[Node<'i>],
    ) -> Result<Verdict, ValidateError> {
        let encode = |source| {
            let (structure, id) = match located {
                Located::Element { structure, id, .. } => (structure.url().to_owned(), id.clone()),
                Located::Targets(_) | Located::Missing => (String::new(), String::new()),
            };
            ValidateError::Encode {
                structure,
                element: id,
                source,
            }
        };
        let verdict = |matches: bool| {
            if matches {
                Verdict::Match
            } else {
                Verdict::NoMatch
            }
        };
        if let Some(fixed) = value_of(element, "fixed").map_err(encode)? {
            return Ok(verdict(nodes.iter().any(|node| {
                node.value.is_some_and(|held| equals_fixed(&fixed, held))
            })));
        }
        if let Some(pattern) = value_of(element, "pattern").map_err(encode)? {
            return Ok(verdict(nodes.iter().any(|node| {
                node.value
                    .is_some_and(|held| contains_pattern(&pattern, held))
            })));
        }
        let binding = element
            .binding
            .as_ref()
            .filter(|binding| binding.strength.value.as_deref() == Some("required"));
        let Some(value_set) = binding
            .and_then(|binding| binding.value_set.as_ref())
            .and_then(|set| set.value.clone())
        else {
            return Ok(Verdict::Unknown(vec![Unknown {
                constraint: discriminator.constraint(),
                reason: Reason::NoDiscriminatorValue,
            }]));
        };
        let Some(from) = located.structure() else {
            return Ok(Verdict::Unknown(vec![Unknown {
                constraint: discriminator.constraint(),
                reason: Reason::NoDiscriminatorValue,
            }]));
        };
        let mut undecided = false;
        for node in nodes {
            let Some(coded) = node.value.and_then(Coded::infer) else {
                continue;
            };
            match self.validator().holds(&value_set, from.origin(), &coded)? {
                Membership::Member => return Ok(Verdict::Match),
                Membership::NotMember => {}
                Membership::Unknown(_) => undecided = true,
            }
        }
        if undecided {
            return Ok(Verdict::Unknown(vec![Unknown {
                constraint: Constraint::DiscriminatorValueSet {
                    path: discriminator.path.clone(),
                    value_set,
                },
                reason: Reason::ValueSetNotEnumerated,
            }]));
        }
        Ok(verdict(false))
    }

    /// The type names a `type` discriminator admits for the slice, `Err`
    /// when a profile naming one is not in the set.
    fn allowed_types(
        &self,
        located: &Located,
        slice: &Structure,
    ) -> Result<Result<Vec<String>, Reason>, ValidateError> {
        let from = located.structure().unwrap_or(slice);
        let type_of = |canonical: &str| -> Result<Result<String, Reason>, ValidateError> {
            match self.validator().structure(canonical, Some(from)) {
                Ok(structure) => Ok(Ok(structure.type_name().to_owned())),
                Err(ValidateError::Resolve(
                    ResolveError::NotFound { .. } | ResolveError::Fragment { .. },
                )) => Ok(Err(Reason::ProfileNotFound)),
                Err(error) => Err(error),
            }
        };
        let mut allowed = Vec::new();
        let mut profiled = Vec::new();
        match located {
            Located::Targets(targets) => profiled.extend(targets.iter().cloned()),
            Located::Element { .. } => {
                for (code, profiles) in located.types() {
                    if (code == "Resource" || code == "DomainResource") && !profiles.is_empty() {
                        profiled.extend(profiles);
                    } else {
                        allowed.push(code);
                    }
                }
            }
            Located::Missing => {}
        }
        for profile in &profiled {
            match type_of(profile)? {
                Ok(name) => allowed.push(name),
                Err(reason) => return Ok(Err(reason)),
            }
        }
        Ok(Ok(allowed))
    }

    /// Answers whether one of `nodes` conforms to one of `profiles`.
    fn conforms_to_any(
        &mut self,
        nodes: &[Node<'i>],
        (profiles, from): (&[String], &Structure),
        location: &str,
        discriminator: &Discriminator,
    ) -> Result<Verdict, ValidateError> {
        let mut unknown = Vec::new();
        for node in nodes {
            let subject = match (node.kind, node.value) {
                (NodeKind::Resource(_), Some(value)) => Subject::Resource { value },
                (NodeKind::Complex(schema), Some(value)) => Subject::Complex {
                    value,
                    name: schema.name,
                },
                _ => {
                    unknown.push(Unknown {
                        constraint: discriminator.constraint(),
                        reason: Reason::NoDiscriminatorValue,
                    });
                    continue;
                }
            };
            for profile in profiles {
                match self.conformance(subject, (profile, from), location)? {
                    Conformance::Checked(found) if found.is_valid() => return Ok(Verdict::Match),
                    Conformance::Checked(_) => {}
                    Conformance::Unknown(reason) => unknown.push(Unknown {
                        constraint: Constraint::TypeProfile {
                            profiles: vec![profile.clone()],
                        },
                        reason,
                    }),
                }
            }
        }
        Ok(if unknown.is_empty() {
            Verdict::NoMatch
        } else {
            Verdict::Unknown(unknown)
        })
    }
}
