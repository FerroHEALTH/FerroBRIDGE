- `openehr_mapping_core::index::WebTemplateIndex::example_composition` builds
  the example with `openehr_sdt::flat::example::example_composition` at the
  `medium` detail level, falling back to `required`, gives it the output
  form's deterministic `uid` and validates it against the template. The
  method takes no `now` argument any more, so a caller drops it, and an
  example names the `uid` `openehr-sdt` derives from the template id
  (`<uuid>::example.server::1`) and that crate's synthetic values. The
  operator API's `GET /operator/templates/{id}/example` answers the same
  example.
