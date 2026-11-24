// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading one App State body against `smart-app-state-basic` and the rules
//! of SMART App Launch 2.2.0, Persisting App State, §Create and §Updates.

use fhir_types::codec::Json as _;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::r4::basic::Basic;
use fhir_types::r4::extension::ExtensionValue;
use http::StatusCode;

use crate::facade::app_state::BASIC;
use crate::facade::app_state::MAX_BODY;
use crate::facade::app_state::StateCode;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::answer::invalid;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The resource types a subject may name: "The EHR SHALL support at least
/// `Patient`, `Practitioner`, `PractitionerRole`, `RelatedPerson`, `Person`"
/// (§Create).
pub const SUBJECT_TYPES: [&str; 5] = [
    "Patient",
    "Practitioner",
    "PractitionerRole",
    "RelatedPerson",
    "Person",
];

/// What a body is read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent<'a> {
    /// `POST [base]/Basic`: no `id` and no `meta.versionId`.
    Create,
    /// `PUT [base]/Basic/[id]`: `Basic.id` present and matching the path.
    Update(&'a str),
}

/// One App State body, read and checked.
#[derive(Debug, Clone)]
pub struct State {
    /// The resource as the client sent it.
    pub resource: Object,
    /// Its state code.
    pub code: StateCode,
    /// `Basic.subject.reference`, `None` for global state.
    pub subject: Option<String>,
}

/// Reads `body` as App State for `intent`, the subject checked to be an
/// absolute reference under `base`.
///
/// # Errors
/// Returns `413` for a body over [`MAX_BODY`], and `400` naming the element
/// for a body that is no R4 `Basic`, breaks `smart-app-state-basic`, or
/// breaks a rule of §Create or §Updates.
pub fn read(body: &[u8], intent: Intent<'_>, base: &str) -> Result<State, Refusal> {
    if body.len() > MAX_BODY {
        return Err(reply::refusal(
            StatusCode::PAYLOAD_TOO_LARGE,
            Issue::error(IssueType::TooLong).diagnosing(format!(
                "an App State resource is at most {MAX_BODY} bytes as serialized"
            )),
        ));
    }
    let resource: Object = serde_json::from_slice(body).map_err(|error| {
        invalid(
            IssueType::Structure,
            BASIC,
            format!("the body is no JSON object: {error}"),
        )
    })?;
    if resource
        .get("resourceType")
        .is_none_or(|kind| *kind != fhir_types::codec::Value::String(BASIC.to_owned()))
    {
        return Err(invalid(
            IssueType::Invalid,
            "resourceType",
            "the App State endpoint takes a Basic",
        ));
    }
    let basic = Basic::from_json(&resource, &mut Path::root(BASIC)).map_err(|error| {
        invalid(
            IssueType::Structure,
            BASIC,
            format!("the body is no R4 Basic: {error}"),
        )
    })?;
    match intent {
        Intent::Create => {
            if basic.id.is_some() {
                return Err(invalid(
                    IssueType::Invalid,
                    "Basic.id",
                    "Basic.id SHALL NOT be included in a create",
                ));
            }
            if basic
                .meta
                .as_ref()
                .is_some_and(|meta| meta.version_id.is_some())
            {
                return Err(invalid(
                    IssueType::Invalid,
                    "Basic.meta.versionId",
                    "Basic.meta.versionId SHALL NOT be included in a create",
                ));
            }
        }
        Intent::Update(id) => {
            if basic.id.as_deref() != Some(id) {
                return Err(invalid(
                    IssueType::Invalid,
                    "Basic.id",
                    "Basic.id SHALL be present and match the id in the request URL",
                ));
            }
        }
    }
    if !basic.modifier_extension.is_empty() {
        return Err(invalid(
            IssueType::NotSupported,
            "Basic.modifierExtension",
            "App State carries no modifier extension",
        ));
    }
    for (at, extension) in basic.extension.iter().enumerate() {
        let element = format!("Basic.extension[{at}]");
        if !extension.extension.is_empty()
            || !matches!(extension.value, Some(ExtensionValue::String(_)))
        {
            return Err(invalid(
                IssueType::NotSupported,
                &element,
                "an App State extension carries a valueString and nothing else",
            ));
        }
    }
    let code = state_code(&basic)?;
    let subject = subject(&basic, base)?;
    Ok(State {
        resource,
        code,
        subject,
    })
}

/// Returns the one app-specified coding of `basic`: "`Basic.code.coding[]`
/// SHALL include exactly one app-specified Coding" (§Create), with the
/// system and the code a `system|code` query matches (§Querying app state).
fn state_code(basic: &Basic) -> Result<StateCode, Refusal> {
    let [coding] = basic.code.coding.as_slice() else {
        return Err(invalid(
            IssueType::Invalid,
            "Basic.code.coding",
            "Basic.code.coding SHALL include exactly one Coding",
        ));
    };
    let system = coding.system.as_ref().and_then(|uri| uri.value.clone());
    let code = coding.code.as_ref().and_then(|code| code.value.clone());
    match (system, code) {
        (Some(system), Some(code)) if !system.is_empty() && !code.is_empty() => {
            Ok(StateCode { system, code })
        }
        _ => Err(invalid(
            IssueType::Required,
            "Basic.code.coding[0]",
            "the state Coding names both a system and a code",
        )),
    }
}

/// Returns `Basic.subject.reference`, checked to be "an absolute reference to
/// a resource in the EHR's primary FHIR server" of one of [`SUBJECT_TYPES`]
/// (§Create).
fn subject(basic: &Basic, base: &str) -> Result<Option<String>, Refusal> {
    let Some(subject) = basic.subject.as_ref() else {
        return Ok(None);
    };
    let reference = subject
        .reference
        .as_ref()
        .and_then(|reference| reference.value.clone())
        .ok_or_else(|| {
            invalid(
                IssueType::Required,
                "Basic.subject.reference",
                "an App State subject is named by its absolute reference",
            )
        })?;
    let prefix = format!("{}/", base.trim_end_matches('/'));
    let named = reference
        .strip_prefix(&prefix)
        .and_then(|rest| rest.split_once('/'))
        .filter(|(kind, id)| SUBJECT_TYPES.contains(kind) && !id.is_empty() && !id.contains('/'));
    if named.is_none() {
        return Err(invalid(
            IssueType::Value,
            "Basic.subject.reference",
            format!(
                "the subject is an absolute reference under {prefix} to a {}",
                SUBJECT_TYPES.join(", ")
            ),
        ));
    }
    Ok(Some(reference))
}

#[cfg(test)]
mod tests {
    use super::Intent;
    use super::read;
    use http::StatusCode;

    const BASE: &str = "https://ehr.example.org/fhir";

    fn body(value: &serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(value).expect("JSON")
    }

    fn state() -> serde_json::Value {
        serde_json::json!({
            "resourceType": "Basic",
            "subject": {"reference": "https://ehr.example.org/fhir/Practitioner/123"},
            "code": {"coding": [{"system": "https://myapp.example.org", "code": "display-preferences"}]},
            "extension": [{"url": "https://myapp.example.org/p", "valueString": "{}"}]
        })
    }

    fn refused_at(value: &serde_json::Value, intent: Intent<'_>) -> StatusCode {
        read(&body(value), intent, BASE)
            .expect_err("refused")
            .into_response()
            .status()
    }

    #[test]
    fn the_example_reads_with_its_code_and_subject() {
        let state = read(&body(&state()), Intent::Create, BASE).expect("read");
        assert_eq!(
            "https://myapp.example.org|display-preferences",
            state.code.token()
        );
        assert_eq!(
            Some("https://ehr.example.org/fhir/Practitioner/123"),
            state.subject.as_deref()
        );
    }

    #[test]
    fn a_create_with_an_id_or_a_version_is_refused() {
        let mut with_id = state();
        with_id["id"] = serde_json::json!("1");
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused_at(&with_id, Intent::Create)
        );
        let mut with_version = state();
        with_version["meta"] = serde_json::json!({"versionId": "a"});
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused_at(&with_version, Intent::Create)
        );
    }

    #[test]
    fn an_update_names_the_id_of_its_url() {
        let mut other = state();
        other["id"] = serde_json::json!("2");
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused_at(&other, Intent::Update("1"))
        );
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused_at(&state(), Intent::Update("1"))
        );
    }

    #[test]
    fn two_codings_or_a_coding_without_a_system_are_refused() {
        let mut two = state();
        two["code"]["coding"] = serde_json::json!([
            {"system": "https://a", "code": "x"},
            {"system": "https://a", "code": "y"}
        ]);
        assert_eq!(StatusCode::BAD_REQUEST, refused_at(&two, Intent::Create));
        let mut bare = state();
        bare["code"]["coding"] = serde_json::json!([{"code": "x"}]);
        assert_eq!(StatusCode::BAD_REQUEST, refused_at(&bare, Intent::Create));
    }

    #[test]
    fn a_subject_off_this_server_or_of_another_type_is_refused() {
        for reference in [
            "Practitioner/123",
            "https://other.example.org/fhir/Practitioner/123",
            "https://ehr.example.org/fhir/Observation/1",
        ] {
            let mut value = state();
            value["subject"]["reference"] = serde_json::json!(reference);
            assert_eq!(
                StatusCode::BAD_REQUEST,
                refused_at(&value, Intent::Create),
                "{reference}"
            );
        }
    }

    #[test]
    fn an_extension_of_another_value_type_is_refused() {
        let mut value = state();
        value["extension"] = serde_json::json!([{"url": "https://a", "valueInteger": 1}]);
        assert_eq!(StatusCode::BAD_REQUEST, refused_at(&value, Intent::Create));
        let mut nested = state();
        nested["extension"] = serde_json::json!([{"url": "https://a", "extension": [{"url": "b", "valueString": "c"}]}]);
        assert_eq!(StatusCode::BAD_REQUEST, refused_at(&nested, Intent::Create));
    }

    #[test]
    fn a_body_over_256_kib_is_refused() {
        let mut value = state();
        value["extension"][0]["valueString"] = serde_json::json!("x".repeat(super::MAX_BODY));
        assert_eq!(
            StatusCode::PAYLOAD_TOO_LARGE,
            refused_at(&value, Intent::Create)
        );
    }

    #[test]
    fn global_state_reads_with_no_subject() {
        let mut value = state();
        value.as_object_mut().expect("object").remove("subject");
        let state = read(&body(&value), Intent::Create, BASE).expect("read");
        assert_eq!(None, state.subject);
    }
}
