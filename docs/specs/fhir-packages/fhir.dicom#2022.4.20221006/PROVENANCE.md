<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: fhir.dicom 2022.4.20221006

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: fhir.dicom
- Version: 2022.4.20221006
- Source: the FHIR package registry, https://packages.fhir.org/fhir.dicom
- Tarball: https://packages.simplifier.net/fhir.dicom/2022.4.20221006
- SHA-1 (registry shasum): a918c40b57c84f462a87aff74fe6c8466e613b3c
- SHA-256 (tarball): 117a6137c7f08220a92aadaec23f10e603730fc13b583d46011a777a1f23e7ed
- Upstream license: free (the `license` field of `package/package.json`)
- Author: FHIR Project for DICOM (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 1320
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `f595412784ad384927f8adcdda697c17354a7aade80775f374e04fa9bfe20e84`
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
