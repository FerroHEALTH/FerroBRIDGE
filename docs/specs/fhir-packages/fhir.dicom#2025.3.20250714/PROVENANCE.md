<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: fhir.dicom 2025.3.20250714

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: fhir.dicom
- Version: 2025.3.20250714
- Source: the FHIR package registry, https://packages.fhir.org/fhir.dicom
- Tarball: https://packages.simplifier.net/fhir.dicom/2025.3.20250714
- SHA-1 (registry shasum): d43a41bcaf3ec6e2001ee90d0f681914a9dfdea1
- SHA-256 (tarball): f101876ed4463ac7b814ba0e38c97cc2a8b15aecaee66916f4644946443ba303
- Upstream license: free (the `license` field of `package/package.json`)
- Author: FHIR Project for DICOM (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 1415
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `47f417064ebb643814b66ee9e557a8a2f92e8ae1a7c0eb6d6817608871493ba1`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1
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
