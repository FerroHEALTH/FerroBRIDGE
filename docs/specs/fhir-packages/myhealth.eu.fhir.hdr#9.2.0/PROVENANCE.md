<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: myhealth.eu.fhir.hdr 9.2.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: myhealth.eu.fhir.hdr
- Version: 9.2.0
- Source: the guide's canonical site, http://fhir.ehdsi.eu/hdr
- Tarball: https://build-fhir.ehdsi.eu/hdr/package.tgz
- SHA-1 (tarball): b5f158b2af07ec4d9dd0aa349220a1e5461cabbd
- SHA-256 (tarball): f3b649348d82c94821ef201ab9513517cfc143aa30cf623093f8772f21b93f6b
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: MyHealth@Eu (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 68
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `525bbbac099beb266d22d9eebb2cc664c84f1cc6279aae286a8319cce82f57de`
- Built: 20261124162202 (the `date` field of `package/package.json`), a CI build
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.5.0, `hl7.fhir.uv.extensions.r4` 5.4.0, `hl7.fhir.uv.ips` 2.0.1, `hl7.fhir.eu.laboratory` 2.1.0, `myhealth.eu.fhir.laboratory` 9.2.0, `hl7.fhir.eu.hdr` 1.0.0, `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.extensions.r4` 1.3.1, `ihe.pharm.mpd.r4` 1.0.0-comment-2, `myhealth.eu.fhir.mvc-package` 9.1.0, `myhealth.eu.fhir.core` 1.0.1, `xtehr.eu.ehds.models` 1.0.0
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

The FHIR package registry lists no 9.2.0 of myhealth.eu.fhir.hdr: the version is a
continuous-integration build of the guide, served at an unversioned URL its
publisher overwrites with every build. The pin is the sha256 in
docs/VERSIONS.md, so a moved build is refused here and reported by
`scripts/checks/pin-freshness.sh`, and a re-pin records the new build.
