// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Deriving the FHIR resource id of one composition entry.
//!
//! No specification governs the rule: our own design, beside the hash the
//! FHIRconnect engine chapter recommends (`engine/id-management.adoc`, §Deterministic IDs), held
//! against three facts. FHIR R4
//! `Resource.id` is `[A-Za-z0-9\-\.]{1,64}` and "once assigned, this value
//! never changes" (<https://hl7.org/fhir/R4/resource.html>). An openEHR
//! `OBJECT_VERSION_ID` is `object_id::creating_system_id::version_tree_id`,
//! and only the leading `object_id` is stable across updates. An entry's
//! `LOCATABLE.uid` is optional, so it cannot be the only input.
//!
//! So the id comes from the entry's `uid` when the entry carries one that the
//! R4 grammar admits, and otherwise from a SHA-256 over the version
//! container, the entry path and the split occurrence, rendered as 52
//! lowercase base32 characters. `meta.versionId` carries the
//! `version_tree_id` and never enters the derivation, because a hash over the
//! full version id would change the FHIR id on every update.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

use fhirconnect::engine::seam::IdentityRequest;

use crate::facade::identity::ExternalResourceId;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::is_fhir_id;
use crate::facade::identity::record::CompositionBinding;

/// The RFC 4648 §6 base32 alphabet, lowercased.
///
/// The FHIR id grammar admits no upper-case-only rendering constraint, and a
/// lowercase alphabet keeps a derived id stable under a case-insensitive
/// store. No specification governs the case: our own design.
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// How many base32 characters a 256-bit digest renders as.
///
/// 256 bits at five bits per character is 51.2, so the rendering is 52
/// characters and the last one carries four bits.
pub const DIGEST_CHARACTERS: usize = 52;

/// The identity derivation this build writes.
///
/// Derivation 2 keys an entry by an openEHR path the BASE Release 1.2.0 path
/// grammar parses (Paths and Locators): `/content[<archetype>]` for the first
/// item of the archetype and `/content[<n>]` for every further one. A binding
/// that names no derivation was written by a build up to 0.0.6, whose entry
/// paths may take the `/content[<archetype>, <n>]` form, and is read as it
/// stands (no specification governs the derivation: our own design).
pub const DERIVATION: u32 = 2;

/// Which input produced a derived id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derivation {
    /// The entry carried a `LOCATABLE.uid` the R4 id grammar admits.
    Uid,
    /// The digest over the version container, the entry path and the split
    /// occurrence.
    Digest,
    /// The identity store already held an id for this entry, and the store
    /// wins from then on.
    Recorded,
}

/// What one composition entry is addressed by, before any store lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryKey {
    /// The version container the entry lives in.
    versioned_object_uid: HierObjectId,
    /// The archetype path of the entry inside the composition.
    entry_path: String,
    /// Which document of a `hierarchy` split this entry belongs to.
    ///
    /// The key of the id the FHIRconnect engine chapter recommends
    /// (`engine/id-management.adoc`, §Deterministic IDs) is not unique under a split, so the
    /// occurrence is part of the input (no specification governs this: our own
    /// design).
    split: u32,
}

impl EntryKey {
    /// Returns the key of the entry at `entry_path` in `versioned_object_uid`.
    #[must_use]
    pub fn new(
        versioned_object_uid: HierObjectId,
        entry_path: impl Into<String>,
        split: u32,
    ) -> Self {
        Self {
            versioned_object_uid,
            entry_path: entry_path.into(),
            split,
        }
    }

    /// Returns the version container the entry lives in.
    #[must_use]
    pub const fn versioned_object_uid(&self) -> &HierObjectId {
        &self.versioned_object_uid
    }

    /// Returns the archetype path of the entry.
    #[must_use]
    pub fn entry_path(&self) -> &str {
        &self.entry_path
    }

    /// Returns the split occurrence.
    #[must_use]
    pub const fn split(&self) -> u32 {
        self.split
    }
}

/// Returns the id the entry's own `uid` or the digest produces.
///
/// `uid` is the entry's `LOCATABLE.uid`, when the composition carries one. A
/// `uid` the R4 grammar does not admit (an `OBJECT_VERSION_ID`, whose `::`
/// separators are outside `[A-Za-z0-9\-\.]`) falls through to the digest, so
/// the answer is always a legal FHIR id.
#[must_use]
pub fn derive(key: &EntryKey, uid: Option<&str>) -> (FhirResourceId, Derivation) {
    if let Some(uid) = uid.filter(|uid| is_fhir_id(uid)) {
        // NOTE: the constructor holds the `Resource.id` grammar
        // (<https://hl7.org/fhir/R4/resource.html>) that `is_fhir_id` just
        // checked, so the digest answers a refusal that cannot happen.
        if let Ok(id) = FhirResourceId::new(uid) {
            return (id, Derivation::Uid);
        }
    }
    (digest(key), Derivation::Digest)
}

/// Returns the identity-map key of one entry.
///
/// The map is keyed by the digest over the version container, the entry path
/// and the split occurrence, which is the one part of an entry's identity that
/// does not move across composition versions. Keying on it is what makes the
/// map win once written: an entry that gains a `LOCATABLE.uid` in a later
/// version still resolves to the id the first version recorded (no
/// specification governs this: our own design).
#[must_use]
pub fn map_key(key: &EntryKey) -> ExternalResourceId {
    let digest = digest(key);
    // The digest is 52 base32 characters, which carry no control character,
    // so the constructor cannot refuse, and the fallback keeps the request
    // path free of a panic.
    ExternalResourceId::new(digest.as_str())
        .unwrap_or_else(|_refusal| ExternalResourceId::of_digest(&digest))
}

/// Returns the digest-derived id of `key`.
///
/// The three inputs are framed with their byte lengths so no two different
/// keys can produce the same byte string: a path carrying the separator
/// cannot borrow a neighbour's field. No specification governs the framing:
/// our own design.
///
/// # Panics
///
/// Never in practice: 32 digest bytes render as exactly 52 characters of the
/// base32 alphabet, and every one of them is inside the R4 `id` grammar.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "base32 emits only alphabet characters, and 32 digest bytes render as exactly 52 of them, so the rendering is always inside the R4 id grammar; a_digest_id_is_52_characters_inside_the_r4_grammar pins both facts"
)]
pub fn digest(key: &EntryKey) -> FhirResourceId {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    for field in [
        key.versioned_object_uid.value().as_bytes(),
        key.entry_path.as_bytes(),
    ] {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(field);
    }
    hasher.update(key.split.to_be_bytes());
    let rendered = base32(hasher.finalize().as_slice());
    FhirResourceId::new(&rendered).expect("a base32 rendering should be a legal FHIR id")
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

/// Renders `bytes` in the lowercase RFC 4648 base32 alphabet, without padding.
fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(symbol((buffer >> bits) & 0x1f));
        }
    }
    if bits > 0 {
        out.push(symbol((buffer << (5 - bits)) & 0x1f));
    }
    out
}

/// Returns the alphabet character of one five-bit group.
fn symbol(value: u32) -> char {
    let index = usize::try_from(value).unwrap_or(0);
    // `value` is masked to five bits, so the index is always inside the
    // 32-entry alphabet, and `get` keeps the request path free of a panicking
    // index.
    ALPHABET.get(index).map_or('a', |&byte| char::from(byte))
}

#[cfg(test)]
mod tests {
    use super::{DIGEST_CHARACTERS, Derivation, EntryKey, base32, derive, digest};
    use crate::facade::identity::is_fhir_id;
    use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

    /// The version container the cases derive against.
    const CONTAINER: &str = "8849182c-82ad-4088-a07f-48ead4180515";

    /// Returns the key of one entry in [`CONTAINER`].
    fn key(path: &str, split: u32) -> EntryKey {
        EntryKey::new(HierObjectId::new(CONTAINER).unwrap(), path, split)
    }

    #[test]
    fn base32_follows_rfc_4648_with_the_lowercase_alphabet() {
        // RFC 4648 §10 test vectors, lowercased and with the padding dropped.
        assert_eq!("my", base32(b"f"));
        assert_eq!("mzxq", base32(b"fo"));
        assert_eq!("mzxw6", base32(b"foo"));
        assert_eq!("mzxw6yq", base32(b"foob"));
        assert_eq!("mzxw6ytb", base32(b"fooba"));
        assert_eq!("mzxw6ytboi", base32(b"foobar"));
    }

    #[test]
    fn a_digest_id_is_52_characters_inside_the_r4_grammar() {
        let id = digest(&key(
            "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]",
            0,
        ));
        assert_eq!(DIGEST_CHARACTERS, id.as_str().chars().count());
        assert!(is_fhir_id(id.as_str()), "{id} is outside the R4 grammar");
        assert!(
            id.as_str()
                .chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c)),
            "{id} leaves the base32 alphabet"
        );
    }

    #[test]
    fn the_same_entry_derives_the_same_id_every_time() {
        let first = digest(&key("/content[0]", 0));
        let second = digest(&key("/content[0]", 0));
        assert_eq!(first, second);
    }

    #[test]
    fn the_split_occurrence_separates_two_entries_of_one_path() {
        assert_ne!(
            digest(&key("/content[0]", 0)),
            digest(&key("/content[0]", 1))
        );
    }

    #[test]
    fn the_framing_keeps_two_keys_apart_when_one_field_borrows_the_next() {
        assert_ne!(digest(&key("ab", 0)), digest(&key("b", 0)));
        assert_ne!(
            digest(&EntryKey::new(HierObjectId::new("aab").unwrap(), "c", 0)),
            digest(&EntryKey::new(HierObjectId::new("aa").unwrap(), "bc", 0))
        );
    }

    #[test]
    fn an_entry_uid_wins_over_the_digest_and_an_object_version_id_does_not() {
        let entry = key("/content[0]", 0);
        let (id, source) = derive(&entry, Some("3a2f1c4e-0000-4000-8000-0000000000ab"));
        assert_eq!(Derivation::Uid, source);
        assert_eq!("3a2f1c4e-0000-4000-8000-0000000000ab", id.as_str());

        let (fallback, source) = derive(&entry, Some("3a2f1c4e::ferroehr::1"));
        assert_eq!(Derivation::Digest, source);
        assert_eq!(digest(&entry), fallback);

        let (none, source) = derive(&entry, None);
        assert_eq!(Derivation::Digest, source);
        assert_eq!(digest(&entry), none);
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
        let entry = key("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]", 0);
        assert_ne!(digest(&entry), created_at(1, &[]));
    }
}
