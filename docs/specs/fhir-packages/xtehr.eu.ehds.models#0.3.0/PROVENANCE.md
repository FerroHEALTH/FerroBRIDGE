<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: xtehr.eu.ehds.models 0.3.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: xtehr.eu.ehds.models
- Version: 0.3.0
- Source: the FHIR package registry, https://packages.fhir.org/xtehr.eu.ehds.models
- Tarball: https://packages.simplifier.net/xtehr.eu.ehds.models/0.3.0
- SHA-1 (registry shasum): 7539da11561a6f0834813375745b1cbc8dcb4ed7
- SHA-256 (tarball): f2cb6b0b5b35cab33e9b618b38c54797ece61f5e2dda59982c952ecd9398865b
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: Xt-EHR (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 49
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `c9ca078a854e0777f4a4d02afe826556b8cfeb3591ec83819c6a1d99a556597c`
- Declared dependencies (`package/package.json`): `hl7.terminology.r5` 7.0.1, `hl7.fhir.uv.extensions.r5` 5.2.0
- FHIR versions (`fhirVersions` of `package/package.json`): 5.0.0
- Stamped: 2026-11-25

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
