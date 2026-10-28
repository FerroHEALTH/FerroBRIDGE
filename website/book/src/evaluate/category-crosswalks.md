<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The category crosswalks

For each priority category of Regulation (EU) 2025/327 Art 14(1), the
[interoperability component](interoperability-component.md) keeps a
crosswalk: the eHealth Network dataset, the Xt-EHR logical model and the HL7
Europe profiles, element by element, held to the vendored packages by a
test. The crosswalks are data in `eehrxf::crosswalk`, keyed on the Xt-EHR
element path, and `Category::crosswalks` names a category's crosswalks, one
per dataset. `Crosswalk::check` holds the rows of one crosswalk to the
packages they name.

<!-- toc -->

## The patient summary crosswalk

The eHealth Network *Guidelines on Patient Summary* (Release 3.4, §4), the
Xt-EHR `EHDSPatientSummary` logical model and the HL7 Europe Patient Summary
composition (`hl7.fhir.eu.eps` 1.0.0-ballot) describe one dataset three
ways. The crosswalk, under the `patient-summary` feature, names in each row:

- the eHealth Network element ids, such as `A.2.1.1` for allergies;
- the obligation the Xt-EHR obligations profile puts on a producer;
- the element or slice of the EPS composition that carries it, such as
  `Composition.section:sectionAllergies.entry:allergyOrIntolerance`;
- the profiles a FHIRconnect context may map to in order to feed it. The
  header, each section's narrative, its empty reason and its note have none,
  because the document is assembled around them.

Every section of the model has a row, and so does every header element a
producer must be able to populate. A test fails when an element a producer
`SHALL:able-to-populate` has no row, when a row names a slice the pinned EPS
profile lacks, or when one of the five required EPS sections (problems,
allergies, medications, procedures, devices) is uncovered. Two EPS sections
have no counterpart in the model and no row: vital signs and the general
patient history. The presented form, a rendering of the whole summary, has
no element in the EPS composition.

Two elements have a row and no openEHR content named to feed them, so a
product that writes the summary decides how it states them: the medical
alert (A.2.1.2), which a producer must be able to populate, and the
functional status (A.2.3.4).

## The medication crosswalks

The electronic prescription (Art 14(1)(b), feature `prescription`) and the
electronic dispensation (Art 14(1)(c), feature `dispensation`) are
crosswalked from the Xt-EHR `EHDSMedicationPrescription` and
`EHDSMedicationDispense` models to the HL7 Europe Medication Prescription
and Dispense profiles (`hl7.fhir.eu.mpd` 1.0.0), `MedicationRequest-eu-mpd`
and `MedicationDispense-eu-mpd`. The `medication` module reads and writes the
two datasets as Rust records against those profiles.

## The discharge report crosswalk

The discharge report (Art 14(1)(f), feature `discharge`) is described three
ways as well: the eHealth Network *Guideline on Hospital Discharge Report*
(Release 1.1, §4), the Xt-EHR `EHDSDischargeReport` logical model and the
HL7 Europe Hospital Discharge Report composition (`hl7.fhir.eu.hdr` 1.0.0).
A test fails when an element a producer must or should be able to populate
has no row, when a row names an element the HDR composition lacks or a
profile its section entries do not target, or when a row's producer
obligation differs from the Xt-EHR obligations profile.

The encounter is the composition's `encounter`, which the HDR profile
requires, so it has no section. The model's devices and procedures of the
patient history sit in the HDR sections for devices and past procedures,
and its presented form in the attachments section.

## The laboratory report crosswalk

Test results (Art 14(1)(e), feature `laboratory`) are crosswalked the same
way: the eHealth Network *Guideline on Laboratory Results* (Release 1.1, §4),
the Xt-EHR `EHDSLaboratoryReport` logical model and the HL7 Europe
Laboratory Report profiles (`hl7.fhir.eu.laboratory` 2.0.0). A laboratory
report keeps its header in the composition and its body in the
`DiagnosticReport` the composition names, so the crosswalk names elements of
both and `Crosswalk::check_with` checks it against both: the orders and their
supporting information sit in the `ServiceRequest` the report is based on,
the specimens in `specimen`, and the results in `result`, each an
`Observation-resultslab-eu-lab`.

## The imaging crosswalks

Imaging (Art 14(1)(d), feature `imaging`) names two things, the imaging
studies and the imaging reports, and the Xt-EHR package models each on its
own, so the crate carries two crosswalks. The imaging report crosswalk keys
on `EHDSImagingReport` and names the elements of the HL7 Europe Imaging
Report composition and the `DiagnosticReport` it names (`hl7.fhir.eu.imaging`
1.0.0-ballot); the imaging study crosswalk keys on `EHDSImagingStudy` and
names the elements of its `ImagingStudy` profile. Both name the element ids
of the eHealth Network *Guideline on Medical imaging studies and reports*
(Release 1.1): those of the report data set (§4.1) and those of the imaging
study manifest (§4.2). The four sections the composition requires (the
imaging study, the order, the history and the procedure) each have a row, as
does the study's DICOM Study Instance UID, the identifier the `ImagingStudy`
profile requires.
