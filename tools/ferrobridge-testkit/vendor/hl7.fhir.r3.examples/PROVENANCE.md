<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.r3.examples

Fetched verbatim at build time by `scripts/vendor/fhir-packages.sh
--build-time` into `package/` beside this file, which `.gitignore` refuses
(.claude/rules/vendored-inputs.md). Never commit a file from there and never
edit one: change the pin in docs/VERSIONS.md, run the script with
`--build-time --stamp`, and record the count and digest it prints.

- Package: hl7.fhir.r3.examples
- Version: 3.0.2
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.r3.examples
- Tarball: https://packages.simplifier.net/hl7.fhir.r3.examples/3.0.2
- SHA-1 (registry shasum): 79b656fef8ff63fd0c704880f1bc1629ebd761fb
- SHA-256 (tarball): ef8af52128c2dacecd6f5220b7631979938b7068e2e3eca36b0a3ce1b37f2e4b
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 8289
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e35e340680f49f88f6f91808f2691bce0bc20cd3faf2794edd2b98ceddb5803a`
- Contents: 8287 resources of 117 resource types, one JSON file each,
  beside `package.json` and `.index.json`
- Stamped: 2026-10-15

## Why build time

The five examples packages unpack to about 680 MB, which would more than
double the 411 MB of FHIR packages that `tools/fhir-codegen/vendor/` commits,
for corpora only the conformance tests read. The licence would permit
vendoring; the size decides. CI restores the trees from a cache keyed on
`scripts/vendor/fhir-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.

The examples are HL7's own published examples, never patient data.
