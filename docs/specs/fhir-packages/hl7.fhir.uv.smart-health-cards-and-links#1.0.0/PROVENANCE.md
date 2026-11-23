<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.uv.smart-health-cards-and-links 1.0.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.uv.smart-health-cards-and-links
- Version: 1.0.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.uv.smart-health-cards-and-links
- Tarball: https://packages.simplifier.net/hl7.fhir.uv.smart-health-cards-and-links/1.0.0
- SHA-1 (registry shasum): 78ba3eb78e7b09c0fe76ebbc89c212e832a7096f
- SHA-256 (tarball): 488d31d5f492e7d6e2876cd7c9ab15dec6aadc7824ce896ad0897e936faa5d92
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 International / FHIR Infrastructure (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 20
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `bf73d0270cce9955a313b755df96a362b21d4faeac6c7a184fc174ff6cf2c480`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.5.0, `hl7.fhir.uv.extensions.r4` 5.2.0
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
- Stamped: 2026-11-23

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
