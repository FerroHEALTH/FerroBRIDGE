- **`serde-saphyr` 2.0.0.** The mapping loader, the search declaration
  reader and the fuzz targets parse YAML with `serde-saphyr` 2.0.0. A
  consumer of `openehr-mapping-core` that walks the source of
  `LoadError::Yaml` now meets `serde_saphyr::Error` of 2.0.0, whose
  `AliasError` carries the original error.
  `openehr-mapping-core` 0.1.135, `fhirconnect` 0.1.203 and
  `ferrobridge-hl7v2` 0.1.161 follow their requirements.
