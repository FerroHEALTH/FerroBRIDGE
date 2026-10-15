<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.terminology.r4 7.1.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.terminology.r4
- Version: 7.1.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.terminology.r4
- Tarball: https://packages.simplifier.net/hl7.terminology.r4/7.1.0
- SHA-1 (registry shasum): 19a681d8bd598b5c0da72e058616d78ae70940c0
- SHA-256 (tarball): 1cb0cd5601972925fcd04f2c175d9cc63a9ecd9346a91a6e735f1a865dc5fba1
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International - Vocabulary Work Group (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 4118
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `0f91757bc29290a3c1c090cf3fe57c884f5af91472182157bac7cf4d0ead73c3`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.fhir.uv.extensions.r4` 5.2.0
- Stamped: 2026-10-15

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`. The line above lets
`--check` assert the closure from the committed files alone.

## Why build time

The package set unpacks to about 570 MB, five versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
