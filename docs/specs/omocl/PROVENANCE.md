<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OMOCL corpus

The whole repository tree, vendored verbatim by `scripts/vendor/omocl.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/SevKohler/OMOCL>
- Pin: commit `cfb26bad3ad035f975736299b53a494696b2e604`
- Fetched: 2026-11-02
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file
- Layout: the whole upstream tree, unchanged
- Line endings: CRLF, as upstream ships them. `.gitattributes` marks this tree
  `-text` so git stores those bytes unchanged.
- Files: 215, of which 208 are mapping files, and all 208 of those
  declare `grammar: OMOCL/v1.0.0`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `07e62cfa408df90c7bdbf02713463f4c4e9a7677a9b33102ec58440e22b3a899`

## Why the pin is a commit

The git tag `v1.0.0` of this repository is not this commit. That tag carries
pre-grammar files headed `engine: EOS/v0.0.62`, and the grammar string
`OMOCL/v1.0.0` first appears at tag `v1.0.1`. Pinning the commit is what makes
the corpus the released grammar (docs/architecture.md section 2).
