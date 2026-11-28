- `eehrxf` 0.52.0: the laboratory project maps over
  `ferrobridge_laborbericht.v1.0.0`, the published `KDS_Laborbericht`
  template with the Comment (`at0007`) of `COMPOSITION.report-result.v1`
  revision 1.1.0 added to the composition's `other_context`. A received
  laboratory report's composition now names the elements it does not carry in
  that Comment, where a clinician reading the record sees them, as well as in
  its feeder audit. The template ships as
  `mappings/laboratory/ferrobridge_laborbericht.opt`. A CDR that stores
  received laboratory reports needs it uploaded, and compositions under the
  unmodified `KDS_Laborbericht` are no longer read by this project.
