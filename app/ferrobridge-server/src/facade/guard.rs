// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every route under the FHIR base is, for the `ferrohealth-smart`
//! layer in front of it, and the `OperationOutcome` a refusal answers with.
//!
//! The layer checks the credential, checks the SMART v2 permission the
//! interaction needs on the resource type the path names, and writes the
//! audit line. A request without a valid credential answers `401`, one whose
//! token grants no permission for the interaction `403` (RFC 6750 §3.1).
//! What a scope reaches inside a type (the patient compartment,
//! `crate::facade::compartment`) and the entries of a transaction or a batch
//! are checked by the handler, which reads the [`Access`] the layer leaves
//! on the request. `GET [base]/metadata` and the SMART discovery document
//! stay open: a client reads both before it holds a token.

use std::time::Instant;

use axum::extract::MatchedPath;
use axum::response::Response;
use ferrohealth_smart::audit::Actor;
use ferrohealth_smart::audit::AuditLine;
use ferrohealth_smart::guard::AuthLayer;
use ferrohealth_smart::guard::Gate;
use ferrohealth_smart::guard::Render;
use ferrohealth_smart::guard::Requirement;
use ferrohealth_smart::guard::Target;
use ferrohealth_smart::refusal::Refusal;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Honoured;
use ferrohealth_smart::resource::configuration::SMART_CONFIGURATION;
use ferrohealth_smart::scope::Permission;
use http::Extensions;
use http::Method;
use http::StatusCode;
use http::Uri;
use http::request::Parts;

use crate::facade::BASE_PATH;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::smart::Smart;

/// One FHIR interaction the facade serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// `GET [base]/metadata`.
    Capabilities,
    /// `POST [base]/{type}`.
    Create,
    /// `GET [base]/{type}/{id}`.
    Read,
    /// `GET [base]/{type}/{id}/_history/{vid}`.
    Vread,
    /// `PUT [base]/{type}/{id}`.
    Update,
    /// `DELETE [base]/{type}/{id}`, a party resource only.
    Delete,
    /// `GET [base]/{type}/{id}/_history`, a party resource only.
    History,
    /// `POST [base]/{type}/$validate`.
    Validate,
    /// `GET [base]/Composition/[id]/$document` and its type-level form.
    Document,
    /// `GET [base]/{type}` and `POST [base]/{type}/_search`.
    Search,
    /// `POST [base]` carrying a `transaction` Bundle, and the route itself.
    Transaction,
    /// `POST [base]` carrying a `batch` Bundle.
    ///
    /// The route is the transaction's, so the route table never names it; the
    /// batch handler names it on its response, and the audit line reads it
    /// from there.
    Batch,
    /// A FHIRconnect operation, `$tofhir` or `$toopenehr`, or its direct form.
    Operation,
    /// The SMART discovery document.
    Discovery,
    /// A route under the base that [`CLASSIFIED`] does not name, which a
    /// guarded deployment refuses.
    Unclassified,
}

impl Interaction {
    /// Returns the name the audit line records.
    ///
    /// The REST interactions keep their R4 codes
    /// (<https://hl7.org/fhir/R4/valueset-type-restful-interaction.html>).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::Create => "create",
            Self::Read => "read",
            Self::Vread => "vread",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::History => "history-instance",
            Self::Validate => "validate",
            Self::Search => "search-type",
            Self::Transaction => "transaction",
            Self::Batch => "batch",
            Self::Operation | Self::Document => "operation",
            Self::Discovery => "smart-configuration",
            Self::Unclassified => "unclassified",
        }
    }

    /// Returns what the interaction requires of the caller on
    /// `resource_type`, the operator scope admitting a dry run when one is
    /// named.
    ///
    /// The SMART v2 letters are SMART App Launch 2.2.0, Scopes and Launch
    /// Context, §Scopes for requesting FHIR Resources. `$validate` is a dry
    /// run of a create or an update, so either permission admits it, and so
    /// does the operator scope, because it commits nothing (no specification
    /// governs the scope of an operation: our own design).
    fn requirement(self, resource_type: Option<&str>, operator: Option<&str>) -> Requirement {
        let fhir = |permission: Permission| match resource_type {
            Some(resource_type) => Requirement::Fhir {
                permission,
                resource_type: resource_type.to_owned(),
            },
            None => Requirement::Caller,
        };
        match self {
            Self::Capabilities | Self::Discovery => Requirement::Public,
            Self::Create => fhir(Permission::Create),
            Self::Read | Self::Vread | Self::History | Self::Document => fhir(Permission::Read),
            Self::Update => fhir(Permission::Update),
            Self::Delete => fhir(Permission::Delete),
            Self::Search => fhir(Permission::Search),
            Self::Validate => {
                let write = Requirement::Either(
                    Box::new(fhir(Permission::Create)),
                    Box::new(fhir(Permission::Update)),
                );
                match operator {
                    Some(scope) => Requirement::Either(
                        Box::new(write),
                        Box::new(Requirement::Scope(scope.to_owned())),
                    ),
                    None => write,
                }
            }
            Self::Transaction | Self::Batch | Self::Operation | Self::Unclassified => {
                Requirement::Caller
            }
        }
    }
}

/// Every route under the FHIR base the guard admits: the method, the route
/// template relative to the base, and the interaction it is.
///
/// A route under the base that this table does not name is
/// [`Interaction::Unclassified`], which a guarded deployment refuses, so a
/// route mounted without a classification fails closed (no specification
/// governs the table: our own design).
pub const CLASSIFIED: [(Method, &str, Interaction); 18] = [
    (Method::GET, "/metadata", Interaction::Capabilities),
    (Method::GET, SMART_CONFIGURATION, Interaction::Discovery),
    (Method::POST, "", Interaction::Transaction),
    (Method::POST, "/{resource_type}", Interaction::Create),
    (Method::GET, "/{resource_type}", Interaction::Search),
    (
        Method::POST,
        "/{resource_type}/_search",
        Interaction::Search,
    ),
    (
        Method::POST,
        "/{resource_type}/$validate",
        Interaction::Validate,
    ),
    (
        Method::GET,
        "/{resource_type}/$document",
        Interaction::Document,
    ),
    (
        Method::GET,
        "/{resource_type}/{id}/$document",
        Interaction::Document,
    ),
    (Method::GET, "/{resource_type}/{id}", Interaction::Read),
    (Method::PUT, "/{resource_type}/{id}", Interaction::Update),
    (Method::DELETE, "/{resource_type}/{id}", Interaction::Delete),
    (
        Method::GET,
        "/{resource_type}/{id}/_history",
        Interaction::History,
    ),
    (
        Method::GET,
        "/{resource_type}/{id}/_history/{vid}",
        Interaction::Vread,
    ),
    (Method::POST, "/$tofhir", Interaction::Operation),
    (Method::POST, "/$toopenehr", Interaction::Operation),
    (Method::POST, "/tofhir", Interaction::Operation),
    (Method::POST, "/toopenehr", Interaction::Operation),
];

/// The request under the FHIR base, as the route table reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    /// The interaction.
    pub interaction: Interaction,
    /// The matched route template.
    pub template: String,
    /// The resource type the path names.
    pub resource_type: Option<String>,
    /// The logical id the path names.
    pub id: Option<String>,
}

/// Returns what `method` on the route `template` matched for `path` is, or
/// `None` for a route outside the FHIR base.
#[must_use]
pub fn classify(method: &Method, template: &str, path: &str) -> Option<Classified> {
    let relative = template.strip_prefix(BASE_PATH)?;
    if !relative.is_empty() && !relative.starts_with('/') {
        return None;
    }
    let interaction = CLASSIFIED
        .iter()
        .find(|(listed, listed_template, _)| listed == method && *listed_template == relative)
        .map_or(Interaction::Unclassified, |(_, _, interaction)| {
            *interaction
        });
    let mut segments = path
        .strip_prefix(BASE_PATH)
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty());
    let first = segments.next().map(str::to_owned);
    let second = segments.next().map(str::to_owned);
    let (resource_type, id) = match interaction {
        Interaction::Create | Interaction::Search | Interaction::Validate => (first, None),
        Interaction::Document => (first, second.filter(|segment| segment != "$document")),
        Interaction::Read
        | Interaction::Update
        | Interaction::Vread
        | Interaction::Delete
        | Interaction::History => (first, second),
        Interaction::Capabilities
        | Interaction::Transaction
        | Interaction::Batch
        | Interaction::Operation
        | Interaction::Discovery
        | Interaction::Unclassified => (None, None),
    };
    Some(Classified {
        interaction,
        template: template.to_owned(),
        resource_type,
        id,
    })
}

/// Returns the name the audit line records for what a request addressed:
/// `Condition/c-1`, `Condition`, or nothing.
fn addressed(resource_type: Option<&str>, id: Option<&str>) -> Option<String> {
    match (resource_type, id) {
        (Some(resource_type), Some(id)) => Some(format!("{resource_type}/{id}")),
        (Some(resource_type), None) => Some(resource_type.to_owned()),
        (None, _) => None,
    }
}

/// How the layer reads a request under the FHIR base.
#[derive(Debug, Clone)]
pub struct Classifier {
    /// Whether the deployment authenticates its callers, so an unclassified
    /// route fails closed.
    guarded: bool,
    /// The operator scope, when the deployment authenticates its callers.
    operator: Option<String>,
}

impl ferrohealth_smart::guard::Classify for Classifier {
    fn classify(&self, method: &Method, uri: &Uri, extensions: &Extensions) -> Option<Target> {
        let template = extensions.get::<MatchedPath>()?.as_str();
        let classified = classify(method, template, uri.path())?;
        // NOTE: no specification governs this: our own design, the discovery
        // document is public and leaves no audit line.
        if classified.interaction == Interaction::Discovery {
            return None;
        }
        // NOTE: no specification governs this: our own design, a route the
        // table cannot classify needs no permission it could check, so it fails closed.
        let requirement = if classified.interaction == Interaction::Unclassified && self.guarded {
            Requirement::Refused
        } else {
            classified.interaction.requirement(
                classified.resource_type.as_deref(),
                self.operator.as_deref(),
            )
        };
        let target = Target::new(classified.interaction.code(), requirement)
            .route(classified.template.as_str());
        Some(
            match addressed(
                classified.resource_type.as_deref(),
                classified.id.as_deref(),
            ) {
                Some(name) => target.target(name),
                None => target,
            },
        )
    }
}

/// The `OperationOutcome` every refusal of the layer answers with.
///
/// The status and the `WWW-Authenticate` challenge are the refusal's
/// (RFC 6750 §3.1); the body names no claim and no scope, so a refusal
/// teaches a caller nothing about the grants it lacks (no specification
/// requires the omission: our own design).
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The realm every challenge names.
    realm: Option<String>,
}

impl Render for Outcome {
    fn render(&self, refusal: &Refusal, _parts: &Parts) -> Response {
        self.respond(refusal)
    }
}

impl Outcome {
    /// Returns the `OperationOutcome` response of `refusal`.
    fn respond(&self, refusal: &Refusal) -> Response {
        let issue = match refusal {
            Refusal::Missing(_) | Refusal::InvalidToken | Refusal::InvalidBasic => {
                Issue::error(IssueType::Login)
                    .diagnosing("the request carries no valid access token for this server")
            }
            Refusal::InvalidRequest => Issue::error(IssueType::Invalid)
                .diagnosing("the Authorization header carries no single credential"),
            Refusal::Forbidden | Refusal::Unclassified => Issue::error(IssueType::Forbidden)
                .diagnosing("the access token does not permit this request"),
            Refusal::Unavailable => Issue::error(IssueType::Transient).diagnosing(
                "the access token could not be checked: the authorization server's keys could not be read",
            ),
            _ => Issue::error(IssueType::Exception)
                .diagnosing("the request was refused at the gate"),
        };
        let headers: Vec<reply::Header> = refusal
            .challenge(self.realm.as_deref())
            .iter()
            .map(|challenge| reply::Header::challenge(challenge))
            .collect();
        reply::issues(refusal.status(), &[issue], &headers)
    }
}

/// Returns the layer that guards the routes under the FHIR base with the
/// gate of `smart`, or leaves them open when inbound authentication is off.
#[must_use]
pub fn layer(smart: Option<&Smart>) -> AuthLayer<Classifier> {
    let classifier = Classifier {
        guarded: smart.is_some(),
        operator: smart.map(|smart| smart.operator_scope().to_owned()),
    };
    let gate = smart.map_or_else(Gate::open, |smart| smart.gate().clone());
    let render = Outcome {
        realm: gate.realm().map(str::to_owned),
    };
    AuthLayer::new(gate, classifier).with_render(render)
}

/// The operator scope, which a handler reads to admit a dry run the scope
/// covers; `None` when inbound authentication is off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperatorScope(pub Option<String>);

/// Returns the `403` of a token that does not permit the request.
#[must_use]
pub fn forbidden() -> reply::Refusal {
    reply::Refusal::new(Outcome::default().respond(&Refusal::Forbidden))
}

/// Checks that `access` grants a permission `interaction` needs on
/// `resource_type`, the type-level check the layer runs on a route, for one
/// entry of a batch.
///
/// # Errors
///
/// Returns the layer's `403` when no permission is granted.
pub(crate) fn admit(
    access: &Access,
    interaction: Interaction,
    resource_type: &str,
) -> Result<(), reply::Refusal> {
    if interaction
        .requirement(Some(resource_type), None)
        .met_by(access, Honoured::default())
    {
        Ok(())
    } else {
        Err(forbidden())
    }
}

/// One entry of a batch, as its audit line records it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AuditedEntry<'a> {
    /// The entry's position in the Bundle, from zero.
    pub(crate) position: usize,
    /// The method its `request` names.
    pub(crate) method: &'a Method,
    /// The interaction it was answered as.
    pub(crate) interaction: Interaction,
    /// The resource type its `request.url` names.
    pub(crate) resource_type: Option<&'a str>,
    /// The logical id its `request.url` names.
    pub(crate) id: Option<&'a str>,
    /// The status it answered.
    pub(crate) status: StatusCode,
}

/// Writes the audit line of one batch entry, under the batch's request id.
///
/// The line carries the request line's fields and the entry's position.
pub(crate) fn audit_entry(
    access: &Access,
    entry: &AuditedEntry<'_>,
    started: Instant,
    request_id: &str,
) {
    let target = addressed(entry.resource_type, entry.id);
    AuditLine::new(entry.method, entry.interaction.code(), entry.status)
        .timed(jiff::Timestamp::now(), started.elapsed())
        .by(Actor::of(Some(access)))
        .on(None, target.as_deref())
        .request((!request_id.is_empty()).then_some(request_id))
        .entry(entry.position)
        .write();
}

#[cfg(test)]
mod tests {
    use super::{Interaction, classify};
    use http::Method;

    #[test]
    fn the_matched_route_names_the_interaction_type_and_id() {
        let read = classify(
            &Method::GET,
            "/fhir/{resource_type}/{id}",
            "/fhir/Condition/c-1",
        )
        .expect("a FHIR route");
        assert_eq!(Interaction::Read, read.interaction);
        assert_eq!(Some("Condition"), read.resource_type.as_deref());
        assert_eq!(Some("c-1"), read.id.as_deref());
        let vread = classify(
            &Method::GET,
            "/fhir/{resource_type}/{id}/_history/{vid}",
            "/fhir/Condition/c-1/_history/2",
        )
        .expect("a FHIR route");
        assert_eq!(Interaction::Vread, vread.interaction);
        assert_eq!(Some("c-1"), vread.id.as_deref());
    }

    #[test]
    fn both_search_routes_are_the_search_interaction_of_their_type() {
        // SMART App Launch 2.x scopes: `s` grants the search interaction.
        for (method, template, path) in [
            (Method::GET, "/fhir/{resource_type}", "/fhir/Condition"),
            (
                Method::POST,
                "/fhir/{resource_type}/_search",
                "/fhir/Condition/_search",
            ),
        ] {
            let search = classify(&method, template, path).expect("a FHIR route");
            assert_eq!(
                Interaction::Search,
                search.interaction,
                "{method} {template}"
            );
            assert_eq!(Some("Condition"), search.resource_type.as_deref());
            assert_eq!(None, search.id);
            assert_eq!("search-type", search.interaction.code());
        }
    }

    #[test]
    fn a_route_outside_the_base_is_not_classified() {
        assert!(classify(&Method::GET, "/health/readiness", "/health/readiness").is_none());
        assert!(classify(&Method::GET, "/fhirx", "/fhirx").is_none());
    }

    #[test]
    fn an_unknown_route_under_the_base_needs_a_token() {
        let other = classify(&Method::POST, "/fhir/$tofhir", "/fhir/$tofhir").expect("FHIR");
        assert_eq!(Interaction::Operation, other.interaction);
        assert_ne!(
            ferrohealth_smart::guard::Requirement::Public,
            other.interaction.requirement(None, None)
        );
    }
}
