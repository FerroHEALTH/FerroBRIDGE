#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhir-mapping-language.sh
#
# Vendors the FHIR R4 (4.0.1) FHIR Mapping Language texts the StructureMap
# engine (#2051) is built and tested against into
# docs/specs/fhir-mapping-language/: the mapping language page, its ANTLR
# grammar `mapping.g4`, the mapping tutorial, the StructureMap resource page
# and the licence page.
#
# Each artefact is pinned by its URL and the sha256 of its bytes
# (scripts/vendor/lib/pinned.sh); the docs/VERSIONS.md row carries the
# pin-set digest.
#
# Usage:
#   scripts/vendor/fhir-mapping-language.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require curl shasum awk
pinned_begin

read_by="FerroHEALTH/FerroBRIDGE#2051 (app/ferrobridge-server/src/structuremap and its tests)"

r4=https://hl7.org/fhir/R4
cc0='Creative Commons "No Rights Reserved" (CC0 1.0), as the license.html page of the release states; HL7 keeps the "FHIR" trademark, which this copy does not use as a mark'

# pin_r4 FILE SHA256
pin_r4() {
  pin commit "$1" "FHIR R4 (4.0.1)" "$r4/$1" "$2" "$cc0"
}

pin_r4 mapping-language.html a934369a47bf09139ff646759931bfb983c1edfd6b686b8ef0bd58dfd86136e6
pin_r4 mapping.g4 fb01ebb0c290fa785d3e37db3a352d6b356b57ac88c626bfbf57964d0c70fe35
pin_r4 mapping-tutorial.html 48b0f28cd977f38247a54d656207fb2ba471ea0082eb50913b6b22312b6dc861
pin_r4 structuremap.html 898f2cb6d84527786f75dc66f222022cc626fbb1240e756048d99e50867ac0a7
pin_r4 license.html b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15
pinned_corpus fhir-mapping-language "FHIR R4 Mapping Language" \
  "the FHIR R4 Mapping Language" \
  "The FHIR Mapping Language page (7.7), its ANTLR grammar, the mapping
tutorial, the StructureMap resource page and the licence page of HL7 FHIR
Release 4 (4.0.1), each file whole as \`hl7.org/fhir/R4\` serves it,
committed under CC0-1.0." "" "$read_by"

say "done"
