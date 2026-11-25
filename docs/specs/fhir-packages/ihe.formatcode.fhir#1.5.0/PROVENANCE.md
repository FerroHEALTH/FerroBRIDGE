<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.formatcode.fhir 1.5.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.formatcode.fhir
- Version: 1.5.0
- Source: the FHIR package registry, https://packages.fhir.org/ihe.formatcode.fhir
- Tarball: https://packages.simplifier.net/ihe.formatcode.fhir/1.5.0
- SHA-1 (registry shasum): 7ba8abb29a7bab98807d807af0b592ec1d8d91e8
- SHA-256 (tarball): e178f9c5b793ca9df8111b4b660245613cdd7e201234a67fc568c6a743e5bb36
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: Integrating the Healthcare Enterprise (IHE) (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 11
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `8727847040984389fd846759d214801d2878e7711a6079b4396f56a12aa8f3a7`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.1.0, `hl7.fhir.uv.extensions.r4` 5.2.0
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
