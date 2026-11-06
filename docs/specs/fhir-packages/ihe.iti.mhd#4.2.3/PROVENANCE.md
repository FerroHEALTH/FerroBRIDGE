<!-- This file describes third-party material fetched at build time; the bytes
     it describes keep their upstream terms and are never committed. -->

# Provenance: ihe.iti.mhd 4.2.3

Fetched verbatim at build time by `scripts/vendor/profile-packages.sh` into
`package/` beside this file, which `.gitignore` refuses, as a run-time input
of the R4 profile validator (.claude/rules/vendored-inputs.md). Never commit a
file from there and never edit one: change the pin in docs/VERSIONS.md, run the
script with `--stamp`, and record the count and digest it prints.

- Package: ihe.iti.mhd
- Version: 4.2.3
- Source: the FHIR package registry, https://packages.fhir.org/ihe.iti.mhd
- Tarball: https://packages.simplifier.net/ihe.iti.mhd/4.2.3
- SHA-1 (registry shasum): dc5a9efa38d8035c4e3cc108e775b219863e36e1
- SHA-256 (tarball): 0a11808dd94c9eac87e4ee56f0f08c2e782f022df9554acf31a3bb64a2624f2e
- Upstream license: CC-BY-4.0 (the `license` field of `package/package.json`)
- Author: IHE IT Infrastructure Technical Committee (the `author` field of `package/package.json`)
- Layout: the tarball's `package/` directory, extracted unchanged
- Files: 184
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e04d62f748649a821d60e439f08817a4bb05505532042c0eb72fdf2da8d5004a`
- Declared dependencies (`package/package.json`): `hl7.fhir.r4.core` 4.0.1, `hl7.terminology.r4` 6.5.0, `hl7.fhir.uv.extensions.r4` 5.2.0, `ihe.iti.balp` 1.1.4, `ihe.formatcode.fhir` 1.4.x
- Resolved ranges (the highest published version each matches): `ihe.formatcode.fhir` 1.4.x as 1.4.0
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
