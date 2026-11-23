<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.eu.fhir.xshare-yb 0.3.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.eu.fhir.xshare-yb
- Version: 0.3.0
- Source: the guide's canonical site, http://hl7.eu/fhir/ig/xshare-yb
- Tarball: https://hl7.eu/fhir/ig/xshare-yb/0.3.0/package.tgz
- SHA-1 (tarball): 51ed10f16c8915cb8f3a6e97823c3553725a4856
- SHA-256 (tarball): ba0ced94b5d4bdb463e46375404f805c433640d07604f518a434a58af3c18682
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: xShare Project (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 13
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `a411d731a3fa7a1f28f29083b4db10cc6f581c92f7b7b2bece8fee642ae4ae65`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.3.0, `hl7.fhir.uv.extensions.r4` 5.3.0, `hl7.fhir.uv.ips` 2.0.1, `hl7.fhir.uv.smart-health-cards-and-links` 1.0.0, `hl7.fhir.eu.laboratory` 2.0.0, `hl7.fhir.eu.mpd` 1.0.0, `hl7.fhir.eu.base` 2.0.0
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

## Why the canonical site

The FHIR package registry (packages.fhir.org and packages2.fhir.org) lists
no 0.3.0 of hl7.eu.fhir.xshare-yb. The guide's publisher released it on the guide's canonical
site, whose `package-list.json` names the release, so the tarball comes from
the versioned path there, pinned by URL and sha256 in docs/VERSIONS.md; any
other bytes are refused. Once the registry lists the version, the row drops
its `tarball` and `sha256` and the package is fetched like any other.
