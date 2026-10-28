// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Deriving the FHIR resource id of one composition entry.
//!
//! An entry's id is the deterministic id FHIRconnect v1.0.0 recommends, "a
//! **hash** of the Composition UID and the Entry Path"
//! (`engine/id-management.adoc`, §Deterministic IDs), derived by
//! [`fhirconnect::engine::identity::deterministic_id`], the same function the
//! `$tofhir` operation names each occurrence with, so the facade's read, its
//! search and `$tofhir` give one occurrence one id. The composition uid is
//! the version container, the one part of an `OBJECT_VERSION_ID` stable
//! across updates, so an update keeps the id as FHIR R4 asks ("once
//! assigned, this value never changes", <https://hl7.org/fhir/R4/resource.html>);
//! `meta.versionId` carries the `version_tree_id`. A resource an outbound run
//! creates beside its entry takes the digest [`created_digest`] derives. The
//! hash, its rendering and the created resources' key are not fixed by the
//! section: our own design.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

use fhirconnect::engine::identity::base32;
use fhirconnect::engine::identity::deterministic_id;
use fhirconnect::engine::seam::IdentityRequest;

use crate::facade::identity::ExternalResourceId;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::record::CompositionBinding;

/// The identity derivation this build writes.
///
/// Derivation 3 keys an entry by the path the engine's `$tofhir` names its
/// occurrence by and derives its id by the engine's deterministic id. The
/// store holds no binding of an earlier derivation: a store file written
/// before derivation 3 is refused by its schema, and its reset is to remove
/// the file (no specification governs the derivation: our own design).
pub const DERIVATION: u32 = 3;

/// What one composition entry is addressed by, before any store lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryKey {
    /// The version container the entry lives in.
    versioned_object_uid: HierObjectId,
    /// The path that names the entry inside the composition.
    entry_path: String,
}

impl EntryKey {
    /// Returns the key of the entry at `entry_path` in `versioned_object_uid`.
    #[must_use]
    pub fn new(versioned_object_uid: HierObjectId, entry_path: impl Into<String>) -> Self {
        Self {
            versioned_object_uid,
            entry_path: entry_path.into(),
        }
    }

    /// Returns the version container the entry lives in.
    #[must_use]
    pub const fn versioned_object_uid(&self) -> &HierObjectId {
        &self.versioned_object_uid
    }

    /// Returns the path that names the entry.
    #[must_use]
    pub fn entry_path(&self) -> &str {
        &self.entry_path
    }
}

/// Returns the id of the entry `key` names: the deterministic id `$tofhir`
/// answers for the same occurrence.
///
/// # Panics
///
/// Never in practice: the engine renders 32 digest bytes as 52 lowercase
/// base32 characters, every one of them inside the R4 `id` grammar.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "the deterministic id is 52 lowercase base32 characters, always inside the R4 id grammar; an_entry_id_is_the_deterministic_id_tofhir_answers pins it"
)]
pub fn entry_id(key: &EntryKey) -> FhirResourceId {
    let id = deterministic_id(key.versioned_object_uid.value(), &key.entry_path);
    FhirResourceId::new(&id).expect("a deterministic id should be a legal FHIR id")
}

/// Returns the identity-map key of one entry: its id, so the map wins once
/// written.
#[must_use]
pub fn map_key(key: &EntryKey) -> ExternalResourceId {
    let id = entry_id(key);
    // NOTE: no specification governs this: our own design; the id carries no
    // control character, so the fallback keeps the request path free of a panic.
    ExternalResourceId::new(id.as_str())
        .unwrap_or_else(|_refusal| ExternalResourceId::of_digest(&id))
}

/// What one resource an outbound run creates is addressed by, before any
/// store lookup.
///
/// The key is the entry its parent is bound to and the request the engine
/// names the created resource with: its type, the parent's type, the mapping
/// that creates it, the openEHR positions of the occurrence and a split's
/// `unique` values. The composition's version uid the request may carry is
/// left out, so the id stays stable across composition versions as an entry's
/// does (no specification governs this: our own design).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedKey {
    /// The version container of the parent's entry.
    versioned_object_uid: String,
    /// The path of the parent's entry.
    entry_path: String,
    /// The split occurrence of the parent's binding.
    split: u32,
    /// The resource type of the created resource.
    resource_type: String,
    /// The resource type of the parent.
    parent_type: String,
    /// The mapping or model that creates the resource.
    mapping: String,
    /// The openEHR positions of the occurrence, outermost first.
    occurrence: Vec<u32>,
    /// The `unique` values of a split, in declaration order.
    unique: Vec<String>,
}

/// Returns the key of the resource `request` describes, created by a run over
/// the entry `parent` binds.
#[must_use]
pub fn created_key(parent: &CompositionBinding, request: &IdentityRequest) -> CreatedKey {
    CreatedKey {
        versioned_object_uid: parent.versioned_object_uid.clone(),
        entry_path: parent.entry_path.clone(),
        split: parent.split,
        resource_type: request.resource_type().to_owned(),
        parent_type: request.parent_type().to_owned(),
        mapping: request.mapping().to_owned(),
        occurrence: request.occurrence().to_vec(),
        unique: request.unique().to_vec(),
    }
}

/// Returns the identity-map key of one created resource.
///
/// As [`map_key`] does for an entry, the key is the digest itself, so the map
/// wins once written.
#[must_use]
pub fn created_map_key(key: &CreatedKey) -> ExternalResourceId {
    let digest = created_digest(key);
    // The digest is 52 base32 characters, which carry no control character,
    // so the constructor cannot refuse, and the fallback keeps the request
    // path free of a panic.
    ExternalResourceId::new(digest.as_str())
        .unwrap_or_else(|_refusal| ExternalResourceId::of_digest(&digest))
}

/// Returns the digest-derived id of one created resource.
///
/// Every field is framed with its byte length and every list with its count,
/// behind a `created` tag no entry key starts with, so no created key frames
/// to the bytes of another key. No specification governs the framing: our
/// own design.
///
/// # Panics
///
/// Never in practice: 32 digest bytes render as exactly 52 characters of the
/// base32 alphabet, and every one of them is inside the R4 `id` grammar.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "base32 emits only alphabet characters, and 32 digest bytes render as exactly 52 of them, so the rendering is always inside the R4 id grammar; a_created_digest_is_52_characters_inside_the_r4_grammar pins both facts"
)]
pub fn created_digest(key: &CreatedKey) -> FhirResourceId {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    let mut frame = |field: &[u8]| {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(field);
    };
    frame(b"created");
    frame(key.versioned_object_uid.as_bytes());
    frame(key.entry_path.as_bytes());
    frame(&key.split.to_be_bytes());
    frame(key.resource_type.as_bytes());
    frame(key.parent_type.as_bytes());
    frame(key.mapping.as_bytes());
    frame(
        &u64::try_from(key.occurrence.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for position in &key.occurrence {
        frame(&position.to_be_bytes());
    }
    frame(
        &u64::try_from(key.unique.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for value in &key.unique {
        frame(value.as_bytes());
    }
    let rendered = base32(hasher.finalize().as_slice());
    FhirResourceId::new(&rendered).expect("a base32 rendering should be a legal FHIR id")
}

#[cfg(test)]
mod tests {
    use super::{EntryKey, entry_id};
    use crate::facade::identity::is_fhir_id;
    use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

    /// The version container the cases derive against.
    const CONTAINER: &str = "8849182c-82ad-4088-a07f-48ead4180515";

    /// The number of characters the engine renders a deterministic id in.
    const DIGEST_CHARACTERS: usize = fhirconnect::engine::identity::DIGEST_CHARACTERS;

    /// Returns the key of one entry in [`CONTAINER`].
    fn key(path: &str) -> EntryKey {
        EntryKey::new(HierObjectId::new(CONTAINER).unwrap(), path)
    }

    // NOTE: FHIRconnect v1.0.0 `engine/id-management.adoc` §Deterministic IDs:
    // the id is a hash of the composition uid and the entry path.
    #[test]
    fn an_entry_id_is_the_deterministic_id_tofhir_answers() {
        let path = "/content[openEHR-EHR-SECTION.adhoc.v1]/items[2]";
        let id = entry_id(&key(path));
        assert_eq!(
            fhirconnect::engine::identity::deterministic_id(CONTAINER, path),
            id.as_str()
        );
        assert_eq!(DIGEST_CHARACTERS, id.as_str().chars().count());
        assert!(is_fhir_id(id.as_str()), "{id} is outside the R4 grammar");
    }

    #[test]
    fn the_same_entry_derives_the_same_id_every_time() {
        assert_eq!(entry_id(&key("/content[2]")), entry_id(&key("/content[2]")));
        assert_ne!(entry_id(&key("/content[2]")), entry_id(&key("/content[3]")));
    }

    /// Returns a parent binding of the first entry in [`CONTAINER`].
    fn parent() -> crate::facade::identity::record::CompositionBinding {
        crate::facade::identity::record::CompositionBinding {
            ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
            versioned_object_uid: String::from(CONTAINER),
            template_id: String::from("ferrobridge.diagnose.v1"),
            resource_type: String::from("Condition"),
            entry_path: String::from("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"),
            split: 0,
            context: String::from("ferrobridge_facade.context"),
            derivation: Some(super::DERIVATION),
            created: None,
        }
    }

    /// Returns the created id of a split occurrence at `position`.
    fn created_at(position: u32, unique: &[&str]) -> crate::facade::identity::FhirResourceId {
        let request =
            fhirconnect::engine::seam::IdentityRequest::new("Condition", "Condition", "split")
                .with_occurrence(vec![position])
                .with_unique(unique.iter().map(|value| String::from(*value)).collect());
        super::created_digest(&super::created_key(&parent(), &request))
    }

    #[test]
    fn a_created_digest_is_52_characters_inside_the_r4_grammar() {
        let id = created_at(2, &[]);
        assert_eq!(DIGEST_CHARACTERS, id.as_str().chars().count());
        assert!(is_fhir_id(id.as_str()), "{id} is outside the R4 grammar");
        assert_eq!(
            id,
            created_at(2, &[]),
            "the same request derives the same id"
        );
    }

    #[test]
    fn the_occurrence_and_the_unique_values_separate_two_created_resources() {
        assert_ne!(created_at(2, &[]), created_at(3, &[]));
        assert_ne!(created_at(2, &["left"]), created_at(2, &["right"]));
        assert_ne!(created_at(2, &["ab"]), created_at(2, &["a", "b"]));
    }

    #[test]
    fn the_composition_version_does_not_move_a_created_id() {
        let request =
            fhirconnect::engine::seam::IdentityRequest::new("Condition", "Condition", "split")
                .with_occurrence(vec![2]);
        let one = request
            .clone()
            .with_composition(format!("{CONTAINER}::ferroehr::1"));
        let two = request.with_composition(format!("{CONTAINER}::ferroehr::2"));
        assert_eq!(
            super::created_digest(&super::created_key(&parent(), &one)),
            super::created_digest(&super::created_key(&parent(), &two))
        );
    }

    #[test]
    fn a_created_digest_never_equals_the_entry_digest_of_its_parent() {
        let entry = key("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]");
        assert_ne!(entry_id(&entry), created_at(1, &[]));
    }
}
