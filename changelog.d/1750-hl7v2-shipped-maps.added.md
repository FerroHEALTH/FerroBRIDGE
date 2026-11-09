- `ferrobridge-hl7v2` embeds every ConceptMap of the v2-to-FHIR guide
  (`hl7.fhir.uv.v2mappings` 1.0.0, 263 maps, about 3.9 MB, CC0-1.0) behind a
  `shipped-maps` feature: `Corpus::shipped()` loads them with no package on
  disk, `with_shipped_supplements()` adds the crate's supplements, and
  `Corpus::load(dir)` still reads a newer package. The vendored package moves
  to `crates/ferrobridge-hl7v2/vendor/hl7.fhir.uv.v2mappings/`; the perf
  gate's `concept_maps` path moves with it.
