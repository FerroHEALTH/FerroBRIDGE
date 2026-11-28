- FerroBRIDGE's discharge report template is now
  `openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.1.0`. The
  `Comment` element added for #2398 is a new optional node, a minor step of
  the template's version, so the template could not keep v1.0.0. The HDR
  project's context names v1.1.0. A system that vendors the template takes
  `tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_discharge_report.adlt`
  from a FerroBRIDGE commit that holds v1.1.0, as it took v1.0.0, and moves
  to `eehrxf` 0.51.
