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

/// HL7 Europe Patient Summary 1.0.0-ballot.
const EPS: &str = "http://hl7.eu/fhir/eps/StructureDefinition";

/// HL7 Europe Hospital Discharge Report 1.0.0.
const HDR: &str = "http://hl7.eu/fhir/hdr/StructureDefinition";

/// HL7 International Patient Summary 2.0.1.
const IPS: &str = "http://hl7.org/fhir/uv/ips/StructureDefinition";

/// Returns the `EHDSAlert` group.
pub(super) fn group_alert() -> Group {
    Group {
        name: "EHDSAlert",
        context: "ferrobridge_ehds_alert.context",
        resource_type: "Flag",
        archetype: "openEHR-EHR-EVALUATION.precaution.v1",
        profile: format!("{BASE}/flag-patient-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Flag",
            "status": "active",
            "code": { "text": "Latex precautions" },
            "period": { "start": "2026-06-01T00:00:00Z" },
            "subject": subject()
        }),
        // R4 Flag search parameters `patient` and `date`.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&date=ge2026-05-01",
        ],
    }
}

/// Returns the `EHDSAdvanceDirective` group, its directive document the
/// `EHDSAttachment`.
pub(super) fn group_advance_directive() -> Group {
    Group {
        name: "EHDSAdvanceDirective",
        context: "ferrobridge_ehds_advance_directive.context",
        resource_type: "Consent",
        archetype: "openEHR-EHR-EVALUATION.advance_care_directive.v2",
        profile: format!("{EPS}/consent-eu-eps"),
        resource: serde_json::json!({
            "resourceType": "Consent",
            "status": "active",
            "scope": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/consentscope", "code": "adr"
            }]},
            "category": [{ "coding": [{ "system": "http://loinc.org", "code": "42348-3" }]}],
            "patient": subject(),
            "dateTime": "2026-02-01T10:00:00Z",
            "sourceAttachment": {
                "contentType": "application/pdf",
                "url": "http://example.org/fhir/Binary/synthetic-directive",
                "size": 2048
            },
            "provision": { "period": { "start": "2026-02-01T00:00:00Z" } }
        }),
        // R4 Consent search parameters `patient`, `scope` and `date`.
        searches: &[
            "patient=Patient/p-1&scope=adr",
            "patient=Patient/p-1&scope=adr&date=ge2026-01-01",
        ],
    }
}

/// Returns the `EHDSBodyStructure` group.
pub(super) fn group_body_structure() -> Group {
    Group {
        name: "EHDSBodyStructure",
        context: "ferrobridge_ehds_body_structure.context",
        resource_type: "BodyStructure",
        archetype: "openEHR-EHR-CLUSTER.anatomical_location.v1",
        profile: format!("{BASE}/bodyStructure-eu-core"),
        resource: serde_json::json!({
            "resourceType": "BodyStructure",
            "extension": [{
                "url": "http://hl7.org/fhir/5.0/StructureDefinition/extension-BodyStructure.includedStructure",
                "extension": [{ "url": "structure", "valueCodeableConcept": { "text": "Left knee" } }]
            }],
            "description": "The medial side of the left knee",
            "patient": subject()
        }),
        // R4 BodyStructure search parameter `patient`.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSCarePlan` group.
pub(super) fn group_care_plan() -> Group {
    Group {
        name: "EHDSCarePlan",
        context: "ferrobridge_ehds_care_plan.context",
        resource_type: "CarePlan",
        archetype: "openEHR-EHR-INSTRUCTION.care_plan_request.v0",
        profile: format!("{HDR}/carePlan-eu-hdr"),
        resource: serde_json::json!({
            "resourceType": "CarePlan",
            "status": "active",
            "intent": "plan",
            "title": "Rehabilitation after knee surgery",
            "description": "Physiotherapy twice a week",
            "period": { "start": "2026-07-01T00:00:00Z" },
            "subject": subject()
        }),
        // R4 CarePlan search parameters `patient` and `date`.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&date=ge2026-06-01",
        ],
    }
}

/// Returns the `EHDSCurrentPregnancyStatus` group.
pub(super) fn group_pregnancy_status() -> Group {
    Group {
        name: "EHDSCurrentPregnancyStatus",
        context: "ferrobridge_ehds_pregnancy_status.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-OBSERVATION.pregnancy_assertion.v0",
        profile: format!("{IPS}/Observation-pregnancy-status-uv-ips"),
        resource: serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "82810-3" }]},
            "effectiveDateTime": "2026-08-01T09:00:00Z",
            "valueCodeableConcept": { "coding": [{ "system": "http://loinc.org", "code": "LA15173-0" }]},
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&code=http://loinc.org|82810-3",
            "patient=Patient/p-1&code=82810-3&date=ge2026-07-01",
        ],
    }
}

/// Returns the `EHDSPregnancyHistory` group.
pub(super) fn group_pregnancy_history() -> Group {
    Group {
        name: "EHDSPregnancyHistory",
        context: "ferrobridge_ehds_pregnancy_history.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-EVALUATION.obstetric_summary.v1",
        profile: format!("{IPS}/Observation-pregnancy-outcome-uv-ips"),
        resource: serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "11636-8" }]},
            "effectiveDateTime": "2026-08-02T09:00:00Z",
            "valueQuantity": { "value": 2 },
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&code=http://loinc.org|11636-8",
            "patient=Patient/p-1&code=11636-8&date=ge2026-07-01",
        ],
    }
}

/// Returns the `EHDSTravelHistory` group.
pub(super) fn group_travel_history() -> Group {
    Group {
        name: "EHDSTravelHistory",
        context: "ferrobridge_ehds_travel_history.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-ADMIN_ENTRY.travel_event.v0",
        profile: format!("{EPS}/observation-travel-eu-eps"),
        resource: serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "94651-7" }]},
            "effectivePeriod": { "start": "2026-05-01T00:00:00Z", "end": "2026-05-15T00:00:00Z" },
            "valueCodeableConcept": { "coding": [{ "system": "urn:iso:std:iso:3166", "code": "IT" }]},
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&code=http://loinc.org|94651-7",
            "patient=Patient/p-1&code=94651-7&date=ge2026-04-01",
        ],
    }
}

/// Returns the `EHDSMedicationAdministration` group, its dosage the
/// `EHDSAdministeredDosage`.
pub(super) fn group_medication_administration() -> Group {
    Group {
        name: "EHDSMedicationAdministration",
        context: "ferrobridge_ehds_medication_administration.context",
        resource_type: "MedicationAdministration",
        archetype: "openEHR-EHR-ACTION.medication.v1",
        profile: format!("{HDR}/medicationAdministration-eu-hdr"),
        resource: serde_json::json!({
            "resourceType": "MedicationAdministration",
            "status": "completed",
            "category": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/medication-admin-category",
                "code": "inpatient"
            }]},
            "medicationCodeableConcept": { "coding": [{
                "system": "http://snomed.info/sct", "code": "387517004", "display": "Paracetamol"
            }]},
            "effectiveDateTime": "2026-09-01T08:00:00Z",
            "dosage": {
                "text": "One gram",
                "dose": { "value": 1, "unit": "g", "system": "http://unitsofmeasure.org", "code": "g" }
            },
            "subject": subject()
        }),
        // R4 MedicationAdministration search parameters `patient` and `effective-time`.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&effective-time=ge2026-08-01",
        ],
    }
}

/// Returns the `EHDSEncounter` group.
pub(super) fn group_encounter() -> Group {
    Group {
        name: "EHDSEncounter",
        context: "ferrobridge_ehds_encounter.context",
        resource_type: "Encounter",
        archetype: "openEHR-EHR-COMPOSITION.ferrobridge_ehds_record.v1",
        profile: format!("{HDR}/encounter-eu-hdr"),
        resource: serde_json::json!({
            "resourceType": "Encounter",
            "status": "finished",
            "class": { "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "IMP" },
            "period": { "start": "2026-09-01T08:00:00Z", "end": "2026-09-03T12:00:00Z" },
            "subject": subject()
        }),
        // R4 Encounter search parameter `patient`.
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
        group_alert(),
        group_advance_directive(),
        group_body_structure(),
        group_care_plan(),
        group_pregnancy_status(),
        group_pregnancy_history(),
        group_travel_history(),
        group_medication_administration(),
        group_encounter(),
    ]
}
