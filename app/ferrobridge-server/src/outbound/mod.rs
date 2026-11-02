// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound FHIR lane: a change-feed consumer that maps each changed
//! composition to FHIR R4 through the loaded FHIRconnect programs and
//! delivers the resources to a configured FHIR endpoint.
//!
//! An event names the compositions a contribution changed. For each one the
//! lane reads the latest version over ITS-REST 1.1.0 `composition_get` and
//! maps it as the facade's read does (`facade::handlers::emit`),
//! each resource carrying its `id`, `meta.versionId` and `meta.source`. A
//! `204` (a deletion) and an erasure retract every resource the composition
//! was last delivered as, and an update retracts the ones its new version no
//! longer maps to, from the record [`emitted`] keeps. The event's resources
//! travel as one `transaction` Bundle ([`sink`]): a `PUT` per resource and a
//! `DELETE` per retraction, so a delivery sent again changes nothing, and
//! the event is acknowledged only once the endpoint accepted it (at least
//! once). A clinical resource without a subject is never emitted: the event
//! is refused, and the follower parks it. No specification governs the
//! outbound lane: our own design.

pub mod emitted;
pub mod sink;

use std::collections::BTreeSet;
use std::sync::Arc;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_its::rest::generated::ehr::client::CompositionGetOutcome;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::CdrClient;
use crate::cdr::error::CdrError;
use crate::cdr::ids::EhrId;
use crate::change_feed::Event;
use crate::change_feed::ReaderName;
use crate::change_feed::follow::Consumer;
use crate::change_feed::follow::ConsumerError;
use crate::change_feed::health::Following;
use crate::facade::Facade;
use emitted::Emitted;
use emitted::EmittedError;
use sink::Sink;
use sink::SinkError;

/// The name the lane reports to readiness and the logs under.
pub const NAME: &str = "fhir-outbound";

/// The RM type name of the versioned objects the lane emits.
const COMPOSITION: &str = "COMPOSITION";

/// The RM type name of an erased EHR in an erasure.
const EHR: &str = "EHR";

/// The lane, resolved from `[fhir_outbound]`.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The reader name the lane follows the change feed under.
    pub reader: ReaderName,
    /// The FHIR endpoint.
    pub sink: Sink,
    /// The file the delivery record is kept in.
    pub emitted: std::path::PathBuf,
}

/// Why the lane did not finish an event.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OutboundError {
    /// The CDR did not answer the composition read.
    #[error("the CDR did not answer the composition read")]
    Cdr(#[source] CdrError),
    /// The composition answer is no canonical composition.
    #[error("the CDR's composition answer is no canonical composition")]
    Body(#[source] openehr_its::json::JsonParseError),
    /// The composition answer names no version in its `ETag`.
    #[error("the CDR's composition answer names no version")]
    NoVersion,
    /// The event names an identifier that is no openEHR identifier.
    #[error("the {what} `{value}` is no openEHR identifier")]
    Identifier {
        /// What the identifier names.
        what: &'static str,
        /// The identifier.
        value: String,
    },
    /// The mapping refused the composition, with the status a read answers.
    #[error("the composition could not be mapped to FHIR ({status})")]
    Mapping {
        /// The status a facade read of it answers.
        status: StatusCode,
    },
    /// A clinical resource names no subject, so it is never emitted.
    #[error("{what} names no subject, so it is not emitted")]
    NoSubject {
        /// The resource, as `[type]/[id]`, or the EHR no subject is bound to.
        what: String,
    },
    /// The identity map could not be read.
    #[error("the identity map could not be read")]
    Identity(#[source] crate::facade::identity::store::StoreError),
    /// The Bundle could not be written as JSON.
    #[error("the transaction Bundle could not be written as JSON")]
    Encode(#[source] serde_json::Error),
    /// The endpoint did not accept the delivery.
    #[error("the FHIR endpoint did not accept the delivery")]
    Sink(#[source] SinkError),
    /// The delivery record could not be read or written.
    #[error("the delivery record could not be read or written")]
    Emitted(#[source] EmittedError),
}

/// Returns the consumer failure `error` is: a fault of the event or of what
/// the CDR holds for it is poisoned, an upstream that did not answer is
/// transient.
fn failure(error: OutboundError) -> ConsumerError {
    let transient = match &error {
        OutboundError::Cdr(_) | OutboundError::Emitted(_) | OutboundError::Identity(_) => true,
        OutboundError::Sink(sink) => sink.is_transient(),
        OutboundError::Mapping { status } => {
            status.is_server_error() && *status != StatusCode::INTERNAL_SERVER_ERROR
        }
        OutboundError::Encode(_)
        | OutboundError::Body(_)
        | OutboundError::NoVersion
        | OutboundError::Identifier { .. }
        | OutboundError::NoSubject { .. } => false,
    };
    if transient {
        ConsumerError::transient(error)
    } else {
        ConsumerError::poisoned(error)
    }
}

/// What one event delivers: the resources to write and the references to
/// retract, and what each composition stands at once it is accepted.
#[derive(Debug, Default)]
struct Delivery {
    /// The resources, each written by a `PUT`.
    resources: Vec<Object>,
    /// The references, each retracted by a `DELETE`.
    retracted: BTreeSet<String>,
    /// Each composition's `(ehr_id, vo_id)` and the references it stands at.
    standing: Vec<(String, String, BTreeSet<String>)>,
}

/// The change-feed consumer that emits FHIR resources.
#[derive(Debug)]
pub struct Outbound {
    /// The facade whose programs and identity map the resources are mapped
    /// with.
    facade: Arc<Facade>,
    /// The CDR the compositions are read from.
    cdr: CdrClient,
    /// The endpoint.
    sink: Sink,
    /// What the lane has delivered.
    emitted: Emitted,
    /// The status each delivered event is counted in, when reported.
    following: Option<Following>,
}

impl Outbound {
    /// Returns the lane `settings` describes over `facade`, reading through
    /// `cdr`.
    ///
    /// # Errors
    /// Returns [`EmittedError`] when the delivery record cannot be opened.
    pub fn start(
        settings: &Settings,
        facade: Arc<Facade>,
        cdr: CdrClient,
    ) -> Result<Self, EmittedError> {
        Ok(Self {
            facade,
            cdr,
            sink: settings.sink.clone(),
            emitted: Emitted::open(&settings.emitted)?,
            following: None,
        })
    }

    /// Returns this lane counting each delivered event in the status
    /// `following` holds.
    #[must_use]
    pub fn with_following(mut self, following: Following) -> Self {
        self.following = Some(following);
        self
    }

    /// Adds what the latest version of the composition `container` of `ehr`
    /// delivers to `delivery`.
    async fn composition(
        &self,
        delivery: &mut Delivery,
        ehr: &EhrId,
        container: &HierObjectId,
    ) -> Result<(), OutboundError> {
        let answered = self
            .cdr
            .composition(ehr, &UidBasedId::HierObjectId(container.clone()), None)
            .await
            .map_err(OutboundError::Cdr)?;
        let headers = match answered.outcome {
            CompositionGetOutcome::Ok { headers, .. } => headers,
            CompositionGetOutcome::NoContent => {
                return self.retract(delivery, ehr.as_str(), container.value());
            }
            CompositionGetOutcome::NotFound { .. } => {
                tracing::debug!(
                    lane = NAME,
                    composition = container.value(),
                    "the CDR knows no such composition; nothing is emitted for it"
                );
                return Ok(());
            }
        };
        let version =
            crate::cdr::optional_version_from_etag("composition_get", headers.etag.as_deref())
                .map_err(OutboundError::Cdr)?
                .ok_or(OutboundError::NoVersion)?;
        let composition: Composition =
            openehr_its::json::from_canonical_json(answered.upstream.body())
                .map_err(OutboundError::Body)?;
        // NOTE: no specification governs this: our own design; a resource of an EHR no subject
        // is bound to could not be attributed to its patient at the endpoint, so none is sent.
        if self
            .facade
            .store()
            .person_of(ehr)
            .map_err(OutboundError::Identity)?
            .is_none()
        {
            return Err(OutboundError::NoSubject {
                what: format!("the EHR {}", ehr.as_str()),
            });
        }
        let resources = crate::facade::handlers::emit::resources_of(
            &self.facade,
            &self.cdr,
            ehr,
            &version,
            &composition,
        )
        .await
        .map_err(|refusal| OutboundError::Mapping {
            status: refusal.into_response().status(),
        })?;
        let mut standing = BTreeSet::new();
        for resource in resources {
            // NOTE: no specification governs this: our own design; a clinical resource with no
            // subject could not be attributed to its patient at the endpoint, so none is sent.
            if resource.lacks_subject() {
                return Err(OutboundError::NoSubject {
                    what: resource.reference(),
                });
            }
            standing.insert(resource.reference());
            delivery.resources.push(resource.body);
        }
        let before = self
            .emitted
            .delivered(container.value())
            .map_err(OutboundError::Emitted)?;
        delivery
            .retracted
            .extend(before.difference(&standing).cloned());
        delivery.standing.push((
            ehr.as_str().to_owned(),
            container.value().to_owned(),
            standing,
        ));
        Ok(())
    }

    /// Retracts every resource the composition `vo_id` of `ehr_id` was last
    /// delivered as.
    fn retract(
        &self,
        delivery: &mut Delivery,
        ehr_id: &str,
        vo_id: &str,
    ) -> Result<(), OutboundError> {
        let before = self
            .emitted
            .delivered(vo_id)
            .map_err(OutboundError::Emitted)?;
        delivery.retracted.extend(before);
        delivery
            .standing
            .push((ehr_id.to_owned(), vo_id.to_owned(), BTreeSet::new()));
        Ok(())
    }

    /// Adds the retraction of everything the erasure `event` names: each
    /// erased composition, and every composition of an erased EHR.
    fn erase(&self, delivery: &mut Delivery, event: &Event) -> Result<(), OutboundError> {
        let Some(ehr) = event.ehr_id.as_ref() else {
            return Ok(());
        };
        for version in &event.versions {
            match (version.kind.as_str(), version.vo_id) {
                (COMPOSITION, Some(vo_id)) => {
                    self.retract(delivery, ehr.as_str(), &vo_id.to_string())?;
                }
                (EHR, _) => {
                    for vo_id in self
                        .emitted
                        .compositions(ehr.as_str())
                        .map_err(OutboundError::Emitted)?
                    {
                        self.retract(delivery, ehr.as_str(), &vo_id)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Sends `delivery` and, once the endpoint accepted it, records what
    /// each composition stands at.
    async fn send(&self, delivery: Delivery) -> Result<(), OutboundError> {
        let retracted: Vec<&String> = delivery
            .retracted
            .iter()
            .filter(|reference| {
                !delivery
                    .standing
                    .iter()
                    .any(|(_, _, standing)| standing.contains(*reference))
            })
            .collect();
        if !delivery.resources.is_empty() || !retracted.is_empty() {
            let bundle = transaction(delivery.resources, &retracted);
            let bytes = serde_json::to_vec(&bundle).map_err(OutboundError::Encode)?;
            self.sink
                .deliver(&bytes)
                .await
                .map_err(OutboundError::Sink)?;
        }
        for (ehr_id, vo_id, standing) in &delivery.standing {
            self.emitted
                .record(ehr_id, vo_id, standing)
                .map_err(OutboundError::Emitted)?;
        }
        Ok(())
    }
}

/// Returns the `transaction` Bundle that writes `resources` and retracts
/// `retracted` (<https://hl7.org/fhir/R4/bundle.html>, `Bundle.entry.request`).
fn transaction(resources: Vec<Object>, retracted: &[&String]) -> Value {
    let text = |value: &str| Value::String(value.to_owned());
    let entry = |method: &str, url: String, resource: Option<Object>| {
        let request = Object::from([
            (String::from("method"), text(method)),
            (String::from("url"), Value::String(url)),
        ]);
        let mut entry = Object::from([(String::from("request"), Value::Object(request))]);
        if let Some(resource) = resource {
            entry.insert(String::from("resource"), Value::Object(resource));
        }
        Value::Object(entry)
    };
    let written = resources.into_iter().map(|resource| {
        let name = |key: &str| {
            resource
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let url = format!("{}/{}", name("resourceType"), name("id"));
        entry("PUT", url, Some(resource))
    });
    let removed = retracted
        .iter()
        .map(|reference| entry("DELETE", (*reference).clone(), None));
    Value::Object(Object::from([
        (String::from("resourceType"), text("Bundle")),
        (String::from("type"), text("transaction")),
        (
            String::from("entry"),
            Value::Array(written.chain(removed).collect()),
        ),
    ]))
}

/// Returns the versioned object id `vo_id` names.
fn container(vo_id: uuid::Uuid) -> Result<HierObjectId, OutboundError> {
    HierObjectId::new(vo_id.to_string()).map_err(|_refused| OutboundError::Identifier {
        what: "versioned object id",
        value: vo_id.to_string(),
    })
}

#[async_trait::async_trait]
impl Consumer for Outbound {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        let mut delivery = Delivery::default();
        if event.erasure {
            self.erase(&mut delivery, event).map_err(failure)?;
        } else if let Some(ehr) = event.ehr_id.as_ref() {
            for vo_id in event.compositions() {
                let container = container(vo_id).map_err(failure)?;
                self.composition(&mut delivery, ehr, &container)
                    .await
                    .map_err(failure)?;
            }
        }
        self.send(delivery).await.map_err(failure)?;
        if let Some(following) = self.following.as_ref() {
            following.delivered();
        }
        Ok(())
    }
}
