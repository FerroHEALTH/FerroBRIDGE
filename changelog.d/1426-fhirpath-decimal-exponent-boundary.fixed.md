- `ferrobridge-fhirpath`: `lowBoundary()` and `highBoundary()` (the
  FHIRPath 3.0.0 STU functions behind
  `Environment::with_fhirpath_stu3_functions`) read a FHIR decimal written
  with an exponent at the precision of its mantissa. `9.5e2` now stands for
  `945` to `955`, so `highBoundary(0)` answers `955` where it answered
  `951`, and `1.5e-3` stands for `0.00145` to `0.00155`.
