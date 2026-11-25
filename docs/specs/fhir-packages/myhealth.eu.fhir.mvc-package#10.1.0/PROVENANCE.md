<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: myhealth.eu.fhir.mvc-package 10.1.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: myhealth.eu.fhir.mvc-package
- Version: 10.1.0
- Source: the guide's canonical site, http://fhir.ehdsi.eu/mvc-package
- Tarball: https://build-fhir.ehdsi.eu/mvc-package/package.tgz
- SHA-1 (tarball): 7d34cd6778d69d72d2a1bb3c5f0fcd91a5009a77
- SHA-256 (tarball): 8f42b6d0e3a5bad3517a7fe01025b1112d80b12503a66fa8495f9806a81de0e4
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: MyHealth@Eu (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 103
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `28134b577e017781e483d2f275a256010301e449ca3a005ae1041951068a0a55`
- Built: 20261029080840 (the `date` field of `package/package.json`), a CI build
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.5.0, `hl7.fhir.uv.extensions.r4` 5.4.0, `hl7.fhir.uv.ips` 1.1.0
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

## Why a CI build

The FHIR package registry lists no 10.1.0 of myhealth.eu.fhir.mvc-package: the version is a
continuous-integration build of the guide, served at an unversioned URL its
publisher overwrites with every build. The pin is the sha256 in
docs/VERSIONS.md, so a moved build is refused here and reported by
`scripts/checks/pin-freshness.sh`, and a re-pin records the new build.
