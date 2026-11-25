// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a source's request proves before anything of it is read or stored.
//!
//! Every verifier runs over the raw body bytes before they are parsed, and
//! every comparison of a secret runs in constant time. A key list holds the
//! current key and the ones still accepted, so a key rotates by adding the
//! new key before the old one leaves. A refusal names the reason and never a
//! value of the request.

use std::time::Duration;

use aws_lc_rs::constant_time::verify_slices_are_equal;
use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::hmac;
use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::HeaderMap;
use secrecy::{ExposeSecret as _, SecretBox};
use serde::Deserialize;

/// The Standard Webhooks message id header.
pub const WEBHOOK_ID: &str = "webhook-id";
/// The Standard Webhooks attempt timestamp header.
pub const WEBHOOK_TIMESTAMP: &str = "webhook-timestamp";
/// The Standard Webhooks signature header.
pub const WEBHOOK_SIGNATURE: &str = "webhook-signature";

/// How a source's request is verified, as its document declares it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Verifier {
    /// A keyed digest of the request in one header.
    Hmac(Hmac),
    /// The Standard Webhooks signature headers.
    StandardWebhooks(StandardWebhooks),
    /// A header carrying one of the source's shared values.
    StaticHeader(StaticHeader),
    /// The TLS client certificate the connection presented.
    Mtls(Mtls),
}

/// An HMAC (RFC 2104) of the request in one header.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hmac {
    /// The digest under the HMAC.
    pub algorithm: HmacAlgorithm,
    /// The header the signature travels in.
    pub header: String,
    /// How the signature is written in the header.
    pub encoding: Encoding,
    /// Text before the signature in the header, such as `sha256=`.
    pub prefix: String,
}

impl Default for Hmac {
    fn default() -> Self {
        Self {
            algorithm: HmacAlgorithm::Sha256,
            header: String::new(),
            encoding: Encoding::Hex,
            prefix: String::new(),
        }
    }
}

/// The digests an HMAC verifier knows (FIPS 180-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HmacAlgorithm {
    /// HMAC-SHA-256.
    Sha256,
    /// HMAC-SHA-512.
    Sha512,
}

/// How a signature is written in its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoding {
    /// Hexadecimal in either case (RFC 4648 §8).
    Hex,
    /// Standard padded base64 (RFC 4648 §4).
    Base64,
}

/// The Standard Webhooks verifier.
///
/// The schemes and their signed content follow Standard Webhooks
/// (<https://github.com/standard-webhooks/standard-webhooks/blob/main/spec/standard-webhooks.md>,
/// §Signature scheme): `v1` and `v1a` as tag v1.3.0 defines them, `v2a` as
/// commit `3d65b1824` of `main` defines it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StandardWebhooks {
    /// The schemes the source may sign with.
    pub schemes: Vec<Scheme>,
    /// How far the attempt timestamp may lie from now, either way.
    #[serde(with = "seconds")]
    pub tolerance: Duration,
    /// The endpoint id the producer gave this endpoint, which `v2a` signs.
    pub endpoint_id: Option<String>,
}

impl Default for StandardWebhooks {
    fn default() -> Self {
        Self {
            schemes: vec![Scheme::V1],
            tolerance: Duration::from_secs(300),
            endpoint_id: None,
        }
    }
}

/// A Standard Webhooks signature scheme, by its signature identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// HMAC-SHA-256 of `msg_id.timestamp.payload` under a `whsec_` secret.
    V1,
    /// Ed25519 over `msg_id.timestamp.payload` under a `whpk_` public key.
    V1a,
    /// Ed25519 over the SHA-256 of `endpoint_id.msg_id.timestamp.payload`
    /// under a `whpk_` public key.
    V2a,
}

impl Scheme {
    /// Returns the signature identifier the scheme is serialized with.
    #[must_use]
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V1a => "v1a",
            Self::V2a => "v2a",
        }
    }
}

/// A header that carries one of the source's shared values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StaticHeader {
    /// The header's name.
    pub header: String,
}

/// The client certificate subjects a source's connection may present.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mtls {
    /// The subjects, as the TLS listener writes a certificate's subject.
    pub subjects: Vec<String>,
}

/// A key a verifier checks with, read from the secret it names.
pub struct Key(SecretBox<[u8]>);

impl Key {
    /// Returns `bytes` as a key.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(SecretBox::new(bytes.into_boxed_slice()))
    }

    /// Returns the key bytes a Standard Webhooks secret serializes after
    /// `prefix` (`whsec_` or `whpk_`).
    fn standard(&self, prefix: &str) -> Option<Vec<u8>> {
        let text = std::str::from_utf8(self.0.expose_secret()).ok()?;
        // NOTE: a key of another form is legitimately not this scheme's key, so it is skipped.
        STANDARD.decode(text.trim().strip_prefix(prefix)?).ok()
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(..)")
    }
}

/// What a request carries that a verifier reads.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The request headers.
    pub headers: &'a HeaderMap,
    /// The raw body, as received.
    pub body: &'a [u8],
    /// The subject of the client certificate the connection presented.
    pub peer_subject: Option<&'a str>,
    /// The instant the request is verified at, in seconds since the epoch.
    pub now: i64,
}

/// Why a request failed verification, naming no value of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refused {
    /// A header the verifier reads is absent or repeated.
    #[error("the request lacks a header its verifier reads, or repeats it")]
    MissingHeader,
    /// A header does not read in the form the verifier declares.
    #[error("a verification header does not read in its declared form")]
    Malformed,
    /// The attempt timestamp lies outside the tolerance.
    #[error("the request's timestamp lies outside the tolerance")]
    Stale,
    /// No key verifies the request.
    #[error("no key verifies the request")]
    Mismatch,
    /// The verifier has no key of the form it needs.
    #[error("the verifier has no usable key")]
    NoKey,
    /// The verifier is declared without what it needs.
    #[error("the verifier is declared incompletely")]
    Declaration,
}

impl Verifier {
    /// Checks `request` against the verifier with `keys`.
    ///
    /// # Errors
    /// Returns [`Refused`] when the request does not prove it is the
    /// source's.
    pub fn verify(&self, request: &Request<'_>, keys: &[Key]) -> Result<(), Refused> {
        match self {
            Self::Hmac(hmac) => hmac.verify(request, keys),
            Self::StandardWebhooks(standard) => standard.verify(request, keys),
            Self::StaticHeader(header) => {
                let value = header_value(request.headers, &header.header)?;
                any_key(keys, |key| {
                    verify_slices_are_equal(key.0.expose_secret(), value).is_ok()
                })
            }
            Self::Mtls(mtls) => {
                let subject = request.peer_subject.ok_or(Refused::Mismatch)?;
                if mtls.subjects.iter().any(|allowed| allowed == subject) {
                    Ok(())
                } else {
                    Err(Refused::Mismatch)
                }
            }
        }
    }
}

impl Hmac {
    fn verify(&self, request: &Request<'_>, keys: &[Key]) -> Result<(), Refused> {
        let value = std::str::from_utf8(header_value(request.headers, &self.header)?)
            .map_err(|_utf8| Refused::Malformed)?;
        let signature = value
            .trim()
            .strip_prefix(self.prefix.as_str())
            .ok_or(Refused::Malformed)?;
        let tag = match self.encoding {
            Encoding::Hex => hex(signature),
            Encoding::Base64 => STANDARD.decode(signature).ok(),
        }
        .ok_or(Refused::Malformed)?;
        let algorithm = match self.algorithm {
            HmacAlgorithm::Sha256 => hmac::HMAC_SHA256,
            HmacAlgorithm::Sha512 => hmac::HMAC_SHA512,
        };
        any_key(keys, |key| {
            let key = hmac::Key::new(algorithm, key.0.expose_secret());
            hmac::verify(&key, request.body, &tag).is_ok()
        })
    }
}

impl StandardWebhooks {
    fn verify(&self, request: &Request<'_>, keys: &[Key]) -> Result<(), Refused> {
        let id = text(header_value(request.headers, WEBHOOK_ID)?)?;
        let timestamp = text(header_value(request.headers, WEBHOOK_TIMESTAMP)?)?;
        let signatures = text(header_value(request.headers, WEBHOOK_SIGNATURE)?)?;
        let sent: i64 = timestamp.parse().map_err(|_number| Refused::Malformed)?;
        if request.now.abs_diff(sent) > self.tolerance.as_secs() {
            return Err(Refused::Stale);
        }
        let mut content = Vec::with_capacity(id.len() + timestamp.len() + request.body.len() + 2);
        content.extend_from_slice(id.as_bytes());
        content.push(b'.');
        content.extend_from_slice(timestamp.as_bytes());
        content.push(b'.');
        content.extend_from_slice(request.body);

        for signature in signatures.split(' ').filter(|part| !part.is_empty()) {
            let Some((identifier, encoded)) = signature.split_once(',') else {
                continue;
            };
            let Some(scheme) = self
                .schemes
                .iter()
                .copied()
                .find(|scheme| scheme.identifier() == identifier)
            else {
                continue;
            };
            // NOTE: Standard Webhooks §Verifying signatures; a signature that does not decode is skipped like any other that fails.
            let Ok(signature) = STANDARD.decode(encoded) else {
                continue;
            };
            if self.check(scheme, &content, &signature, keys)? {
                return Ok(());
            }
        }
        Err(Refused::Mismatch)
    }

    /// Returns whether one key verifies `signature` over `content` by
    /// `scheme`.
    fn check(
        &self,
        scheme: Scheme,
        content: &[u8],
        signature: &[u8],
        keys: &[Key],
    ) -> Result<bool, Refused> {
        let mut usable = false;
        for key in keys {
            let verified = match scheme {
                Scheme::V1 => key.standard("whsec_").map(|secret| {
                    let key = hmac::Key::new(hmac::HMAC_SHA256, &secret);
                    hmac::verify(&key, content, signature).is_ok()
                }),
                Scheme::V1a => key.standard("whpk_").map(|public| {
                    UnparsedPublicKey::new(&ED25519, public)
                        .verify(content, signature)
                        .is_ok()
                }),
                Scheme::V2a => {
                    let endpoint = self
                        .endpoint_id
                        .as_deref()
                        .filter(|id| !id.is_empty() && !id.contains('.'))
                        .ok_or(Refused::Declaration)?;
                    key.standard("whpk_").map(|public| {
                        let mut signed = Vec::with_capacity(endpoint.len() + 1 + content.len());
                        signed.extend_from_slice(endpoint.as_bytes());
                        signed.push(b'.');
                        signed.extend_from_slice(content);
                        let hash = digest(&SHA256, &signed);
                        UnparsedPublicKey::new(&ED25519, public)
                            .verify(hash.as_ref(), signature)
                            .is_ok()
                    })
                }
            };
            match verified {
                Some(true) => return Ok(true),
                Some(false) => usable = true,
                None => {}
            }
        }
        if usable {
            Ok(false)
        } else {
            Err(Refused::NoKey)
        }
    }
}

/// Answers `Ok` when one of `keys` passes `check`, trying every key.
fn any_key(keys: &[Key], check: impl Fn(&Key) -> bool) -> Result<(), Refused> {
    if keys.is_empty() {
        return Err(Refused::NoKey);
    }
    // NOTE: every key is tried, so the time taken does not tell which key matched.
    let matched = keys.iter().fold(false, |matched, key| check(key) | matched);
    if matched {
        Ok(())
    } else {
        Err(Refused::Mismatch)
    }
}

/// Returns the one value of header `name`.
fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a [u8], Refused> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => Ok(value.as_bytes()),
        _ => Err(Refused::MissingHeader),
    }
}

/// Returns a header value as text.
fn text(value: &[u8]) -> Result<&str, Refused> {
    std::str::from_utf8(value).map_err(|_utf8| Refused::Malformed)
}

/// Returns the bytes hexadecimal `text` writes, `None` when it is not
/// hexadecimal.
fn hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|[high, low]| {
            let high = char::from(*high).to_digit(16)?;
            let low = char::from(*low).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

/// A duration written as whole seconds.
mod seconds {
    use std::time::Duration;

    use serde::{Deserialize as _, Deserializer};

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn hex_reads_either_case_and_refuses_the_rest() {
        assert_eq!(Some(vec![0xab, 0x01]), hex("aB01"));
        assert_eq!(None, hex("abc"));
        assert_eq!(None, hex("zz"));
    }
}
