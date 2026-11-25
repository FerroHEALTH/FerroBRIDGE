<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.r6.examples

Fetched verbatim at build time by `scripts/vendor/fhir-packages.sh
--build-time` into `package/` beside this file, which `.gitignore` refuses
(.claude/rules/vendored-inputs.md). Never commit a file from there and never
edit one: change the pin in docs/VERSIONS.md, run the script with
`--build-time --stamp`, and record the count and digest it prints.

- Package: hl7.fhir.r6.examples
- Version: 6.0.0-snapshot1
- Source: the FHIR package registry, https://packages2.fhir.org/packages/hl7.fhir.r6.examples
- Tarball: https://packages-cdn.fhir.org/web/hl7.fhir.r6.examples-6.0.0-snapshot1.tgz
- SHA-1 (registry shasum): ade2e483ee9e91b596ce0a6fa97b694840781545
- SHA-256 (tarball): d615c89f3ceb80943bb026be33147dac70c6d8bd12678cf0a5572b8aa990c843
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 2323
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `cde674bd7b53b7030c8d4d7f012ed66f7f934e82a7e41f6aea8d8a705a19636a`
- Contents: 2321 resources of 119 resource types, one JSON file each,
  beside `package.json` and `.index.json`
- Stamped: 2026-11-24

## Why build time

The five examples packages unpack to about 680 MB, which would more than
double the 411 MB of FHIR packages that `tools/fhir-codegen/vendor/` commits,
for corpora only the conformance tests read. The licence would permit
vendoring; the size decides. CI restores the trees from a cache keyed on
`scripts/vendor/fhir-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.

The examples are HL7's own published examples, never patient data.
