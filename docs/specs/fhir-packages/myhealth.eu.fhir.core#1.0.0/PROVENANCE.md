<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: myhealth.eu.fhir.core 1.0.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: myhealth.eu.fhir.core
- Version: 1.0.0
- Source: the FHIR package registry, https://packages.fhir.org/myhealth.eu.fhir.core
- Tarball: https://packages.simplifier.net/myhealth.eu.fhir.core/1.0.0
- SHA-1 (registry shasum): 356895732b806d4d4693f8dd01ab13863e0633f4
- SHA-256 (tarball): d40b9979dd971a76fc39452aee4091a5f79a9d46192ece7c28e70b4d429a9270
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: MyHealth@Eu (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 128
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e39cf821244120cf6b54133c7e86ce3f5076c0a2d0568b3d12c569bd2a3b7512`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.1.0, `hl7.fhir.uv.extensions.r4` 5.2.0, `hl7.fhir.uv.ips` 2.0.0, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.base` 2.0.0, `myhealth.eu.fhir.mvc-package` 9.1.0, `xtehr.eu.ehds.models` 0.3.0, `hl7.fhir.eu.extensions.r4` 1.3.0
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
