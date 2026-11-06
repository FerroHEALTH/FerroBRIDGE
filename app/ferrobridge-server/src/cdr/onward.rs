// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade's caller carried onto the CDR: the access token a person
//! presented, exchanged for a token of the CDR's issuer that names the same
//! person, as `[cdr.credentials.onward]` declares it.
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

/// The onward lane of the CDR: the client of the issuer's grant, the target
/// it asks for, and the kind of that issuer.
///
/// The client is cheap to clone: clones share one cache.
#[derive(Debug, Clone)]
pub struct CdrOnward {
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

/// Why a caller does not reach the CDR under an onward token.
///
/// No variant carries a token, a secret or a claim value.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OnwardRefusal {
    /// The caller is of a kind the onward issuer does not carry.
    #[error("a {caller} caller is never exchanged onto a {issuer} issuer")]
    NotCarried {
        /// The caller's kind, as the audit line names it.
        caller: &'static str,
        /// The onward issuer's kind.
        issuer: IssuerKind,
    },
    /// The caller authenticated without a bearer token the grant can send.
    #[error("the caller presented no bearer token to exchange")]
    NoBearer,
    /// The exchange was refused or its token failed the onward check.
    #[error("the onward exchange failed")]
    Exchange(#[source] OnwardError),
}

impl CdrOnward {
    /// Returns the lane `lane` resolves to, whose issuer signs tokens of
    /// `kind`.
    #[must_use]
    pub fn new(lane: OnwardLane, kind: IssuerKind) -> Self {
        Self {
            client: lane.onward,
            target: lane.target,
            kind,
        }
    }

    /// Returns the kind of the onward issuer.
    #[must_use]
    pub const fn kind(&self) -> IssuerKind {
        self.kind
    }

    /// Returns the target the grant asks for.
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }

    /// Returns how a caller of `kind` reaches the CDR under this lane.
    #[must_use]
    pub const fn route(&self, kind: &CallerKind) -> Route {
        route(kind, self.kind)
    }

    /// Returns the token the caller behind `access` reaches the CDR with,
    /// exchanged for the bearer token `headers` carry, or `None` when the
    /// caller is a machine and reaches it under the bridge's own credential.
    ///
    /// # Errors
    ///
    /// [`OnwardRefusal::NotCarried`] for a caller this lane does not carry,
    /// an open surface included; [`OnwardRefusal::NoBearer`] for a caller
    /// without a bearer token, as a Basic user is; and
    /// [`OnwardRefusal::Exchange`] with the upstream status when the token
    /// endpoint refused, or its token failed the onward check.
    pub async fn token(
        &self,
        access: &Access,
        headers: &HeaderMap,
    ) -> Result<Option<SecretString>, OnwardRefusal> {
        let Access::Caller(caller) = access else {
            return Err(OnwardRefusal::NotCarried {
                caller: "anonymous",
                issuer: self.kind,
            });
        };
        match self.route(caller.kind()) {
            Route::Own => Ok(None),
            Route::Refused => Err(OnwardRefusal::NotCarried {
                caller: caller.kind().as_str(),
                issuer: self.kind,
            }),
            Route::Exchanged => self.exchange(caller, headers).await.map(Some),
        }
    }

    /// Returns the token the onward issuer exchanges `caller`'s for.
    async fn exchange(
        &self,
        caller: &Caller,
        headers: &HeaderMap,
    ) -> Result<SecretString, OnwardRefusal> {
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
        issued
            .header()
            .to_str()
            .ok()
            .and_then(|value| credential(value, BEARER))
            .map(|token| SecretString::from(token.to_owned()))
            .ok_or(OnwardRefusal::Exchange(OnwardError::Token(
                ferrohealth_smart::client::TokenError::Unusable,
            )))
    }
}

/// Returns how a caller of `caller` reaches the CDR when the onward issuer
/// signs tokens of `issuer`.
///
/// A machine keeps the bridge's credential: the on-behalf-of flow "only
/// works for user principals" (Microsoft identity platform, "On-behalf-of
/// flow"). A person of this organisation, acting alone, through a client or
/// for another, is exchanged at a `persons` issuer, and a professional of
/// another party at a `federation` issuer. A patient, another party's
/// organisation and a guest are carried by neither, so they are refused.
#[must_use]
pub const fn route(caller: &CallerKind, issuer: IssuerKind) -> Route {
    match (caller, issuer) {
        (CallerKind::System, _) => Route::Own,
        (
            CallerKind::Person | CallerKind::Delegated { .. } | CallerKind::Representative { .. },
            IssuerKind::Persons,
        )
        | (CallerKind::ExternalPerson(_), IssuerKind::Federation) => Route::Exchanged,
        _ => Route::Refused,
    }
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
    fn a_patient_and_a_guest_are_never_exchanged() {
        let guest = CallerKind::Guest(Box::new(CallerKind::Person));
        for kind in [&CallerKind::Patient, &guest] {
            assert_eq!(Route::Refused, route(kind, IssuerKind::Persons));
            assert_eq!(Route::Refused, route(kind, IssuerKind::Federation));
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
