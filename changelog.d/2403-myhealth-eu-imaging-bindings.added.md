- `eehrxf`: the MyHealth@EU Imaging 9.1.1 CI build is vendored under
  `docs/specs/ehdsi-imaging/`, outside the profile validator's package set
  because its dependency `hl7.fhir.eu.imaging` 1.0.0-alpha is published
  nowhere. The binding tests read its required bindings: the one on an
  element the component writes, `ImagingStudy.series.bodySite`
  (`eHDSIBodySiteWithExceptions`), takes SNOMED CT and needs no translation,
  and every MyHealth@EU value set the guide requires resolves.
