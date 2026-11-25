<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: myhealth.eu.fhir.laboratory 9.2.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: myhealth.eu.fhir.laboratory
- Version: 9.2.0
- Source: the guide's canonical site, http://fhir.ehdsi.eu/laboratory
- Tarball: https://build-fhir.ehdsi.eu/laboratory/package.tgz
- SHA-1 (tarball): 82c1d73e5bcfa88ec1743b1e23ff92be4033a5b2
- SHA-256 (tarball): e258e974945caa6f0f41a0816231d617638f56c3f16bf6382760ecb2ca3dd0ce
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: MyHealth@EU (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 54
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `fcd9be9e5d78ba46d7cae52b2f8572221310e5a6a13f139fc8b6977c02ac94be`
- Built: 20261124114216 (the `date` field of `package/package.json`), a CI build
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.5.0, `hl7.fhir.uv.extensions.r4` 5.4.0, `hl7.fhir.uv.ips` 2.0.1, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.laboratory` 2.1.0, `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.extensions.r4` 1.3.1, `myhealth.eu.fhir.mvc-package` 9.1.0, `myhealth.eu.fhir.core` 1.0.1, `xtehr.eu.ehds.models` 1.0.0
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

The FHIR package registry lists no 9.2.0 of myhealth.eu.fhir.laboratory: the version is a
continuous-integration build of the guide, served at an unversioned URL its
publisher overwrites with every build. The pin is the sha256 in
docs/VERSIONS.md, so a moved build is refused here and reported by
`scripts/checks/pin-freshness.sh`, and a re-pin records the new build.
