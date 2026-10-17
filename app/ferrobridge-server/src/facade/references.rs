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
//! - Every reference that is not resolved is a declared skip of the engine
//!   (`Loss::Skipped`), and a fetch that failed is logged with the upstream
//!   status. Every fetch is logged by the referenced type and id and the
//!   base, never a body.
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

use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::handlers::read;
use crate::facade::identity::FhirResourceId;

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
    /// Each reference that was tried and not resolved.
    skipped: Vec<Unresolved>,
}

impl core::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Resolved")
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .field("skipped", &self.skipped)
            .finish()
    }
}

impl Resolved {
    /// Returns each reference that was tried and not resolved, in the order
    /// it was tried.
    #[must_use]
    pub fn skipped(&self) -> &[Unresolved] {
        &self.skipped
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
        Ok(self.held.get(reference).cloned())
    }
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

/// Where one literal reference points.
#[derive(Debug)]
enum Target {
    /// A resource this facade serves.
    Local {
        /// Its type.
        resource_type: String,
        /// Its logical id.
        id: FhirResourceId,
    },
    /// A resource of an allow-listed base.
    Remote {
        /// The base it is fetched from.
        base: String,
        /// The URL it is fetched at.
        url: Url,
        /// Its type.
        resource_type: String,
        /// Its logical id.
        id: String,
    },
}

/// Resolves every reference `document` holds that a loaded `reference`
/// mapping may read, to the configured depth.
///
/// Only a reference whose type some `reference` mapping names is followed,
/// because no other is read. A reference that is not resolved is recorded in
/// [`Resolved::skipped`] and logged, and the engine proceeds without it.
pub async fn resolve(facade: &Facade, client: &CdrClient, document: &Value) -> Resolved {
    resolve_from(facade, client, document, Resolved::default()).await
}

/// Resolves every reference a posted Bundle holds, as [`resolve`] does, with
/// the Bundle's own entries looked up first.
///
/// An entry answers a reference that names its `fullUrl`, or its `<Type>/<id>`
/// when it carries an id (<https://hl7.org/fhir/R4/bundle.html#references>),
/// because the references chapter fetches a resource only when it is "not
/// contained in the Bundle" (`engine/references.adoc`). Such a reference is
/// never read from this server or fetched.
pub async fn resolve_bundle(facade: &Facade, client: &CdrClient, bundle: &Value) -> Resolved {
    let mut seeded = Resolved::default();
    for entry in bundle
        .get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default()
    {
        let Some(resource) = entry.get("resource") else {
            continue;
        };
        if let Some(full_url) = entry.get("fullUrl").and_then(Value::as_str) {
            seeded.held.insert(String::from(full_url), resource.clone());
        }
        if let (Some(kind), Some(id)) = (
            resource.get("resourceType").and_then(Value::as_str),
            resource.get("id").and_then(Value::as_str),
        ) {
            seeded
                .held
                .entry(format!("{kind}/{id}"))
                .or_insert_with(|| resource.clone());
        }
    }
    resolve_from(facade, client, bundle, seeded).await
}

/// Resolves the references `document` holds onto `resolved`, passing over
/// every literal `resolved` already holds.
async fn resolve_from(
    facade: &Facade,
    client: &CdrClient,
    document: &Value,
    mut resolved: Resolved,
) -> Resolved {
    let policy = facade.references();
    let wanted = facade.programs().referenced_types();
    if wanted.is_empty() {
        return resolved;
    }
    let mut tried: BTreeSet<String> = BTreeSet::new();
    let mut frontier = references_in(document);
    for _level in 0..policy.max_depth() {
        let mut next = Vec::new();
        for literal in frontier {
            if literal.starts_with('#')
                || resolved.held.contains_key(&literal)
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

/// Records and logs one reference that was not resolved.
///
/// The reference literal is a resource type and id, an identifier, and the
/// base is configuration, so the line carries no clinical content.
fn skip(resolved: &mut Resolved, literal: String, reason: Reason, base: &str) {
    let status = match reason {
        Reason::Status { status } => Some(status.as_u16()),
        _ => None,
    };
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

/// Returns where `literal` points under `policy`, with this server's own
/// `own_base`.
fn target(literal: &str, policy: &Policy, own_base: &str) -> Result<Target, Reason> {
    let Ok(url) = Url::parse(literal) else {
        let (resource_type, id) = type_and_id(literal).ok_or(Reason::Unreadable)?;
        return Ok(Target::Local {
            resource_type: resource_type.to_owned(),
            id,
        });
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Reason::Unreadable);
    }
    let full = String::from(url.path());
    let path = full.trim_end_matches('/');
    let (head, id_text) = path.rsplit_once('/').ok_or(Reason::Unreadable)?;
    let (head, resource_type) = head.rsplit_once('/').ok_or(Reason::Unreadable)?;
    let (_, id) = type_and_id(&format!("{resource_type}/{id_text}")).ok_or(Reason::Unreadable)?;
    let mut base = url.clone();
    base.set_path(head);
    let base_text = String::from(trimmed(base.as_str()));
    if base_text == trimmed(own_base) {
        return Ok(Target::Local {
            resource_type: resource_type.to_owned(),
            id,
        });
    }
    // NOTE: no specification governs this: our own design, `engine/references.adoc`
    // names the GET and nothing else, so only an allow-listed base is fetched.
    if !policy.allows(&base_text) {
        return Err(Reason::NotAllowed);
    }
    Ok(Target::Remote {
        base: base_text,
        url,
        resource_type: resource_type.to_owned(),
        id: String::from(id.as_str()),
    })
}

/// Returns the type and id a relative `<Type>/<id>` names.
///
/// R4 names a resource type in upper camel case and an id in
/// `[A-Za-z0-9\-\.]{1,64}` (<https://hl7.org/fhir/R4/resource.html>); a
/// versioned `<Type>/<id>/_history/<vid>` is no such pair.
fn type_and_id(literal: &str) -> Option<(&str, FhirResourceId)> {
    let (resource_type, id) = literal.split_once('/')?;
    let typed = resource_type
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && resource_type.chars().all(|c| c.is_ascii_alphabetic());
    if !typed {
        return None;
    }
    // NOTE: a parse failure IS the answer here: a text outside the R4 id
    // grammar names no resource (<https://hl7.org/fhir/R4/resource.html>).
    let id = FhirResourceId::new(id).ok()?;
    Some((resource_type, id))
}

/// Returns the resource type a literal reference names, when it names one:
/// the segment before its last.
fn type_of(literal: &str) -> Option<String> {
    // NOTE: a parse failure IS the answer here: a literal that is no absolute
    // URL is relative (<https://hl7.org/fhir/R4/references.html#literal>).
    let path = Url::parse(literal)
        .map_or_else(|_relative| literal.to_owned(), |url| url.path().to_owned());
    let mut segments = path.trim_end_matches('/').rsplit('/');
    segments.next()?;
    segments.next().map(str::to_owned)
}

/// Returns `text` without a trailing `/`.
fn trimmed(text: &str) -> &str {
    text.trim_end_matches('/')
}

/// Returns every `reference` string `value` holds, at any depth, in document
/// order.
fn references_in(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect(value, &mut found);
    found
}

/// Adds every `reference` string under `value` to `found`.
fn collect(value: &Value, found: &mut Vec<String>) {
    match *value {
        Value::Object(ref object) => {
            for (key, member) in object {
                if key == "reference"
                    && let Value::String(ref text) = *member
                {
                    found.push(text.clone());
                    continue;
                }
                collect(member, found);
            }
        }
        Value::Array(ref items) => {
            for item in items {
                collect(item, found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::Policy;
    use super::Reason;
    use super::Target;
    use super::target;
    use super::type_of;
    use std::time::Duration;

    /// This server's own base in the cases.
    const OWN: &str = "http://ferrobridge.invalid/fhir";

    fn allowing(base: &str) -> Policy {
        Policy::new(
            vec![base.parse().expect("a legal base")],
            2,
            Duration::from_secs(1),
        )
        .expect("the client builds")
    }

    #[test]
    fn a_relative_reference_and_one_under_the_own_base_are_local() {
        for literal in [
            "Observation/obs-1",
            "http://ferrobridge.invalid/fhir/Observation/obs-1",
        ] {
            let found = target(literal, &Policy::default(), OWN).expect("a readable reference");
            assert!(
                matches!(found, Target::Local { ref resource_type, ref id } if resource_type == "Observation" && id.as_str() == "obs-1"),
                "{literal}: {found:?}"
            );
        }
    }

    #[test]
    fn an_absolute_reference_off_the_allow_list_is_not_fetched() {
        let refused = target(
            "https://elsewhere.invalid/fhir/Observation/obs-1",
            &Policy::default(),
            OWN,
        )
        .expect_err("no base is allowed by default");
        assert_eq!(Reason::NotAllowed, refused);
    }

    #[test]
    fn an_absolute_reference_on_the_allow_list_is_remote() {
        let found = target(
            "https://sender.invalid/fhir/Observation/obs-1",
            &allowing("https://sender.invalid/fhir/"),
            OWN,
        )
        .expect("an allowed base");
        assert!(
            matches!(found, Target::Remote { ref base, .. } if base == "https://sender.invalid/fhir"),
            "{found:?}"
        );
    }

    #[test]
    fn a_reference_that_names_no_type_and_id_is_unreadable() {
        for literal in [
            "Observation/obs-1/_history/2",
            "observation/obs-1",
            "urn:uuid:9c2b",
            "https://sender.invalid/fhir/Observation/obs-1?x=1",
        ] {
            assert_eq!(
                Err(Reason::Unreadable),
                target(literal, &allowing("https://sender.invalid/fhir"), OWN).map(|_| ()),
                "{literal}"
            );
        }
    }

    #[test]
    fn the_type_of_a_reference_is_read_from_its_last_two_segments() {
        assert_eq!(
            Some(String::from("Observation")),
            type_of("Observation/obs-1")
        );
        assert_eq!(
            Some(String::from("Observation")),
            type_of("https://elsewhere.invalid/fhir/Observation/obs-1")
        );
        assert_eq!(None, type_of("obs-1"));
    }
}
