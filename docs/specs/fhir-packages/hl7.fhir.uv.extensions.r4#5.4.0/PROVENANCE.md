<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.uv.extensions.r4 5.4.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.uv.extensions.r4
- Version: 5.4.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.uv.extensions.r4
- Tarball: https://packages.simplifier.net/hl7.fhir.uv.extensions.r4/5.4.0
- SHA-1 (registry shasum): e33711cde624bcc5203e0202399195b9e7b9e996
- SHA-256 (tarball): e93eb1ece7d14e6b3dfaa7af5ac510577957cb5fd2b61ef95cdcfbbdca81c96d
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International / FHIR Management Group (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 1647
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `6fd1c64e6da1c0b8cfb070c82702f1e8d1c1a5753cfffed14340b0a699e6ad7c`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.5.0
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
