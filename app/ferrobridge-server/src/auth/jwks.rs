// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The authorization server's signing keys (RFC 7517), read at boot and read
//! again when a token names a key the set does not hold.
//!
//! An unknown `kid` is how a key rotation reaches the resource server, so it
//! triggers one read of the set; a second unknown `kid` within the configured
//! interval reads nothing, so a stream of forged `kid`s cannot turn the
//! bridge into a load on the authorization server (no specification governs
//! the interval: our own design).

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use http::StatusCode;
use jsonwebtoken::Algorithm;
use jsonwebtoken::AlgorithmFamily;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::jwk::AlgorithmParameters;
use jsonwebtoken::jwk::Jwk;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;

/// One signing key of the set.
#[derive(Clone)]
struct Key {
    /// The key's `kid`, when it names one.
    kid: Option<String>,
    /// The key's `alg`, when it names one.
    alg: Option<Algorithm>,
    /// The algorithm family the key belongs to.
    family: AlgorithmFamily,
    /// The verification key.
    verifier: DecodingKey,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Key")
            .field("kid", &self.kid)
            .field("alg", &self.alg)
            .finish_non_exhaustive()
    }
}

/// The JWK Set document (RFC 7517 §5), each member read on its own.
#[derive(Deserialize)]
struct Document {
    /// The keys, as JSON objects.
    keys: Vec<serde_json::Value>,
}

/// The keys as last read.
#[derive(Debug)]
struct Cached {
    /// The usable signing keys.
    keys: Arc<[Key]>,
    /// When they were read.
    read_at: Instant,
}

/// The key set of one authorization server.
#[derive(Debug)]
pub(super) struct KeySet {
    /// The client the set is read with.
    http: reqwest::Client,
    /// The `jwks_uri`.
    url: Url,
    /// The shortest time between two reads an unknown `kid` triggers.
    interval: Duration,
    /// The keys as last read; the write lock is held across a read of the
    /// set, so concurrent misses share one fetch.
    cached: tokio::sync::RwLock<Cached>,
}

impl KeySet {
    /// Reads the set at `url`.
    pub(super) async fn load(
        http: reqwest::Client,
        url: Url,
        interval: Duration,
    ) -> Result<Self, AuthError> {
        let keys = fetch(&http, &url).await?;
        Ok(Self {
            http,
            url,
            interval,
            cached: tokio::sync::RwLock::new(Cached {
                keys: Arc::from(keys),
                read_at: Instant::now(),
            }),
        })
    }

    /// Returns how many usable keys the set holds.
    pub(super) async fn len(&self) -> usize {
        self.cached.read().await.keys.len()
    }

    /// Returns the key a token signed with `alg` under `kid` verifies with.
    ///
    /// A miss reads the set again unless it was read within the interval;
    /// `Ok(None)` is a token no key of the set verifies.
    pub(super) async fn find(
        &self,
        kid: Option<&str>,
        alg: Algorithm,
    ) -> Result<Option<DecodingKey>, AuthError> {
        let keys = Arc::clone(&self.cached.read().await.keys);
        if let Some(key) = select(&keys, kid, alg) {
            return Ok(Some(key));
        }
        let mut cached = self.cached.write().await;
        if let Some(key) = select(&cached.keys, kid, alg) {
            return Ok(Some(key));
        }
        if cached.read_at.elapsed() < self.interval {
            return Ok(None);
        }
        tracing::info!(url = %self.url, "a token names a key the set does not hold; reading the set again");
        let fresh = fetch(&self.http, &self.url).await;
        cached.read_at = Instant::now();
        cached.keys = Arc::from(fresh?);
        Ok(select(&cached.keys, kid, alg))
    }
}

/// Returns the key of `keys` that verifies `alg` under `kid`.
///
/// A token without a `kid` (RFC 7515 §4.1.4 makes it optional) verifies with
/// the one key of its family when the set holds exactly one.
fn select(keys: &[Key], kid: Option<&str>, alg: Algorithm) -> Option<DecodingKey> {
    let mut candidates = keys
        .iter()
        .filter(|key| key.family == alg.family() && key.alg.is_none_or(|named| named == alg));
    if let Some(kid) = kid {
        return candidates
            .find(|key| key.kid.as_deref() == Some(kid))
            .map(|key| key.verifier.clone());
    }
    let only = candidates.next()?;
    candidates.next().is_none().then(|| only.verifier.clone())
}

/// Reads the set at `url` and keeps its usable signing keys.
async fn fetch(http: &reqwest::Client, url: &Url) -> Result<Vec<Key>, AuthError> {
    let (url, status, body) = super::discovery::get(http, url.clone()).await?;
    if status != StatusCode::OK {
        return Err(AuthError::KeySetStatus { url, status, body });
    }
    let document: Document =
        serde_json::from_str(&body).map_err(|source| AuthError::KeySet { url, source })?;
    Ok(document.keys.iter().filter_map(usable).collect())
}

/// Returns the signing key `member` describes.
///
/// A member that is an encryption key (`use` of `enc`, RFC 7517 §4.2), a
/// symmetric key, a key whose `alg` is no signature algorithm, or a key this
/// reader cannot load verifies no token.
// NOTE: RFC 7517 §5 has a reader ignore a set member it does not understand,
// so such a member is legitimately absent here rather than a defective set.
fn usable(member: &serde_json::Value) -> Option<Key> {
    if member.get("use").and_then(serde_json::Value::as_str) == Some("enc") {
        return None;
    }
    let alg = match member.get("alg").and_then(serde_json::Value::as_str) {
        None => None,
        Some(name) => Some(Algorithm::from_str(name).ok()?),
    };
    let jwk: Jwk = serde_json::from_value(member.clone()).ok()?;
    let family = match &jwk.algorithm {
        AlgorithmParameters::RSA(_) => AlgorithmFamily::Rsa,
        AlgorithmParameters::EllipticCurve(_) => AlgorithmFamily::Ec,
        AlgorithmParameters::OctetKeyPair(_) => AlgorithmFamily::Ed,
        AlgorithmParameters::OctetKey(_) | _ => return None,
    };
    let verifier = DecodingKey::from_jwk(&jwk).ok()?;
    Some(Key {
        kid: jwk.common.key_id,
        alg,
        family,
        verifier,
    })
}

#[cfg(test)]
mod tests {
    use super::usable;

    #[test]
    fn an_encryption_key_and_a_symmetric_key_verify_nothing() {
        let encryption = serde_json::json!({
            "kty": "RSA", "use": "enc", "alg": "RSA-OAEP", "kid": "e",
            "n": "sXchDaQebHnPiGvyDOAT4saGEUetSyo9MKLOoWFsueri23bOdgWp4Dy1WlUzewbgBHod5pcM9H95GQRV3JDXboIRROSBigeC5yjU1hGzHHyXss8UDprecbAYxknTcQkhslANGRUZmdTOQ5qTRsLAt6BTYuyvVRdhS8exSZEy_c4gs_7svlJJQ4H9_NxsiIoLwAEk7-Q3UXERGYw_75IDrGA84-lA_-Ct4eTlXHBIY2EaV7t7LjJaynVJCpkv4LKjTTAumiGUIuQhrNhZLuF_RJLqHpM2kgWFLU7-VTdL1VbC2tejvcI2BlMkEpk1BzBZI0KQB0GaDWFLN-aEAw3vRw",
            "e": "AQAB"
        });
        assert!(usable(&encryption).is_none());
        let symmetric = serde_json::json!({ "kty": "oct", "k": "c2VjcmV0", "kid": "s" });
        assert!(usable(&symmetric).is_none());
    }
}
