// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The contexts of an STU3 extension: one `contextType` with a list of
//! `context` strings, read with the STU3 codes `resource`, `datatype` and
//! `extension` (<https://hl7.org/fhir/STU3/codesystem-extension-context.html>).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::stu3::stu3_profile;
use crate::stu3::stu3_set;
use crate::stu3::validate_stu3;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;

/// A simple STU3 extension `id` with a `boolean` value, usable where
/// `context_type` and `contexts` say.
fn extension(id: &str, context_type: &str, contexts: &[&str]) -> serde_json::Value {
    let mut definition = stu3_profile(
        id,
        "Extension",
        &json!([
            {"id": "Extension", "path": "Extension", "min": 0, "max": "*"},
            {"id": "Extension.extension", "path": "Extension.extension", "min": 0, "max": "0"},
            {"id": "Extension.url", "path": "Extension.url", "min": 1, "max": "1",
             "type": [{"code": "uri"}], "fixedUri": format!("{BASE}{id}")},
            {"id": "Extension.value[x]", "path": "Extension.value[x]", "min": 1, "max": "1",
             "type": [{"code": "boolean"}]}
        ]),
    );
    definition["contextType"] = json!(context_type);
    definition["context"] = json!(contexts);
    definition
}

/// Validates `resource` against its core definition, with the extensions
/// `on-patient` (resource `Patient`) and `on-string` (datatype `string`).
fn report(resource: &serde_json::Value) -> TestResult<Report> {
    let (_synthetic, set) = stu3_set(&[
        extension("on-patient", "resource", &["Patient"]),
        extension("on-string", "datatype", &["string"]),
    ])?;
    validate_stu3(&set, resource, None)
}

/// An extension of the definition `id` with the value `true`.
fn used(id: &str) -> serde_json::Value {
    json!([{"url": format!("{BASE}{id}"), "valueBoolean": true}])
}

#[test]
fn a_resource_context_admits_its_resource() -> TestResult {
    let report = report(&json!({"resourceType": "Patient", "extension": used("on-patient")}))?;
    assert!(
        locations(&report, MessageId::ExtensionContext).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_resource_context_refuses_another_resource() -> TestResult {
    let report = report(&json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "weight"},
        "extension": used("on-patient")
    }))?;
    assert_eq!(
        locations(&report, MessageId::ExtensionContext),
        ["Observation.extension[0]"]
    );
    Ok(())
}

#[test]
fn a_datatype_context_admits_its_primitive() -> TestResult {
    let report = report(&json!({
        "resourceType": "Patient",
        "name": [{"family": "Doe", "_family": {"extension": used("on-string")}}]
    }))?;
    assert!(
        locations(&report, MessageId::ExtensionContext).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_string_context_refuses_a_code_in_stu3() -> TestResult {
    // STU3 StructureDefinition.context: a context of string does not admit the string
    // patterns such as id, and code is one of them.
    let report = report(&json!({
        "resourceType": "Patient",
        "gender": "male",
        "_gender": {"extension": used("on-string")}
    }))?;
    assert_eq!(
        locations(&report, MessageId::ExtensionContext),
        ["Patient.gender.extension[0]"]
    );
    Ok(())
}
