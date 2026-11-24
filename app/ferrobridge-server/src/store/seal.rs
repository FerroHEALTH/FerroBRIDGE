// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys the state store seals stored bodies under.
//!
//! Each key is named by the id a sealed row carries, so a key can be rotated
//! while rows sealed under the one before it are still read. No
//! specification governs the store's sealing: our own design.

use std::fmt;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use secrecy::{ExposeSecret as _, SecretBox};

/// The length of a sealing key in bytes.
pub const KEY_LEN: usize = 32;

/// The longest key id.
const ID_MAX: usize = 64;

/// Why a sealing key or its id is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KeyError {
    /// The id is empty, longer than 64 characters, or holds a character
    /// outside `A-Z`, `a-z`, `0-9`, `.`, `_` and `-`.
    #[error("a key id is 1 to 64 characters of A-Z, a-z, 0-9, '.', '_' and '-'")]
    Id,
    /// The key is not the standard padded base64 of exactly 32 bytes.
    #[error("a sealing key is the base64 of exactly 32 bytes")]
    Format,
}

/// The id a sealed row names its key by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyId(String);

impl KeyId {
    /// Returns `id` as a key id.
    ///
    /// # Errors
    /// Returns [`KeyError::Id`] for an empty or longer id, or one with a
    /// character outside the id alphabet.
    pub fn new(id: &str) -> Result<Self, KeyError> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        if id.is_empty() || id.len() > ID_MAX || !id.chars().all(allowed) {
            return Err(KeyError::Id);
        }
        Ok(Self(id.to_owned()))
    }

    /// Returns the id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A 32-byte sealing key; it renders as `***` and is zeroed on drop.
#[derive(Clone)]
pub struct SealingKey(Arc<SecretBox<[u8; KEY_LEN]>>);

impl SealingKey {
    /// Reads a key from the base64 of its 32 bytes, as `openssl rand -base64
    /// 32` writes it.
    ///
    /// # Errors
    /// Returns [`KeyError::Format`] when `text` is not standard padded base64
    /// or does not decode to exactly 32 bytes.
    pub fn from_base64(text: &str) -> Result<Self, KeyError> {
        // NOTE: no specification governs this: our own design; the decoder's error
        // names the offending byte, which is key material, so it is dropped.
        let decoded = SecretBox::new(Box::new(
            STANDARD.decode(text.trim()).ok().ok_or(KeyError::Format)?,
        ));
        let bytes = <[u8; KEY_LEN]>::try_from(decoded.expose_secret().as_slice())
            .ok()
            .ok_or(KeyError::Format)?;
        Ok(Self(Arc::new(SecretBox::new(Box::new(bytes)))))
    }

    /// Returns the key's bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SealingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SealingKey(***)")
    }
}

/// The sealing keys, the one that seals first.
#[derive(Debug, Clone)]
pub struct SealKeys {
    /// The key new rows are sealed under.
    current: (KeyId, SealingKey),
    /// The keys rows sealed before a rotation are opened with.
    previous: Vec<(KeyId, SealingKey)>,
}

impl SealKeys {
    /// Returns the keys with `current` sealing and `previous` opening.
    #[must_use]
    pub const fn new(current: (KeyId, SealingKey), previous: Vec<(KeyId, SealingKey)>) -> Self {
        Self { current, previous }
    }

    /// Returns the id and the key new rows are sealed under.
    #[must_use]
    pub const fn current(&self) -> (&KeyId, &SealingKey) {
        (&self.current.0, &self.current.1)
    }

    /// Returns the key `id` names, to open a row sealed under it.
    #[must_use]
    pub fn opening(&self, id: &KeyId) -> Option<&SealingKey> {
        std::iter::once(&self.current)
            .chain(&self.previous)
            .find(|(named, _)| named == id)
            .map(|(_, key)| key)
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyError, KeyId, SealKeys, SealingKey};

    /// The base64 of 32 bytes of `A`, made for the tests.
    const TEST_KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

    #[test]
    fn a_key_is_the_base64_of_32_bytes_and_never_renders() {
        let key = SealingKey::from_base64(TEST_KEY).expect("a 32-byte key");
        assert_eq!(&[b'A'; 32], key.bytes());
        assert_eq!("SealingKey(***)", format!("{key:?}"));
        for text in ["QUFB", "not base64!"] {
            assert_eq!(
                Some(KeyError::Format),
                SealingKey::from_base64(text).err(),
                "{text}"
            );
        }
    }

    #[test]
    fn an_id_is_short_and_plain() {
        assert!(KeyId::new("2026-11.a_b").is_ok());
        for id in ["", "has space", "\u{fc}mlaut", &"x".repeat(65)] {
            assert_eq!(Err(KeyError::Id), KeyId::new(id), "{id}");
        }
    }

    #[test]
    fn every_key_opens_and_the_first_seals() {
        let key = SealingKey::from_base64(TEST_KEY).expect("a 32-byte key");
        let id = |text| KeyId::new(text).expect("a plain id");
        let keys = SealKeys::new((id("new"), key.clone()), vec![(id("old"), key)]);
        assert_eq!("new", keys.current().0.as_str());
        assert!(keys.opening(&id("old")).is_some());
        assert!(keys.opening(&id("gone")).is_none());
    }
}
