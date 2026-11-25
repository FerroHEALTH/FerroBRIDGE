#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ehds-drafts.sh
#
# Vendors the draft implementing acts of Regulation (EU) 2025/327 that the
# eehrxf component is held to, and the two Xt-EHR deliverables they build on:
#
# - the draft EEHRxF specifications (Art 15(1)), Have your say initiative
#   17512, Ares(2026)10884928, published 2026-11-23, the act and Annexes I
#   to X;
# - the draft common specifications of the European digital testing
#   environment (Art 40(4)), initiative 17513, Ares(2026)10805685, published
#   2026-11-19, the act and its annex;
# - Xt-EHR D5.2 (EEHRxF metadata) and D8.2 (the conformity assessment
#   scheme).
#
# Each PDF is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh), and
# its text is extracted beside it so code and documentation can cite an
# article or annex line. The Commission documents are reusable under
# Commission Decision 2011/833/EU and are committed. The Xt-EHR deliverables
# carry no licence statement, so they are fetched into the git-ignored cache
# and never committed. scripts/checks/pin-freshness.sh watches both
# initiatives for a new draft or the adopted text.
#
# Usage:
#   scripts/vendor/ehds-drafts.sh
#   PINNED_DIGESTS_ONLY=1 scripts/vendor/ehds-drafts.sh   # print each pin-set digest
#
# Requires: curl, shasum, awk, pdftotext (poppler).
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

download=https://ec.europa.eu/info/law/better-regulation/api/download
reuse='Commission Decision 2011/833/EU on the reuse of Commission documents: reuse is authorised provided the source is acknowledged and the document is not distorted'
draft='A draft for feedback, not adopted: re-pin when the adopted text is published in the Official Journal.'

pin commit act.pdf "Draft, Ares(2026)10884928, 2026-11-23" "$download/090166e5374c0181" \
  58bcf64645fda2cf0e0914d3035afbbc27af4e98dee0ebe73f42c436eb85954a "$reuse" \
  "$draft Served as PART-2026-1023487V3.pdf."
pin pdftext act.txt "Draft, Ares(2026)10884928, 2026-11-23" act.pdf - "$reuse"
pin commit annexes.pdf "Draft, Ares(2026)10884928, 2026-11-23" "$download/090166e5374c0182" \
  7e3db6da13c7760b122691f466423dd57c5eb519457e3ca0dcae60f3565c00fb "$reuse" \
  "$draft Annexes I to X. Served as PART-2026-1023488V3.pdf."
pin pdftext annexes.txt "Draft, Ares(2026)10884928, 2026-11-23" annexes.pdf - "$reuse"
pinned_corpus ehds-eehrxf-draft "EHDS draft EEHRxF implementing act" \
  "the draft EEHRxF implementing act" \
  "The draft Commission Implementing Regulation laying down the European
electronic health record exchange format under Article 15(1) of Regulation
(EU) 2025/327, published for feedback on Have your say initiative 17512
(<https://ec.europa.eu/info/law/better-regulation/have-your-say/initiatives/17512>)
until 2026-12-21, committed under Commission Decision 2011/833/EU." "" \
  $'#2078 (the authority for the data sets, metadata, representations and\n  coding rule the eehrxf component emits)'

pin commit act.pdf "Draft, Ares(2026)10805685, 2026-11-19" "$download/090166e537270100" \
  d970e4eb3705991b9bee18d44d5bea2375bf7977fb0753f56da0edfb18d0c9ca "$reuse" "$draft"
pin pdftext act.txt "Draft, Ares(2026)10805685, 2026-11-19" act.pdf - "$reuse"
pin commit annex.pdf "Draft, Ares(2026)10805685, 2026-11-19" "$download/090166e537270101" \
  c28a667e0d6a666f908f00ad50e50345795a7cfc1d6d88b71bb4028a3749c8c7 "$reuse" "$draft"
pin pdftext annex.txt "Draft, Ares(2026)10805685, 2026-11-19" annex.pdf - "$reuse"
pinned_corpus ehds-testing-draft "EHDS draft testing environment implementing act" \
  "the draft European digital testing environment implementing act" \
  "The draft Commission Implementing Regulation laying down the common
specifications of the European digital testing environment under Article
40(4) of Regulation (EU) 2025/327, published for feedback on Have your say
initiative 17513
(<https://ec.europa.eu/info/law/better-regulation/have-your-say/initiatives/17513>)
until 2026-12-17, committed under Commission Decision 2011/833/EU." "" \
  $'#2078 (the testing procedure the EEHRxF conformance corpora prepare\n  for)'

xtehr=https://www.xt-ehr.eu/wp-content/uploads/2026/05
unclear='None stated: the deliverable carries the EU funding disclaimer and no licence, so it is not redistributed'
pin cache Xt-EHR-D5.2.pdf "D5.2, Ares(2026)3690435, 2026-04-09" "$xtehr/Xt-EHR-D5.2.pdf" \
  5d2bb693af07646a506f831854166fcd4d6a3ebf4f092b6937d2a9976410333a "$unclear" \
  "Technical Requirements for EEHRxF metadata."
pin pdftext Xt-EHR-D5.2.txt "D5.2, Ares(2026)3690435, 2026-04-09" Xt-EHR-D5.2.pdf - "$unclear"
pin cache Xt-EHR-D8.2.pdf "D8.2, Ares(2026)5060558, 2026-05-19" "$xtehr/Xt-EHR-D8.2.pdf" \
  a8cd93d994bceb1c7de952b006384d2c638a038907bba48325985ed7d851fbf4 "$unclear" \
  "EHR Conformity Assessment Scheme: Assertion Document and Checklists."
pin pdftext Xt-EHR-D8.2.txt "D8.2, Ares(2026)5060558, 2026-05-19" Xt-EHR-D8.2.pdf - "$unclear"
pinned_corpus xtehr-deliverables "Xt-EHR deliverables D5.2 and D8.2" \
  "the Xt-EHR deliverables D5.2 and D8.2" \
  "Two public deliverables of the Xt-EHR Joint Action (proposal 101128085):
D5.2, the technical requirements for EEHRxF metadata, and D8.2, the EHR
conformity assessment scheme with its checklists. Neither states a licence,
so both PDFs and their text are read from the git-ignored cache and this
repository carries none of their content." "" \
  $'#2078 (the metadata the eehrxf component records as followed or not,\n  and the conformity checklists its corpora follow)'

say "done"
