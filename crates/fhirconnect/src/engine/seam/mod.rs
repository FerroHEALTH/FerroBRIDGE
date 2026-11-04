// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a run asks of its caller.
//!
//! A run asks for the functions a `mappingCode` names, the resources a
//! reference points at, the identity of a resource the run creates, the
//! terminology server a `conceptmap` translates through, and the demographics
//! endpoint a party reference resolves through.
//!
//! The engine makes no call of its own. Going into openEHR, a `reference`
//! mapping maps the referenced resource, which "should" be fetched when it is
//! not in the Bundle, and "if not possible, the engine proceeds with the
//! mapping" (`docs/specs/fhirconnect/modules/ROOT/pages/engine/references.adoc`),
//! so the fetch is a seam and a reference it cannot resolve is a declared
//! skip. Coming out of openEHR, `reference` and `hierarchy.split` create
//! resources, and no specification says which id a created resource takes, so
//! that is a seam too; the default derives it from the request. No
//! specification governs the shape of either seam: our own design.

use core::error::Error;
use core::fmt;

use fhir_types::codec::Value;

use crate::engine::bundle::Entry;
use crate::engine::bundle::resolve;
use crate::engine::demographics::Demographics;
use crate::engine::demographics::NoDemographics;
use crate::engine::linked::LinkedCompositions;
use crate::engine::terminology::NoTerminology;
use crate::engine::terminology::Terminology;
use crate::engine::traverse::functions::MappingFunctions;
use crate::engine::traverse::functions::NoMappingFunctions;
use crate::resolve::program::binding::ResourceType;

pub mod identity;

use identity::{DerivedIdentity, IdentitySink, ResourceIdentity};

/// Where the resource a FHIR reference points at comes from.
pub trait ReferenceSource: fmt::Debug + Send + Sync {
    /// Returns the resource `reference` points at, `None` when the source
    /// holds none.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when the source could not answer, which is
    /// a failure and never an absence.
    fn fetch(
        &self,
        reference: &str,
        expected: &ResourceType,
    ) -> Result<Option<Value>, ReferenceError>;
}

/// The source that holds no resource, for a run that resolves no reference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoReferences;

impl ReferenceSource for NoReferences {
    fn fetch(
        &self,
        _reference: &str,
        _expected: &ResourceType,
    ) -> Result<Option<Value>, ReferenceError> {
        Ok(None)
    }
}

/// The entries of one Bundle, looked up before a further source.
///
/// A reference names an entry by the R4 rules for resolving a reference
/// inside a Bundle (<https://hl7.org/fhir/R4/bundle.html#references>,
/// §2.36.4.1), the same resolver the `$toopenehr` subject check runs: an
/// absolute reference matches a `fullUrl`, a relative `<type>/<id>` is read
/// against the service base of the referrer's server `fullUrl`, and a
/// version-specific reference matches the entry's `meta.versionId`.
#[derive(Debug)]
pub struct BundleReferences<'source> {
    entries: Vec<Entry>,
    referrer: Option<String>,
    further: &'source dyn ReferenceSource,
}

impl<'source> BundleReferences<'source> {
    /// Creates a source over the entries of one Bundle, each with its
    /// `fullUrl` when it carries one, asking `further` for what the Bundle
    /// does not hold.
    #[must_use]
    pub fn new(
        entries: Vec<(Option<String>, Value)>,
        further: &'source dyn ReferenceSource,
    ) -> Self {
        Self {
            entries,
            referrer: None,
            further,
        }
    }

    /// Returns this source with the `fullUrl` of the entry whose references
    /// it resolves, which a relative reference is read against.
    #[must_use]
    pub fn with_referrer(mut self, full_url: impl Into<String>) -> Self {
        self.referrer = Some(full_url.into());
        self
    }
}

impl ReferenceSource for BundleReferences<'_> {
    fn fetch(
        &self,
        reference: &str,
        expected: &ResourceType,
    ) -> Result<Option<Value>, ReferenceError> {
        let found = resolve(&self.entries, self.referrer.as_deref(), reference);
        match *found.as_slice() {
            [] => self.further.fetch(reference, expected),
            [only] => Ok(self.entries.get(only).map(|(_, resource)| resource.clone())),
            ref several => Err(ReferenceError::Ambiguous {
                reference: String::from(reference),
                count: several.len(),
            }),
        }
    }
}

/// Why a reference or an identity could not be answered.
#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    /// The source failed while fetching the reference.
    #[error("the resource {reference} could not be fetched")]
    Fetch {
        /// The reference the source was asked for.
        reference: String,
        /// The failure the source reported.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The reference points at a resource of another type.
    #[error("{reference} resolves to a {found}, and the mapping reads a {expected}")]
    WrongType {
        /// The reference that was resolved.
        reference: String,
        /// The resource type the mapping names.
        expected: String,
        /// The resource type the source answered with.
        found: String,
    },
    /// The reference names more than one entry of the Bundle.
    ///
    /// Two entries answer one reference when they share a `fullUrl`, which
    /// the R4 Bundle invariant `bdl-7` allows only for different versions of
    /// one resource (<https://hl7.org/fhir/R4/bundle.html>), or, read against
    /// no server `fullUrl`, a `resourceType` and `id`; a reference that names
    /// no version cannot choose between them.
    #[error("{reference} names {count} entries of the bundle")]
    Ambiguous {
        /// The reference that was resolved.
        reference: String,
        /// How many entries it names.
        count: usize,
    },
    /// The caller could not link the composition of the resource a `link`
    /// names.
    #[error("the composition of {reference} could not be linked")]
    Link {
        /// The reference the `link` names.
        reference: String,
        /// Why the caller could not link it.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The sink could not assign an id.
    #[error("no id could be assigned to the {resource_type} the run creates")]
    Identity {
        /// The type of the resource the run creates.
        resource_type: String,
        /// The failure the sink reported.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

/// Everything a run calls out to, in one place.
///
/// [`Seams::default`] registers no function, resolves no reference, derives
/// every created id with [`DerivedIdentity`], leaves the id of the mapped
/// resource to the operations' own derivation (no [`ResourceIdentity`]) and
/// translates through [`NoTerminology`], so a `conceptmap` fails closed.
///
/// Every seam trait is `Send + Sync`, so the seams of one run are `Send` and
/// an async caller can hold them across an `.await`
/// (<https://doc.rust-lang.org/std/marker/trait.Sync.html>).
#[derive(Debug, Clone, Copy)]
pub struct Seams<'seam> {
    functions: &'seam dyn MappingFunctions,
    references: &'seam dyn ReferenceSource,
    identities: &'seam dyn IdentitySink,
    terminology: &'seam dyn Terminology,
    demographics: &'seam dyn Demographics,
    resources: Option<&'seam dyn ResourceIdentity>,
    linked: Option<&'seam dyn LinkedCompositions>,
}

// NOTE: no specification governs this: our own design, the seams of a run cross
// an `.await` in an async caller, so a seam trait that loses `Sync` fails here.
const _: fn() = || {
    const fn send<T: Send + Sync>() {}
    send::<Seams<'static>>();
};

impl Default for Seams<'_> {
    fn default() -> Self {
        Self {
            functions: &NoMappingFunctions,
            references: &NoReferences,
            identities: &DerivedIdentity,
            terminology: &NoTerminology,
            demographics: &NoDemographics,
            resources: None,
            linked: None,
        }
    }
}

impl<'seam> Seams<'seam> {
    /// Returns these seams with `functions` as the `mappingCode` registry.
    #[must_use]
    pub fn with_functions(mut self, functions: &'seam dyn MappingFunctions) -> Self {
        self.functions = functions;
        self
    }

    /// Returns these seams with `references` as the reference source.
    #[must_use]
    pub fn with_references(mut self, references: &'seam dyn ReferenceSource) -> Self {
        self.references = references;
        self
    }

    /// Returns these seams with `identities` as the identity sink.
    #[must_use]
    pub fn with_identities(mut self, identities: &'seam dyn IdentitySink) -> Self {
        self.identities = identities;
        self
    }

    /// Returns the `mappingCode` registry.
    #[must_use]
    pub fn functions(&self) -> &'seam dyn MappingFunctions {
        self.functions
    }

    /// Returns the reference source.
    #[must_use]
    pub fn references(&self) -> &'seam dyn ReferenceSource {
        self.references
    }

    /// Returns these seams with `terminology` as the server a `conceptmap`
    /// translates through.
    #[must_use]
    pub fn with_terminology(mut self, terminology: &'seam dyn Terminology) -> Self {
        self.terminology = terminology;
        self
    }

    /// Returns the identity sink.
    #[must_use]
    pub fn identities(&self) -> &'seam dyn IdentitySink {
        self.identities
    }

    /// Returns the terminology server.
    #[must_use]
    pub fn terminology(&self) -> &'seam dyn Terminology {
        self.terminology
    }

    /// Returns these seams with `demographics` as the endpoint a party
    /// reference resolves through.
    #[must_use]
    pub fn with_demographics(mut self, demographics: &'seam dyn Demographics) -> Self {
        self.demographics = demographics;
        self
    }

    /// Returns the demographics endpoint.
    #[must_use]
    pub fn demographics(&self) -> &'seam dyn Demographics {
        self.demographics
    }

    /// Returns these seams with `resources` as what assigns the id of the
    /// resource a `$tofhir` run maps.
    #[must_use]
    pub fn with_resource_identity(mut self, resources: &'seam dyn ResourceIdentity) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Returns what assigns the id of the mapped resource, `None` when the
    /// run derives it (`crate::operations::run`).
    #[must_use]
    pub fn resource_identity(&self) -> Option<&'seam dyn ResourceIdentity> {
        self.resources
    }

    /// Returns these seams with `linked` as where the composition each
    /// linked resource was committed as
    /// ([`crate::engine::linked`]).
    #[must_use]
    pub fn with_linked(mut self, linked: &'seam dyn LinkedCompositions) -> Self {
        self.linked = Some(linked);
        self
    }

    /// Returns where linked compositions stand, `None` when the run produces
    /// one composition and refuses a `link` that names a resource.
    #[must_use]
    pub fn linked(&self) -> Option<&'seam dyn LinkedCompositions> {
        self.linked
    }
}

#[cfg(test)]
mod tests {
    use super::BundleReferences;
    use super::NoReferences;
    use super::ReferenceError;
    use super::ReferenceSource;
    use super::identity::DerivedIdentity;
    use super::identity::IdentityRequest;
    use super::identity::IdentitySink;
    use crate::resolve::program::binding::ResourceType;
    use fhir_types::codec::Value;

    #[test]
    fn a_derived_id_is_stable_and_inside_the_id_grammar() {
        let request = IdentityRequest::new("Condition", "Condition", "context")
            .with_occurrence(vec![2])
            .with_unique(vec![String::from("left")]);
        let first = DerivedIdentity
            .identify(&request)
            .expect("a digest is total");
        let second = DerivedIdentity
            .identify(&request)
            .expect("a digest is total");
        assert_eq!(first, second, "the same request derives the same id");
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn two_occurrences_derive_two_ids() {
        let at = |position: u32| {
            DerivedIdentity
                .identify(
                    &IdentityRequest::new("Condition", "Condition", "context")
                        .with_occurrence(vec![position]),
                )
                .expect("a digest is total")
        };
        assert_ne!(at(1), at(2));
    }

    #[test]
    fn a_bundle_entry_answers_by_full_url_and_by_type_and_id() {
        let specimen = Value::from_serde_json(serde_json::json!({
            "resourceType": "Specimen",
            "id": "synthetic-specimen-1"
        }));
        let source = BundleReferences::new(
            vec![(Some(String::from("urn:uuid:5e3c")), specimen.clone())],
            &NoReferences,
        );
        let expected = ResourceType::new("Specimen");
        assert_eq!(
            source
                .fetch("urn:uuid:5e3c", &expected)
                .expect("no failure"),
            Some(specimen.clone())
        );
        assert_eq!(
            source
                .fetch("Specimen/synthetic-specimen-1", &expected)
                .expect("no failure"),
            Some(specimen)
        );
        assert_eq!(
            source
                .fetch("Specimen/another", &expected)
                .expect("no failure"),
            None
        );
    }

    /// Returns a synthetic `Specimen` with `id`, at `version` when given.
    fn specimen(id: &str, version: Option<&str>) -> Value {
        let mut resource = serde_json::json!({ "resourceType": "Specimen", "id": id });
        if let (Some(version), Some(object)) = (version, resource.as_object_mut()) {
            object.insert(
                String::from("meta"),
                serde_json::json!({ "versionId": version }),
            );
        }
        Value::from_serde_json(resource)
    }

    #[test]
    fn a_relative_reference_is_read_against_the_server_base_of_the_referrer() {
        // bundle.html §2.36.4.1: a relative reference is relative to the
        // server base of the referrer's fullUrl.
        let near = specimen("synthetic-specimen-1", None);
        let far = specimen("synthetic-specimen-1", None);
        let source = BundleReferences::new(
            vec![
                (
                    Some(String::from(
                        "http://example.net/fhir/Specimen/synthetic-specimen-1",
                    )),
                    far,
                ),
                (
                    Some(String::from(
                        "http://example.org/fhir/Specimen/synthetic-specimen-1",
                    )),
                    near.clone(),
                ),
            ],
            &NoReferences,
        )
        .with_referrer("http://example.org/fhir/Observation/synthetic-9");
        let expected = ResourceType::new("Specimen");
        assert_eq!(
            source
                .fetch("Specimen/synthetic-specimen-1", &expected)
                .expect("no failure"),
            Some(near)
        );
    }

    #[test]
    fn a_versioned_reference_answers_the_entry_at_that_version() {
        let url = "http://example.org/fhir/Specimen/synthetic-specimen-1";
        let second = specimen("synthetic-specimen-1", Some("2"));
        let source = BundleReferences::new(
            vec![
                (
                    Some(String::from(url)),
                    specimen("synthetic-specimen-1", Some("1")),
                ),
                (Some(String::from(url)), second.clone()),
            ],
            &NoReferences,
        )
        .with_referrer("http://example.org/fhir/Observation/synthetic-9");
        let expected = ResourceType::new("Specimen");
        assert_eq!(
            source
                .fetch("Specimen/synthetic-specimen-1/_history/2", &expected)
                .expect("no failure"),
            Some(second)
        );
        let refused = source
            .fetch(url, &expected)
            .expect_err("two versions answer an unversioned reference");
        assert!(
            matches!(refused, ReferenceError::Ambiguous { count: 2, .. }),
            "the refusal counts the entries: {refused}"
        );
    }
}
