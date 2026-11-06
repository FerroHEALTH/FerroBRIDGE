<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.eu.health-data-api 1.0.0-ballot

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.eu.health-data-api
- Version: 1.0.0-ballot
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.eu.health-data-api
- Tarball: https://packages.simplifier.net/hl7.fhir.eu.health-data-api/1.0.0-ballot
- SHA-1 (registry shasum): 03bc5bae2996adaa0137ff8468fa7e536e56ed68
- SHA-256 (tarball): 5204af60bf7f74f301a627e641a34683d392014f53d5df8ba3651c964e92977b
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 Europe (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 44
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `3256c01a6532d870b697caa1a1e6d5b82f042e5780148ff8cb5816accf7bb0ee`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.1.0, `hl7.fhir.uv.extensions.r4` 5.2.0, `hl7.fhir.eu.base` 2.0.0-ballot, `ihe.iti.pdqm` 3.2.x, `ihe.iti.mhd` 4.2.x, `hl7.fhir.uv.ipa` 1.1.x
- Resolved ranges (the highest published version each matches): `ihe.iti.pdqm` 3.2.x as 3.2.0, `ihe.iti.mhd` 4.2.x as 4.2.5, `hl7.fhir.uv.ipa` 1.1.x as 1.1.0
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
