// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The layer that records each facade access an authenticated caller makes.
//!
//! The layer sits inside the `ferrohealth-smart` guard, so the request it
//! sees carries the caller's [`Access`]. Before the request runs it takes the
//! record's place in the spool, and a spool with no room refuses the request
//! with a `503` before the CDR is reached. The request then runs with a
//! [`Reach`] around it, and once it is answered the layer fills the place
//! with the record of Regulation (EU) 2025/327 Annex II 3.2: who accessed
//! (a, b), the categories of what the request reached (c), when (d) and the
//! CDR the data come from (e). A record that cannot be stored withholds the
//! answer: ITI TF-2 §3.20.4.1.1 has the record kept, and the `ihe-atna`
//! recorder contract uses an answer only once its record is stored.

use std::sync::Arc;

use axum::extract::MatchedPath;
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::Response;
use ehds_logging::classify::Basis;
use ehds_logging::classify::Evidence;
use ehds_logging::record::AccessRecord;
use ehds_logging::record::Accessor;
use ehds_logging::record::Action;
use ehds_logging::record::Coded;
use ehds_logging::record::DataSubject;
use ehds_logging::record::EhrAt;
use ehds_logging::record::Origin;
use ehds_logging::record::Outcome;
use ehds_logging::record::PatientIdentifier;
use ehds_logging::record::PatientLookup;
use ehds_logging::record::Professional;
use ehds_logging::sink::AccessSink as _;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Caller;
use http::StatusCode;
use secrecy::SecretString;

use crate::audit::AccessLog;
use crate::cdr::ids::EhrId;
use crate::cdr::reach::Reach;
use crate::cdr::reach::Reached;
use crate::facade::Facade;
use crate::facade::guard::Interaction;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;

/// What the layer records through: the lane, and the facade whose identity
/// map names the patient behind an EHR.
#[derive(Debug)]
pub struct Recording {
    /// The access-record lane.
    log: Arc<AccessLog>,
    /// The facade.
    facade: Arc<Facade>,
}

impl Recording {
    /// Returns the recording of `facade`'s accesses through `log`.
    #[must_use]
    pub const fn new(log: Arc<AccessLog>, facade: Arc<Facade>) -> Self {
        Self { log, facade }
    }
}

/// One access as the request states it, read before the request runs.
#[derive(Debug)]
struct Pending {
    /// The interaction.
    interaction: Interaction,
    /// Who asked.
    accessor: Accessor,
    /// What the request asked for.
    request: ehds_logging::record::Request,
}

/// Records the access `request` makes, refusing it when its record has no
/// place.
pub async fn record(
    State(recording): State<Arc<Recording>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(asked) = asked(&request) else {
        return next.run(request).await;
    };
    let place = match recording.log.sink().reserve().await {
        Ok(place) => place,
        Err(error) => {
            tracing::warn!(error = %ferrohealth_server::run::chain(&error), "an access was refused: its record has no place");
            return unrecorded("the access could not be recorded, so it was not made");
        }
    };
    let reach = Reach::default();
    let response = reach.clone().around(next.run(request)).await;
    let record = recording.record(asked, &reach.reached(), response.status());
    match place.store(record).await {
        Ok(()) => response,
        Err(error) => {
            tracing::warn!(error = %ferrohealth_server::run::chain(&error), "an answer was withheld: its access record could not be stored");
            unrecorded("the access record could not be stored, so the answer is withheld")
        }
    }
}

/// Returns the `503` of an access the recorder could not take.
fn unrecorded(diagnostics: &str) -> Response {
    reply::issues(
        StatusCode::SERVICE_UNAVAILABLE,
        &[Issue::error(IssueType::Transient).diagnosing(diagnostics)],
        &[],
    )
}

/// Returns the access `request` makes, or `None` for a request that is no
/// recorded access: one outside the FHIR base, the conformance statement,
/// the SMART discovery document, a `$validate` dry run, or one no
/// authenticated caller made.
fn asked(request: &Request) -> Option<Pending> {
    let template = request.extensions().get::<MatchedPath>()?.as_str();
    let classified =
        crate::facade::guard::classify(request.method(), template, request.uri().path())?;
    match classified.interaction {
        Interaction::Capabilities
        | Interaction::Discovery
        | Interaction::Validate
        | Interaction::Unclassified => return None,
        Interaction::Create
        | Interaction::Read
        | Interaction::Vread
        | Interaction::Update
        | Interaction::Delete
        | Interaction::History
        | Interaction::Document
        | Interaction::Search
        | Interaction::Transaction
        | Interaction::Batch
        | Interaction::Operation => {}
    }
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2(b): the record names a natural person, and
    // `[audit]` needs `[auth]`, so a request no caller made is no access this layer records.
    let caller = request.extensions().get::<Access>()?.caller()?;
    let resource = match (classified.resource_type, classified.id) {
        (Some(resource_type), Some(id)) => Some(format!("{resource_type}/{id}")),
        _ => None,
    };
    let query = (classified.interaction == Interaction::Search).then(|| {
        SecretString::from(
            request
                .uri()
                .path_and_query()
                .map_or_else(|| request.uri().path().to_owned(), ToString::to_string),
        )
    });
    let id = request
        .headers()
        .get(ferrohealth_server::request_id::HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    Some(Pending {
        interaction: classified.interaction,
        accessor: accessor(caller),
        request: ehds_logging::record::Request {
            id,
            operation: classified.interaction.code().to_owned(),
            operation_system: None,
            resource,
            query,
            stored_query: None,
            client_address: None,
            correlation: None,
        },
    })
}

/// Returns who accessed (Annex II 3.2(a), (b)) as the guard verified them:
/// the token's issuer, subject, client and `jti`, or the Basic user.
fn accessor(caller: &Caller) -> Accessor {
    Accessor {
        issuer: caller.issuer().map(str::to_owned),
        subject: caller.subject().to_owned(),
        client_id: caller.client_id().map(str::to_owned),
        audience: None,
        provider: None,
        acting: None,
        roles: caller
            .roles()
            .iter()
            .map(|role| Coded {
                system: None,
                code: role.clone(),
            })
            .collect(),
        token_id: caller.claim_text("jti").map(str::to_owned),
        assurance: None,
        professional: Professional::default(),
        alt_id: None,
        purposes: Vec::new(),
        policies: Vec::new(),
        relayed: None,
        // NOTE: no specification governs this: our own design; the bridge binds no
        // party and relays no actor chain or representation of its own.
        binding: None,
        actors: Vec::new(),
        represents: None,
    }
}

impl Recording {
    /// Returns the record of `asked`, which reached `reached` and was answered
    /// with `status`.
    fn record(&self, asked: Pending, reached: &Reached, status: StatusCode) -> AccessRecord {
        let action = action(asked.interaction, reached);
        let basis = match action {
            Action::Create | Action::Update | Action::Delete => Basis::Written,
            _ => Basis::Returned,
        };
        let categories = self
            .log
            .categories()
            .classify(&Evidence::reached(basis, reached.objects()));
        // NOTE: Regulation (EU) 2025/327 Annex II 3.2(e): every datum the facade reaches is
        // the CDR's, so the CDR is the one origin of an access that reached any.
        let origins = if reached.objects().is_empty() && reached.ehrs().is_empty() {
            Vec::new()
        } else {
            vec![Origin {
                endpoint: Some(self.log.origin().to_owned()),
                node: None,
                system_id: None,
                status: None,
                rows: None,
                version: None,
                created_ehr: None,
                categories: None,
            }]
        };
        let recorded = jiff::Timestamp::now();
        let keys = || origins.iter().filter_map(Origin::key);
        let retention = self.log.retention().retain(recorded, &categories, keys());
        let labels = self.log.labels().label(&categories, keys());
        AccessRecord {
            action,
            recorded,
            outcome: outcome(status),
            outcome_description: None,
            accessor: asked.accessor,
            subject: DataSubject {
                patient: None,
                ehrs: reached
                    .ehrs()
                    .iter()
                    .map(|ehr_id| EhrAt {
                        endpoint: self.log.origin().to_owned(),
                        ehr_id: ehr_id.clone(),
                        patient: self.patient_of(ehr_id),
                    })
                    .collect(),
                population: None,
            },
            categories,
            delivered: None,
            origins,
            origin_count: None,
            request: asked.request,
            retention,
            labels,
            emergency: None,
            inserted: None,
            transformations: Vec::new(),
        }
    }

    /// Returns the patient the identity map binds to the EHR `ehr_id`.
    fn patient_of(&self, ehr_id: &str) -> PatientLookup {
        let Ok(ehr) = EhrId::new(ehr_id) else {
            return PatientLookup::Unavailable;
        };
        match self.facade.store().person_of(&ehr) {
            Ok(Some(person)) => PatientLookup::Found(vec![PatientIdentifier {
                namespace: Some(person.namespace().to_owned()),
                value: SecretString::from(person.id().to_owned()),
            }]),
            Ok(None) => PatientLookup::NotFound,
            Err(error) => {
                tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the identity map could not name the patient of an access record");
                PatientLookup::Unavailable
            }
        }
    }
}

/// Returns what an access of `interaction` that reached `reached` did.
///
/// A transaction, a batch and an operation are no REST interaction of their
/// own in BALP, so each is recorded by what it did: a write when it wrote,
/// a create when every write was a first version, and a read otherwise (no
/// specification governs this: our own design).
const fn action(interaction: Interaction, reached: &Reached) -> Action {
    match interaction {
        Interaction::Search => Action::Query,
        Interaction::Create => Action::Create,
        Interaction::Update => Action::Update,
        Interaction::Delete => Action::Delete,
        Interaction::Transaction | Interaction::Batch | Interaction::Operation
            if reached.wrote() =>
        {
            if reached.only_created() {
                Action::Create
            } else {
                Action::Update
            }
        }
        _ => Action::Read,
    }
}

/// Returns how an access answered with `status` ended.
///
/// FHIR R4 `audit-event-outcome` leaves the meaning of each failure to the
/// recording application: a refusal the caller can correct is minor, a
/// failure of the bridge or the CDR serious (no specification governs the
/// split: our own design).
fn outcome(status: StatusCode) -> Outcome {
    if status.is_success() || status.is_redirection() {
        Outcome::Success
    } else if status.is_client_error() {
        Outcome::MinorFailure
    } else {
        Outcome::SeriousFailure
    }
}
