- `ferrobridge-hl7v2` composes any HL7 v2 message: `write::Writer` joins
  `write::value` segments, fields, repetitions, components and subcomponents
  in the message's delimiters, escapes each value of its own by HL7 v2.9
  chapter 2 §2.7.1 (the truncation character as `\P\` among them), echoes a
  field read in the same delimiters as the message wrote it, and
  `Writer::compose` checks the message against the structure the
  `hl7v2-types` tables define for the type and version its MSH declares and
  encodes it in the character set MSH-18 names. `write::header::Msh` writes
  an originated header, and `ack::Header::reply` and `ack::Header::msa` the
  header and MSA of any reply. `ack::render` is built on the writer, and an
  `ack::Error` converts to its `ERR` segment.
