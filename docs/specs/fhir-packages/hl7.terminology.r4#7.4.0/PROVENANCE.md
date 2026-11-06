<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.terminology.r4 7.4.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.terminology.r4
- Version: 7.4.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.terminology.r4
- Tarball: https://packages.simplifier.net/hl7.terminology.r4/7.4.0
- SHA-1 (registry shasum): 6e66667ecd95120a35de31f9642c0a612fa567ab
- SHA-256 (tarball): 1dc558c204f5ee1f2b96ab9a8cd83d0811475b08a4e06a7c2bcea55d1ab116ed
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International - Vocabulary Work Group (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 4226
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `1c1031303b355e2ea90579354cbd4d7665afa1a2ec39c831cc7ce773d4f51826`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1
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
