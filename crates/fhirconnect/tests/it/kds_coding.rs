// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The coding of the KDS diagnosis in both directions.
//!
//! The `icd10ProblemDiagnose` append of the KDS project context writes the
//! ICD-10-GM system onto `Condition.code.coding`. Its `fhirCondition` gates
//! only a FHIR input, since "conditions are always applied on the input data"
//! (`basics/Conditions.adoc`), so the context carries an `openehrCondition`
//! that gates a composition the same way. A diagnosis in any other code
//! system keeps its system in both directions.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::laws;
use crate::roundtrip::kds_condition;
use crate::roundtrip::kds_flat;
use crate::roundtrip::kds_program;

/// The ICD-10-GM code system the KDS context names.
const ICD_10_GM: &str = "http://fhir.de/CodeSystem/bfarm/icd-10-gm";

/// A code system the KDS context does not name.
const SNOMED_CT: &str = "http://snomed.info/sct";

/// The FLAT key of the coded diagnosis terminology in the synthetic fixture.
const DIAGNOSIS_TERMINOLOGY: &str = "diagnose/diagnose:0/kodierte_diagnose|terminology";

/// Maps `resource` into openEHR through the KDS program.
fn put(
    program: &Program,
    index: &WebTemplateIndex,
    resource: &serde_json::Value,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let inbound = to_openehr(
        program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(resource.clone()),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    Ok(index.accept(inbound.value().value().clone())?)
}

/// Maps `composition` out of openEHR through the KDS program.
fn get(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let outbound = to_fhir(
        program,
        &SCHEMAS,
        index,
        composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    Ok(outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root(
            program.resource().as_str(),
        ))?)
}

/// Builds the synthetic KDS composition with its diagnosis coded in `system`.
fn coded_in(
    index: &WebTemplateIndex,
    system: &str,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let mut flat = kds_flat()?;
    flat.insert(
        String::from(DIAGNOSIS_TERMINOLOGY),
        serde_json::Value::from(system),
    );
    Ok(index.build_from_flat(&flat, laws::NOW)?)
}

/// Returns the synthetic KDS Condition with its diagnosis coded in SNOMED CT.
fn snomed_condition() -> Result<serde_json::Value, Box<dyn Error>> {
    let mut condition = kds_condition()?;
    let coding = condition
        .pointer_mut("/code/coding/0")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("the KDS fixture carries a coding")?;
    coding.remove("extension");
    coding.insert(String::from("system"), serde_json::Value::from(SNOMED_CT));
    Ok(condition)
}

#[test]
fn a_composition_coded_outside_icd_10_gm_keeps_its_system_going_out() -> Result<(), Box<dyn Error>>
{
    let (program, index) = kds_program()?;
    let read = get(&program, &index, &coded_in(&index, SNOMED_CT)?)?;
    assert_eq!(
        read.pointer("/code/coding/0/system"),
        Some(&serde_json::Value::from(SNOMED_CT))
    );
    assert_eq!(read.pointer("/code/coding/1"), None);
    Ok(())
}

#[test]
fn a_composition_coded_in_icd_10_gm_reads_with_the_icd_10_gm_system() -> Result<(), Box<dyn Error>>
{
    let (program, index) = kds_program()?;
    let read = get(&program, &index, &coded_in(&index, ICD_10_GM)?)?;
    assert_eq!(
        read.pointer("/code/coding/0/system"),
        Some(&serde_json::Value::from(ICD_10_GM))
    );
    Ok(())
}

#[test]
fn a_snomed_condition_stores_and_reads_back_its_snomed_system() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_program()?;
    let input = snomed_condition()?;
    let composition = put(&program, &index, &input)?;
    let flat = index.flatten(&composition)?;
    assert_eq!(
        flat.get(DIAGNOSIS_TERMINOLOGY),
        Some(&serde_json::Value::from(SNOMED_CT))
    );
    let read = get(&program, &index, &composition)?;
    assert_eq!(read.pointer("/code/coding"), input.pointer("/code/coding"));
    Ok(())
}
