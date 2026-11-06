- `eehrxf` maps discharge reports between openEHR and HL7 Europe Hospital
  Discharge Report 1.0.0 through the shipped `HDR` project (features
  `openehr` and `discharge`), over FerroBRIDGE's discharge report template,
  a transfer of care summary over published CKM archetypes. The composition
  maps to `composition-eu-hdr`, its context to the `encounter-eu-hdr` of the
  stay, and each problem or diagnosis of its diagnostic summary to a
  `condition-eu-core` entry, with one `Provenance` per run.
  `Document::with_mapped_composition` takes the run into a `bundle-eu-hdr`,
  answering each mapped section placed in the form's slot as a
  `MappedSection`, and `Mapping::to_openehr` maps a received discharge report
  into one composition of the template. `scripts/vendor/ckm-discharge.sh`
  vendors the three archetypes the template adds to the CKM medication set.
