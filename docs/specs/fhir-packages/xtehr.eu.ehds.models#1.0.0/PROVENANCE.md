<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: xtehr.eu.ehds.models 1.0.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: xtehr.eu.ehds.models
- Version: 1.0.0
- Source: the FHIR package registry, https://packages.fhir.org/xtehr.eu.ehds.models
- Tarball: https://packages.simplifier.net/xtehr.eu.ehds.models/1.0.0
- SHA-1 (registry shasum): 702a1a247db08e31868c0b69931dc697f9af7ea0
- SHA-256 (tarball): a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: Xt-EHR (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 131
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e973efecdf61b2ae646407cafbf27177f81c82630938e1f8f2b6b0d20185b184`
- Declared dependencies (`package/package.json`): `hl7.terminology.r5` 7.1.0, `hl7.fhir.uv.extensions.r5` 5.2.0
- FHIR versions (`fhirVersions` of `package/package.json`): 5.0.0
- Stamped: 2026-11-06

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`, and except the
dependencies of a package of another FHIR release, which the R4 set holds
and never reads. The two lines above let `--check` assert the closure from
the committed files alone.

## Why build time

The package set unpacks to about 650 MB, seven versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
