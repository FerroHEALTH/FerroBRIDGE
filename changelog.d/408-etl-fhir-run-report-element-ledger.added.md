- The run report of `etl fhir` carries an element ledger (#408): per input
  resource, every element with a value and one of four outcomes. `mapped`
  names the openEHR node a FHIRconnect mapping read the element into and the
  CDM `table.column` targets an OMOCL column reads there, `dropped` names the
  loss a mapping declared, `refused` the reason the resource or its
  composition was refused, and `unmapped` marks an element no mapping names
  or no column reads where it went. The text report adds a `ledger` line with
  the counts. `fhirconnect` 0.1.193 records every element a mapping reads
  going into openEHR (`Outcome::reads`, `Transformed::reads`), the OMOCL
  program names the composition node each CDM column reads
  (`Program::column_reads`), and `ferrobridge-hl7v2` moves to 0.1.147.
