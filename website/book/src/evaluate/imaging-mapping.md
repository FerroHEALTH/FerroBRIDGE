<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Imaging reports from openEHR

The `openehr` and `imaging` features map an openEHR imaging examination
result to the HL7 Europe Imaging Report 1.0.0-ballot document, and map a
received imaging report document into openEHR. Regulation (EU) 2025/327
names medical imaging studies and related imaging reports as a priority
category in Art 14(1)(d).

## The template

The openEHR international CKM publishes no imaging report composition
template, so the component maps over FerroBRIDGE's own ADL 2 template, as
it does for prescriptions and dispensations. The template uses published CKM
archetypes only and adds no archetype of its own: a
`COMPOSITION.report-result.v1` holding one
`OBSERVATION.imaging_exam_result.v1`. The test suites build the
operational template from it at test time. A system that maps its own
imaging content compiles the project against the template its CDR holds.

## The shipped mapping

`IMAGING` is the imaging project, one of the FHIRconnect 1.0.0 projects the
crate ships under `mappings/` and embeds in `eehrxf::mappings`.
`Mapping::compile_project` compiles it from its embedded text.

| Context | openEHR | Imaging Report profile |
|---|---|---|
| `ferrobridge_eu_imaging.context` | `OBSERVATION.imaging_exam_result.v1` | `DiagnosticReportEuImaging`, with the `ImagingStudyEuImaging` it references |

The report takes the event's time, the study name as its `code`, the
overall impression as its `conclusion`, the report identifier, and the
overall result status. The archetype's status codes are the R4
`diagnostic-report-status` codes except `entered-in-error`, which the
archetype does not define. The study takes the study instance identifier,
which `ImagingStudyEuImaging` requires as a DICOM UID (`urn:dicom:uid`, an
`urn:oid:` value), the study status over the R4 `imagingstudy-status`
codes, and the description.

The archetype holds the modality and the study date in the observation's
event, and its protocol holds the rest of the study. The study's reference
sits below the event, so `ImagingStudy.modality` and `ImagingStudy.started`
come from the event the report describes, and the identifier, the status
and the description from the protocol outside it, through the `$archetype`
paths FHIRconnect resolves against the archetype root.

## Mapping out of openEHR

`Mapping::to_fhir_with` maps a committed composition to a FHIR R4
`collection` Bundle: the report, the study and one `Provenance` covering
both. `Document::with_mapped_report` takes that Bundle into an imaging
report document built on `IMAGING_REPORT_FORM`: the report becomes the
`DiagnosticReportEuImaging` the `BundleReportEuImaging` holds beside its
composition, and the study and the `Provenance` become entries. The caller
lists the study in the imaging study section and writes the four sections
`CompositionEuImaging` requires. The document's assembly writes the two
categories the composition and the report require, LOINC `85430-7` and
`Medical-Imaging`, and makes the report's performing organizations the
composition's `Organization` authors (`dr-comp-author-org`).

## Receiving an imaging report

`CompositionEuImaging` binds `Composition.type` at strength `preferred`
only, and the guide's own examples type their reports with LOINC and CPT
codes. `ReceivedDocument::category` therefore names an imaging report by
the `Medical-Imaging` coding of `Composition.category`, which the profile's
`category:imaging` slice requires (`Category::document_category`).
`Category::profiles` then names `BundleReportEuImaging` and
`CompositionEuImaging`, which `ReceivedDocument::check` holds the document
to. `Mapping::to_openehr` maps the document's report into one composition
of the imaging template and keeps the document as received in the
composition's `FEEDER_AUDIT.original_content`.

The archetype holds one report identifier. A report in a
`BundleReportEuImaging` that the assembly wrote carries the document's
identifier beside its own, so the run keeps one, declares the other lost,
and the original document still holds both.

The guide writes `ImagingStudy.basedOn` as an accession number by
`identifier` alone, with no `type`. FHIR R4 admits such a logical
reference, and `ImagingStudy.basedOn` names no resource that can be a
patient, so the reader admits it there. An untyped logical reference stays
refused where the element's target types include `Patient`, `Group` or any
resource.

Full validation of a received imaging report by `ferrobridge-validator`
needs the Imaging Report package and its dependency closure in the
profile package set, which names cross-version packages and version
ranges.
