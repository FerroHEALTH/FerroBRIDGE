// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chained `patient.identifier` and `subject.identifier` values of one
//! search, resolved to the EHRs they name.
//!
//! Each value runs the configured identifier query over `POST /query/aql`
//! (`crate::facade::search::identifier`). Each party it finds is the patient
//! whose logical id is the party's uid, and that patient's EHR comes from the
//! identity map or the CDR's lookup by subject, as every subject of the
//! facade does (`crate::facade::ehr`). A bare value matches every patient
//! that carries it (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.10), and
//! a chain answers the resources of every matching target (§3.1.1.4.15), so
//! the search reads the EHR of each party. An identifier no party carries,
//! or a party with no EHR, adds nothing. No specification governs the
//! identifier-to-EHR resolution: our own design.

use http::StatusCode;
use openehr_its::rest::client::ErrorBody;
use openehr_its::rest::generated::query::client::QueryExecuteAdhocQueryBodyOutcome;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::ehr;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::write;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::search::identifier;
use crate::facade::search::value::Token;
use crate::facade::status;

/// Returns the EHRs of every patient one identifier value of the chained
/// parameter `name` names, in the order the identifier query returns them.
pub(super) async fn ehrs_of(
    facade: &Facade,
    client: &CdrClient,
    name: &str,
    token: &Token,
) -> Result<Vec<EhrId>, Refusal> {
    let search = facade.identifier_search().ok_or_else(|| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "{name} was read with no facade.identifier_search configured"
            )),
        )
    })?;
    let request = search.request(token).map_err(|error| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::NotSupported)
                .diagnosing(format!("{name}: {error}"))
                .at(name.to_owned()),
        )
    })?;
    let answered = client
        .query_aql(&request)
        .await
        .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
    let rows = match answered.outcome {
        QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => body.rows,
        QueryExecuteAdhocQueryBodyOutcome::BadRequest { .. }
        | QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { .. } => {
            return Err(write::refuse(&refused(&answered.upstream)));
        }
    };
    let parties = identifier::parties(&rows).map_err(|error| {
        write::refuse(&status::Answer::new(status::UPSTREAM, error.to_string()))
    })?;
    let mut ehrs = Vec::new();
    for party in &parties {
        if let Some(found) = ehr_of_party(facade, client, party).await?
            && !ehrs.contains(&found)
        {
            ehrs.push(found);
        }
    }
    Ok(ehrs)
}

/// Returns the EHR of the patient whose logical id is `party`, `None` when
/// the CDR holds none.
async fn ehr_of_party(
    facade: &Facade,
    client: &CdrClient,
    party: &str,
) -> Result<Option<EhrId>, Refusal> {
    let link = &facade.settings().subject;
    // NOTE: no specification governs this: our own design, a party uid no person key
    // can hold names no subject the facade links, so it matches nothing.
    let Ok(person) = link.person_of_patient(party) else {
        return Ok(None);
    };
    match ehr::resolve(client, facade.store(), &person, link, ehr::Policy::Existing).await {
        Ok(found) => Ok(Some(found)),
        Err(ehr::EhrError::Absent { .. }) => Ok(None),
        Err(ehr::EhrError::Client { ref source }) => {
            Err(write::refuse(&status::of_client_error(source)))
        }
        Err(error) => Err(reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(crate::facade::outcome::chain(&error)),
        )),
    }
}

/// Returns the answer a CDR refusal of the identifier query renders as: the
/// query rows of the status table, the diagnostics naming the extension.
fn refused(upstream: &crate::cdr::error::Upstream) -> status::Answer {
    let row = if upstream.status() == StatusCode::REQUEST_TIMEOUT {
        status::QUERY_TIMEOUT
    } else {
        status::QUERY_REFUSED
    };
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    status::Answer::new(
        row,
        format!(
            "the CDR refused the {} identifier query: {}",
            identifier::EXTENSION,
            status::diagnostics(upstream.status(), &body)
        ),
    )
}
