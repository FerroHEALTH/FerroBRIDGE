<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.formatcode.fhir 1.6.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.formatcode.fhir
- Version: 1.6.0
- Source: the FHIR package registry, https://packages.fhir.org/ihe.formatcode.fhir
- Tarball: https://packages.simplifier.net/ihe.formatcode.fhir/1.6.0
- SHA-1 (registry shasum): f7c889247fe144ecc4e673c38ac1963cd72f231b
- SHA-256 (tarball): 638f8e6caa39fced9e39a106d7e82064b07d78bd60892b74b812dfd40e6c5b06
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: Integrating the Healthcare Enterprise (IHE) (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 11
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `75224d1142b43eeede33ab06aa3ffa69fec1a3c46a97335b1e9c476d6221a06b`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.3.0, `hl7.fhir.uv.extensions.r4` 5.3.0
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
- Stamped: 2026-11-06

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`, and except the
dependencies of a package of another FHIR release, which the R4 set holds
and never reads. The two lines above let `--check` assert the closure from
the committed files alone.

## Why build time

The package set unpacks to about 970 MB, eight versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
