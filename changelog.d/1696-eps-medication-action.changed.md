- The `eehrxf` patient summary project (`mappings::EPS`) has one root context,
  `ferrobridge_eu_eps.context`, which maps the composition to a
  `composition-eu-eps` and enters each section through the sub-context its
  section path names (FHIRconnect 1.0.0 sub-contexts). A medication action
  under the medication summary section is now a `MedicationStatement-eu-eps`
  of the medication summary, and one under the immunisations section stays an
  immunisation. This breaks the project's API: `EPS.contexts()` names the
  root alone, `ferrobridge_eu_eps_medication.context` and
  `ferrobridge_eu_eps_immunization.context` are `scope: sub` and are no
  longer compiled on their own, and a document takes the run with
  `Document::with_mapped_composition`.
- `Document::with_mapped_section` takes one section of a run whose context
  wrote the `Composition`: the entries that section names, and each
  `Provenance` narrowed to them. A document assembled one section at a time
  takes only what it assembles from each run.
