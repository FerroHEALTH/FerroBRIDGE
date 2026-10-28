<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# EHDS readiness

## Legal status

The European Health Data Space regulation, Regulation (EU) 2025/327
(<https://eur-lex.europa.eu/eli/reg/2025/327/oj>), names six priority
categories of personal electronic health data in Article 14(1). It requires them
in the European electronic health record exchange format (Article 15), whose
technical specifications the Commission lays down by implementing act. That
implementing act has not been adopted. No profile version is legally pinned,
and nothing on this page is a conformance claim.

The HL7 Europe implementation guides are the current proxy for the exchange
format, and FerroBRIDGE targets them on FHIR R4. When the implementing act is
adopted, the targets below are re-read against it.

This page is the FHIR facade's mapping matrix. The European interoperability
software component an EHR system carries under Art 25(1), with its dataset
model, its category crosswalks and the receipt of a document, is the
`eehrxf` crate ([The interoperability component](interoperability-component.md)).

## The matrix

Each row is a target profile with the committed template it maps from and the
round-trip test that proves the transform. A test generates the matrix from
`docs/ehds/readiness.toml` and fails when a template, a context or a test it
names is gone. "Produced, not claimed" means the resource is built but does not
name the profile in `meta.profile`. A gap means the openEHR international CKM
publishes no template for the category.

<!-- readiness:begin (generated from docs/ehds/readiness.toml; do not edit) -->

### Article 14(1)(a): Patient summaries

HL7 Europe Patient Summary 1.0.0-ballot is pinned for the profile validator; no context maps its Composition yet. The Base and Core profiles below are its building blocks.

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `patient-eu-core` | hl7.fhir.eu.base 2.0.0 | KDS_Person | `the_profile_elements_of_patient_eu_core_survive_the_round_trip` |
| `condition-eu-core` | hl7.fhir.eu.base 2.0.0 | KDS_Diagnose | `the_profile_elements_of_condition_eu_core_survive_the_round_trip` |
| `medicalTestResult-eu-core` | hl7.fhir.eu.base 2.0.0 | KDS_Laborbericht | `the_profile_elements_of_the_test_result_survive_the_round_trip` |
| `Patient summary Composition` | hl7.fhir.eu.eps 1.0.0-ballot | none: no patient summary template in the openEHR international CKM | no |

### Article 14(1)(b): Electronic prescriptions

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `MedicationRequest-eu-mpd` | hl7.fhir.eu.mpd 1.0.0 | FerroBRIDGE medication template over CKM archetypes | `the_profile_elements_of_the_prescription_survive_the_round_trip` |
| `Medication-eu-mpd` | hl7.fhir.eu.mpd 1.0.0 | FerroBRIDGE medication template over CKM archetypes | `the_product_of_the_prescription_survives_the_round_trip` |
| `Dosage-eu-mpd` (produced, not claimed) | hl7.fhir.eu.mpd 1.0.0 | FerroBRIDGE medication template over CKM archetypes | `the_dosage_of_the_prescription_survives_the_round_trip` |

### Article 14(1)(c): Electronic dispensations

The openEHR international CKM publishes no dispensation template; the template is FerroBRIDGE's, over published CKM archetypes only (owner ruling on #116).

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `MedicationDispense-eu-mpd` | hl7.fhir.eu.mpd 1.0.0 | FerroBRIDGE medication template over CKM archetypes | `the_profile_elements_of_the_dispense_survive_the_round_trip` |

### Article 14(1)(d): Medical imaging studies and related reports

HL7 Europe Imaging Report is at 1.0.0-ballot and deferred (docs/architecture.md section 4.8).

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `Imaging report Composition` | hl7.fhir.eu.imaging 1.0.0-ballot | none: no imaging report template in the openEHR international CKM | no |

### Article 14(1)(e): Medical test results, laboratory results and related reports

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `DiagnosticReport-eu-lab` | hl7.fhir.eu.laboratory 2.0.0 | KDS_Laborbericht | `the_profile_elements_of_the_laboratory_report_survive_the_round_trip` |
| `Observation-resultslab-eu-lab` | hl7.fhir.eu.laboratory 2.0.0 | KDS_Laborbericht | `the_profile_elements_of_the_laboratory_result_survive_the_round_trip` |
| `Specimen-eu-lab` | hl7.fhir.eu.laboratory 2.0.0 | KDS_Laborbericht | `the_profile_elements_of_the_laboratory_specimen_survive_the_round_trip` |
| `Composition-eu-lab` | hl7.fhir.eu.laboratory 2.0.0 | KDS_Laborbericht | `the_profile_elements_of_the_document_composition_survive_the_round_trip` |
| `Bundle-eu-lab` (produced, not claimed); assembled by the facade's Composition/$document, proven on a synthetic Composition context; the claimed document is the interoperability component's (`eehrxf`) laboratory document | hl7.fhir.eu.laboratory 2.0.0 | KDS_Laborbericht | `the_document_is_the_composition_and_every_resource_it_creates` |

### Article 14(1)(f): Discharge reports

HL7 Europe Hospital Discharge Report 1.0.0 (R4, 2026-10-16) is pinned, and the interoperability component (`eehrxf`) holds its crosswalk; no context maps its Composition, because no template carries a discharge report.

| Target profile | Package | Committed template | Transform proven |
|---|---|---|---|
| `Discharge report Composition` | hl7.fhir.eu.hdr 1.0.0 | none: no discharge report template in the openEHR international CKM | no |

<!-- readiness:end -->
