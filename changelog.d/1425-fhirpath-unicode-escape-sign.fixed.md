- `ferrobridge-fhirpath`: a `\u` escape in a string or delimited identifier
  takes four hexadecimal digits and no sign, as the FHIRPath N1 grammar's
  `UNICODE` rule writes it. `'\u+041'` reads as `u+041` (N1 §4.1.2: a
  backslash before a non-escape is dropped) where it read as `A`, and
  `unescape('json')` refuses `\u+041` (RFC 8259 §7).
