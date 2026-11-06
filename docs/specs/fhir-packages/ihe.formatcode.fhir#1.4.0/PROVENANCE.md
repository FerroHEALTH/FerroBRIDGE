<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.formatcode.fhir 1.4.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.formatcode.fhir
- Version: 1.4.0
- Source: the FHIR package registry, https://packages.fhir.org/ihe.formatcode.fhir
- Tarball: https://packages.simplifier.net/ihe.formatcode.fhir/1.4.0
- SHA-1 (registry shasum): 4744b61f8a65a176348f5177509d07a8d9e92164
- SHA-256 (tarball): d803d12f3b51940ae44f8a72372918cc409052e072d7e22bc968de58e301a34c
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: Integrating the Healthcare Enterprise (IHE) (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 11
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `57640df5a7d0136621359309c1c4c4f37b06e67f77769230ed34366509e962ff`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.3.0, `hl7.fhir.uv.extensions.r4` 5.2.0
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
