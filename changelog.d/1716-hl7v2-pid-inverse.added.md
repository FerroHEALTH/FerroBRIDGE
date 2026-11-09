- `ferrobridge-hl7v2` writes a FHIR R4 resource back as a v2 segment through
  the same v2-to-FHIR maps (`map::inverse::segment`), a `Patient` as a PID,
  returned as a `write::value::Segment`. The guide maps v2 to FHIR only, so
  the inverse follows rules of our own over its rows, stated in the module
  documentation; every element it does not write back, a table value no map
  translates back and one that translates back to several codes are typed,
  counted `inverse::Outcome`s, never a guess. Five shipped FHIR-to-v2 table
  maps name one code where the guide's maps, reversed, give several
  (administrative-gender, name-use, address-use, address-type and
  contact-point-system). `parse::structure::segment_of` gives a segment's
  table as a structure of a version places it, and `map::convert::temporal`
  converts a FHIR date or time back to its v2 form.
