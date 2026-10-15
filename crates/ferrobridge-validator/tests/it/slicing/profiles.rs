// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `profile` discriminator, and the profiles a type names:
//! "If any profiles are specified, then the content must conform to at
//! least one of them" (ElementDefinition.type.profile,
//! <https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.type.profile>).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use super::against;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::profile;

/// An Organization profile requiring a name.
fn named_organization() -> serde_json::Value {
    profile(
        "named-organization",
        "Organization",
        &json!([
            {"id": "Organization", "path": "Organization", "min": 0, "max": "*"},
            {"id": "Organization.name", "path": "Organization.name", "min": 1, "max": "1",
             "type": [{"code": "string"}]}
        ]),
    )
}

#[test]
fn a_profile_discriminator_matches_the_slice_whose_profile_the_target_conforms_to() -> TestResult {
    let definition = profile(
        "named-gp",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.generalPractitioner", "path": "Patient.generalPractitioner",
             "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "profile", "path": "resolve()"}], "rules": "closed"}},
            {"id": "Patient.generalPractitioner:named", "path": "Patient.generalPractitioner",
             "sliceName": "named", "min": 0, "max": "*",
             "type": [{"code": "Reference", "targetProfile": [format!("{BASE}named-organization")]}]}
        ]),
    );
    let patient = |organization: serde_json::Value| {
        json!({"resourceType": "Patient", "contained": [organization],
               "generalPractitioner": [{"reference": "#o1"}]})
    };
    let url = format!("{BASE}named-gp");
    let set = [definition, named_organization()];
    let report = against(
        &set,
        &patient(json!({"resourceType": "Organization", "id": "o1", "name": "Clinic"})),
        &url,
    )?;
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = against(
        &set,
        &patient(json!({"resourceType": "Organization", "id": "o1", "active": true})),
        &url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.generalPractitioner[0]"]
    );
    Ok(())
}

/// An Identifier profile fixing the system to `system`.
fn identifier_in(id: &str, system: &str) -> serde_json::Value {
    let mut definition = profile(
        id,
        "Identifier",
        &json!([
            {"id": "Identifier", "path": "Identifier", "min": 0, "max": "*"},
            {"id": "Identifier.system", "path": "Identifier.system", "min": 1, "max": "1",
             "type": [{"code": "uri"}], "fixedUri": system}
        ]),
    );
    definition["kind"] = json!("complex-type");
    definition
}

/// A Patient profile whose identifiers conform to one of `profiles`.
fn typed_identifiers(profiles: &[String]) -> serde_json::Value {
    profile(
        "typed-identifiers",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.identifier", "path": "Patient.identifier", "min": 0, "max": "*",
             "type": [{"code": "Identifier", "profile": profiles}]}
        ]),
    )
}

/// Validates a Patient whose one identifier has `system` against
/// [`typed_identifiers`] over `profiles`.
fn identified(
    profiles: &[String],
    system: &str,
) -> TestResult<ferrobridge_validator::report::Report> {
    let set = [
        typed_identifiers(profiles),
        identifier_in("identifier-x", "urn:x"),
        identifier_in("identifier-y", "urn:y"),
    ];
    let resource =
        json!({"resourceType": "Patient", "identifier": [{"system": system, "value": "1"}]});
    against(&set, &resource, &format!("{BASE}typed-identifiers"))
}

#[test]
fn content_conforming_to_one_of_the_type_profiles_passes() -> TestResult {
    let profiles = [format!("{BASE}identifier-x"), format!("{BASE}identifier-y")];
    let report = identified(&profiles, "urn:y")?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn content_conforming_to_none_of_the_type_profiles_names_each() -> TestResult {
    let profiles = [format!("{BASE}identifier-x"), format!("{BASE}identifier-y")];
    let report = identified(&profiles, "urn:z")?;
    let found: Vec<_> = report
        .with_message(MessageId::TypeProfileMismatch)
        .collect();
    assert_eq!(found.len(), 2, "{:#?}", report.issues());
    assert!(
        found
            .iter()
            .all(|issue| issue.expression == "Patient.identifier[0]")
    );
    assert!(found[0].message.contains(&profiles[0]) && found[1].message.contains(&profiles[1]));
    Ok(())
}

#[test]
fn one_type_profile_reports_its_own_findings() -> TestResult {
    let report = identified(&[format!("{BASE}identifier-x")], "urn:z")?;
    assert_eq!(
        locations(&report, MessageId::FixedMismatch),
        ["Patient.identifier[0].system"]
    );
    Ok(())
}

#[test]
fn a_type_profile_the_set_does_not_carry_is_named_unevaluated() -> TestResult {
    let absent = format!("{BASE}identifier-absent");
    let report = identified(std::slice::from_ref(&absent), "urn:z")?;
    let entry = report
        .unevaluated()
        .iter()
        .find(|entry| matches!(&entry.constraint, Constraint::TypeProfile { profiles } if profiles == std::slice::from_ref(&absent)))
        .expect("the absent profile is named");
    assert_eq!(entry.reason, Reason::ProfileNotFound);
    assert_eq!(entry.location, "Patient.identifier[0]");
    Ok(())
}
