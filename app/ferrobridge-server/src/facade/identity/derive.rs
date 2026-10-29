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
//! creates beside its entry takes the id [`created_id`] derives. The
//! hash, its rendering and the created resources' key are not fixed by the
//! section: our own design.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

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

/// Returns the id of the resource `request` describes.
///
/// The resource is created by a run over the occurrence `parent` binds, and
/// its id is the deterministic id `$tofhir` gives the same created resource
/// ([`fhirconnect::engine::identity::created_id`]).
///
/// # Panics
///
/// Never in practice: the engine renders 32 digest bytes as 52 lowercase
/// base32 characters, every one of them inside the R4 `id` grammar.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "the created id is 52 lowercase base32 characters, always inside the R4 id grammar; a_created_id_is_the_one_tofhir_answers pins it"
)]
pub fn created_id(parent: &CompositionBinding, request: &IdentityRequest) -> FhirResourceId {
    let id = fhirconnect::engine::identity::created_id(
        &parent.versioned_object_uid,
        &parent.entry_path,
        request,
    );
    FhirResourceId::new(&id).expect("a created id should be a legal FHIR id")
}

/// Returns the identity-map key of one created resource: its id, so the map
/// wins once written.
#[must_use]
pub fn created_map_key(id: &FhirResourceId) -> ExternalResourceId {
    // NOTE: no specification governs this: our own design; the id carries no
    // control character, so the fallback keeps the request path free of a panic.
    ExternalResourceId::new(id.as_str())
        .unwrap_or_else(|_refusal| ExternalResourceId::of_digest(id))
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
        super::created_id(&parent(), &request)
    }

    #[test]
    fn a_created_id_is_the_one_tofhir_answers() {
        let request =
            fhirconnect::engine::seam::IdentityRequest::new("Condition", "Condition", "split")
                .with_occurrence(vec![2]);
        let id = super::created_id(&parent(), &request);
        assert_eq!(
            fhirconnect::engine::identity::created_id(
                CONTAINER,
                "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]",
                &request
            ),
            id.as_str()
        );
        assert_eq!(DIGEST_CHARACTERS, id.as_str().chars().count());
        assert!(is_fhir_id(id.as_str()), "{id} is outside the R4 grammar");
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
            super::created_id(&parent(), &one),
            super::created_id(&parent(), &two)
        );
    }

    #[test]
    fn a_created_id_never_equals_the_entry_id_of_its_parent() {
        let entry = key("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]");
        assert_ne!(entry_id(&entry), created_at(1, &[]));
    }
}
