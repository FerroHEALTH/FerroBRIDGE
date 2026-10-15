<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.uv.smart-app-launch 2.0.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.uv.smart-app-launch
- Version: 2.0.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.uv.smart-app-launch
- Tarball: https://packages.simplifier.net/hl7.fhir.uv.smart-app-launch/2.0.0
- SHA-1 (registry shasum): 36f40e4fe48c8f5bc9a86712d78545bc349f2146
- SHA-256 (tarball): 627e7a64e7abe130201fbae2ee7d2cb85d7d94972458d302c4d06d59e2c7d084
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: HL7 International - FHIR Infrastructure WG (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 8
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `68fd66519d2ff373d8f9e58bfa62455f3e0aeb95a780f2218e5ffee5c4b49c3e`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1
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
