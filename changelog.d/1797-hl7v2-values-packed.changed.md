- The HL7 v2 value package keeps its FHIR manifest as
  `terminology/hl7v2-values/fhir-package.json`, and
  `scripts/terminology/pack-hl7v2-values.sh` packs it into the FHIR package
  tarball with `package/package.json`, which is the form you load into the
  terminology server. A file named `package.json` in the source tree was read
  as an npm manifest whose FHIR package dependencies npm resolves to other
  projects. A committed guard now refuses such a manifest.
