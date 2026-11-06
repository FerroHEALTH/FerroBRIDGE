<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.iti.balp 1.1.4

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.iti.balp
- Version: 1.1.4
- Source: the FHIR package registry, https://packages.fhir.org/ihe.iti.balp
- Tarball: https://packages.simplifier.net/ihe.iti.balp/1.1.4
- SHA-1 (registry shasum): 8af15f01244ab5e3a48dd40e1afb0696d6905898
- SHA-256 (tarball): be46dda3088ee9d486d7163458dc5a4d7192e8a1c587fd47e2beb70e4bcefa90
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: IHE IT Infrastructure Technical Committee (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 126
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `3a150751bc5fad01361b05518a8e757639fe7cc44b1b9faf7d0844dcce352ea2`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.5.0, `hl7.fhir.uv.extensions.r4` 5.2.0
- FHIR versions (`fhirVersions` of `package/package.json`): 4.0.1
- Stamped: 2026-11-06

Each declared dependency is fetched beside this tree at the version named,
except `hl7.fhir.r4.core` 4.0.1, which is read from the committed
`tools/fhir-codegen/vendor/hl7.fhir.r4.core/`, and except the
dependencies of a package of another FHIR release, which the R4 set holds
and never reads. The two lines above let `--check` assert the closure from
the committed files alone.

## Why build time

The package set unpacks to about 970 MB, eight versions of
`hl7.terminology.r4` and `hl7.fhir.uv.xver-r5.r4` among it, for input only
the validator and its tests read. The licences would permit vendoring; the
size decides. CI restores the trees from a cache keyed on
`scripts/vendor/profile-packages.sh --cache-key`, and the script verifies the
count and digest of a restored tree as it does of a download.
