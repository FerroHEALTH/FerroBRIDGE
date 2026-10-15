<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.uv.extensions.r4 5.2.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.uv.extensions.r4
- Version: 5.2.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.uv.extensions.r4
- Tarball: https://packages.simplifier.net/hl7.fhir.uv.extensions.r4/5.2.0
- SHA-1 (registry shasum): 2c7e21436dfa35ba7209696d63247538b851968a
- SHA-256 (tarball): b406e75575f05676559d0759770c5939d023ee72fb2ef38e0b3259328487720a
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International / FHIR Infrastructure (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 1401
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `727a50ef9b1c4472653d772ae0b470405b305c2c983c02f1f621ebcc5e05a5b3`
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
