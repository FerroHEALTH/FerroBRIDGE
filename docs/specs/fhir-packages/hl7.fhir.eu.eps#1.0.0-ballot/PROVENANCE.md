<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: hl7.fhir.eu.eps 1.0.0-ballot

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: hl7.fhir.eu.eps
- Version: 1.0.0-ballot
- Source: the FHIR package registry, https://packages.fhir.org/hl7.fhir.eu.eps
- Tarball: https://packages.simplifier.net/hl7.fhir.eu.eps/1.0.0-ballot
- SHA-1 (registry shasum): e387722a24e1e4236728e34f5f50b9e131b802d2
- SHA-256 (tarball): e0fff1fb20d3daf75609259faa6b860131ebf4fc8890bb68f0da882307afc33c
- Upstream license: CC0-1.0 (the `license` field of `package/package.json`)
- Author: HL7 Europe (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 76
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d8fe50b7359f698ce513821235502dd5bf43ad2e4e70c25a4dbe12d0b8b39f2e`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 7.1.0, `hl7.fhir.uv.extensions.r4` 5.3.0, `hl7.fhir.uv.ips` 2.0.0, `hl7.fhir.eu.base` 2.0.0, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.fhir.eu.extensions.r4` 1.3.0, `ihe.pharm.mpd.r4` 1.0.0-comment-2
- Stamped: 2026-10-15

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`. The line above lets
`--check` assert the closure from the committed files alone.

## Why build time

The package set unpacks to about 570 MB, five versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
