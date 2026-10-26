// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deterministic id of a resource mapped out of a composition.
//!
//! "To ensure deterministic IDs, we recommend generating an ID based on a hash
//! of the Composition UID and the Entry Path" (FHIRconnect v1.0.0
//! `engine/id-management.adoc`, §Deterministic IDs), for a resource whose
//! input "was not previously provided in FHIR". The `$tofhir` operation and
//! the FHIR facade both derive the id here, so one occurrence takes one id on
//! every surface.
//!
//! The section fixes neither the hash nor its rendering nor which part of the
//! composition uid enters it: no specification governs those, our own design.
//! The composition uid is the `object_id` of the `OBJECT_VERSION_ID`, the one
//! part stable across versions (openEHR RM Common, §`OBJECT_VERSION_ID`), so
//! an update keeps the id "the same" on every export as the section asks. The
//! entry path is the path [`openehr_mapping_core::index::WebTemplateIndex::entry_path`]
//! names the start archetype's instance by. The hash is SHA-256 over the two
//! fields, each framed with its byte length, rendered as 52 lowercase RFC 4648
//! base32 characters, which fit the R4 id grammar `[A-Za-z0-9\-\.]{1,64}`
//! (<https://hl7.org/fhir/R4/datatypes.html#id>).

use sha2::Digest;

/// The RFC 4648 §6 base32 alphabet, lowercased.
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// How many base32 characters a 256-bit digest renders as: 256 bits at five
/// bits per character is 51.2, so the last of 52 carries one bit.
pub const DIGEST_CHARACTERS: usize = 52;

/// The entry path of a context whose start is the composition itself, the
/// root of the archetyped structure (openEHR BASE Release 1.2.0, Paths and
/// Locators).
pub const ROOT_PATH: &str = "/";

/// Returns the `object_id` of an `OBJECT_VERSION_ID`, its leading segment
/// before the first `::`, or the whole text when it carries none.
#[must_use]
pub fn versioned_object_uid(uid: &str) -> &str {
    uid.split("::").next().unwrap_or(uid)
}

/// Returns the deterministic id of the resource mapped from the instance at
/// `entry_path` of the composition whose version container is
/// `versioned_object_uid`.
#[must_use]
pub fn deterministic_id(versioned_object_uid: &str, entry_path: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    for field in [versioned_object_uid.as_bytes(), entry_path.as_bytes()] {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(field);
    }
    base32(hasher.finalize().as_slice())
}

/// Renders `bytes` in the lowercase RFC 4648 base32 alphabet, without padding.
#[must_use]
pub fn base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits = bits.saturating_add(8);
        while bits >= 5 {
            bits = bits.saturating_sub(5);
            out.push(symbol((buffer >> bits) & 0x1f));
        }
    }
    if bits > 0 {
        out.push(symbol((buffer << (5_u32.saturating_sub(bits))) & 0x1f));
    }
    out
}

/// Returns the alphabet character of one five-bit group.
fn symbol(value: u32) -> char {
    let index = usize::try_from(value).unwrap_or(0);
    // NOTE: `value` is masked to five bits, so `get` always finds a character;
    // no specification governs this, our own design, kept free of a panic.
    ALPHABET.get(index).map_or('a', |&byte| char::from(byte))
}

#[cfg(test)]
mod tests {
    use super::DIGEST_CHARACTERS;
    use super::base32;
    use super::deterministic_id;
    use super::versioned_object_uid;

    #[test]
    fn a_deterministic_id_is_52_characters_inside_the_r4_grammar() {
        let id = deterministic_id(
            "d2b3c1a0-0000-4000-8000-000000000001",
            "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]",
        );
        assert_eq!(id.len(), DIGEST_CHARACTERS);
        assert!(
            id.chars()
                .all(|character| character.is_ascii_lowercase() || ('2'..='7').contains(&character))
        );
    }

    #[test]
    fn the_id_moves_with_either_input_and_never_with_the_version() {
        let uid = "d2b3c1a0-0000-4000-8000-000000000001";
        let first = deterministic_id(uid, "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]");
        assert_ne!(first, deterministic_id(uid, "/content[2]"));
        assert_ne!(
            first,
            deterministic_id(
                "d2b3c1a0-0000-4000-8000-000000000002",
                "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"
            )
        );
        assert_eq!(
            versioned_object_uid("d2b3c1a0-0000-4000-8000-000000000001::ferrobridge.example::3"),
            uid
        );
    }

    #[test]
    fn framing_keeps_a_field_boundary() {
        assert_ne!(deterministic_id("ab", "c"), deterministic_id("a", "bc"));
    }

    #[test]
    fn base32_renders_the_rfc_4648_vectors_in_lower_case() {
        // NOTE: RFC 4648 §10 test vectors, lowercased and unpadded.
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"f"), "my");
        assert_eq!(base32(b"fo"), "mzxq");
        assert_eq!(base32(b"foo"), "mzxw6");
        assert_eq!(base32(b"foob"), "mzxw6yq");
        assert_eq!(base32(b"fooba"), "mzxw6ytb");
        assert_eq!(base32(b"foobar"), "mzxw6ytboi");
    }
}
