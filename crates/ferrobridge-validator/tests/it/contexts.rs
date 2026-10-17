// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where an extension may be used: `StructureDefinition.context` and
//! `contextInvariant` (<https://hl7.org/fhir/R4/defining-extensions.html#context>,
//! §2.5.1.2: "Extensions SHALL only be used on a target that appears in their
//! context list"; the context types as the R4 code system
//! `extension-context-type` defines them).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::InvariantStage;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::extension;
use crate::support::issues;
use crate::support::locations;
use crate::support::patient;
use crate::support::patient_profile;
use crate::support::patient_url;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// The boolean extension `where` admitted by `contexts`, each a
/// `(type, expression)` pair, under the context invariants `invariants`.
fn placed(contexts: &[(&str, &str)], invariants: &[&str]) -> serde_json::Value {
    let mut definition = extension("where", "boolean", false);
    definition["context"] = contexts
        .iter()
        .map(|(kind, expression)| json!({"type": kind, "expression": expression}))
        .collect();
    if !invariants.is_empty() {
        definition["contextInvariant"] = json!(invariants);
    }
    definition
}

/// The extension `where` as an instance.
fn used() -> serde_json::Value {
    json!({"url": format!("{BASE}where"), "valueBoolean": true})
}

/// Validates `resource` with `definitions` beside the patient profile,
/// against `profile` or the core definition.
fn check(
    definitions: &[serde_json::Value],
    resource: &serde_json::Value,
    profile: Option<&str>,
) -> TestResult<Report> {
    let mut all = vec![patient_profile()];
    all.extend_from_slice(definitions);
    let (_synthetic, set) = set_with(&all)?;
    validate(&set, resource, profile)
}

/// The locations of the extension-context refusals in `report`.
fn refused(report: &Report) -> Vec<String> {
    locations(report, MessageId::ExtensionContext)
}

/// Answers whether `report` lists an extension context as unevaluated.
fn context_listed(report: &Report) -> Option<Reason> {
    report
        .unevaluated()
        .iter()
        .find(|entry| entry.constraint == Constraint::ExtensionContext)
        .map(|entry| entry.reason.clone())
}

#[test]
fn an_element_context_admits_the_base_element_it_names() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&[placed(&[("element", "Patient")], &[])], &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert_eq!(context_listed(&report), None);
    Ok(())
}

#[test]
fn an_element_context_refuses_an_element_it_does_not_name() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["extension"] = json!([used()]);
    let report = check(&[placed(&[("element", "Patient")], &[])], &resource, None)?;
    let found = issues(&report, MessageId::ExtensionContext);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].expression, "Patient.name[0].extension[0]");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(!report.is_valid());
    Ok(())
}

#[test]
fn an_element_context_naming_a_datatype_admits_every_element_of_it() -> TestResult {
    let definitions = [placed(&[("element", "HumanName")], &[])];
    let mut resource = patient();
    resource["name"][0]["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    assert_eq!(refused(&report), ["Patient.extension[0]"]);
    Ok(())
}

#[test]
fn an_element_context_naming_a_type_admits_its_specializations() -> TestResult {
    // R4 code: its StructureDefinition's baseDefinition is string.
    let mut resource = patient();
    resource["_gender"] = json!({"extension": [used()]});
    let report = check(&[placed(&[("element", "string")], &[])], &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    let mut resource = patient();
    resource["_gender"] = json!({"extension": [used()]});
    let report = check(&[placed(&[("element", "boolean")], &[])], &resource, None)?;
    assert_eq!(refused(&report), ["Patient.gender.extension[0]"]);
    Ok(())
}

#[test]
fn the_element_context_element_admits_a_primitive_and_a_resource() -> TestResult {
    let mut resource = patient();
    resource["_gender"] = json!({"extension": [used()]});
    resource["extension"] = json!([used()]);
    let report = check(&[placed(&[("element", "Element")], &[])], &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_element_context_inside_a_datatype_names_the_datatype_path() -> TestResult {
    let definitions = [placed(&[("element", "Address.line")], &[])];
    let mut resource = patient();
    resource["address"] = json!([{"line": ["1 Main St"], "_line": [{"extension": [used()]}]}]);
    let report = check(&definitions, &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    let mut resource = patient();
    resource["address"] = json!([{"city": "Utrecht", "_city": {"extension": [used()]}}]);
    let report = check(&definitions, &resource, None)?;
    assert_eq!(refused(&report), ["Patient.address[0].city.extension[0]"]);
    Ok(())
}

#[test]
fn an_element_context_of_a_profile_admits_the_slice_it_names() -> TestResult {
    let context = format!("{}#Patient.identifier:local", patient_url());
    let mut resource = patient();
    resource["identifier"] = json!([
        {"system": "http://example.org/local", "value": "1", "extension": [used()]}
    ]);
    let mut profile = patient_profile();
    profile["snapshot"]["element"]
        .as_array_mut()
        .expect("the snapshot is an array")
        .insert(
            3,
            json!({"id": "Patient.identifier:local.system", "path": "Patient.identifier.system",
                   "min": 1, "max": "1", "type": [{"code": "uri"}],
                   "fixedUri": "http://example.org/local"}),
        );
    let (_synthetic, set) = set_with(&[profile, placed(&[("element", &context)], &[])])?;
    let report = validate(&set, &resource, Some(&patient_url()))?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert_eq!(context_listed(&report), None, "{:#?}", report.unevaluated());
    Ok(())
}

#[test]
fn an_element_context_naming_a_slice_the_item_may_not_be_placed_in_is_listed() -> TestResult {
    // The patient profile's slice `local` sets no system, so its discriminator places nothing.
    let context = format!("{}#Patient.identifier:local", patient_url());
    let mut resource = patient();
    resource["identifier"] = json!([{"system": "http://example.org/local", "extension": [used()]}]);
    let report = check(
        &[placed(&[("element", &context)], &[])],
        &resource,
        Some(&patient_url()),
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert!(
        matches!(context_listed(&report), Some(Reason::ExtensionContext(why)) if why.contains("no slice")),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn an_element_context_of_a_profile_refuses_another_element_of_it() -> TestResult {
    let context = format!("{}#Patient.identifier:local", patient_url());
    let mut resource = patient();
    resource["name"][0]["extension"] = json!([used()]);
    let report = check(
        &[placed(&[("element", &context)], &[])],
        &resource,
        Some(&patient_url()),
    )?;
    assert_eq!(refused(&report), ["Patient.name[0].extension[0]"]);
    Ok(())
}

#[test]
fn an_element_context_of_a_profile_the_element_is_not_held_to_is_listed() -> TestResult {
    let context = format!("{}#Patient.identifier:local", patient_url());
    let mut resource = patient();
    resource["identifier"] = json!([{"system": "http://example.org/local", "extension": [used()]}]);
    let report = check(&[placed(&[("element", &context)], &[])], &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert!(
        matches!(context_listed(&report), Some(Reason::ExtensionContext(why)) if why.contains("does not derive")),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn an_element_context_naming_no_r4_element_is_listed() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(
        &[placed(&[("element", "Patient.nothing")], &[])],
        &resource,
        None,
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert!(
        matches!(context_listed(&report), Some(Reason::ExtensionContext(why)) if why.contains("Patient.nothing")),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn a_context_admitting_the_element_wins_over_one_that_is_not_decided() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let contexts = [("element", "Patient.nothing"), ("element", "Patient")];
    let report = check(&[placed(&contexts, &[])], &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert_eq!(context_listed(&report), None);
    Ok(())
}

/// A complex extension `outer` whose children are free.
fn outer() -> serde_json::Value {
    profile(
        "outer",
        "Extension",
        &json!([
            {"id": "Extension", "path": "Extension", "min": 0, "max": "*"},
            {"id": "Extension.extension", "path": "Extension.extension", "min": 0, "max": "*"},
            {"id": "Extension.url", "path": "Extension.url", "min": 1, "max": "1",
             "type": [{"code": "uri"}], "fixedUri": format!("{BASE}outer")},
            {"id": "Extension.value[x]", "path": "Extension.value[x]", "min": 0, "max": "0",
             "type": [{"code": "boolean"}]}
        ]),
    )
}

#[test]
fn an_extension_context_admits_the_extension_it_names() -> TestResult {
    let definitions = [
        outer(),
        placed(&[("extension", &format!("{BASE}outer"))], &[]),
    ];
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}outer"), "extension": [used()]}]);
    let report = check(&definitions, &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert_eq!(context_listed(&report), None);
    Ok(())
}

#[test]
fn an_extension_context_refuses_an_element_that_is_no_such_extension() -> TestResult {
    let definitions = [
        outer(),
        placed(&[("extension", &format!("{BASE}outer"))], &[]),
    ];
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    assert_eq!(refused(&report), ["Patient.extension[0]"]);
    Ok(())
}

#[test]
fn an_extension_context_with_a_code_admits_that_child_of_the_complex_extension() -> TestResult {
    let context = format!("{BASE}outer#part");
    let definitions = [outer(), placed(&[("extension", &context)], &[])];
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}outer"), "extension": [
        {"url": "part", "extension": [used()]}
    ]}]);
    let report = check(&definitions, &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}outer"), "extension": [
        {"url": "other", "extension": [used()]}
    ]}]);
    let report = check(&definitions, &resource, None)?;
    assert_eq!(
        refused(&report),
        ["Patient.extension[0].extension[0].extension[0]"]
    );
    Ok(())
}

#[test]
fn a_fhirpath_context_admits_an_element_its_query_selects() -> TestResult {
    let definitions = [placed(
        &[("fhirpath", "Patient.name | Patient.gender")],
        &[],
    )];
    let mut resource = patient();
    resource["name"][0]["extension"] = json!([used()]);
    resource["_gender"] = json!({"extension": [used()]});
    let report = check(&definitions, &resource, None)?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert_eq!(context_listed(&report), None);
    Ok(())
}

#[test]
fn a_fhirpath_context_refuses_an_element_its_query_does_not_select() -> TestResult {
    let mut resource = patient();
    resource["address"] = json!([{"city": "Utrecht", "extension": [used()]}]);
    let report = check(
        &[placed(&[("fhirpath", "Patient.name")], &[])],
        &resource,
        None,
    )?;
    assert_eq!(refused(&report), ["Patient.address[0].extension[0]"]);
    Ok(())
}

#[test]
fn a_fhirpath_context_that_does_not_parse_is_listed() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(
        &[placed(&[("fhirpath", "Patient.(")], &[])],
        &resource,
        None,
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    assert!(
        matches!(context_listed(&report), Some(Reason::ExtensionContext(why)) if why.contains("does not parse")),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn a_context_invariant_the_holder_meets_raises_nothing() -> TestResult {
    let definitions = [placed(&[("element", "Patient")], &["gender.exists()"])];
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    assert!(
        locations(&report, MessageId::ContextInvariant).is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        report
            .unevaluated()
            .iter()
            .all(|entry| !matches!(entry.constraint, Constraint::ContextInvariant { .. }))
    );
    Ok(())
}

#[test]
fn a_context_invariant_the_holder_breaks_is_an_error() -> TestResult {
    let definitions = [placed(&[("element", "Patient")], &["birthDate.exists()"])];
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    let found = issues(&report, MessageId::ContextInvariant);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].expression, "Patient.extension[0]");
    assert_eq!(found[0].severity, Severity::Error);
    Ok(())
}

#[test]
fn a_context_invariant_reads_the_extension_as_percent_extension() -> TestResult {
    let definitions = [placed(
        &[("element", "Patient")],
        &["%extension.value = false"],
    )];
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    assert_eq!(
        locations(&report, MessageId::ContextInvariant),
        ["Patient.extension[0]"],
        "{:#?}",
        report.unevaluated()
    );
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}where"), "valueBoolean": false}]);
    let report = check(&definitions, &resource, None)?;
    assert!(
        locations(&report, MessageId::ContextInvariant).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_context_invariant_on_a_primitive_holder_has_the_primitive_as_focus() -> TestResult {
    let definitions = [placed(&[("element", "code")], &["$this = 'female'"])];
    let mut resource = patient();
    resource["_gender"] = json!({"extension": [used()]});
    let report = check(&definitions, &resource, None)?;
    assert!(
        locations(&report, MessageId::ContextInvariant).is_empty(),
        "{:#?}",
        report.issues()
    );
    resource["gender"] = json!("male");
    let report = check(&definitions, &resource, None)?;
    assert_eq!(
        locations(&report, MessageId::ContextInvariant),
        ["Patient.gender.extension[0]"]
    );
    Ok(())
}

#[test]
fn a_context_invariant_that_does_not_parse_is_listed() -> TestResult {
    let definitions = [placed(&[("element", "Patient")], &["gender.("])];
    let mut resource = patient();
    resource["extension"] = json!([used()]);
    let report = check(&definitions, &resource, None)?;
    let listed = report
        .unevaluated()
        .iter()
        .find(|entry| matches!(entry.constraint, Constraint::ContextInvariant { .. }))
        .expect("the context invariant is listed");
    assert!(
        matches!(
            &listed.reason,
            Reason::InvariantRefused {
                stage: InvariantStage::Parse,
                ..
            }
        ),
        "{listed:#?}"
    );
    assert_eq!(listed.location, "Patient.extension[0]");
    Ok(())
}
