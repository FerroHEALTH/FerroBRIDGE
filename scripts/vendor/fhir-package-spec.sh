#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhir-package-spec.sh
#
# Vendors the FHIR NPM Packages page (packages.html) the profile validator's
# package set cites for its dependency rules: the R5 page, the current
# published version, and the 6.0.0-ballot4 page, the first published version
# that completes the sentence on the core package dependency ("defines the
# FHIR version used to resolve the contents of this dependency tree"), each
# with the licence page of its version. hl7.org serves no R4 packages.html.
# Each page is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh) and
# committed whole under docs/specs/fhir-package-spec/, and the corpus row of
# docs/VERSIONS.md carries the pin-set digest.
#
# Usage:
#   scripts/vendor/fhir-package-spec.sh
#   PINNED_DIGESTS_ONLY=1 scripts/vendor/fhir-package-spec.sh   # print the pin-set digest
#
# Requires: curl, shasum, awk.
#
# No specification governs this file; it is FerroBRIDGE's own design.

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

r5=https://hl7.org/fhir/R5
r6=https://hl7.org/fhir/6.0.0-ballot4
r5cc0='CC0-1.0: FHIR R5 license.html licenses the specification under Creative Commons No Rights Reserved (CC0)'
r6cc0='CC0-1.0: FHIR 6.0.0-ballot4 license.html licenses the specification under Creative Commons No Rights Reserved (CC0)'
read_by=$'#1600 (the FHIR release of a package and of the dependency tree,\n  crates/ferrobridge-validator/src/set.rs, and the closure check of\n  scripts/vendor/profile-packages.sh)'

pin commit packages-r5.html "5.0.0, 2.1.10 FHIR NPM Packages" "$r5/packages.html" \
  bbbab9a296066ab54dcdcadc429072344c16331d472a6098065c2b28c69bf411 "$r5cc0"
pin commit license-r5.html "5.0.0, License and Legal Terms" "$r5/license.html" \
  452f8651729bb0e6b93c75a22f772cb7ece300a3585e1da17be944964b44aa90 "$r5cc0" \
  "The licence statement of packages-r5.html."
pin commit packages-6.0.0-ballot4.html "6.0.0-ballot4, 2.1.10 FHIR NPM Packages" "$r6/packages.html" \
  d1b2d2290a5dde8973a5714f81055822e776099d9ac297249b1920d17fd14a5a "$r6cc0"
pin commit license-6.0.0-ballot4.html "6.0.0-ballot4, License and Legal Terms" "$r6/license.html" \
  4963cc7186b1def7864b47eddb333c71e9d6e63c0b7186c9e880aecf21950230 "$r6cc0" \
  "The licence statement of packages-6.0.0-ballot4.html."
pinned_corpus fhir-package-spec "FHIR NPM Packages page" \
  "the FHIR NPM Packages page" \
  "The FHIR NPM Packages page (2.1.10) of HL7 FHIR R5 (5.0.0) and of the
R6 ballot 6.0.0-ballot4, with the License and Legal Terms page of each, as
\`hl7.org/fhir/\` serves them, committed under CC0-1.0, which each licence
page states for its specification." "" "$read_by"

say "done"
