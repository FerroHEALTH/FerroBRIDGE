// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys the state store seals stored bodies under.
//!
//! Each key is named by the id a sealed row carries, so a key can be rotated
//! while rows sealed under the one before it are still read. No
//! specification governs the store's sealing: our own design.

use std::fmt;
use std::sync::Arc;

use aws_lc_rs::aead::{AES_256_GCM, Aad, NONCE_LEN, Nonce, RandomizedNonceKey};
use aws_lc_rs::hmac;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use secrecy::{ExposeSecret as _, SecretBox};

/// The length of a sealing key in bytes.
pub const KEY_LEN: usize = 32;

/// The longest key id.
const ID_MAX: usize = 64;

/// Why a value could not be sealed or opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SealError {
    /// The row names a key `[state.seal]` does not list.
    #[error("a sealed row names a key [state.seal] does not list")]
    UnknownKey,
    /// The AES-256-GCM context refused, or the sealed bytes do not open
    /// under their key and associated data.
    #[error("a value could not be sealed or did not open")]
    Cipher,
}

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

/// The sealing keys, the one that seals first, and the lookup key.
#[derive(Debug, Clone)]
pub struct SealKeys {
    /// The key new rows are sealed under.
    current: (KeyId, SealingKey),
    /// The keys rows sealed before a rotation are opened with.
    previous: Vec<(KeyId, SealingKey)>,
    /// The key lookup columns are digested under.
    lookup: LookupKey,
}

impl SealKeys {
    /// Returns the keys with `current` sealing, `previous` opening and
    /// `lookup` digesting.
    #[must_use]
    pub const fn new(
        current: (KeyId, SealingKey),
        previous: Vec<(KeyId, SealingKey)>,
        lookup: LookupKey,
    ) -> Self {
        Self {
            current,
            previous,
            lookup,
        }
    }

    /// Returns the key lookup columns are digested under.
    #[must_use]
    pub const fn lookup(&self) -> &LookupKey {
        &self.lookup
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

impl SealKeys {
    /// Returns `plaintext` sealed under the current key with AES-256-GCM, a
    /// fresh random nonce and `context` as the associated data, as the
    /// nonce followed by the ciphertext and its tag, with the key's id.
    ///
    /// # Errors
    /// Returns [`SealError::Cipher`] when the cipher context fails.
    pub fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<(KeyId, Vec<u8>), SealError> {
        let (id, key) = self.current();
        let key = RandomizedNonceKey::new(&AES_256_GCM, key.bytes())
            .map_err(|_unspecified| SealError::Cipher)?;
        let mut sealed = plaintext.to_vec();
        let nonce = key
            .seal_in_place_append_tag(Aad::from(context), &mut sealed)
            .map_err(|_unspecified| SealError::Cipher)?;
        let mut framed = nonce.as_ref().to_vec();
        framed.append(&mut sealed);
        Ok((id.clone(), framed))
    }

    /// Returns what [`Self::seal`] sealed under the key `id` names, with
    /// the same `context`.
    ///
    /// # Errors
    /// Returns [`SealError::UnknownKey`] when no listed key has `id`, and
    /// [`SealError::Cipher`] when the bytes do not open.
    pub fn open(&self, id: &KeyId, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
        let key = self.opening(id).ok_or(SealError::UnknownKey)?;
        let (nonce, body) = sealed
            .split_at_checked(NONCE_LEN)
            .ok_or(SealError::Cipher)?;
        let nonce =
            Nonce::try_assume_unique_for_key(nonce).map_err(|_unspecified| SealError::Cipher)?;
        let key = RandomizedNonceKey::new(&AES_256_GCM, key.bytes())
            .map_err(|_unspecified| SealError::Cipher)?;
        let mut body = body.to_vec();
        let opened = key
            .open_in_place(nonce, Aad::from(context), &mut body)
            .map_err(|_unspecified| SealError::Cipher)?;
        Ok(opened.to_vec())
    }
}

/// The key lookup columns are digested under.
///
/// An identifier value is stored as its HMAC-SHA-256 under this key, so a
/// row can be found by the value and never read back as it. Unlike a sealing
/// key it never rotates, since every stored digest depends on it.
#[derive(Clone)]
pub struct LookupKey(hmac::Key);

impl LookupKey {
    /// Returns the lookup key `key` holds.
    #[must_use]
    pub fn new(key: &SealingKey) -> Self {
        Self(hmac::Key::new(hmac::HMAC_SHA256, key.bytes()))
    }

    /// Returns the digest of `value` for the column `purpose` names, so one
    /// value gives unrelated digests in two columns.
    #[must_use]
    pub fn digest(&self, purpose: &str, value: &str) -> Vec<u8> {
        let mut context = hmac::Context::with_key(&self.0);
        context.update(purpose.as_bytes());
        context.update(&[0x1f]);
        context.update(value.as_bytes());
        context.sign().as_ref().to_vec()
    }

    /// Returns the fingerprint a store pins, so a different key refuses the
    /// start rather than missing every stored digest.
    #[must_use]
    pub fn fingerprint(&self) -> Vec<u8> {
        hmac::sign(&self.0, b"ferrobridge lookup key fingerprint")
            .as_ref()
            .to_vec()
    }
}

impl fmt::Debug for LookupKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LookupKey(***)")
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
        let keys = SealKeys::new(
            (id("new"), key.clone()),
            vec![(id("old"), key.clone())],
            super::LookupKey::new(&key),
        );
        assert_eq!("new", keys.current().0.as_str());
        assert!(keys.opening(&id("old")).is_some());
        assert!(keys.opening(&id("gone")).is_none());
    }

    #[test]
    fn a_sealed_value_opens_under_its_key_and_context_alone() {
        let key = SealingKey::from_base64(TEST_KEY).expect("a 32-byte key");
        let id = |text| KeyId::new(text).expect("a plain id");
        let keys = SealKeys::new(
            (id("new"), key.clone()),
            vec![(id("old"), key.clone())],
            super::LookupKey::new(&key),
        );
        let (named, sealed) = keys.seal(b"row-1", b"secret").expect("sealed");
        assert_eq!("new", named.as_str());
        assert!(!sealed.windows(6).any(|window| window == b"secret"));
        assert_eq!(
            b"secret".to_vec(),
            keys.open(&named, b"row-1", &sealed).expect("opened")
        );
        assert_eq!(
            Err(super::SealError::Cipher),
            keys.open(&named, b"row-2", &sealed)
        );
        assert_eq!(
            Err(super::SealError::UnknownKey),
            keys.open(&id("gone"), b"row-1", &sealed)
        );
        assert_eq!(
            Err(super::SealError::Cipher),
            keys.open(&named, b"row-1", b"short")
        );
    }

    #[test]
    fn a_lookup_digest_depends_on_its_purpose_and_never_renders() {
        let key = super::LookupKey::new(&SealingKey::from_base64(TEST_KEY).expect("a key"));
        assert_eq!(key.digest("a", "x"), key.digest("a", "x"));
        assert_ne!(key.digest("a", "x"), key.digest("b", "x"));
        assert_eq!(32, key.fingerprint().len());
        assert_eq!("LookupKey(***)", format!("{key:?}"));
    }
}
