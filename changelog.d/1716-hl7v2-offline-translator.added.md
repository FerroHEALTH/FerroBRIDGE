- `ferrobridge-hl7v2` translates a v2 table value through
  `map::translate::Translator`: a terminology server's
  `ConceptMap/$translate` (`Translator::Server`), or the same v2-to-FHIR
  table maps read from the corpus in process (`Translator::Offline`), which
  answers as R4 `$translate` does, `reverse` included, so a consumer with no
  terminology server maps table values from the one source. A value no map
  holds is still counted as `untranslated`. `map::single::segment` maps one
  placed segment to one resource through its segment map, outside a message
  `Bundle` (a PID to the `Patient` a FHIR source would send), with the same
  outcomes and the resources its `(Type)` rows create beside it.
- A shipped supplement of `datatype-xpn-to-string` writes XPN.1 through
  `$value` where the guide writes `$this`, a target its notation does not
  define, so PID-6 maps to the mother's maiden name extension (#1728).
