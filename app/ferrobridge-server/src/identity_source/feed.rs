// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The receiver of the identity source's PMIR feed (PMIR 1.6.0 ITI-93,
//! §2:3.93), the channel endpoint of the deployment's subscription.
//!
//! Each message forgets, on every replica, the source identifiers whose
//! pseudonym it may have changed (every pseudonym, for a merge or a
//! delete), and only then is acknowledged, so the
//! identity source sends again a message whose change did not reach every
//! cache. The route admits only an access token whose `client_id` the
//! deployment lists. No specification governs the route's path: our own
//! design.

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use ferrohealth_smart::guard::{AuthLayer, Requirement};
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use ihe_iti::pmir::feed::{Event, Feed, ResponseId, refusal};
use secrecy::ExposeSecret as _;

use super::cache::PseudonymCache;
use crate::smart::{Routed, Smart};

/// The path the identity source sends each ITI-93 message to.
pub const PATH: &str = "/pmir/feed";

/// The media type of every answer (ITI TF-2 Appendix Z.6).
const FHIR_JSON: &str = "application/fhir+json";

/// The interaction the gate's audit line records.
const INTERACTION: &str = "pmir-feed";

/// Returns the receiver over `cache`, guarded by the gate of `smart`,
/// admitting the access tokens of `clients`; `source` is the endpoint the
/// acknowledgements name.
pub fn routes(
    cache: PseudonymCache,
    source: String,
    smart: &Smart,
    clients: Vec<String>,
) -> Router {
    // NOTE: PMIR 1.6.0 §2:3.93.5 leaves the feed's authentication to the two parties;
    // the identity source sends its client-credentials token, so the route admits its client ids.
    let requirement = Requirement::Client {
        client_ids: clients,
        basic_users: Vec::new(),
    };
    let layer = AuthLayer::new(smart.gate().clone(), Routed::new(INTERACTION, requirement));
    Router::new()
        .route(PATH, post(receive))
        .with_state(Receiver { cache, source })
        .route_layer(layer)
}

/// What the receiver holds.
#[derive(Debug, Clone)]
struct Receiver {
    /// The cache each message forgets from.
    cache: PseudonymCache,
    /// The endpoint the acknowledgements name.
    source: String,
}

/// Takes one ITI-93 message.
async fn receive(State(receiver): State<Receiver>, headers: HeaderMap, body: Bytes) -> Response {
    let media = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let feed = match Feed::read(media, &body) {
        Ok(feed) => feed,
        Err(error) => {
            return match refusal(&error) {
                Ok(outcome) => fhir(StatusCode::BAD_REQUEST, outcome),
                Err(_unwritable) => StatusCode::BAD_REQUEST.into_response(),
            };
        }
    };
    let forgotten = match changed(&feed) {
        Changed::Identifiers(changed) => {
            receiver
                .cache
                .forget(
                    changed
                        .iter()
                        .map(|(system, value)| (system.as_str(), value.as_str())),
                )
                .await
        }
        Changed::Everything => receiver.cache.forget_all().await,
    };
    if let Err(error) = forgotten {
        tracing::warn!(error = %ferrohealth_server::run::chain(&error), "a PMIR feed message could not reach every replica; it is refused for the identity source to send again");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let response_id = uuid::Uuid::new_v4().hyphenated().to_string();
    let acknowledged = ResponseId::new(&response_id)
        .map_err(|_invalid| "the response id is not a lowercase hyphenated UUID")
        .and_then(|id| {
            feed.acknowledgement(&id, &receiver.source)
                .map_err(|_unwritable| "the acknowledgement cannot be written as FHIR JSON")
        });
    match acknowledged {
        Ok(body) => fhir(StatusCode::OK, body),
        Err(reason) => {
            tracing::warn!(
                reason,
                "a PMIR feed message was taken and could not be acknowledged"
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// What one message may have changed.
#[derive(Debug, PartialEq, Eq)]
enum Changed {
    /// The pseudonyms of these identifiers, each its system and value.
    Identifiers(Vec<(String, String)>),
    /// Any pseudonym.
    Everything,
}

/// Returns what `feed` may have changed.
fn changed(feed: &Feed) -> Changed {
    let mut changed = Vec::new();
    for event in feed.events() {
        let identity = match event {
            // NOTE: no specification governs this: our own design; a merge or a delete may name
            // the person without the source identifiers that resolved to it, so it forgets all.
            Event::Merged { .. } | Event::Deleted(_) => return Changed::Everything,
            // NOTE: PMIR 1.6.0 §2:3.93.4.1.3: an update that reinstates a merged patient is
            // its unmerge, and any update may move a link, so its identifiers are forgotten.
            Event::Updated(identity) => identity,
            _ => continue,
        };
        for identifier in identity.identifiers() {
            if let Some(system) = identifier.system() {
                changed.push((
                    system.to_owned(),
                    identifier.value().expose_secret().to_owned(),
                ));
            }
        }
    }
    Changed::Identifiers(changed)
}

/// Returns `body` as FHIR JSON with `status`.
fn fhir(status: StatusCode, body: Vec<u8>) -> Response {
    (
        status,
        [(CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON))],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use ihe_iti::pmir::feed::Feed;

    use super::{Changed, changed};

    /// A synthetic ITI-93 message whose history carries `entries`.
    fn message(entries: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "resourceType": "Bundle",
            "id": "message-1",
            "type": "message",
            "entry": [
                {
                    "fullUrl": "urn:uuid:7f4b3c2a-0000-4000-8000-000000000001",
                    "resource": {
                        "resourceType": "MessageHeader",
                        "id": "header-1",
                        "eventUri": "urn:ihe:iti:pmir:2019:patient-feed",
                        "destination": [{ "endpoint": "https://bridge.example.org/pmir/feed" }],
                        "source": { "endpoint": "https://pix.example.org/fhir" },
                        "focus": [{ "reference": "urn:uuid:7f4b3c2a-0000-4000-8000-000000000002" }]
                    }
                },
                {
                    "fullUrl": "urn:uuid:7f4b3c2a-0000-4000-8000-000000000002",
                    "resource": { "resourceType": "Bundle", "type": "history", "entry": entries }
                }
            ]
        }))
        .expect("a JSON message")
    }

    /// A history entry that `method`s the Patient `id` with `body`.
    fn entry(method: &str, id: &str, body: &serde_json::Value) -> serde_json::Value {
        let mut patient = body.clone();
        patient["resourceType"] = serde_json::json!("Patient");
        patient["id"] = serde_json::json!(id);
        serde_json::json!({
            "fullUrl": format!("https://pix.example.org/fhir/Patient/{id}"),
            "resource": patient,
            "request": {
                "method": method,
                "url": if method == "POST" { String::from("Patient") } else { format!("Patient/{id}") }
            },
            "response": { "status": if method == "POST" { "201" } else { "200" } }
        })
    }

    #[test]
    fn an_update_forgets_its_identifiers_and_a_create_none() {
        let body = message(&serde_json::json!([
            entry(
                "POST",
                "p3",
                &serde_json::json!({
                    "active": true,
                    "identifier": [{ "system": "urn:oid:2.999.1", "value": "created-1" }]
                })
            ),
            entry(
                "PUT",
                "p4",
                &serde_json::json!({
                    "active": true,
                    "identifier": [{ "system": "urn:oid:2.999.1", "value": "unmerged-1" }]
                })
            ),
        ]));
        let feed = Feed::read(Some("application/fhir+json"), &body).expect("a PMIR feed message");
        assert_eq!(
            Changed::Identifiers(vec![(
                String::from("urn:oid:2.999.1"),
                String::from("unmerged-1")
            )]),
            changed(&feed)
        );
    }

    #[test]
    fn a_merge_forgets_every_pseudonym() {
        // The subsumed patient as FerroPIX sends it: its master identity alone.
        let body = message(&serde_json::json!([
            entry(
                "PUT",
                "p2",
                &serde_json::json!({
                    "active": false,
                    "identifier": [{ "system": "https://pix.example.org", "value": "p2" }],
                    "link": [{ "other": { "reference": "Patient/p1" }, "type": "replaced-by" }]
                })
            ),
            entry(
                "PUT",
                "p1",
                &serde_json::json!({
                    "active": true,
                    "identifier": [{ "system": "urn:oid:2.999.1", "value": "survivor-1" }]
                })
            ),
        ]));
        let feed = Feed::read(Some("application/fhir+json"), &body).expect("a PMIR feed message");
        assert_eq!(Changed::Everything, changed(&feed));
    }
}
