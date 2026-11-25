// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Annex X groups served from an entry of a composition, each with the
//! resource its context maps and the searches its option asks for.

use super::super::SUBJECT;
use super::BASE;
use super::Group;

/// Returns the subject reference every resource carries.
pub(super) fn subject() -> serde_json::Value {
    serde_json::json!({ "reference": SUBJECT })
}

/// Returns the `EHDSAllergyIntolerance` group.
pub(super) fn group_allergy_intolerance() -> Group {
    Group {
        name: "EHDSAllergyIntolerance",
        context: "ferrobridge_ehds_allergy_intolerance.context",
        resource_type: "AllergyIntolerance",
        archetype: "openEHR-EHR-EVALUATION.adverse_reaction_risk.v2",
        profile: format!("{BASE}/allergyIntolerance-eu-core"),
        resource: serde_json::json!({
            "resourceType": "AllergyIntolerance",
            "clinicalStatus": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                "code": "active"
            }]},
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "227493005", "display": "Cashew nuts"
            }]},
            "patient": subject()
        }),
        // PCC-44 2:3.44.4.1.2.2.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSCondition` group.
pub(super) fn group_condition() -> Group {
    Group {
        name: "EHDSCondition",
        context: "ferrobridge_ehds_condition.context",
        resource_type: "Condition",
        archetype: "openEHR-EHR-EVALUATION.problem_diagnosis.v1",
        profile: format!("{BASE}/condition-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Condition",
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "38341003", "display": "Hypertension"
            }]},
            "onsetDateTime": "2026-01-10",
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.3.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&onset-date=ge2026-01-01",
        ],
    }
}

/// Returns the `EHDSProcedure` group.
pub(super) fn group_procedure() -> Group {
    Group {
        name: "EHDSProcedure",
        context: "ferrobridge_ehds_procedure.context",
        resource_type: "Procedure",
        archetype: "openEHR-EHR-ACTION.procedure.v1",
        profile: format!("{BASE}/procedure-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Procedure",
            "status": "completed",
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "80146002", "display": "Appendectomy"
            }]},
            "performedDateTime": "2026-02-01T10:00:00Z",
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.7.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&date=ge2026-01-01&date=lt2026-03-01",
        ],
    }
}

/// Returns the `EHDSMedicationUse` group.
pub(super) fn group_medication_use() -> Group {
    Group {
        name: "EHDSMedicationUse",
        context: "ferrobridge_ehds_medication_use.context",
        resource_type: "MedicationStatement",
        archetype: "openEHR-EHR-INSTRUCTION.medication_order.v3",
        profile: format!("{BASE}/medicationStatement-eu-core"),
        resource: serde_json::json!({
            "resourceType": "MedicationStatement",
            "status": "active",
            "contained": [{
                "resourceType": "Medication",
                "id": "medicine",
                "code": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "387517004", "display": "Paracetamol"
                }]}
            }],
            "medicationReference": { "reference": "#medicine" },
            "effectivePeriod": { "start": "2026-03-01" },
            "dosage": [{ "text": "One tablet twice a day" }],
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.5.
        searches: &["patient=Patient/p-1&_include=MedicationStatement:medication"],
    }
}

/// Returns the `EHDSImmunisation` group.
pub(super) fn group_immunisation() -> Group {
    Group {
        name: "EHDSImmunisation",
        context: "ferrobridge_ehds_immunisation.context",
        resource_type: "Immunization",
        archetype: "openEHR-EHR-ACTION.medication.v1",
        profile: format!("{BASE}/immunization-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Immunization",
            "status": "completed",
            "vaccineCode": { "coding": [{
                "system": "http://snomed.info/sct", "code": "1119349007", "display": "SARS-CoV-2 mRNA vaccine"
            }]},
            "occurrenceDateTime": "2026-04-01T09:00:00Z",
            "patient": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.6.
        searches: &["patient=Patient/p-1", "patient=Patient/p-1&date=2026-04"],
    }
}

/// Returns the `EHDSObservation` group.
pub(super) fn group_observation() -> Group {
    Group {
        name: "EHDSObservation",
        context: "ferrobridge_ehds_observation.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-OBSERVATION.body_weight.v2",
        profile: String::from("http://hl7.org/fhir/StructureDefinition/bodyweight"),
        resource: serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "category": [{ "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "vital-signs"
            }]}],
            "code": { "coding": [{ "system": "http://loinc.org", "code": "29463-7" }]},
            "effectiveDateTime": "2026-05-01T08:00:00Z",
            "valueQuantity": {
                "value": 72.5, "unit": "kg", "system": "http://unitsofmeasure.org", "code": "kg"
            },
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&category=vital-signs",
            "patient=Patient/p-1&category=vital-signs&code=http://loinc.org|29463-7",
            "patient=Patient/p-1&category=vital-signs&date=ge2026-05-01",
            "patient=Patient/p-1&category=vital-signs&code=29463-7&date=lt2026-06-01",
        ],
    }
}

/// HL7 Europe Laboratory Report 2.0.0.
pub(super) const LAB: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition";

/// Returns a laboratory report whose result and specimen it contains, in
/// the shapes HL7 Europe Laboratory Report 2.0.0 constrains.
pub(super) fn laboratory_report() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "meta": { "profile": [format!("{LAB}/DiagnosticReport-eu-lab")] },
        "contained": [
            {
                "resourceType": "Specimen",
                "id": "serum",
                "meta": { "profile": [format!("{LAB}/Specimen-eu-lab")] },
                "type": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "119364003", "display": "Serum specimen"
                }]},
                "subject": subject(),
                "collection": { "collectedDateTime": "2026-03-02T07:05:00+01:00" }
            },
            {
                "resourceType": "Observation",
                "id": "cholesterol",
                "meta": { "profile": [format!("{LAB}/Observation-resultslab-eu-lab")] },
                "status": "final",
                "category": [{ "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                    "code": "laboratory"
                }]}],
                "code": { "coding": [{
                    "system": "http://loinc.org", "code": "2093-3",
                    "display": "Cholesterol [Mass/volume] in Serum or Plasma"
                }]},
                "subject": subject(),
                "effectiveDateTime": "2026-03-02T08:15:00+01:00",
                "valueQuantity": {
                    "value": 182, "unit": "mg/dL", "system": "http://unitsofmeasure.org", "code": "mg/dL"
                }
            }
        ],
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-lab-reports",
            "value": "synthetic-report-0001"
        }],
        "status": "final",
        "code": { "coding": [{
            "system": "http://loinc.org", "code": "24331-1", "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": subject(),
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "specimen": [{ "reference": "#serum" }],
        "result": [{ "reference": "#cholesterol" }]
    })
}

/// Returns the laboratory `DiagnosticReport`, the IHE `QEDm` Diagnostic
/// Reports option, which is no Annex X group.
pub(super) fn diagnostic_report() -> Group {
    Group {
        name: "DiagnosticReport",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "DiagnosticReport",
        archetype: "openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
        profile: format!("{LAB}/DiagnosticReport-eu-lab"),
        resource: laboratory_report(),
        // PCC-44 2:3.44.4.1.2.1.4.
        searches: &[
            "patient=Patient/p-1&category=LAB",
            "patient=Patient/p-1&category=LAB&date=ge2026-03-01",
            "patient=Patient/p-1&category=http://terminology.hl7.org/CodeSystem/v2-0074|LAB&date=lt2026-04-01",
            "patient=Patient/p-1&category=LAB&code=http://loinc.org|24331-1",
        ],
    }
}

/// Returns the `EHDSLaboratoryObservation` group: each analyte of a
/// laboratory report.
pub(super) fn group_laboratory_observation() -> Group {
    Group {
        name: "EHDSLaboratoryObservation",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-CLUSTER.laboratory_test_analyte.v1",
        profile: format!("{LAB}/Observation-resultslab-eu-lab"),
        resource: laboratory_report(),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&category=laboratory",
            "patient=Patient/p-1&category=laboratory&code=http://loinc.org|2093-3",
            "patient=Patient/p-1&category=laboratory&date=ge2026-03-01",
            "patient=Patient/p-1&category=laboratory&code=2093-3&date=lt2026-04-01",
        ],
    }
}

/// Returns the `EHDSServiceRequest` group: the testkit's synthetic
/// laboratory request.
pub(super) fn group_service_request() -> Group {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::KDS_LABORAUFTRAG_SERVICE_REQUEST)
            .expect("the testkit request parses");
    resource["subject"] = subject();
    resource["meta"] = serde_json::json!({ "profile": [format!("{LAB}/ServiceRequest-eu-lab")] });
    Group {
        name: "EHDSServiceRequest",
        context: "ferrobridge_ehds_service_request.context",
        resource_type: "ServiceRequest",
        archetype: "openEHR-EHR-INSTRUCTION.service_request.v1",
        profile: format!("{LAB}/ServiceRequest-eu-lab"),
        resource,
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSDeviceUse` group.
pub(super) fn group_device_use() -> Group {
    Group {
        name: "EHDSDeviceUse",
        context: "ferrobridge_ehds_device_use.context",
        resource_type: "DeviceUseStatement",
        archetype: "openEHR-EHR-EVALUATION.device_summary.v0",
        profile: String::from(
            "http://hl7.eu/fhir/eps/StructureDefinition/deviceUseStatement-eu-eps",
        ),
        resource: serde_json::json!({
            "resourceType": "DeviceUseStatement",
            "status": "active",
            "contained": [{
                "resourceType": "Device",
                "id": "pacemaker",
                "type": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "14106009", "display": "Cardiac pacemaker"
                }]}
            }],
            "device": { "reference": "#pacemaker" },
            "timingPeriod": { "start": "2025-06-01" },
            "subject": subject()
        }),
        // R4 DeviceUseStatement search parameter `patient`.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSSpecimen` group: each specimen of a laboratory report.
pub(super) fn group_specimen() -> Group {
    Group {
        name: "EHDSSpecimen",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "Specimen",
        archetype: "openEHR-EHR-CLUSTER.specimen.v1",
        profile: format!("{LAB}/Specimen-eu-lab"),
        resource: laboratory_report(),
        // R4 Specimen search parameter `patient`.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the groups this suite serves.
pub(super) fn groups() -> Vec<Group> {
    vec![
        group_allergy_intolerance(),
        group_condition(),
        group_procedure(),
        group_medication_use(),
        group_immunisation(),
        group_observation(),
        group_laboratory_observation(),
        group_device_use(),
        group_specimen(),
        group_service_request(),
    ]
}
