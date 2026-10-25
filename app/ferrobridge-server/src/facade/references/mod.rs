// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources a `reference` mapping reads when the request does not carry
//! them.
//!
//! "When a reference is provided in a resource and not contained in the
//! Bundle, the engine should attempt to GET it from the sending site. If not
//! possible, the engine proceeds with the mapping", and the engine "should
//! either stop at a given level of references or keep track of which ones are
//! already resolved" (`engine/references.adoc`). The chapter fixes nothing
//! else, so the policy here is FerroBRIDGE's own (no specification governs
//! this: our own design):
//!
//! - A relative reference `<Type>/<id>`, or an absolute one under this
//!   server's own base, is relative to the service base
//!   (<https://hl7.org/fhir/R4/references.html#literal>), so it names a
//!   resource the facade committed: it is read through the identity map and
//!   the CDR as a read answers it.
//! - An absolute reference is fetched only from a base on the configured
//!   allow-list (`[facade.references] fetch_bases`, empty by default), with
//!   redirects refused, so a client cannot make the bridge call an arbitrary
//!   URL it names.
//! - References are followed to `max_depth` levels, each literal at most
//!   once, which keeps a cycle from looping.
//! - A reference whose resource is absent (off the allow-list, no `<Type>/<id>`,
//!   unknown to the identity map, or answered `404` or `410`) is a declared
//!   skip of the engine (`Loss::Skipped`), as the chapter allows.
//! - A fetch that failed (no answer, any other status, or a body that is not
//!   the referenced resource) refuses the request if a mapping reads the
//!   reference, with the upstream status: the chapter's "proceeds with the
//!   mapping" yields to the strictness of `engine/Fail.adoc`, and an upstream
//!   failure is never flattened into an absence.
//! - Every fetch is logged by the referenced type and id and the base, never
//!   a body.
//!
//! The engine reads synchronously, so the references are resolved before the
//! run and handed to it as [`Resolved`].

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::Duration;

use fhir_types::codec::Value;
use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::seam::ReferenceSource;
use fhirconnect::resolve::program::binding::ResourceType;
use http::StatusCode;
use url::Url;

mod literal;

use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::handlers::read;
use crate::facade::identity::FhirResourceId;
use crate::facade::references::literal::Target;
use crate::facade::references::literal::references_in;
use crate::facade::references::literal::target;
use crate::facade::references::literal::trimmed;
use crate::facade::references::literal::type_of;

/// How many levels of references are followed when the configuration names
/// none.
pub const DEFAULT_MAX_DEPTH: u32 = 2;

/// How long one fetch from an allow-listed base may take, in milliseconds,
/// when the configuration names none.
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// The media type a fetch asks for (<https://hl7.org/fhir/R4/http.html#mime-type>).
const FHIR_JSON: &str = "application/fhir+json";

/// What the facade may fetch to resolve a reference.
#[derive(Debug, Clone)]
pub struct Policy {
    /// The bases an absolute reference may be fetched from.
    fetch_bases: Vec<Url>,
    /// How many levels of references are followed.
    max_depth: u32,
    /// The client every fetch goes through, when any base is allowed.
    http: Option<reqwest::Client>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            fetch_bases: Vec::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            http: None,
        }
    }
}

impl Policy {
    /// Returns the policy that fetches from `fetch_bases`, follows
    /// `max_depth` levels and gives each fetch `timeout`.
    ///
    /// # Errors
    ///
    /// Returns [`reqwest::Error`] when the HTTP client cannot be built.
    pub fn new(
        fetch_bases: Vec<Url>,
        max_depth: u32,
        timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        let http = if fetch_bases.is_empty() {
            None
        } else {
            Some(
                reqwest::Client::builder()
                    .timeout(timeout)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?,
            )
        };
        Ok(Self {
            fetch_bases,
            max_depth,
            http,
        })
    }

    /// Returns the bases an absolute reference may be fetched from.
    #[must_use]
    pub fn fetch_bases(&self) -> &[Url] {
        &self.fetch_bases
    }

    /// Returns how many levels of references are followed.
    #[must_use]
    pub const fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Returns whether `base` is on the allow-list.
    fn allows(&self, base: &str) -> bool {
        self.fetch_bases
            .iter()
            .any(|allowed| trimmed(allowed.as_str()) == trimmed(base))
    }
}

/// The resources resolved for one request, by the literal reference that
/// names them.
#[derive(Default)]
pub struct Resolved {
    /// Each resolved resource, under the literal reference that named it.
    held: BTreeMap<String, Value>,
    /// Each reference that was tried and found absent.
    skipped: Vec<Unresolved>,
    /// Each reference whose fetch failed, with why.
    failed: BTreeMap<String, Reason>,
}

impl core::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Resolved")
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .field("skipped", &self.skipped)
            .field("failed", &self.failed)
            .finish()
    }
}

impl Resolved {
    /// Returns each reference that was tried and found absent, in the order
    /// it was tried.
    #[must_use]
    pub fn skipped(&self) -> &[Unresolved] {
        &self.skipped
    }

    /// Returns each reference whose fetch failed, by its literal.
    ///
    /// The engine refuses the run that reads one of them, so these are never
    /// skips.
    #[must_use]
    pub const fn failed(&self) -> &BTreeMap<String, Reason> {
        &self.failed
    }

    /// Returns how many resources were resolved.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether no resource was resolved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

impl ReferenceSource for Resolved {
    fn fetch(
        &self,
        reference: &str,
        _expected: &ResourceType,
    ) -> Result<Option<Value>, ReferenceError> {
        if let Some(reason) = self.failed.get(reference) {
            return Err(ReferenceError::Fetch {
                reference: String::from(reference),
                source: Box::new(Unfetched {
                    reference: String::from(reference),
                    reason: reason.clone(),
                }),
            });
        }
        Ok(self.held.get(reference).cloned())
    }
}

/// A referenced resource whose fetch failed, which refuses the run that
/// reads it.
///
/// The references chapter lets the engine proceed when a fetch is "not
/// possible" (`engine/references.adoc`), and `engine/Fail.adoc` asks that
/// "either all data should be mapped, or the mapping should fail". An absent
/// resource is the first case; a sending site or CDR that failed to answer is
/// an upstream failure, which never becomes an absence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the referenced resource {reference} could not be fetched: {reason}")]
pub struct Unfetched {
    /// The literal reference.
    reference: String,
    /// Why the fetch failed.
    reason: Reason,
}

impl Unfetched {
    /// Returns the literal reference.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Returns why the fetch failed.
    #[must_use]
    pub const fn reason(&self) -> &Reason {
        &self.reason
    }
}

/// Returns the failed reference fetch anywhere in `error`'s cause chain.
#[must_use]
pub fn unfetched<'e>(error: &'e (dyn core::error::Error + 'static)) -> Option<&'e Unfetched> {
    let mut cause = Some(error);
    while let Some(current) = cause {
        if let Some(found) = current.downcast_ref::<Unfetched>() {
            return Some(found);
        }
        cause = current.source();
    }
    None
}

/// One reference that was tried and not resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// The literal reference.
    reference: String,
    /// Why it was not resolved.
    reason: Reason,
}

impl Unresolved {
    /// Returns the literal reference.
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Returns why it was not resolved.
    #[must_use]
    pub const fn reason(&self) -> &Reason {
        &self.reason
    }
}

/// Why a reference was not resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Reason {
    /// The reference names a base that is not on the allow-list.
    #[error("its base is not on the [facade.references] fetch_bases allow-list")]
    NotAllowed,
    /// The reference is no `<Type>/<id>` this facade can read.
    #[error("it names no resource by type and id")]
    Unreadable,
    /// The server answered a status other than `200`.
    #[error("the server answered {status}")]
    Status {
        /// The status the server answered.
        status: StatusCode,
    },
    /// The server answered no response in time, or none at all.
    #[error("the server gave no answer: {detail}")]
    NoAnswer {
        /// What the HTTP client reported.
        detail: String,
    },
    /// The server answered a body that is no resource of the referenced type
    /// and id.
    #[error("the server answered something other than the referenced resource")]
    Mismatch,
    /// The facade's identity map does not bind the referenced resource.
    #[error("this server holds no such resource")]
    Absent,
}

impl Reason {
    /// Returns whether the reason says the resource is absent, which the
    /// engine skips, rather than that its fetch failed, which refuses.
    ///
    /// A `404` or `410` says the server holds no such resource
    /// (<https://hl7.org/fhir/R4/http.html#read>); every other status, a
    /// missing answer and a body that is not the resource are failures.
    #[must_use]
    pub fn is_absence(&self) -> bool {
        match *self {
            Self::NotAllowed | Self::Unreadable | Self::Absent => true,
            Self::Status { status } => {
                status == StatusCode::NOT_FOUND || status == StatusCode::GONE
            }
            Self::NoAnswer { .. } | Self::Mismatch => false,
        }
    }
}

/// Returns the type and id of this server's resource `literal` names, if any.
#[must_use]
pub fn local(literal: &str, own_base: &str) -> Option<(String, FhirResourceId)> {
    // NOTE: a refusal IS the answer: a literal this server does not serve names none of its own.
    match target(literal, &Policy::default(), own_base) {
        Ok(Target::Local { resource_type, id }) => Some((resource_type, id)),
        Ok(Target::Remote { .. }) | Err(_) => None,
    }
}

/// Resolves every reference `document` holds that a loaded `reference`
/// mapping may read, to the configured depth.
///
/// Only a reference whose type some `reference` mapping names is followed,
/// because no other is read. A reference that is not resolved is recorded in
/// [`Resolved::skipped`] and logged, and the engine proceeds without it.
pub async fn resolve(facade: &Facade, client: &CdrClient, document: &Value) -> Resolved {
    resolve_from(facade, client, document, &BTreeSet::new()).await
}

/// Resolves every reference a posted Bundle holds that none of its own
/// entries answers, as [`resolve`] does.
///
/// An entry answers a reference that names its `fullUrl`, or its `<Type>/<id>`
/// when it carries an id (<https://hl7.org/fhir/R4/bundle.html#references>),
/// because the references chapter fetches a resource only when it is "not
/// contained in the Bundle" (`engine/references.adoc`). Such a reference is
/// never read from this server or fetched: the caller looks the entries up
/// first, through `BundleReferences` over [`entries_of`] with this result as
/// the further source.
pub async fn resolve_bundle(facade: &Facade, client: &CdrClient, bundle: &Value) -> Resolved {
    let mut answered = BTreeSet::new();
    for (full_url, resource) in entries_of(bundle) {
        answered.extend(full_url);
        if let (Some(kind), Some(id)) = (
            resource.get("resourceType").and_then(Value::as_str),
            resource.get("id").and_then(Value::as_str),
        ) {
            answered.insert(format!("{kind}/{id}"));
        }
    }
    resolve_from(facade, client, bundle, &answered).await
}

/// Returns each entry of `bundle` that carries a resource, with its
/// `fullUrl` when it has one, in Bundle order.
#[must_use]
pub fn entries_of(bundle: &Value) -> Vec<(Option<String>, Value)> {
    bundle
        .get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let resource = entry.get("resource")?;
            let full_url = entry
                .get("fullUrl")
                .and_then(Value::as_str)
                .map(String::from);
            Some((full_url, resource.clone()))
        })
        .collect()
}

/// Resolves the references `document` holds, passing over every literal in
/// `answered`.
async fn resolve_from(
    facade: &Facade,
    client: &CdrClient,
    document: &Value,
    answered: &BTreeSet<String>,
) -> Resolved {
    let mut resolved = Resolved::default();
    let policy = facade.references();
    let wanted = facade.programs().resolved_types();
    if wanted.is_empty() {
        return resolved;
    }
    let mut tried: BTreeSet<String> = BTreeSet::new();
    let mut frontier = references_in(document);
    for _level in 0..policy.max_depth() {
        let mut next = Vec::new();
        for literal in frontier {
            if literal.starts_with('#')
                || answered.contains(&literal)
                || !tried.insert(literal.clone())
            {
                continue;
            }
            let target = match target(&literal, policy, &facade.settings().base_url) {
                Ok(target) => target,
                Err(reason) => {
                    if type_of(&literal).is_some_and(|kind| wanted.contains(&kind)) {
                        skip(&mut resolved, literal, reason, "-");
                    }
                    continue;
                }
            };
            let kind = match target {
                Target::Local {
                    ref resource_type, ..
                }
                | Target::Remote {
                    ref resource_type, ..
                } => resource_type.clone(),
            };
            if !wanted.contains(&kind) {
                continue;
            }
            match fetch(facade, client, policy, &target).await {
                Ok(found) => {
                    next.extend(references_in(&found));
                    resolved.held.insert(literal, found);
                }
                Err(reason) => {
                    let base = match target {
                        Target::Local { .. } => String::from("local"),
                        Target::Remote { ref base, .. } => base.clone(),
                    };
                    skip(&mut resolved, literal, reason, &base);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    resolved
}

/// Records and logs one reference that was not resolved, as an absence the
/// engine skips or as a failure that refuses the run reading it.
///
/// The reference literal is a resource type and id, an identifier, and the
/// base is configuration, so the line carries no clinical content.
fn skip(resolved: &mut Resolved, literal: String, reason: Reason, base: &str) {
    let status = match reason {
        Reason::Status { status } => Some(status.as_u16()),
        _ => None,
    };
    if !reason.is_absence() {
        tracing::warn!(
            reference = literal.as_str(),
            base,
            status,
            reason = %reason,
            "a referenced resource could not be fetched, and a mapping that reads it refuses"
        );
        resolved.failed.insert(literal, reason);
        return;
    }
    tracing::warn!(
        reference = literal.as_str(),
        base,
        status,
        reason = %reason,
        "a referenced resource was not resolved, and the mapping proceeds without it"
    );
    resolved.skipped.push(Unresolved {
        reference: literal,
        reason,
    });
}

/// Fetches the resource `target` names.
async fn fetch(
    facade: &Facade,
    client: &CdrClient,
    policy: &Policy,
    target: &Target,
) -> Result<Value, Reason> {
    match *target {
        Target::Local {
            ref resource_type,
            ref id,
        } => {
            let found = read::current(facade, client, resource_type, id)
                .await
                .map_err(|refusal| Reason::Status {
                    status: refusal.into_response().status(),
                })?
                .ok_or(Reason::Absent)?;
            tracing::info!(
                resource_type = resource_type.as_str(),
                id = id.as_str(),
                base = "local",
                "the facade read a referenced resource"
            );
            Ok(Value::Object(found))
        }
        Target::Remote {
            ref base,
            ref url,
            ref resource_type,
            ref id,
        } => {
            let http = policy.http.as_ref().ok_or(Reason::NotAllowed)?;
            let answered = http
                .get(url.clone())
                .header(http::header::ACCEPT, FHIR_JSON)
                .send()
                .await
                .map_err(|error| Reason::NoAnswer {
                    detail: error.to_string(),
                })?;
            let status = answered.status();
            tracing::info!(
                resource_type = resource_type.as_str(),
                id = id.as_str(),
                base = base.as_str(),
                status = status.as_u16(),
                "the facade fetched a referenced resource"
            );
            if status != StatusCode::OK {
                return Err(Reason::Status { status });
            }
            let bytes = answered.bytes().await.map_err(|error| Reason::NoAnswer {
                detail: error.to_string(),
            })?;
            let found: Value =
                serde_json::from_slice(&bytes).map_err(|_unread| Reason::Mismatch)?;
            let same = found.get("resourceType").and_then(Value::as_str)
                == Some(resource_type.as_str())
                && found.get("id").and_then(Value::as_str) == Some(id.as_str());
            if same {
                Ok(found)
            } else {
                Err(Reason::Mismatch)
            }
        }
    }
}
