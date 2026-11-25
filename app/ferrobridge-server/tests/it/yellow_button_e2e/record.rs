// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient record the journey reads, written into the reference CDR
//! through its own routes: the archetypes and templates on the Definition API
//! (ITS-REST 1.1.0, `definition.html`, and the CDR's ADL 1.4 archetype
//! route), the patients on the Demographic API, the EHR and the patient
//! summary composition on the EHR API.

use std::error::Error;

use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_PATIENT_SUMMARY_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ADL2;
use http::StatusCode;
use http::header;

use crate::facade::summary::NAMESPACE;
use crate::facade::summary::composition;
use crate::facade::summary::person;

/// The issuer of the identifier that names each patient by the subject id of
/// their EHR, which FerroSMART pairs a FHIR patient with an EHR by.
pub(super) const SUBJECT_SYSTEM: &str = "urn:ferrobridge:journey:ehr-subject";

/// The two patients the journey seeds: the one with a patient summary and a
/// second one, whose token is refused the first one's record.
#[derive(Debug)]
pub(super) struct Patients {
    /// The patient whose EHR holds the patient summary composition.
    pub(super) patient: String,
    /// The EHR of [`Self::patient`].
    pub(super) ehr_id: String,
    /// A second patient, with no EHR.
    pub(super) other: String,
}

/// Uploads the definitions, then writes the two patients and the first
/// one's EHR and patient summary into the CDR at `base`.
pub(super) async fn seed(base: &str) -> Result<Patients, Box<dyn Error>> {
    let client = reqwest::Client::new();
    for (file, source) in CKM_MEDICATION_ARCHETYPES
        .iter()
        .chain(CKM_PATIENT_SUMMARY_ARCHETYPES.iter())
    {
        let response = client
            .post(format!("{base}/definition/archetype/adl1.4"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(*source)
            .send()
            .await?;
        expect(response, &[StatusCode::CREATED], file).await?;
    }
    for (name, source, accepted) in [
        (
            "the patient PERSON",
            FERROEHR_PATIENT_PERSON_ADL2,
            &[StatusCode::CREATED, StatusCode::CONFLICT][..],
        ),
        (
            "the patient summary template",
            FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE,
            &[StatusCode::CREATED][..],
        ),
    ] {
        let response = client
            .post(format!("{base}/definition/template/adl2"))
            .header(header::CONTENT_TYPE, "text/plain")
            .body(source)
            .send()
            .await?;
        expect(response, accepted, name).await?;
    }
    let patient = create_patient(&client, base, "SYN-PAT-2018-A").await?;
    let other = create_patient(&client, base, "SYN-PAT-2018-B").await?;
    let ehr_id = create_ehr(&client, base, &patient).await?;
    let response = client
        .post(format!("{base}/ehr/{ehr_id}/composition"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(composition()?.to_string())
        .send()
        .await?;
    expect(
        response,
        &[StatusCode::CREATED],
        "the patient summary composition",
    )
    .await?;
    Ok(Patients {
        patient,
        ehr_id,
        other,
    })
}

/// Returns `response` when its status is one of `accepted`, and an error
/// naming `what` with the body otherwise.
async fn expect(
    response: reqwest::Response,
    accepted: &[StatusCode],
    what: &str,
) -> Result<reqwest::Response, Box<dyn Error>> {
    let status = StatusCode::from_u16(response.status().as_u16())?;
    if accepted.contains(&status) {
        return Ok(response);
    }
    Err(format!(
        "the CDR answered {what} with {status}: {}",
        response.text().await?
    )
    .into())
}

/// Returns the value of the `ETag` of `response`, its quotes removed.
fn etag(response: &reqwest::Response) -> Result<String, Box<dyn Error>> {
    let value = response
        .headers()
        .get(header::ETAG)
        .ok_or("an ETag")?
        .to_str()?;
    Ok(value.trim_start_matches("W/").trim_matches('"').to_owned())
}

/// Creates a patient `PERSON` whose identifier first carries `number`, then
/// updates it to carry its own object id under [`SUBJECT_SYSTEM`], and
/// returns that object id.
async fn create_patient(
    client: &reqwest::Client,
    base: &str,
    number: &str,
) -> Result<String, Box<dyn Error>> {
    let mut body = person("at11");
    if let Some(members) = body.as_object_mut() {
        members.remove("uid");
    }
    body["details"]["items"][0]["value"]["id"] = serde_json::json!(number);
    let response = client
        .post(format!("{base}/demographic/person"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("Prefer", "return=minimal")
        .body(body.to_string())
        .send()
        .await?;
    let response = expect(response, &[StatusCode::CREATED], "a patient PERSON").await?;
    let version = etag(&response)?;
    let uid = version
        .split("::")
        .next()
        .ok_or("a version uid")?
        .to_owned();
    body["details"]["items"][0]["value"]["issuer"] = serde_json::json!(SUBJECT_SYSTEM);
    body["details"]["items"][0]["value"]["id"] = serde_json::json!(uid);
    let response = client
        .put(format!("{base}/demographic/person/{uid}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, format!("\"{version}\""))
        .header("Prefer", "return=minimal")
        .body(body.to_string())
        .send()
        .await?;
    expect(
        response,
        &[StatusCode::OK, StatusCode::NO_CONTENT],
        "the patient PERSON's update",
    )
    .await?;
    Ok(uid)
}

/// Creates the EHR of `patient`, linked by its object id in [`NAMESPACE`],
/// and returns its id.
async fn create_ehr(
    client: &reqwest::Client,
    base: &str,
    patient: &str,
) -> Result<String, Box<dyn Error>> {
    let status = serde_json::json!({
        "_type": "EHR_STATUS",
        "name": {"_type": "DV_TEXT", "value": "EHR Status"},
        "archetype_node_id": "openEHR-EHR-EHR_STATUS.generic.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": "openEHR-EHR-EHR_STATUS.generic.v1"},
            "rm_version": "1.1.0"
        },
        "subject": {
            "_type": "PARTY_SELF",
            "external_ref": {
                "_type": "PARTY_REF",
                "namespace": NAMESPACE,
                "type": "PERSON",
                "id": {"_type": "HIER_OBJECT_ID", "value": patient}
            }
        },
        "is_queryable": true,
        "is_modifiable": true
    });
    let response = client
        .post(format!("{base}/ehr"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("Prefer", "return=minimal")
        .body(status.to_string())
        .send()
        .await?;
    let response = expect(response, &[StatusCode::CREATED], "the EHR").await?;
    etag(&response)
}
