- `eehrxf` writes the discharge report in the MyHealth@EU Hospital Discharge
  Report 9.2.0 form (`MYHEALTHEU_DISCHARGE_REPORT_FORM`), which a national
  contact point sends: its language is held to eHDSILanguage, and its author
  is the one the record names, never the gateway's `Device`
  (`Authorship::Record`). `recorded_author` writes the composition's
  identified composer as a `Practitioner`, or else its health care facility
  as an `Organization`, and refuses a composition that names neither.
  `MYHEALTHEU_PATIENT` moves from `document::vaccination` to `document`.
