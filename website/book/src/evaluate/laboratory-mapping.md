<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Laboratory reports from openEHR

The `openehr` and `laboratory` features map an openEHR laboratory report to
the HL7 Europe Laboratory Report 2.0.0 document, and map a received
laboratory report document into openEHR. Regulation (EU) 2025/327 names
medical test results, laboratory results included, as a priority category
in Art 14(1)(e).

## The shipped mappings

The crate ships the FHIRconnect 1.0.0 projects it runs under `mappings/` in
its package, and `eehrxf::mappings` embeds them as text. `MPD` is the
[prescription and dispensation](prescription-mapping.md) project.
`LABORATORY` is the laboratory project over the published `KDS_Laborbericht`
template, whose laboratory result, analyte and specimen archetypes are
CKM's. `Project::write_to` writes a project's files where a system keeps
its mappings, the paths `Mapping::compile` takes. FerroBRIDGE's own
round-trip tests read the same directories, so a system that depends on the
published crate runs the files those tests prove.

| Context | openEHR | Laboratory Report profile |
|---|---|---|
| `ferrobridge_eu_laboratory.context` | `OBSERVATION.laboratory_test_result.v1` | `DiagnosticReport-eu-lab`, with an `Observation-resultslab-eu-lab` per analyte and a `Specimen-eu-lab` per specimen |
| `ferrobridge_eu_lab_composition.context` | `COMPOSITION.report-result.v1` | `Composition-eu-lab`, whose section references the report |

`LABORATORY` compiles the report context. The composition context maps the
same template, so a mapping compiles it on its own.

## Mapping out of openEHR

`Mapping::to_fhir_with` maps a committed composition to a FHIR R4
`collection` Bundle: the report, its results and specimens, and one
`Provenance` covering them. `Document::with_mapped_report` takes that Bundle
into a laboratory report document: the report becomes the
`DiagnosticReport-eu-lab` the `Bundle-eu-lab` holds beside its composition,
the results, the specimens and the `Provenance` become entries, and the
method answers the `fullUrl` of each result for the section that lists
them. The document's `Composition` is the system's own, as for every form
of the component; the caller chooses the section by the study type of the
report, one of the `LAB_SLOTS`.

## Receiving a laboratory report

`ReceivedDocument::category` names a document whose `Composition.type`
carries LOINC `11502-2` a laboratory report. `Composition-eu-lab` binds
`Composition.type` to the `lab-reportType-eu-lab` value set, which holds
that one code. `Category::profiles` then names `Bundle-eu-lab` and
`Composition-eu-lab`, which `ReceivedDocument::check` holds the document
to. `Mapping::to_openehr` maps the document's report into one
`KDS_Laborbericht` composition and keeps the document as received in the
composition's `FEEDER_AUDIT.original_content`.

`KDS_Laborbericht` holds one report identifier. A report in a
`Bundle-eu-lab` carries its own identifier and the document's
(`dr-comp-identifier`), so the run keeps one, declares the other lost in
the composition's `OperationOutcome`, and the original document still
holds both.
