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
CKM's. `Mapping::compile_project` compiles a project from its embedded
text with no file written, and `Project::write_to` writes a project's files
where a system keeps its mappings, the paths `Mapping::compile` takes. FerroBRIDGE's own
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

Each result is an `Observation-resultslab-eu-lab`, which requires an
`effective[x]` and a `performer`. The result takes the time of its
laboratory test result's event. `KDS_Laborbericht` holds no party for the
performing laboratory, so the mapping writes `performer` absent: a
`Reference` carrying only the FHIR R4 `data-absent-reason` extension with
the code `unknown`. The run declares that element as a loss in its
`OperationOutcome`, so the gap stays visible. A template that carries the
performing laboratory as a party would let the mapping write the real
performer and remove the absent one.

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

A result's reference range maps into the `normal_range` of its quantity,
both bounds with their units, and back out as `referenceRange`. The range
mapping is appended under `analyteMeasurement`, the mapping that writes the
analyte value. `KDS_Laborbericht` allows any number of analyte clusters and
exactly one value element `items[at0001]` in each, and `analyteMeasurement`
writes that `DV_QUANTITY` whole, so the bounds merge into it as its
children. A mapping of its own onto `items[at0001]` would write a second
value with no `magnitude`, which the reference model refuses. R4 reads a
range with no `type` as the normal range, so such a range maps to
`normal_range`. A range with a `type`, such as a treatment range, maps to
one element of the quantity's `other_reference_ranges`: its `type` is the
element's `meaning`, and its `low` and `high` the bounds of the element's
`range`. Served out of openEHR, each element of `other_reference_ranges`
comes back as a `referenceRange` with that `type`, after the normal range,
in the order the composition holds them. The
order a report answers maps by its identifier into the template's test
request details (`protocol[at0004]/items[at0094]/items[at0063]`), which
`KDS_Laborbericht` annotates with `DiagnosticReport.basedOn`, and comes
back as a `Reference` to a `ServiceRequest`.

The laboratory project accounts for `DiagnosticReport.basedOn`,
`Observation.basedOn` and `Observation.referenceRange`
(`LABORATORY.accounted()`). Each received instance of one that the run
carries into no structured element is a `warning` issue in the
composition's `OperationOutcome`, its `expression` the element's path: a
range's `text`, the order a result answers, or an
order named by `reference` alone. The original document still holds them.
