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
use sha2::Digest;

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

/// What assigns the id of a resource a run creates.
pub trait IdentitySink: fmt::Debug + Send + Sync {
    /// Returns the FHIR id the resource `request` describes takes.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when no id can be assigned.
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError>;
}

/// Everything a run knows about one resource it creates.
///
/// The same composition and the same program produce the same request, so a
/// sink that derives the id from it answers the same id on every run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRequest {
    resource_type: String,
    parent_type: String,
    parent_id: Option<String>,
    composition: Option<String>,
    mapping: String,
    occurrence: Vec<u32>,
    unique: Vec<String>,
}

impl IdentityRequest {
    /// Creates the request for a `resource_type` a mapping creates under a
    /// resource of `parent_type`.
    #[must_use]
    pub fn new(
        resource_type: impl Into<String>,
        parent_type: impl Into<String>,
        mapping: impl Into<String>,
    ) -> Self {
        Self {
            resource_type: resource_type.into(),
            parent_type: parent_type.into(),
            parent_id: None,
            composition: None,
            mapping: mapping.into(),
            occurrence: Vec::new(),
            unique: Vec::new(),
        }
    }

    /// Returns this request with the parent's id, when the run knows it.
    #[must_use]
    pub fn with_parent_id(mut self, id: impl Into<String>) -> Self {
        self.parent_id = Some(id.into());
        self
    }

    /// Returns this request with the `LOCATABLE.uid` of the composition the
    /// run reads.
    #[must_use]
    pub fn with_composition(mut self, uid: impl Into<String>) -> Self {
        self.composition = Some(uid.into());
        self
    }

    /// Returns this request with the openEHR positions of the occurrence the
    /// resource is created for, outermost first.
    #[must_use]
    pub fn with_occurrence(mut self, occurrence: Vec<u32>) -> Self {
        self.occurrence = occurrence;
        self
    }

    /// Returns this request with the `unique` values of a split.
    #[must_use]
    pub fn with_unique(mut self, unique: Vec<String>) -> Self {
        self.unique = unique;
        self
    }

    /// Returns the type of the resource created.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the type of the resource it is created under.
    #[must_use]
    pub fn parent_type(&self) -> &str {
        &self.parent_type
    }

    /// Returns the id of the resource it is created under, when known.
    #[must_use]
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// Returns the `LOCATABLE.uid` of the composition read, when it has one.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    /// Returns the dotted name of the mapping or the context that creates it.
    #[must_use]
    pub fn mapping(&self) -> &str {
        &self.mapping
    }

    /// Returns the openEHR positions of the occurrence, outermost first.
    #[must_use]
    pub fn occurrence(&self) -> &[u32] {
        &self.occurrence
    }

    /// Returns the `unique` values of a split, in declaration order.
    #[must_use]
    pub fn unique(&self) -> &[String] {
        &self.unique
    }
}

/// What assigns the id of the resource a `$tofhir` run maps a composition to.
///
/// The engine "must implement its own ID management" and an export's resource
/// ids "must remain the same" every time a composition is exported
/// (`docs/specs/fhirconnect/modules/ROOT/pages/engine/id-management.adoc`,
/// §Resource IDs and §Deterministic IDs). A caller with an identity store
/// answers that here; without one the operations keep the id the mapping
/// wrote or derive it from the composition (`crate::operations::run`). The
/// shape of the seam is our own design.
pub trait ResourceIdentity: fmt::Debug + Send + Sync {
    /// Returns the FHIR id the mapped resource `request` describes takes.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when no id can be assigned, which refuses
    /// the run.
    fn identify(&self, request: &ResourceRequest) -> Result<String, ReferenceError>;
}

/// Everything a `$tofhir` run knows about the resource it maps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRequest {
    resource_type: String,
    template: String,
    composition: Option<String>,
    written: Option<String>,
}

impl ResourceRequest {
    /// Creates the request for a `resource_type` mapped from a composition of
    /// `template`.
    #[must_use]
    pub fn new(resource_type: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            template: template.into(),
            composition: None,
            written: None,
        }
    }

    /// Returns this request with the `OBJECT_VERSION_ID` of the composition.
    #[must_use]
    pub fn with_composition(mut self, uid: impl Into<String>) -> Self {
        self.composition = Some(uid.into());
        self
    }

    /// Returns this request with the `id` the mapping wrote.
    #[must_use]
    pub fn with_written(mut self, id: impl Into<String>) -> Self {
        self.written = Some(id.into());
        self
    }

    /// Returns the type of the mapped resource.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the template of the composition the run read.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }

    /// Returns the `OBJECT_VERSION_ID` of the composition, when it has one.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    /// Returns the `id` the mapping wrote, when it wrote one.
    #[must_use]
    pub fn written(&self) -> Option<&str> {
        self.written.as_deref()
    }
}

/// The sink that derives an id from the request by a digest.
///
/// Every field is framed with its byte length before it is hashed, so two
/// different requests never frame to the same bytes, and the SHA-256 digest
/// renders as 64 lowercase hexadecimal characters, inside the R4 id grammar
/// `[A-Za-z0-9\-\.]{1,64}` (<https://hl7.org/fhir/R4/datatypes.html#id>).
/// No specification governs the derivation: our own design.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DerivedIdentity;

impl IdentitySink for DerivedIdentity {
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError> {
        let mut hasher = sha2::Sha256::new();
        let mut frame = |field: &[u8]| {
            hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
            hasher.update(field);
        };
        frame(request.resource_type.as_bytes());
        frame(request.parent_type.as_bytes());
        frame(request.parent_id.as_deref().unwrap_or_default().as_bytes());
        frame(
            request
                .composition
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
        );
        frame(request.mapping.as_bytes());
        for position in &request.occurrence {
            frame(&position.to_be_bytes());
        }
        frame(b"unique");
        for value in &request.unique {
            frame(value.as_bytes());
        }
        let mut id = String::with_capacity(64);
        for byte in hasher.finalize().as_slice() {
            for nibble in [byte >> 4, byte & 0x0f] {
                id.extend(char::from_digit(u32::from(nibble), 16));
            }
        }
        Ok(id)
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
    use super::DerivedIdentity;
    use super::IdentityRequest;
    use super::IdentitySink;
    use super::NoReferences;
    use super::ReferenceError;
    use super::ReferenceSource;
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
