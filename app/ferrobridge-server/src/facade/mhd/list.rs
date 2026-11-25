// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `List`: Find Document Lists [ITI-66] and the read of one submission set.
//!
//! A submission set is the CONTRIBUTION that committed the latest version of
//! one or more document compositions: its `id` the CONTRIBUTION's uid, its
//! `identifier` that uid as a URN, its `date` and `meta.lastUpdated` the
//! commit time, its `ihe-sourceId` the committing system, and an entry per
//! composition naming its `DocumentReference`. Each submission set holds the
//! documents of one EHR, so its `subject` is that EHR's patient. This server
//! keeps no Folder, so a search for one finds none. No specification maps a
//! CONTRIBUTION to a submission set: our own design.

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Object;
use serde_json::Value;

use super::DOCUMENT_REFERENCE;
use super::Found;
use super::LIST;
use super::SUBMISSION_SET_PROFILE;
use super::listing;
use super::listing::Listed;
use super::params;
use super::params::Span;
use super::params::Values;
use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::reply::Refusal;

/// The `rest.resource.documentation` of `List`.
pub(super) const DOCUMENTATION: &str = "The IHE MHD 4.2.3 Document Responder: one SubmissionSet per CONTRIBUTION that committed the latest version of a document composition, its entries the DocumentReferences of those documents; no Folder is kept. Read, and Find Document Lists [ITI-66] by GET and POST; a parameter this server does not support is ignored unless Prefer: handling=strict asks for a refusal.";

/// The search parameters of `List` (ITI TF-2: 3.66.4.1.2.1).
pub(super) const DECLARED: &[params::Declared] = &[
    (
        "_id",
        "http://hl7.org/fhir/SearchParameter/Resource-id",
        "token",
    ),
    (
        "_lastUpdated",
        "http://hl7.org/fhir/SearchParameter/Resource-lastUpdated",
        "date",
    ),
    (
        "code",
        "http://hl7.org/fhir/SearchParameter/clinical-code",
        "token",
    ),
    (
        "date",
        "http://hl7.org/fhir/SearchParameter/clinical-date",
        "date",
    ),
    (
        "designationType",
        "https://profiles.ihe.net/ITI/MHD/SearchParameter/List-DesignationType",
        "token",
    ),
    (
        "identifier",
        "http://hl7.org/fhir/SearchParameter/clinical-identifier",
        "token",
    ),
    (
        "patient",
        "http://hl7.org/fhir/SearchParameter/clinical-patient",
        "reference",
    ),
    (
        "patient.identifier",
        "http://hl7.org/fhir/SearchParameter/Patient-identifier",
        "token",
    ),
    (
        "source.given",
        "http://hl7.org/fhir/SearchParameter/individual-given",
        "string",
    ),
    (
        "source.family",
        "http://hl7.org/fhir/SearchParameter/individual-family",
        "string",
    ),
    (
        "sourceId",
        "https://profiles.ihe.net/ITI/MHD/SearchParameter/List-SourceId",
        "token",
    ),
    (
        "status",
        "http://hl7.org/fhir/SearchParameter/List-status",
        "token",
    ),
];

/// The code system of the MHD list types.
const LIST_TYPES: &str = "https://profiles.ihe.net/ITI/MHD/CodeSystem/MHDlistTypes";

/// The extension naming the source of a submission set.
const SOURCE_ID: &str = "https://profiles.ihe.net/ITI/MHD/StructureDefinition/ihe-sourceId";

/// The extension naming the designation type of a submission set.
const DESIGNATION_TYPE: &str =
    "https://profiles.ihe.net/ITI/MHD/StructureDefinition/ihe-designationType";

/// The R4 code system of `List.status`.
const STATUS: &str = "http://hl7.org/fhir/list-status";

/// The `Identifier.system` of a URI value.
const URI_SYSTEM: &str = "urn:ietf:rfc:3986";

/// `GET [base]/List/[id]`: the submission set of the CONTRIBUTION `id`.
pub(super) async fn read(
    facade: &Facade,
    access: &Access,
    client: &CdrClient,
    id: &str,
) -> Result<Object, Refusal> {
    // NOTE: a CONTRIBUTION uid is a UUID (ITS-REST 1.1.0 `contribution_get`), so any other id
    // names no submission set.
    if uuid::Uuid::try_parse(id).is_err() {
        return Err(super::unknown(LIST, id));
    }
    let listed = listing::listed(facade, client, listing::Scope::Contribution(id)).await?;
    let Some(first) = listed.first() else {
        return Err(super::unknown(LIST, id));
    };
    crate::facade::compartment::admit_ehr(facade, access, Permission::Read, LIST, &first.ehr_id)
        .await?;
    let held: Vec<&Listed> = listed
        .iter()
        .filter(|one| one.ehr_id == first.ehr_id)
        .collect();
    let (object, _) = submission_set(facade, &held).await?;
    Ok(object)
}

/// Find Document Lists [ITI-66]: the submission sets of the EHRs the search
/// reaches that match every parameter applied.
pub(super) async fn search(
    facade: &Facade,
    access: &Access,
    client: &CdrClient,
    pairs: &[(String, String)],
    strict: bool,
) -> Result<Found, Refusal> {
    let request = params::read(pairs, DECLARED, strict)?;
    let named = patients(facade, client, &request).await?;
    let ehrs = super::reach(facade, access, LIST, named).await?;
    let mut found = Vec::new();
    for ehr_id in &ehrs {
        let listed = listing::listed(facade, client, listing::Scope::Ehr(ehr_id)).await?;
        let mut contributions: Vec<&str> =
            listed.iter().map(|one| one.contribution.as_str()).collect();
        contributions.sort_unstable();
        contributions.dedup();
        for contribution in contributions {
            let held: Vec<&Listed> = listed
                .iter()
                .filter(|one| one.contribution == contribution)
                .collect();
            let (object, json) = submission_set(facade, &held).await?;
            if request
                .applied
                .iter()
                .all(|occurrence| matches(occurrence.name, &occurrence.values, &json))
            {
                found.push(object);
            }
        }
    }
    Ok(Found::paged(found, request))
}

/// Returns the EHRs the `patient` and `patient.identifier` occurrences of
/// `request` name together, `None` when it names neither.
///
/// A `patient` value is a `Patient` reference, relative, absolute under
/// this server's base or a bare id (R4 `search.html` §3.1.1.4.11); a
/// reference to another server names no patient of this one.
pub(super) async fn patients(
    facade: &Facade,
    client: &CdrClient,
    request: &params::Read,
) -> Result<Option<Vec<EhrId>>, Refusal> {
    let mut named: Option<Vec<EhrId>> = None;
    for occurrence in &request.applied {
        let mut found = Vec::new();
        match (occurrence.name, &occurrence.values) {
            ("patient", Values::References(references)) => {
                for reference in references {
                    if let Some(id) = patient_id(facade, reference)
                        && let Some(ehr) =
                            crate::facade::handlers::search::identifier::ehr_of_party(
                                facade, client, &id,
                            )
                            .await?
                    {
                        found.push(ehr);
                    }
                }
            }
            ("patient.identifier", Values::Tokens(tokens)) => {
                for token in tokens {
                    found.extend(
                        crate::facade::handlers::search::identifier::ehrs_of(
                            facade,
                            client,
                            "patient.identifier",
                            token,
                        )
                        .await?,
                    );
                }
            }
            _ => continue,
        }
        named = Some(match named {
            None => found,
            Some(earlier) => earlier
                .into_iter()
                .filter(|ehr| found.contains(ehr))
                .collect(),
        });
    }
    Ok(named)
}

/// Returns the logical id a `patient` value names on this server.
fn patient_id(facade: &Facade, reference: &str) -> Option<String> {
    let base = facade.settings().base_url.trim_end_matches('/');
    let relative = reference
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(reference);
    match relative.split_once('/') {
        Some(("Patient", id)) if !id.contains('/') => Some(id.to_owned()),
        Some(_) => None,
        None => Some(relative.to_owned()),
    }
}

/// Returns the submission set of `held`, the documents one CONTRIBUTION
/// committed in one EHR, and its JSON for matching.
async fn submission_set(facade: &Facade, held: &[&Listed]) -> Result<(Object, Value), Refusal> {
    let Some(first) = held.first() else {
        return Err(super::internal(String::from(
            "a SubmissionSet was written with no document",
        )));
    };
    let subject = crate::facade::handlers::render::subject_of(facade, &first.ehr_id).await?;
    let entries: Vec<Value> = held
        .iter()
        .map(|one| {
            serde_json::json!({"item": {"reference": format!("{DOCUMENT_REFERENCE}/{}", one.id.as_str())}})
        })
        .collect();
    let mut json = serde_json::json!({
        "resourceType": LIST,
        "id": first.contribution,
        "meta": {"profile": [SUBMISSION_SET_PROFILE], "lastUpdated": first.committed},
        "extension": [{"url": SOURCE_ID, "valueIdentifier": {"value": first.system_id}}],
        "identifier": [{
            "use": "official",
            "system": URI_SYSTEM,
            "value": format!("urn:uuid:{}", first.contribution),
        }],
        "status": "current",
        "mode": "working",
        "code": {"coding": [{"system": LIST_TYPES, "code": "submissionset"}]},
        "date": first.committed,
        "entry": entries,
    });
    if let (Some(subject), Value::Object(members)) = (subject, &mut json) {
        let subject = serde_json::to_value(&subject)
            .map_err(|error| super::internal(format!("a subject does not encode: {error}")))?;
        members.insert(String::from("subject"), subject);
    }
    let object: Object = serde_json::from_value(json.clone()).map_err(|error| {
        super::internal(format!("a SubmissionSet could not be written: {error}"))
    })?;
    Ok((object, json))
}

/// Whether the submission set `json` matches one occurrence of `name`; it
/// names no `source` resource, so `source.given` and `source.family` match
/// none.
fn matches(name: &str, values: &Values, json: &Value) -> bool {
    let at = |pointer: &str| json.pointer(pointer);
    let extension = |url: &str| {
        json.get("extension")
            .and_then(Value::as_array)
            .and_then(|extensions| {
                extensions
                    .iter()
                    .find(|extension| extension.get("url").and_then(Value::as_str) == Some(url))
            })
    };
    match (name, values) {
        ("_id", Values::Tokens(tokens)) => tokens
            .iter()
            .any(|token| token.code() == at("/id").and_then(Value::as_str)),
        ("_lastUpdated", Values::Dates(dates)) => params::dates(
            dates,
            at("/meta/lastUpdated")
                .and_then(Value::as_str)
                .and_then(Span::of_date),
        ),
        ("code", Values::Tokens(tokens)) => params::tokens_code(tokens, at("/code").into_iter()),
        ("date", Values::Dates(dates)) => params::dates(
            dates,
            at("/date").and_then(Value::as_str).and_then(Span::of_date),
        ),
        ("designationType", Values::Tokens(tokens)) => params::tokens_code(
            tokens,
            extension(DESIGNATION_TYPE)
                .and_then(|found| found.get("valueCodeableConcept"))
                .into_iter(),
        ),
        ("identifier", Values::Tokens(tokens)) => params::tokens_identifier(
            tokens,
            at("/identifier")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        ),
        ("sourceId", Values::Tokens(tokens)) => params::tokens_identifier(
            tokens,
            extension(SOURCE_ID)
                .and_then(|found| found.get("valueIdentifier"))
                .into_iter(),
        ),
        ("status", Values::Tokens(tokens)) => {
            params::tokens_plain(tokens, STATUS, at("/status").and_then(Value::as_str))
        }
        ("patient" | "patient.identifier", _) => true,
        _ => false,
    }
}
