- `eehrxf::draft` crosswalks every element of Annexes III to X of the draft
  EEHRxF implementing act (initiative 17512, 2026-11-23) to the Xt-EHR
  logical models 1.0.0, with the cardinality on both sides and a verdict
  per row: same, renamed, cardinality differs, missing in Xt-EHR or missing
  in the draft. `AnnexCrosswalk::check_xtehr` and `check_annexes` hold the
  rows to the package and to the annex text, so a change on either side
  fails the crate's tests. `Cardinality::new` builds a cardinality.
