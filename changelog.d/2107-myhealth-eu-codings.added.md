- `eehrxf` adds the codings the MyHealth@EU Core 1.0.1 bindings take. A
  translator given `MYHEALTH_EU_CORE` (`Translator::with_bindings`) asks the
  terminology server's `ConceptMap/$translate` for the ICD-10 coding of
  `Condition.code` and the ATC coding of `AllergyIntolerance.code`, and adds
  it beside the source's SNOMED CT coding with its mapping provenance (draft
  EEHRxF Art 6(2)). `Translator::unbound` reports each bound concept no
  translation reaches; no code is invented for it (#2107).
