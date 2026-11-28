- `fhir-types` 0.5.0 carries two more parameters of the terminology
  ecosystem overlay for R4, R4B, R5 and R6, both defined by the ecosystem
  IG's ICD-11 page (`icd-11.md`). `CodeSystem/$lookup` gains the
  `post-coordination-metadata` input (`0..1`, `boolean`; Properties): with it
  true, each `postcoordination` property lists every axis the code declares,
  valued or not. `CodeSystem/$validate-code` and `ValueSet/$validate-code`
  gain the `classificationCode` output (`0..1`, `code`): the short
  classification code of the normal form, when the code was given with
  entity ids. The new public fields break a struct literal of these
  contracts, so a consumer that builds one adds the new fields. The
  workspace builds on the published `fhir-types` 0.4.0 until `ihe-atna` and
  `ihe-iti` move to 0.5.
