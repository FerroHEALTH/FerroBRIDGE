// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade's caller carried onto the CDR, its token exchanged at the
//! onward issuer of its kind.
//!
//! The access token a person or a patient presented is exchanged for a
//! token of the CDR's issuer that names the same person, as
//! `[cdr.credentials.onward]` and each `[[cdr.onward]]` table declare it.
//!
//! `ferrohealth-smart` runs the grant its issuer profile picks: token
//! exchange (RFC 8693 §2.1), the caller's token as `subject_token` and the
//! CDR named by `resource` or `audience`, or the JWT bearer grant (RFC 7523
//! §2.1) with the Microsoft identity platform's on-behalf-of parameters,
//! the caller's token as `assertion` and the CDR named in `scope`. It checks
//! every issued token, the person, the audience, the actor and the lifetime,
//! and caches it under the caller's token and the target until a margin
//! before the earlier of the two expiries. This module decides which caller
//! is exchanged at all: a machine keeps the bridge's own credential, and
//! every other caller the issuer cannot carry is refused, never sent under
//! the bridge's credential. No specification governs that choice: our own
//! design.

use ferrohealth_smart::client::onward::OnwardError;
use ferrohealth_smart::client::onward::SubjectToken;
use ferrohealth_smart::client::onward::Target;
use ferrohealth_smart::client::onward::exchange::Onward;
use ferrohealth_smart::config::onward::OnwardLane;
use ferrohealth_smart::kind::CallerKind;
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Caller;
use http::HeaderMap;
use http::header::AUTHORIZATION;
use secrecy::SecretString;

/// The scheme of an RFC 6750 §2.1 bearer credential.
const BEARER: &str = "bearer";

/// The onward lane of the CDR: for each issuer, the client of its grant,
/// the target it asks for, and its kind.
///
/// It holds at most one issuer per kind: a `persons`, a `federation` and a
/// `patients` issuer, each carrying the callers of its kind. The client is
/// cheap to clone: clones share one cache per lane.
#[derive(Debug, Clone)]
pub struct CdrOnward {
    /// The issuers, at most one per kind.
    lanes: Vec<Lane>,
}

/// One onward issuer and the target it issues tokens for.
#[derive(Debug, Clone)]
struct Lane {
    /// The client of the issuer's onward grant, with its cache.
    client: Onward,
    /// The CDR, as the grant names it.
    target: Target,
    /// Whose tokens the issuer signs.
    kind: IssuerKind,
}

/// How one caller reaches the CDR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Under the bridge's own credential: a machine, whose act is the
    /// bridge's.
    Own,
    /// Under a token the onward issuer exchanges the caller's for.
    Exchanged,
    /// Not at all: the onward issuer carries no caller of this kind.
    Refused,
}

/// The person an onward token names: its `iss` and `sub` (RFC 7519 §4.1.1,
/// §4.1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnwardSubject {
    /// The issuer that signed the onward token.
    pub issuer: String,
    /// The subject it names.
    pub subject: String,
}

/// A token the onward issuer exchanged a caller's for, with the person it
/// names.
///
/// The `Debug` rendering never carries the token.
#[derive(Debug, Clone)]
pub struct Exchanged {
    /// The token, sent as the bearer credential of every CDR call.
    pub token: SecretString,
    /// The person it names.
    pub subject: OnwardSubject,
}

/// Why a caller does not reach the CDR under an onward token.
///
/// No variant carries a token, a secret or a claim value.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OnwardRefusal {
    /// The caller is of a kind no onward issuer of the lane carries.
    #[error("no onward issuer of the CDR lane carries a {caller} caller")]
    NotCarried {
        /// The caller's kind, as the audit line names it.
        caller: &'static str,
    },
    /// The caller authenticated without a bearer token the grant can send.
    #[error("the caller presented no bearer token to exchange")]
    NoBearer,
    /// The exchange was refused or its token failed the onward check.
    #[error("the onward exchange failed")]
    Exchange(#[source] OnwardError),
}

impl CdrOnward {
    /// Returns the onward lane of the one issuer `lane` resolves to, whose
    /// issuer signs tokens of `kind`.
    #[must_use]
    pub fn new(lane: OnwardLane, kind: IssuerKind) -> Self {
        Self { lanes: Vec::new() }.with_issuer(lane, kind)
    }

    /// Returns this lane carrying the callers of `kind` through the issuer
    /// `lane` resolves to, beside its other issuers.
    #[must_use]
    pub fn with_issuer(mut self, lane: OnwardLane, kind: IssuerKind) -> Self {
        self.lanes.push(Lane {
            client: lane.onward,
            target: lane.target,
            kind,
        });
        self
    }

    /// Returns the kind of every onward issuer, in declaration order.
    #[must_use]
    pub fn kinds(&self) -> Vec<IssuerKind> {
        self.lanes.iter().map(|lane| lane.kind).collect()
    }

    /// Returns the target the issuer of `kind` asks for, when the lane has one.
    #[must_use]
    pub fn target(&self, kind: IssuerKind) -> Option<&Target> {
        self.lanes
            .iter()
            .find(|lane| lane.kind == kind)
            .map(|lane| &lane.target)
    }

    /// Returns how a caller of `kind` reaches the CDR under this lane, with
    /// the issuer that carries it when it is exchanged.
    fn route_of(&self, kind: &CallerKind) -> (Route, Option<&Lane>) {
        if matches!(kind, CallerKind::System) {
            return (Route::Own, None);
        }
        self.lanes
            .iter()
            .find(|lane| route(kind, lane.kind) == Route::Exchanged)
            .map_or((Route::Refused, None), |lane| {
                (Route::Exchanged, Some(lane))
            })
    }

    /// Returns how a caller of `kind` reaches the CDR under this lane.
    #[must_use]
    pub fn route(&self, kind: &CallerKind) -> Route {
        self.route_of(kind).0
    }

    /// Returns the token the caller behind `access` reaches the CDR with,
    /// exchanged for the bearer token `headers` carry at the issuer of its
    /// kind, or `None` when the caller is a machine and reaches it under the
    /// bridge's own credential.
    ///
    /// # Errors
    ///
    /// [`OnwardRefusal::NotCarried`] for a caller no issuer of this lane
    /// carries, an open surface included; [`OnwardRefusal::NoBearer`] for a
    /// caller without a bearer token, as a Basic user is; and
    /// [`OnwardRefusal::Exchange`] with the upstream status when the token
    /// endpoint refused, or its token failed the onward check.
    pub async fn token(
        &self,
        access: &Access,
        headers: &HeaderMap,
    ) -> Result<Option<Exchanged>, OnwardRefusal> {
        let Access::Caller(caller) = access else {
            return Err(OnwardRefusal::NotCarried {
                caller: "anonymous",
            });
        };
        match self.route_of(caller.kind()) {
            (Route::Own, _) => Ok(None),
            (Route::Exchanged, Some(lane)) => lane.exchange(caller, headers).await.map(Some),
            (Route::Refused | Route::Exchanged, _) => Err(OnwardRefusal::NotCarried {
                caller: caller.kind().as_str(),
            }),
        }
    }
}

impl Lane {
    /// Returns the token the onward issuer exchanges `caller`'s for.
    async fn exchange(
        &self,
        caller: &Caller,
        headers: &HeaderMap,
    ) -> Result<Exchanged, OnwardRefusal> {
        let presented = bearer(headers).ok_or(OnwardRefusal::NoBearer)?;
        let subject =
            SubjectToken::from_caller(caller, presented).map_err(|error| match error {
                OnwardError::NoSubjectToken => OnwardRefusal::NoBearer,
                other => OnwardRefusal::Exchange(other),
            })?;
        let issued = self
            .client
            .token(&subject, None, &self.target)
            .await
            .map_err(OnwardRefusal::Exchange)?;
        let unusable = || {
            OnwardRefusal::Exchange(OnwardError::Token(
                ferrohealth_smart::client::TokenError::Unusable,
            ))
        };
        let token = issued
            .header()
            .to_str()
            .ok()
            .and_then(|value| credential(value, BEARER))
            .ok_or_else(unusable)?;
        let (named_issuer, named_subject) = issued_party(token).ok_or_else(unusable)?;
        Ok(Exchanged {
            token: SecretString::from(token.to_owned()),
            subject: OnwardSubject {
                issuer: named_issuer,
                subject: named_subject,
            },
        })
    }
}

/// Returns how a caller of `caller` reaches the CDR when the onward issuer
/// signs tokens of `issuer`.
///
/// A machine keeps the bridge's credential: the on-behalf-of flow "only
/// works for user principals" (Microsoft identity platform, "On-behalf-of
/// flow"). A person of this organisation, acting alone, through a client or
/// for another, is exchanged at a `persons` issuer, a professional of another
/// party at a `federation` issuer, and a patient at the deployment's
/// `patients` issuer. Another party's organisation and a guest are carried by
/// none, so they are refused.
#[must_use]
pub const fn route(caller: &CallerKind, issuer: IssuerKind) -> Route {
    match (caller, issuer) {
        (CallerKind::System, _) => Route::Own,
        (
            CallerKind::Person | CallerKind::Delegated { .. } | CallerKind::Representative { .. },
            IssuerKind::Persons,
        )
        | (CallerKind::ExternalPerson(_), IssuerKind::Federation)
        | (CallerKind::Patient, IssuerKind::Patients) => Route::Exchanged,
        _ => Route::Refused,
    }
}

/// Returns the `iss` and `sub` of `token`, a JWS whose payload is a JSON
/// object (RFC 7515 §7.1), or `None` when it is not one or names neither.
///
/// The token is decoded, never verified: the onward client already checked
/// it as it arrived from the token endpoint.
fn issued_party(token: &str) -> Option<(String, String)> {
    use base64::Engine as _;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&bytes).ok()?;
    let claim = |name: &str| {
        claims
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    Some((claim("iss")?, claim("sub")?))
}

/// Returns the bearer token the `Authorization` field of `headers` carries.
fn bearer(headers: &HeaderMap) -> Option<SecretString> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| credential(value, BEARER))
        .map(|token| SecretString::from(token.to_owned()))
}

/// Returns the credential of `value`, an `Authorization` field value, when
/// its scheme is `scheme`.
///
/// RFC 9110 §11.1 makes the scheme case-insensitive.
fn credential<'a>(value: &'a str, scheme: &str) -> Option<&'a str> {
    let (named, token) = value.split_once(' ')?;
    let token = token.trim();
    (named.eq_ignore_ascii_case(scheme) && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::{Route, credential, route};
    use ferrohealth_smart::kind::CallerKind;
    use ferrohealth_smart::kind::IssuerKind;

    #[test]
    fn a_machine_keeps_the_bridge_credential_under_either_issuer() {
        assert_eq!(Route::Own, route(&CallerKind::System, IssuerKind::Persons));
        assert_eq!(
            Route::Own,
            route(&CallerKind::System, IssuerKind::Federation)
        );
        assert_eq!(Route::Own, route(&CallerKind::System, IssuerKind::Patients));
    }

    #[test]
    fn a_person_is_exchanged_at_a_persons_issuer_alone() {
        let delegated = CallerKind::Delegated { actor: None };
        let representative = CallerKind::Representative {
            represented: String::from("synthetic-subject"),
            ground: None,
        };
        for kind in [&CallerKind::Person, &delegated, &representative] {
            assert_eq!(Route::Exchanged, route(kind, IssuerKind::Persons));
            assert_eq!(Route::Refused, route(kind, IssuerKind::Federation));
        }
    }

    #[test]
    fn a_patient_is_exchanged_at_a_patients_issuer_alone() {
        assert_eq!(
            Route::Exchanged,
            route(&CallerKind::Patient, IssuerKind::Patients)
        );
        assert_eq!(
            Route::Refused,
            route(&CallerKind::Patient, IssuerKind::Persons)
        );
        assert_eq!(
            Route::Refused,
            route(&CallerKind::Patient, IssuerKind::Federation)
        );
        assert_eq!(
            Route::Refused,
            route(&CallerKind::Person, IssuerKind::Patients)
        );
    }

    #[test]
    fn a_guest_is_never_exchanged() {
        let guest = CallerKind::Guest(Box::new(CallerKind::Person));
        for issuer in [
            IssuerKind::Persons,
            IssuerKind::Federation,
            IssuerKind::Patients,
        ] {
            assert_eq!(Route::Refused, route(&guest, issuer));
        }
    }

    #[test]
    fn the_scheme_reads_case_insensitively() {
        assert_eq!(Some("abc"), credential("Bearer abc", "bearer"));
        assert_eq!(Some("abc"), credential("bEaReR abc", "bearer"));
        assert_eq!(None, credential("Basic abc", "bearer"));
        assert_eq!(None, credential("Bearer ", "bearer"));
        assert_eq!(None, credential("Bearer", "bearer"));
    }
}
