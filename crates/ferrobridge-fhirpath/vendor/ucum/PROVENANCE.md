<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the UCUM essence

The machine-readable tables of The Unified Code for Units of Measure (UCUM),
vendored verbatim by `scripts/vendor/ucum.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/ucum-org/ucum>
- Pin: tag `v2.2`, commit `d060c6d3dfe3b0deb196c190b8024d95385e0b92`, path `ucum-essence.xml`
- File: `ucum-essence.xml`, 82776 bytes, sha256 `dfccea1b5dc284245ebae97edd1dc03c45864da4e87df55bc9851797b4fd0b61`
- Fetched: 2026-10-15
- Copyright: The Unified Code for Units of Measure (UCUM) is copyright
  1999-2024, Regenstrief Institute, Inc. All rights reserved.
- Upstream licence: the UCUM Copyright Notice and License, Version 1.1,
  vendored beside this file as `LICENSE.md` (sha256 `b7989c84c68b7a80d42aad28cfdd824d756c2f70f26eaf1347d2e49f7838bbdf`) and
  published at <https://unitsofmeasure.org/license>. The Work is provided on
  an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND (Section 7
  of that licence). It is not covered by the licence of this repository.
- Use: embedded whole and unmodified in the `ferrobridge-fhirpath` crate,
  which reads the prefixes and unit atoms from it to compare and convert
  FHIRPath quantities, and ships this file and the licence beside it.
