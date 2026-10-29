// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every operator route requires, for the `ferrohealth-smart` layer in
//! front of it, and the problem details a refusal answers with.
//!
//! A request without a valid credential answers `401` and one whose token
//! does not carry the operator scope answers `403` (RFC 6750 §3.1). The
//! layer's audit line names the caller and the matched route, never a path
//! value or a body. No specification governs the operator surface: our own
//! design.

use axum::extract::MatchedPath;
use axum::response::Response;
use ferrohealth_smart::guard::AuthLayer;
use ferrohealth_smart::guard::Classify;
use ferrohealth_smart::guard::Render;
use ferrohealth_smart::guard::Requirement;
use ferrohealth_smart::guard::Target;
use ferrohealth_smart::refusal::Refusal;
use http::Extensions;
use http::Method;
use http::Uri;
use http::request::Parts;

use crate::problem::{Kind, respond};
use crate::smart::Smart;

/// The `tracing` target of the change record a save or a reload writes
/// beside the layer's audit line, so a deployment can route it apart.
pub const AUDIT_TARGET: &str = "ferrobridge::audit";

/// The interaction the audit line records for every operator route.
const OPERATOR: &str = "operator";

/// How the layer reads an operator request: every route the operator router
/// matched requires the operator scope.
#[derive(Debug, Clone)]
pub struct Classifier {
    /// The operator scope.
    scope: String,
}

impl Classify for Classifier {
    fn classify(&self, _method: &Method, _uri: &Uri, extensions: &Extensions) -> Option<Target> {
        let template = extensions.get::<MatchedPath>()?.as_str();
        Some(Target::new(OPERATOR, Requirement::Scope(self.scope.clone())).route(template))
    }
}

/// The problem details every refusal of the layer answers with (RFC 9457).
///
/// A refusal whose status says all there is to say is `about:blank`.
#[derive(Debug, Clone)]
pub struct Body {
    /// The realm every challenge names.
    realm: Option<String>,
}

impl Render for Body {
    fn render(&self, refused: &Refusal, _parts: &Parts) -> Response {
        let problem = match refused {
            Refusal::Missing(_) | Refusal::InvalidToken | Refusal::InvalidBasic => Kind::Unauthenticated
                .problem("the request carries no valid access token for this server"),
            Refusal::InvalidRequest => Kind::InvalidRequest
                .problem("the Authorization header carries no single credential"),
            Refusal::Forbidden | Refusal::Unclassified => Kind::NotPermitted
                .problem("the access token does not carry the operator scope"),
            Refusal::Unavailable => Kind::AuthorizationUnavailable.problem(
                "the access token could not be checked: the authorization server's keys could not be read",
            ),
            _ => ferrohealth_server::problem::Problem::new(refused.status())
                .with_detail("the request was refused at the gate"),
        };
        respond(problem, refused.challenge(self.realm.as_deref()).as_deref())
    }
}

/// Returns the layer that guards the operator routes with the gate of
/// `smart` and the operator scope.
#[must_use]
pub fn layer(smart: &Smart) -> AuthLayer<Classifier> {
    let gate = smart.gate().clone();
    let body = Body {
        realm: gate.realm().map(str::to_owned),
    };
    let classifier = Classifier {
        scope: smart.operator_scope().to_owned(),
    };
    AuthLayer::new(gate, classifier).with_render(body)
}
