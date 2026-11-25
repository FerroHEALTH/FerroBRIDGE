<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.terminology.r4 7.5.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.terminology.r4
- Version: 7.5.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.terminology.r4
- Tarball: https://packages.simplifier.net/hl7.terminology.r4/7.5.0
- SHA-1 (registry shasum): 4e0979b25b8cceb84bf854a9c1e98024436125ea
- SHA-256 (tarball): 1da3004587602c3f5120061abc2309fef1f96668a477a9964354fa9af6fa6e60
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International - Vocabulary Work Group (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 4235
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d6e3916c8eee0ff581a50cc19ca1183a847d1891dbc2f96ad172fc884d4bf298`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1
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
