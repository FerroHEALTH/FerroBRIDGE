<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: myhealth.eu.fhir.euvac 10.0.0-preview

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: myhealth.eu.fhir.euvac
- Version: 10.0.0-preview
- Source: the FHIR package registry, https://packages.fhir.org/myhealth.eu.fhir.euvac
- Tarball: https://packages.simplifier.net/myhealth.eu.fhir.euvac/10.0.0-preview
- SHA-1 (registry shasum): 47971806ef3a83b32305946f02461893047b4480
- SHA-256 (tarball): 96bf16bc13cf01b1c9a7642aef20704cb210c4900d83ae4ec6511081535fe3f6
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: MyHealth@Eu (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 28
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e49726139bce7a8120eee801905ba45c8394839e44758097b5da26994a9c521e`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.2.0, `hl7.fhir.uv.extensions.r4` 5.3.0, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.base` 2.0.0, `xtehr.eu.ehds.models` 0.3.0, `myhealth.eu.fhir.core` 1.0.0
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
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

## Why a preview

The version is a preview of the guide, published to the FHIR package
registry ahead of a release and not a release itself, built
20260716191120 (the `date` field of `package/package.json`). The registry's
tarball is the pin; a later preview or the release is a new pin with its own
row.
