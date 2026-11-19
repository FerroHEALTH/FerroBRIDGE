// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading one launch `Task` against `task-ehr-launch` and
//! `task-standalone-launch` (SMART App Launch 2.2.0, the two
//! `StructureDefinition`s of the package and `task-launch.html`).
//!
//! Both profiles constrain `Task.code` by the pattern of their launch code,
//! bound required to `smart-launch-types`, and slice `Task.input` by the
//! value of `type`: `launchurl`, 1..1, a `valueUrl` under the type
//! `smartonfhir-application`, and `launchcontext`, 0..1, a `valueString` under
//! `smartonfhir-appcontext`. The slicing is open, so another input stands.

use fhir_types::codec::Json as _;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::task::Task;
use fhir_types::r4::task::TaskInputValue;
use http::StatusCode;

use crate::facade::launch_task::EHR_LAUNCH;
use crate::facade::launch_task::STANDALONE_LAUNCH;
use crate::facade::launch_task::TASK;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The `smart-codes` code system the launch codes and input types are in.
pub const SMART_CODES: &str = "http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes";

/// The `Task.code` of a request for an EHR launch.
pub const LAUNCH_APP_EHR: &str = "launch-app-ehr";

/// The `Task.code` of a request for a standalone launch.
pub const LAUNCH_APP_STANDALONE: &str = "launch-app-standalone";

/// The input type of the application's launch URL.
const APPLICATION: &str = "smartonfhir-application";

/// The input type of the launch's `appContext`.
const APP_CONTEXT: &str = "smartonfhir-appcontext";

/// Which launch a `Task` requests, and so which profile it conforms to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// `task-ehr-launch`.
    Ehr,
    /// `task-standalone-launch`.
    Standalone,
}

impl Launch {
    /// Returns the canonical of the profile.
    #[must_use]
    pub const fn profile(self) -> &'static str {
        match self {
            Self::Ehr => EHR_LAUNCH,
            Self::Standalone => STANDALONE_LAUNCH,
        }
    }
}

/// One launch `Task`, read and checked.
#[derive(Debug, Clone)]
pub struct LaunchTask {
    /// The resource as the client sent it.
    pub resource: Object,
    /// The decoded `Task`.
    pub task: Box<Task>,
    /// The launch it requests.
    pub launch: Launch,
    /// `Task.for.reference`, when the `Task` names one.
    pub patient: Option<String>,
}

/// Returns the `400` naming `element`.
fn invalid(kind: IssueType, element: &str, text: impl Into<String>) -> Refusal {
    reply::refusal(
        StatusCode::BAD_REQUEST,
        Issue::error(kind).diagnosing(text).at(element.to_owned()),
    )
}

/// Returns the refusal of a `Task` that conforms to neither profile, naming
/// `element`.
fn unprofiled(element: &str, text: impl Into<String>) -> Refusal {
    reply::refusal(
        StatusCode::UNPROCESSABLE_ENTITY,
        Issue::error(IssueType::Processing)
            .diagnosing(text)
            .at(element.to_owned()),
    )
}

/// Returns whether `concept` holds a coding of `smart-codes` with `code`,
/// what a `patternCodeableConcept` of one such coding asks
/// (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.pattern_x_>).
fn holds(concept: &CodeableConcept, code: &str) -> bool {
    concept.coding.iter().any(|coding| {
        coding.system.as_ref().and_then(|uri| uri.value.as_deref()) == Some(SMART_CODES)
            && coding
                .code
                .as_ref()
                .and_then(|value| value.value.as_deref())
                == Some(code)
    })
}

/// Reads `body` as a launch `Task`.
///
/// # Errors
/// Returns `400` for a body that is no R4 `Task` or carries a modifier
/// extension, and `422` naming the element for a `Task` that conforms to
/// neither launch profile.
pub fn read(body: &[u8]) -> Result<LaunchTask, Refusal> {
    let resource: Object = serde_json::from_slice(body).map_err(|error| {
        invalid(
            IssueType::Structure,
            TASK,
            format!("the body is no JSON object: {error}"),
        )
    })?;
    if resource
        .get("resourceType")
        .is_none_or(|kind| *kind != fhir_types::codec::Value::String(TASK.to_owned()))
    {
        return Err(invalid(
            IssueType::Invalid,
            "resourceType",
            "the launch endpoint takes a Task",
        ));
    }
    let task = Task::from_json(&resource, &mut Path::root(TASK)).map_err(|error| {
        invalid(
            IssueType::Structure,
            TASK,
            format!("the body is no R4 Task: {error}"),
        )
    })?;
    if !task.modifier_extension.is_empty() {
        return Err(invalid(
            IssueType::NotSupported,
            "Task.modifierExtension",
            "this server reads no modifier extension of a launch Task",
        ));
    }
    if task.status.value.is_none() || task.intent.value.is_none() {
        return Err(invalid(
            IssueType::Required,
            TASK,
            "Task.status and Task.intent are required",
        ));
    }
    let launch = launch_of(&task)?;
    inputs(&task)?;
    let patient = task
        .r#for
        .as_ref()
        .and_then(|reference| reference.reference.as_ref())
        .and_then(|reference| reference.value.clone());
    Ok(LaunchTask {
        resource,
        task: Box::new(task),
        launch,
        patient,
    })
}

/// Returns the launch `Task.code` names: "`Task.code` 1..1" with the pattern
/// of one launch code, bound required to `smart-launch-types`.
fn launch_of(task: &Task) -> Result<Launch, Refusal> {
    let code = task.code.as_ref().ok_or_else(|| {
        unprofiled(
            "Task.code",
            "a launch Task names launch-app-ehr or launch-app-standalone in Task.code",
        )
    })?;
    match (
        holds(code, LAUNCH_APP_EHR),
        holds(code, LAUNCH_APP_STANDALONE),
    ) {
        (true, false) => Ok(Launch::Ehr),
        (false, true) => Ok(Launch::Standalone),
        (true, true) => Err(unprofiled(
            "Task.code",
            "a launch Task requests one launch, never both",
        )),
        (false, false) => Err(unprofiled(
            "Task.code",
            "a launch Task names launch-app-ehr or launch-app-standalone of smart-codes in Task.code",
        )),
    }
}

/// Checks the two input slices: `launchurl` 1..1 with a `valueUrl` and
/// `launchcontext` 0..1 with a `valueString`.
fn inputs(task: &Task) -> Result<(), Refusal> {
    let mut urls = 0_usize;
    let mut contexts = 0_usize;
    for (at, input) in task.input.iter().enumerate() {
        let element = format!("Task.input[{at}]");
        if holds(&input.r#type, APPLICATION) {
            let url = match &input.value {
                TaskInputValue::Url(url) => url.value.as_deref().filter(|url| !url.is_empty()),
                _ => None,
            };
            if url.is_none() {
                return Err(unprofiled(
                    &format!("{element}.value[x]"),
                    "the smartonfhir-application input carries the application's URL as valueUrl",
                ));
            }
            urls = urls.saturating_add(1);
        } else if holds(&input.r#type, APP_CONTEXT) {
            if !matches!(&input.value, TaskInputValue::String(_)) {
                return Err(unprofiled(
                    &format!("{element}.value[x]"),
                    "the smartonfhir-appcontext input carries the appContext as valueString",
                ));
            }
            contexts = contexts.saturating_add(1);
        }
    }
    if urls != 1 {
        return Err(unprofiled(
            "Task.input",
            "a launch Task carries exactly one smartonfhir-application input",
        ));
    }
    if contexts > 1 {
        return Err(unprofiled(
            "Task.input",
            "a launch Task carries at most one smartonfhir-appcontext input",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Launch;
    use super::read;
    use http::StatusCode;
    use serde_json::json;

    fn ehr() -> serde_json::Value {
        json!({
            "resourceType": "Task",
            "status": "requested",
            "intent": "proposal",
            "code": {"coding": [{"system": super::SMART_CODES, "code": "launch-app-ehr"}]},
            "for": {"reference": "https://example.org/fhir/Patient/123"},
            "input": [
                {"type": {"coding": [{"system": super::SMART_CODES, "code": "smartonfhir-application"}]},
                 "valueUrl": "https://www.example.org/myapplication"},
                {"type": {"coding": [{"system": super::SMART_CODES, "code": "smartonfhir-appcontext"}]},
                 "valueString": "{\"field1\":\"value\"}"}
            ]
        })
    }

    fn status(value: &serde_json::Value) -> StatusCode {
        read(&serde_json::to_vec(value).expect("JSON"))
            .expect_err("refused")
            .into_response()
            .status()
    }

    #[test]
    fn an_ehr_launch_reads_with_its_profile_and_patient() {
        let task = read(&serde_json::to_vec(&ehr()).expect("JSON")).expect("read");
        assert_eq!(Launch::Ehr, task.launch);
        assert_eq!(
            Some("https://example.org/fhir/Patient/123"),
            task.patient.as_deref()
        );
    }

    #[test]
    fn a_task_of_no_launch_code_is_unprocessable() {
        let mut value = ehr();
        value["code"]["coding"][0]["code"] = json!("fulfill");
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&value));
        value.as_object_mut().expect("object").remove("code");
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&value));
    }

    #[test]
    fn the_launch_url_is_one_value_url() {
        let mut none = ehr();
        none["input"] = json!([none["input"][1].clone()]);
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&none));
        let mut two = ehr();
        two["input"] = json!([two["input"][0].clone(), two["input"][0].clone()]);
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&two));
        let mut string = ehr();
        string["input"][0] = json!({
            "type": {"coding": [{"system": super::SMART_CODES, "code": "smartonfhir-application"}]},
            "valueString": "https://www.example.org/myapplication"
        });
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&string));
    }

    #[test]
    fn the_app_context_is_at_most_one_value_string() {
        let mut two = ehr();
        two["input"] = json!([
            two["input"][0].clone(),
            two["input"][1].clone(),
            two["input"][1].clone()
        ]);
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status(&two));
    }

    #[test]
    fn another_input_stands_beside_the_slices() {
        let mut other = ehr();
        other["input"]
            .as_array_mut()
            .expect("inputs")
            .push(json!({"type": {"text": "note"}, "valueString": "x"}));
        assert!(read(&serde_json::to_vec(&other).expect("JSON")).is_ok());
    }

    #[test]
    fn a_body_of_another_type_is_refused() {
        let mut basic = ehr();
        basic["resourceType"] = json!("Basic");
        assert_eq!(StatusCode::BAD_REQUEST, status(&basic));
    }
}
