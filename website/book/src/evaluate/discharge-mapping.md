<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Discharge reports from openEHR

The `openehr` and `discharge` features map an openEHR discharge report to
the HL7 Europe Hospital Discharge Report 1.0.0 document, and map a received
discharge report document into openEHR. Regulation (EU) 2025/327 names
discharge reports as a priority category in Art 14(1)(f).

## The template

The openEHR international CKM publishes no discharge report template. By
the owner ruling on #116 of 2026-11-06, FerroBRIDGE writes its own ADL 2
template over published CKM archetypes only, as it does for the medication
template, and never a new archetype. The template is
`openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.0.0`:

| Node | CKM archetype | HDR section |
|---|---|---|
| the composition | `COMPOSITION.transfer_summary.v1`, whose context is the hospital stay | the `Composition` and its `encounter` |
| Diagnostic summary | `SECTION.adhoc.v1` holding `EVALUATION.problem_diagnosis.v1` | `sectionDiagnosticSummary` |
| Significant procedures | `SECTION.adhoc.v1` holding `ACTION.procedure.v1` | `sectionSignificantProcedures` |
| Discharge medications | `SECTION.adhoc.v1` holding `INSTRUCTION.medication_order.v3` | `sectionDischargeMedications` |

The CKM describes the transfer of care summary as a clinical report about "an
episode, or period, of care", conveying "diagnoses, investigation results,
management at discharge and plans for follow-up". The test suites build the
operational template from the template and the vendored archetypes at test
time.

## The shipped mapping

`HDR` is the project the crate ships under `mappings/hdr/`, and
`eehrxf::mappings` embeds it as text. Its one context,
`ferrobridge_eu_hdr.context`, starts at the composition and maps:

| openEHR | Hospital Discharge Report |
|---|---|
| the composition | `composition-eu-hdr`, of type LOINC `34105-7` |
| the composition's `context/start_time` and `context/end_time` | `encounter-eu-hdr`, its `period` |
| each `EVALUATION.problem_diagnosis.v1` of the diagnostic summary | one `condition-eu-core` entry of `sectionDiagnosticSummary`: its `code`, `onset[x]`, `assertedDate` extension, `bodySite` and `note` |
| each `ACTION.procedure.v1` of the significant procedures | one `procedure-eu-core` entry of `sectionSignificantProcedures`: its `status` from the careflow step, `code`, `performed[x]` from the action's time, `bodySite` and `note` |
| each `INSTRUCTION.medication_order.v3` of the discharge medications | one `medicationRequest-eu-core` entry of `sectionDischargeMedications`: its `medicationCodeableConcept`, the overall directions as `dosageInstruction.text`, and `note` |

The template records neither the status nor the class of the stay, and
`encounter-eu-hdr` requires both. The mapping writes the status `finished`
and the class `IMP` from the v3 `ActCode` system, a member of the
`encounter-class-eu-hdr` value set, because a discharge report reports a
finished hospital stay. A procedure commenced is `in-progress`, one
completed `completed` and one aborted `stopped`, the three careflow steps of
the CKM procedure action at which it was performed. An `INSTRUCTION` records
no request status, so a discharge medication is `unknown` and an `order`, as
in the MPD project. A section is written only when the composition holds an
entry for it.

## Mapping out of openEHR

`Mapping::to_fhir_with` maps a committed composition to a FHIR R4
`collection` Bundle: the composition, the encounter, every condition,
procedure and medication request, and one `Provenance` covering them.
`Document::with_mapped_composition` takes that Bundle into a discharge
report document. The run's `Composition` stays out,
because the document writes its own from the form. The encounter it names
becomes the document's encounter, every other resource becomes an entry, and
the method answers each section of the run placed in the form's slot by its
LOINC code, with the `fullUrl` of each entry. A section whose code names no
slot of the form, or an entry the run does not hold, is refused.

`composition-eu-hdr` requires a discharge summary or a hospital course
section (`cmp-hdr-2`). Both are narrative sections the document assembly
writes from the paragraphs the system gives it, as the crosswalk records,
so a system adds the hospital course with `Document::with_section` beside
the mapped sections. `ReceivedDocument::validate` holds the whole document
to `bundle-eu-hdr` and to the profile of every entry through
`ferrobridge-validator`, and the suite validates a mapped report that way.

## Receiving a discharge report

`ReceivedDocument::category` names a document whose `Composition.type`
carries LOINC `34105-7` a discharge report, and `Category::profiles` names
`bundle-eu-hdr` and `composition-eu-hdr`, which `ReceivedDocument::check`
holds the document to. `Mapping::to_openehr` maps the document from its
`composition-eu-hdr` into one composition of the template, following the
composition's encounter and the entries of its three sections. The
document as received is kept in the composition's
`FEEDER_AUDIT.original_content`.

The composition's `OperationOutcome` declares every mapping that runs only
out of openEHR (the fixed type, title, status and date, the encounter's
status and class, and the claimed profiles) and every composition field the
engine fills: the composer, the setting, the language and the territory.
Nothing the mapping does not carry is passed in silence.
