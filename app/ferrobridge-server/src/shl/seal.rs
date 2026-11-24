// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The secrets of a link and the encryption of its files.
//!
//! A file is "encrypted using JSON Web Encryption (JOSE JWE) compact
//! serialization with `"alg": "dir"`, `"enc": "A256GCM"`", with "a `cty`
//! header indicating the content type of the payload" and "a unique nonce
//! (also known as initialization vector, or IV) for each encryption
//! operation" (SMART Health Cards and Links 1.0.0, `links-specification`,
//! Encrypting and Decrypting Files). The JWE form is RFC 7516 §7.1 with an
//! empty encrypted key (RFC 7518 §4.5) and the protected header as the
//! additional authenticated data (RFC 7516 §5.1 step 14).

use aws_lc_rs::aead::AES_256_GCM;
use aws_lc_rs::aead::Aad;
use aws_lc_rs::aead::RandomizedNonceKey;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::ExposeSecret as _;
use secrecy::SecretBox;
use secrecy::SecretString;

/// The bytes of a link's key: "32 random bytes" (`links-specification`,
/// SMART Health Link Payload `key`).
const KEY_BYTES: usize = 32;

/// The bytes of a link's manifest id, its path segment of 43 characters:
/// the manifest URL "SHALL include at least 256 bits of entropy".
const ID_BYTES: usize = 32;

/// Why a secret could not be drawn or a file could not be sealed.
#[derive(Debug, thiserror::Error)]
#[error("the system random source or the AES-256-GCM context failed")]
pub struct SealError;

/// Returns 32 random bytes, base64url-encoded without padding: 43
/// characters carrying 256 bits.
///
/// # Errors
///
/// Returns [`SealError`] when the system random source fails.
pub fn random_token() -> Result<String, SealError> {
    let mut bytes = [0_u8; ID_BYTES];
    aws_lc_rs::rand::fill(&mut bytes).map_err(|_unspecified| SealError)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// The key of one link, held only while its files are sealed and its
/// payload written.
pub struct LinkKey(SecretBox<[u8; KEY_BYTES]>);

impl std::fmt::Debug for LinkKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkKey(..)")
    }
}

impl LinkKey {
    /// Draws a fresh key.
    ///
    /// # Errors
    ///
    /// Returns [`SealError`] when the system random source fails.
    pub fn generate() -> Result<Self, SealError> {
        let mut bytes = Box::new([0_u8; KEY_BYTES]);
        aws_lc_rs::rand::fill(bytes.as_mut()).map_err(|_unspecified| SealError)?;
        Ok(Self(SecretBox::new(bytes)))
    }

    /// Returns the key as the payload's `key` carries it: base64url, 43
    /// characters.
    #[must_use]
    pub fn encoded(&self) -> SecretString {
        SecretString::from(URL_SAFE_NO_PAD.encode(self.0.expose_secret()))
    }

    /// Returns `plaintext` sealed as a compact JWE whose `cty` is
    /// `content_type`, under a fresh random nonce.
    ///
    /// # Errors
    ///
    /// Returns [`SealError`] when the AES-256-GCM context cannot be built or
    /// the seal fails.
    pub fn seal(&self, content_type: &str, plaintext: &[u8]) -> Result<String, SealError> {
        let header = serde_json::json!({"alg": "dir", "enc": "A256GCM", "cty": content_type});
        let protected = URL_SAFE_NO_PAD.encode(header.to_string());
        let key = RandomizedNonceKey::new(&AES_256_GCM, self.0.expose_secret())
            .map_err(|_unspecified| SealError)?;
        let mut sealed = plaintext.to_vec();
        let (nonce, tag) = key
            .seal_in_place_separate_tag(Aad::from(protected.as_bytes()), &mut sealed)
            .map_err(|_unspecified| SealError)?;
        Ok(format!(
            "{protected}..{}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce.as_ref()),
            URL_SAFE_NO_PAD.encode(&sealed),
            URL_SAFE_NO_PAD.encode(tag.as_ref()),
        ))
    }
}
