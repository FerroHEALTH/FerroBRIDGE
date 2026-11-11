- `eehrxf`: the `IMAGING` project carries the organization that performed an
  imaging report. A received `DiagnosticReport.performer` naming an
  `Organization` is written into the Imaging service slot of
  `OBSERVATION.imaging_exam_result.v1` as a `CLUSTER.organisation.v1` (its
  name and identifiers), and a served report names that organisation as its
  performer, an `organization-eu-core`. A performer that is no
  `Organization` is still declared not carried. FerroBRIDGE's imaging report
  template fills the slot, so a CDR that stores these reports needs the
  template's new revision.
