- The `eehrxf` coding rule follows draft EEHRxF Art 6 (initiative 17512,
  2026-11-23). A translation the terminology server states `equivalent` or
  `equal` carries the concept map that made it in the `elementSource`
  extension, and `Translator::mapped` records each one with its original
  coding, equivalence and map. With `Translator::for_non_english`, a code no
  map covers takes its English designation from `$lookup`, as its display
  or in the `translation` extension of its display, and
  `Translator::unmapped` counts the gaps. Breaking for a consumer:
  `Terminology::translate` answers `Translated` values in place of codings,
  `Match` gains `source`, and `TerminologyServer` gains `designation`, which
  an implementation answers from the `designation` parts of `$lookup`.
