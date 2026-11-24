- The FHIRPath evaluator reads an R4 decimal the exact decimal cannot hold
  with its stated precision, such as `1e999999999` (FHIR R4 datatypes.html
  §2.24.0.1), as `Item::Scaled` in its exponent form, where it refused the
  value before. It compares exactly, every computation on it is empty (N1
  §6.6), and `toString()` refuses one whose fixed-point form passes 4096
  digits with `EvalError::Unwritable`. `round()` past 28 places is empty.
  Breaking for a consumer that matches `Item` or `EvalError` exhaustively or
  calls `eval::convert::fhir_quantity`, which now returns an `Item`:
  `ferrobridge-fhirpath` 0.2.0, with `ferrobridge-validator` 0.19.0,
  `ferrobridge-hl7v2` 0.27.0 and `eehrxf` 0.27.0 on it.
