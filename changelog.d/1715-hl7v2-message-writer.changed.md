- `ferrobridge-hl7v2` reads `\P\` as the truncation character a v2.7 or
  later MSH-2 declares, and refuses it only in a message that declares none
  (HL7 v2.9 chapter 2 §2.7.1). A field beyond the table of a segment whose
  last field is `varies` (QPD-3 "User Parameters (in successive fields)",
  RDT) is no longer counted as `extra-field`. `parse::Field::written` returns
  a field as the message wrote it, `parse::Location::components` the ERL
  components, and `parse::Delimiters::escape` escapes the truncation
  character. Breaking for a consumer: `ack::Header` holds `parse::Delimiters`
  (`Header::delimiters`), and a reply's trailing empty fields are left out
  up to the last field valued or echoed.
