- The published crates drop the functions that only forwarded to another,
  so a caller names the one that decides: `fhirconnect` takes the
  template pin in `resolve::select::select_by_profile`, the code in
  `engine::library::ratio::to_fhir` and `to_openehr`, and
  `resolve::derive::STRUCTURAL` in place of `is_structural`;
  `ferrobridge-hl7v2` takes the row's source in `map::condition::parse`
  and drops `parse::Component::text` for `subcomponent(1)`; `eehrxf` takes
  an optional template context in `terminology::transcode::transcode` and
  `transcode_with`, which replace `transcode_local` and
  `transcode_local_with`; `ferrobridge-validator` names the release in
  `set::PackageSet::load` and `new`, which replace `load_release` and
  `new_release`; `omop-cdm` takes the origin in `writer::CdmWriter::commit`,
  which replaces `commit_from`, and rebuilds an era through
  `derived::era`; the mapping sets and the registry are built with
  `Default`.
