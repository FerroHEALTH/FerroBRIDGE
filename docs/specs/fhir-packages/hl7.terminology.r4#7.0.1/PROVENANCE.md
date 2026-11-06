<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.terminology.r4 7.0.1

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.terminology.r4
- Version: 7.0.1
- Source: the FHIR package registry, https://packages.fhir.org/hl7.terminology.r4
- Tarball: https://packages.simplifier.net/hl7.terminology.r4/7.0.1
- SHA-1 (registry shasum): 978f824f2bded123a8e7f1431dfb8c20acc468e8
- SHA-256 (tarball): 7e88619af8d6ec06f0f521a64800da7d8197ceb1c451c293ec1b796efce3840e
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International - Vocabulary Work Group (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 4111
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d603014a453916eadf2323990ab6c7c20b0f6d05deae055b2c1abb149b153441`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.fhir.uv.extensions.r4` 5.2.0
- Stamped: 2026-11-06

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`. The line above lets
`--check` assert the closure from the committed files alone.

## Why build time

The package set unpacks to about 650 MB, six versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
