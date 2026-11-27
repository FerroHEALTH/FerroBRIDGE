<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Loading FHIR into the CDM with no CDR

**Audience:** integrators whose site exposes FHIR R4 and runs no openEHR CDR.

`ferrobridge etl fhir` chains a FHIRconnect set and an OMOCL set to load FHIR
resources into the CDM. The CDM side works as [The OMOP ETL](omop-etl.md)
describes.

## Loading FHIR with no CDR

A site that exposes FHIR R4 and runs no openEHR CDR loads the CDM with
`ferrobridge etl fhir PATH`. The job reads a Bundle, one resource or an NDJSON
file of resources (the FHIR Bulk Data format), and runs each resource through
the two mapping languages in turn: the FHIRconnect set of `[mappings]
directory`, compiled against the templates of `[mappings] templates`, maps it
into an openEHR composition held in memory, and the OMOCL set of `[mappings]
omocl` maps that composition to CDM rows the writer commits. No composition is
stored anywhere. The job reads `[etl]` for the type concepts and the
vocabulary aliases, and `[cdm]` for the database; it needs no `[cdr]` and no
`[etl] aql`. No specification governs the chaining: it is FerroBRIDGE's own
design, and each hop stays governed by its own specification.

- **Identity.** A composition built in memory has no identifiers a CDR issued,
  so the job derives them: the versioned composition from the resource's type
  and id, the version from `meta.versionId` when it is a positive integer
  (`1` otherwise), and the EHR from the reference the resource names its
  subject by, a Patient naming itself. Running the job again over the same
  input writes the same rows under the same ids, and a resource whose
  `meta.versionId` moves replaces its rows.
- **Time.** A resource is mapped at its `meta.lastUpdated`, so a composition
  field FHIRconnect defaults to the run time takes the same value on every
  run. A resource without one takes the time the job started.
- **What reaches the CDM** is what the two mapping sets cover. The published
  FHIRconnect library's Procedure and MedicationAdministration mappings are
  left out at load (`mapping check` names why), so a site maps those
  resources with mappings of its own. An Encounter reaches no VISIT_OCCURRENCE,
  and the job derives no visit, since the visit query reads a CDR.
- **Nothing disappears.** A resource no FHIRconnect program maps, one the
  FHIRconnect run refuses, one with no id or no subject, and a composition
  OMOCL refuses are each counted and listed in the run report, which adds a
  `resources` line (read, mapped, unmapped, refused, and the losses the
  FHIRconnect run declared).
- **Every element has an outcome.** The JSON report carries a `ledger`: per
  input resource, every element with a value and one of four outcomes:
  `mapped` (with the openEHR node it reached and the `table.column` targets
  that read it), `dropped` (a mapping declared it does not run, with the
  loss), `refused` (with the reason), or `unmapped` (no mapping names it, or
  no OMOCL column reads where it went). A data-type cell reads its element
  whole, so the elements below one share its outcome, and the text report
  adds a `ledger` line with the counts.
- **Every row names its resource.** The bridge schema's `resource_origin`
  table ties each composition to the resource type, id and `meta.versionId`
  it was mapped from, and through the natural-key side table each CDM row to
  that resource. To find where a row came from, or every row a resource
  became:

  ```sql
  SELECT o.resource_type, o.resource_id, o.resource_version
  FROM ferrobridge.record r
  JOIN ferrobridge.resource_origin o USING (versioned_object_uid)
  WHERE r.cdm_table = 'condition_occurrence' AND r.surrogate_id = 1;

  SELECT r.cdm_table, r.surrogate_id
  FROM ferrobridge.resource_origin o
  JOIN ferrobridge.record r USING (versioned_object_uid)
  WHERE o.resource_type = 'Condition' AND o.resource_id = 'c-1';
  ```
