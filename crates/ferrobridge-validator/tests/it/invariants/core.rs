// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of the R4 core definitions, each broken by a synthetic
//! resource and reported with its key, at the root of a resource and below
//! it.

use ferrobridge_validator::issue::IssueType;
use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::InvariantStage;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::BestPractice;
use ferrobridge_validator::validate::Validator;
use serde_json::json;

use super::against_core;
use super::failed_at;
use super::failures;
use super::listed;
use crate::support::TestResult;
use crate::support::core;
use crate::support::instance;

/// A narrative, so `dom-6` holds.
fn text() -> serde_json::Value {
    json!({"status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">synthetic</div>"})
}

/// A synthetic `AllergyIntolerance`, with a clinical status when `status`.
fn allergy(status: bool) -> serde_json::Value {
    let mut resource = json!({
        "resourceType": "AllergyIntolerance",
        "text": text(),
        "patient": {"reference": "Patient/p1"}
    });
    if status {
        resource["clinicalStatus"] = json!({"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
            "code": "active"
        }]});
    }
    resource
}

#[test]
fn ait_1_reports_an_allergy_without_a_clinical_status() -> TestResult {
    let report = against_core(&allergy(false))?;
    let found = failures(&report, "ait-1");
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    let issue = found[0];
    assert_eq!(issue.expression, "AllergyIntolerance");
    assert_eq!(issue.severity, Severity::Error);
    assert_eq!(issue.code, IssueType::Invariant);
    assert_eq!(
        issue.rule,
        Rule::Constraint {
            key: "ait-1".to_owned(),
            source: "http://hl7.org/fhir/StructureDefinition/AllergyIntolerance".to_owned()
        }
    );
    assert!(!report.is_valid());
    assert!(failed_at(&against_core(&allergy(true))?, "ait-1").is_empty());
    Ok(())
}

/// A synthetic Composition with the sections `sections`.
fn composition(sections: &serde_json::Value) -> serde_json::Value {
    json!({
        "resourceType": "Composition",
        "text": text(),
        "status": "final",
        "type": {"text": "synthetic"},
        "date": "2026-01-01",
        "author": [{"display": "Synthetic author"}],
        "title": "Synthetic",
        "section": sections
    })
}

#[test]
fn cmp_1_reports_an_empty_section_below_the_root_and_in_a_nested_section() -> TestResult {
    let report = against_core(&composition(&json!([
        {"title": "empty"},
        {"title": "holds", "text": text(), "section": [{"title": "nested and empty"}]}
    ])))?;
    assert_eq!(
        failed_at(&report, "cmp-1"),
        [
            "Composition.section[0]",
            "Composition.section[1].section[0]"
        ]
    );
    Ok(())
}

#[test]
fn cmp_2_reports_a_section_with_entries_and_an_empty_reason() -> TestResult {
    let report = against_core(&composition(&json!([{
        "title": "both",
        "entry": [{"display": "a synthetic entry"}],
        "emptyReason": {"text": "nothing"}
    }])))?;
    assert_eq!(failed_at(&report, "cmp-2"), ["Composition.section[0]"]);
    assert!(failed_at(&report, "cmp-1").is_empty());
    Ok(())
}

#[test]
fn ele_1_reports_an_element_with_only_an_id_once() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient", "text": text(), "name": [{"id": "n1"}]
    }))?;
    assert_eq!(failed_at(&report, "ele-1"), ["Patient.name[0]"]);
    Ok(())
}

#[test]
fn ele_1_reports_a_primitive_with_only_an_id() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient", "text": text(), "_birthDate": {"id": "b1"}
    }))?;
    assert_eq!(failed_at(&report, "ele-1"), ["Patient.birthDate"]);
    Ok(())
}

#[test]
fn ext_1_reports_an_extension_with_a_value_and_extensions() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient",
        "text": text(),
        "extension": [{
            "url": "http://example.org/fhir/StructureDefinition/unknown",
            "valueString": "a",
            "extension": [{"url": "part", "valueBoolean": true}]
        }]
    }))?;
    assert_eq!(failed_at(&report, "ext-1"), ["Patient.extension[0]"]);
    Ok(())
}

#[test]
fn dom_3_is_listed_with_its_evaluation_error_and_the_upstream_report() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient",
        "text": text(),
        "contained": [{"resourceType": "Practitioner", "id": "unreferenced"}]
    }))?;
    assert!(
        failed_at(&report, "dom-3").is_empty(),
        "dom-3 is never passed or failed"
    );
    let entries = listed(&report, "dom-3");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(entries[0].location, "Patient");
    assert!(
        matches!(&entries[0].reason, Reason::InvariantRefused {
            stage: InvariantStage::Evaluate,
            upstream: Some(report),
            ..
        } if report.ends_with("/issues/444")),
        "{:#?}",
        entries[0].reason
    );
    Ok(())
}

/// A Patient whose name is valid over the period `start` to `end`.
fn named_over(start: &str, end: &str) -> serde_json::Value {
    json!({
        "resourceType": "Patient",
        "text": text(),
        "name": [{"family": "Doe", "period": {"start": start, "end": end}}]
    })
}

#[test]
fn per_1_passes_an_ordered_period_and_reports_a_reversed_one() -> TestResult {
    assert!(
        failed_at(
            &against_core(&named_over("2020-01-01", "2020-02-01"))?,
            "per-1"
        )
        .is_empty()
    );
    let report = against_core(&named_over("2020-02-01", "2020-01-01"))?;
    assert_eq!(failed_at(&report, "per-1"), ["Patient.name[0].period"]);
    Ok(())
}

#[test]
fn per_1_is_listed_when_mixed_precisions_leave_the_comparison_empty() -> TestResult {
    // N1 §6.2: a date and a datetime compare as unknown, so per-1 answers empty, which is
    // neither the true conformance-rules.html §2.1.0.6 asks for nor false.
    let report = against_core(&named_over("2020-01-01", "2020-01-01T10:00:00Z"))?;
    assert!(
        failed_at(&report, "per-1").is_empty(),
        "{:#?}",
        report.issues()
    );
    let entries = listed(&report, "per-1");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(entries[0].location, "Patient.name[0].period");
    assert_eq!(entries[0].reason, Reason::InvariantEmpty);
    Ok(())
}

/// An Observation whose value is the range `low` mg to `high` `unit`.
fn ranged(low: &str, high: &str, unit: &str) -> serde_json::Value {
    let ucum = |value: &str, code: &str| {
        json!({"value": value.parse::<f64>().unwrap_or_default(), "unit": code,
               "system": "http://unitsofmeasure.org", "code": code})
    };
    json!({
        "resourceType": "Observation",
        "text": text(),
        "status": "final",
        "code": {"text": "synthetic"},
        "valueRange": {"low": ucum(low, "mg"), "high": ucum(high, unit)}
    })
}

#[test]
fn rng_2_compares_low_and_high_across_ucum_units() -> TestResult {
    assert!(failed_at(&against_core(&ranged("500", "1", "g"))?, "rng-2").is_empty());
    let report = against_core(&ranged("1500", "1", "g"))?;
    assert_eq!(
        failed_at(&report, "rng-2"),
        ["Observation.value.ofType(Range)"]
    );
    Ok(())
}

#[test]
fn rng_2_is_listed_when_incommensurable_units_leave_the_comparison_empty() -> TestResult {
    // N1 §6.2: quantities of different dimensions compare as empty.
    let report = against_core(&ranged("1", "1", "mL"))?;
    assert!(
        failed_at(&report, "rng-2").is_empty(),
        "{:#?}",
        report.issues()
    );
    let entries = listed(&report, "rng-2");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(entries[0].reason, Reason::InvariantEmpty);
    Ok(())
}

#[test]
fn dom_6_is_a_best_practice_warning_and_an_error_when_asked() -> TestResult {
    let set = PackageSet::load([core()])?;
    let patient = instance(&json!({"resourceType": "Patient", "name": [{"family": "Doe"}]}))?;
    let report = Validator::new(&set).validate(&patient)?;
    let found = failures(&report, "dom-6");
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Warning);
    assert!(report.is_valid());
    let strict = Validator::new(&set)
        .with_best_practice(BestPractice::Error)
        .validate(&patient)?;
    let found = failures(&strict, "dom-6");
    assert_eq!(found.len(), 1, "{:#?}", strict.issues());
    assert_eq!(found[0].severity, Severity::Error);
    assert!(!strict.is_valid());
    Ok(())
}

#[test]
fn txt_1_reads_the_narrative_through_html_checks() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient",
        "text": {"status": "generated",
                 "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\"><script>x</script>synthetic</div>"}
    }))?;
    assert_eq!(failed_at(&report, "txt-1"), ["Patient.text.div"]);
    Ok(())
}

#[test]
fn a_valid_patient_raises_no_invariant_and_lists_none() -> TestResult {
    let report = against_core(&json!({
        "resourceType": "Patient",
        "text": text(),
        "name": [{"family": "Doe", "given": ["Ada"], "period": {"start": "2020-01-01"}}],
        "gender": "female",
        "birthDate": "1970-01-01"
    }))?;
    let raised: Vec<_> = report
        .issues()
        .iter()
        .filter(|issue| matches!(issue.rule, Rule::Constraint { .. }))
        .collect();
    assert!(raised.is_empty(), "{raised:#?}");
    let unevaluated: Vec<_> = report
        .unevaluated()
        .iter()
        .filter(|entry| {
            matches!(
                entry.reason,
                Reason::InvariantRefused { .. } | Reason::NoExpression | Reason::InvariantEmpty
            )
        })
        .collect();
    assert!(unevaluated.is_empty(), "{unevaluated:#?}");
    Ok(())
}

/// A synthetic `RiskAssessment` with one prediction whose probability is
/// `probability` under the JSON member `member`.
fn assessment(member: &str, probability: &serde_json::Value) -> serde_json::Value {
    let mut prediction = json!({"outcome": {"text": "synthetic"}});
    prediction[member] = probability.clone();
    json!({
        "resourceType": "RiskAssessment",
        "text": text(),
        "status": "final",
        "subject": {"reference": "Patient/p1"},
        "prediction": [prediction]
    })
}

/// A `Range` from 10 to 20 in the UCUM unit `code`.
fn range(code: &str) -> serde_json::Value {
    let bound =
        |value: u8| json!({"value": value, "system": "http://unitsofmeasure.org", "code": code});
    json!({"low": bound(10), "high": bound(20)})
}

#[test]
fn ras_1_holds_on_a_decimal_probability() -> TestResult {
    // fhirpath.html §2.9.1.1 and N1 §3.2: ras-1 reads the Range the choice
    // admits, and on a decimal `low` and `high` are empty.
    let report = against_core(&assessment("probabilityDecimal", &json!(0.25)))?;
    assert!(
        failed_at(&report, "ras-1").is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        listed(&report, "ras-1").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn ras_1_reads_the_units_of_a_range_probability() -> TestResult {
    let percent = against_core(&assessment("probabilityRange", &range("%")))?;
    assert!(
        failed_at(&percent, "ras-1").is_empty(),
        "{:#?}",
        percent.issues()
    );
    assert!(
        listed(&percent, "ras-1").is_empty(),
        "{:#?}",
        percent.unevaluated()
    );
    let milligrams = against_core(&assessment("probabilityRange", &range("mg")))?;
    assert_eq!(
        failed_at(&milligrams, "ras-1"),
        ["RiskAssessment.prediction[0].probability.ofType(Range)"],
        "{:#?}",
        milligrams.issues()
    );
    Ok(())
}
