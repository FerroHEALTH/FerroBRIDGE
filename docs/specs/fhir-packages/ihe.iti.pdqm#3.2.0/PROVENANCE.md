<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.iti.pdqm 3.2.0

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.iti.pdqm
- Version: 3.2.0
- Source: the FHIR package registry, https://packages.fhir.org/ihe.iti.pdqm
- Tarball: https://packages.simplifier.net/ihe.iti.pdqm/3.2.0
- SHA-1 (registry shasum): f728e969afa2a622c34b35e4f3f65e155a540a0c
- SHA-256 (tarball): 61e09fbee991ff7c131b6ba5474921001e07782209961f3e85cee5f3ebcaedc2
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: IHE IT Infrastructure Technical Committee (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 53
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `cc2ca55cef4d1181108ab9867470252293f2e2f49538ce2123d5bef62d0f83b9`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.5.0, `hl7.fhir.uv.extensions.r4` 5.2.0, `ihe.iti.balp` 1.1.x
- Resolved ranges (the highest published version each matches): `ihe.iti.balp` 1.1.x as 1.1.4
- Stamped: 2026-11-06

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`. The line above lets
`--check` assert the closure from the committed files alone.

## Why build time

The package set unpacks to about 650 MB, six versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
