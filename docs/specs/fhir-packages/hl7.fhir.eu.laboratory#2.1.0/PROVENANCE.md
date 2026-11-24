<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.eu.laboratory 2.1.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.eu.laboratory
- Version: 2.1.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.eu.laboratory
- Tarball: https://packages.simplifier.net/hl7.fhir.eu.laboratory/2.1.0
- SHA-1 (registry shasum): e7ca1d9d947ddf41579e59348041cabff1b60a2e
- SHA-256 (tarball): 87019c396f4cff934ec5230ce1f97d76b79b40591be08161e694ad661129379c
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 Europe (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 110
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `bffb6f7bb6972965d22e53f4b80be1f626f739dc42b116eb24c204b31230a9f9`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.4.0, `hl7.fhir.uv.extensions.r4` 5.3.0, `hl7.fhir.uv.ips` 2.0.1, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.extensions.r4` 1.3.1, `hl7.fhir.eu.base` 2.0.1
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
- Stamped: 2026-11-24

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`, and except the
dependencies of a package of another FHIR release, which the R4 set holds
and never reads. The two lines above let `--check` assert the closure from
the committed files alone.

## Why build time

The package set holds several versions of large packages,
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among them, for input only
the validator and its tests read; docs/VERSIONS.md lists every pin. The
licences would permit vendoring; the size decides. CI restores the trees from
a cache keyed on `scripts/vendor/profile-packages.sh --cache-key`, and the
script verifies the count and digest of a restored tree as it does of a
download.
