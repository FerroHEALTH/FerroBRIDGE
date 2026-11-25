<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.eu.imaging-manifest 1.0.0-ballot

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.eu.imaging-manifest
- Version: 1.0.0-ballot
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.eu.imaging-manifest
- Tarball: https://packages.simplifier.net/hl7.fhir.eu.imaging-manifest/1.0.0-ballot
- SHA-1 (registry shasum): da571eeb67fba6321483e4166a80b9f223c8440f
- SHA-256 (tarball): 5cc2fc0ef3c03b4563deeda71acaa33c93ced3876b5a270a23738cbe7b444be6
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 Europe (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 47
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b51c4f440b48fdd7a942001507f010f8698e2ab944e4241b6a8a688ebaf8ceac`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.4.0, `hl7.fhir.uv.extensions.r4` 5.3.0, `ihe.rad.mado` 1.0.0, `ihe.iti.mhd` 4.2.3, `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.extensions.r4` 1.3.1, `hl7.fhir.uv.xver-r5.r4` 0.1.0
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
