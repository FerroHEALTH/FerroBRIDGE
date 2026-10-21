// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Real bindings: the HL7 Europe patient summary pregnancy observations
//! (`hl7.fhir.eu.eps` 1.0.0-ballot), read with their closure when
//! `scripts/vendor/profile-packages.sh` has fetched the packages; each test
//! returns early otherwise. The observations are synthetic.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use serde_json::json;

use crate::eu::eu_set;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::validate;

/// The estimated delivery date observation, whose code is bound (required)
/// to the IPS value set `edd-method-uv-ips`, three listed LOINC codes.
const EDD: &str = "http://hl7.eu/fhir/eps/StructureDefinition/observation-pregnancy-edd-eu-eps";

/// The gestational age observation, whose code is bound (required) to a
/// value set filtering LOINC, which the package set does not carry whole.
const GESTATIONAL_AGE: &str =
    "http://hl7.eu/fhir/eps/StructureDefinition/observation-pregnancy-gestationalAge-eu-eps";

/// A synthetic observation coded with the LOINC code `code`.
fn observation(code: &str) -> serde_json::Value {
    json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"coding": [{"system": "http://loinc.org", "code": code}]},
        "subject": {"reference": "Patient/p1"},
        "effectiveDateTime": "2026-01-01",
        "valueDateTime": "2026-06-01"
    })
}

#[test]
fn an_eps_code_the_ips_value_set_lists_meets_its_required_binding() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let report = validate(&set, &observation("11778-8"), Some(EDD))?;
    assert!(
        locations(&report, MessageId::NotInValueSet).is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        !report.unevaluated().iter().any(|entry| entry.element == "Observation.code"
            && matches!(&entry.constraint, Constraint::Binding { strength, .. } if strength == "required")),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn an_eps_code_outside_the_ips_value_set_is_refused() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let report = validate(&set, &observation("29463-7"), Some(EDD))?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Observation.code"]
    );
    Ok(())
}

#[test]
fn an_eps_binding_over_a_loinc_filter_waits_for_a_terminology_server() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let report = validate(&set, &observation("11884-4"), Some(GESTATIONAL_AGE))?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        Vec::<String>::new()
    );
    assert!(
        report
            .unevaluated()
            .iter()
            .any(|entry| entry.element == "Observation.code"
                && matches!(&entry.constraint, Constraint::Binding { strength, .. } if strength == "required")),
        "{:#?}",
        report.unevaluated()
    );
    assert!(
        report
            .pending_bindings()
            .iter()
            .any(|pending| pending.location() == "Observation.code")
    );
    Ok(())
}
