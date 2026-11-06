<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.eu.hdr 1.0.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.eu.hdr
- Version: 1.0.0
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.eu.hdr
- Tarball: https://packages.simplifier.net/hl7.fhir.eu.hdr/1.0.0
- SHA-1 (registry shasum): b5bdad5f7928c6317a37e7567bf2c8952231cb0f
- SHA-256 (tarball): eedf9af9f66b68cd66f7d6c678a36dfba86b0920df4ef15e8d2da900c4134879
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 Europe (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 120
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `184630f1c3f11ac5b27b3d024347fc68cfc4aa55abcb545255a3fa5b4c9f3834`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.4.0, `xtehr.eu.ehds.models` 1.0.0, `hl7.fhir.uv.ips` 2.0.1, `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.uv.extensions.r4` 5.3.0, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.extensions.r4` 1.3.1, `ihe.pharm.mpd.r4` 1.0.0-comment-2
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
- Stamped: 2026-11-06

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
