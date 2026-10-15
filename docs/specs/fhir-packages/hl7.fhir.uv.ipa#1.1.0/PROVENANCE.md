<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.uv.ipa 1.1.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.uv.ipa
- Version: 1.1.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.uv.ipa
- Tarball: https://packages.simplifier.net/hl7.fhir.uv.ipa/1.1.0
- SHA-1 (registry shasum): 96070135b1bfed2fde420929088f775ced151c45
- SHA-256 (tarball): 013b9f91683c496fc09eed93a8798b47dcbfb775587890fa3e11253a2409feef
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International / Patient Care (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 53
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `70a5f0881a37859ef7fefd5502c0db05c6ce6c2d6dfa1e71828785f523ba7021`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.2.0, `hl7.fhir.uv.extensions.r4` 5.2.0, `hl7.fhir.uv.smart-app-launch` 2.0.0
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
