#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The conformance gate (#24): runs the sixteen corpus tests, compares what they
# measured with the committed pass lists under conformance/, and renders one
# shields.io endpoint badge per corpus (https://shields.io/badges/endpoint-badge),
# one per HL7 v2 message family and one per HL7 v2 version (#366), and the
# README block between badges:begin and badges:end from those badges, with
# conformance/README.md, the page every corpus badge links to.
#
#   scripts/checks/conformance.sh            # run and compare; warn on drift
#   scripts/checks/conformance.sh --update   # run and rewrite lists, badges, README block
#   scripts/checks/conformance.sh --check    # run and fail on any drift (CI)
#   scripts/checks/conformance.sh --update fhir-r4b   # one corpus: its test, list, badge
#   scripts/checks/conformance.sh --self-test         # the badge and upstream-row cases
#   scripts/checks/conformance.sh --render            # rewrite the badges, page and README block from the lists
#
# The MyHealth@EU corpora (#2083) are measured by HL7's FHIR Validator in
# scripts/checks/fhir-validator.sh --myhealth-eu, which writes their results
# under target/conformance/; this script runs no test for them. Named alone,
# such a corpus compares or rewrites its list from that result, which must
# exist; a run of every corpus compares one whose result is present and
# otherwise keeps its list and badge as they are.
#
# A corpus name after the mode (or alone, to compare) runs that corpus's test
# only and compares or rewrites only its list and badge. The README block is
# still compared or rewritten. The family and version badges count both HL7 v2
# corpora, so naming either one also runs the other's test, without rewriting
# its list, and renders those badges; any other corpus keeps them as they are.
#
# The tests write target/conformance/<corpus>.json through the testkit's
# conformance module, and rewrite the pass lists themselves when
# FERROBRIDGE_CONFORMANCE_UPDATE is 1, so the lists come from the same tests
# the suite runs. A case the list records that no longer passes fails every
# mode. Under --check, an unlisted passing case, a moved total, a badge that
# disagrees with what --update would write, a family or version badge no case
# counts, and a README block that disagrees with the badges fail too, because
# the list is what the ratchet protects. Every badge file, and every static
# badge in the README, carries a logo from scripts/checks/badge-logos/, and a
# badge without one fails --check, as does a README without the caption
# under the conformance heading that says once what the badges count (owner
# decision, 2026-11-25). A badge shows its count alone: a label past 30
# characters or a message past 20 fails every mode (owner decision,
# 2026-11-27). conformance/README.md, rendered from the same lists and rows,
# breaks each count down: the cases an upstream holds with their issues, the
# unmapped and reviewed counts, and the edition a corpus reads; --check fails
# when it disagrees.
#
# A failing case an open upstream-report issue holds (#2334, owner decision on
# #2333 of 2026-11-27) is a row of conformance/<corpus>/upstream.tsv: a case<TAB>issue
# header, then the case id and the issue number, tab-separated. The badge
# keeps them in the denominator and is yellowgreen when every case the
# upstream does not hold passes, green only when every case passes;
# conformance/README.md names their count and issues. The rows are committed, so a badge is rendered from
# the tree alone; scripts/checks/upstream-rows.sh holds each issue to open and
# labelled upstream-report on the tracker, so a row whose issue closes leaves
# the file and counts against the colour again. A malformed row, a repeated
# case, and (with a result) a case the corpus does not hold fail every mode; a
# held case that passes fails --check, and --update drops its row.
#
# The family and version badges count the cases of both HL7 v2 corpora by the
# family (MSH-9.1) and the version (MSH-12) the corpus test records per case:
# one badge per family with at least one case, by family code, and one per
# version seen. They link to the hl7v2 pass list.
#
# The hl7v2-smoke corpus reads the NIST and AIRA sets, which it fetches first
# through scripts/vendor/hl7v2-samples.sh --build-time (a no-op when they are
# on disk at their pins). The five FHIR model corpora and the R4 facade corpus
# read the HL7 examples packages (#373), fetched the same way through
# scripts/vendor/fhir-packages.sh --build-time. The R4 profile validator's
# package set is fetched the same way through scripts/vendor/profile-packages.sh,
# for the whole run and for the validator-agreement corpus alone.
#
# Needs cargo, cargo-nextest, jq, and the tools that script needs. Exit 0 when
# clean, 1 on a regression, (under --check) drift or no cargo on PATH, 2 on a
# usage error, a missing result or a failed fetch.
set -euo pipefail

# Each corpus, the package whose test measures it, and that test's name, one
# quoted row per line; scripts/checks/affected.sh reads the rows in this shape.
readonly CORPUS_TESTS=(
  'fhirconnect-mapping-lib fhirconnect conformance_the_fhirconnect_corpus_holds_its_pass_list'
  'omocl omocl conformance_the_omocl_corpus_holds_its_pass_list'
  'roundtrip fhirconnect conformance_the_round_trip_chains_hold_their_pass_list'
  'rest-api fhirconnect conformance_the_rest_api_operation_definitions_hold_their_pass_list'
  'hl7v2 ferrobridge-hl7v2 conformance_the_vendored_hl7v2_corpora_hold_their_pass_list'
  'hl7v2-smoke ferrobridge-hl7v2 conformance_the_fetched_hl7v2_corpora_hold_their_pass_list'
  'fhir-stu3 ferrobridge-testkit conformance_the_fhir_stu3_examples_hold_their_pass_list'
  'fhir-r4 ferrobridge-testkit conformance_the_fhir_r4_examples_hold_their_pass_list'
  'fhir-r4b ferrobridge-testkit conformance_the_fhir_r4b_examples_hold_their_pass_list'
  'fhir-r5 ferrobridge-testkit conformance_the_fhir_r5_examples_hold_their_pass_list'
  'fhir-r6 ferrobridge-testkit conformance_the_fhir_r6_examples_hold_their_pass_list'
  'fhir-r4-facade ferrobridge-server conformance_the_fhir_r4_examples_hold_their_facade_pass_list'
  'fhirpath-r4 ferrobridge-fhirpath conformance_fhirpath_r4'
  'fhirpath-r4-stu3 ferrobridge-fhirpath conformance_fhirpath_r4_stu3'
  'validator-agreement ferrobridge-validator conformance_the_validator_agrees_with_the_hl7_validator_on_its_corpus'
  'eehrxf-annex-x ferrobridge-server conformance_the_annex_x_groups_hold_their_pass_list'
)
# The corpora measured outside this script, whose results it reads and never
# produces: the MyHealth@EU corpora HL7's FHIR Validator measures
# (scripts/checks/fhir-validator.sh --myhealth-eu), the EEHRxF draft
# corpora the eehrxf draft corpus suite measures over the documents the other
# eehrxf suites emitted (EEHRXF_DRAFT_CORPUS_DIR), each after those suites run,
# and the Xt-EHR crosswalk, which that suite measures with every eehrxf
# feature on, since a row resolves only through the category crosswalks.
readonly DRAFT_CORPORA=(eehrxf-draft-patient-summary eehrxf-draft-eprescription eehrxf-draft-edispensation
  eehrxf-draft-medical-test-report eehrxf-draft-imaging eehrxf-draft-discharge)
readonly EXTERNAL_CORPORA=(myhealth-eu-core myhealth-eu-laboratory myhealth-eu-hdr myhealth-eu-mvc myhealth-eu-euvac "${DRAFT_CORPORA[@]}" xtehr-crosswalk)

# Whether $1 equals one of the remaining arguments.
contains() {
  local needle=$1 item
  shift
  for item in "$@"; do
    [[ $item == "$needle" ]] && return 0
  done
  return 1
}

corpora=()
packages=()
for row in "${CORPUS_TESTS[@]}"; do
  corpora+=("${row%% *}")
  package="${row#* }"
  package="${package%% *}"
  contains "$package" ${packages[@]+"${packages[@]}"} || packages+=("$package")
done
readonly CORPORA=("${corpora[@]}")
# The packages of the table, each once, in the order it first names them.
readonly PACKAGES=("${packages[@]}")

# The package and the test name of corpus $1, or status 1 for no such corpus.
test_of() {
  local corpus_name="$1"
  local row
  for row in "${CORPUS_TESTS[@]}"; do
    if [[ "${row%% *}" = "$corpus_name" ]]; then
      echo "${row#* }"
      return 0
    fi
  done
  return 1
}

usage() {
  echo "usage: $0 [--update|--check] [corpus] | --render | --self-test" >&2
  echo "corpora: ${CORPORA[*]} ${EXTERNAL_CORPORA[*]}" >&2
  exit 2
}

mode=compare
only=""
self_test=0
render_only=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --update) mode=update ;;
    --check) mode=check ;;
    --self-test) self_test=1 ;;
    --render)
      mode=update
      render_only=1
      ;;
    -*) usage ;;
    *)
      [[ -z "$only" ]] || usage
      test_of "$1" >/dev/null || contains "$1" "${EXTERNAL_CORPORA[@]}" \
        || { echo "conformance: no corpus is named $1" >&2; usage; }
      only="$1"
      ;;
  esac
  shift
done

[[ "$render_only" -eq 0 || -z "$only" ]] || usage
external_only=0
if [[ "$self_test" -eq 1 || "$render_only" -eq 1 ]]; then
  external_only=1
fi
if [[ -n "$only" ]] && contains "$only" "${EXTERNAL_CORPORA[@]}"; then
  external_only=1
fi

# A missing cargo would otherwise read as every corpus test failing.
if [[ "$external_only" -eq 0 ]] && ! command -v cargo >/dev/null 2>&1; then
  echo "conformance: cargo not found on PATH" >&2
  exit 1
fi
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/badge-logos/logo.sh
source scripts/checks/badge-logos/logo.sh

readonly OUT=target/conformance
# The tree of the pass lists and upstream rows; the self-test points it elsewhere.
root=conformance
readonly UPSTREAM_HEADER=$'case\tissue'
# The counts a pass list records after its total line, each named on
# conformance/README.md: unmapped annex elements (the EEHRxF draft corpora) and
# reviewed extensible-binding misses (myhealth-eu-mvc), and annex elements
# whose Xt-EHR element differs (xtehr-crosswalk).
readonly COUNT_KINDS=(unmapped reviewed differ)
readonly COUNT_LINE='^(total|unmapped|reviewed|differ) '
readonly BADGES=conformance/badges
# The shields.io endpoint prefix every conformance badge file is read through.
readonly ENDPOINT='https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2F'
# The project and quality badges, the first two rows of the README block.
readonly PROJECT_BADGES=(
  '[![Release](https://img.shields.io/github/v/release/FerroHEALTH/FerroBRIDGE?sort=semver&logo=github&label=release)](https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest)'
  '[![CI](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroBRIDGE/ci.yml?branch=main&logo=githubactions&logoColor=white&label=CI)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml)'
  '[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue?logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0ibm9uZSIgc3Ryb2tlPSIjZmZmIiBzdHJva2Utd2lkdGg9IjIiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIgZD0iTTEyIDN2MThNNyAyMWgxME00IDdoMTZNNCA3TDEgMTRhMyAzIDAgMCAwIDYgMFpNMjAgN2wtMyA3YTMgMyAwIDAgMCA2IDBaIi8%2BPC9zdmc%2B)](LICENSE)'
  '[![Docs](https://img.shields.io/badge/docs-ferrobridge.eu-blue?logo=mdbook)](https://ferrobridge.eu/docs/)'
  '[![GHCR](https://img.shields.io/badge/ghcr.io-ferrobridge-2496ED?logo=docker&logoColor=white)](https://github.com/FerroHEALTH/FerroBRIDGE/pkgs/container/ferrobridge)'
)
# The security and compliance badges, the last group of the README block; the
# post-quantum badge is scripts/checks/post-quantum.sh's.
readonly COMPLIANCE_BADGES=(
  '[![CRA](https://img.shields.io/badge/CRA-user%20information-blue?logo=europeanunion&labelColor=003399&logoColor=FFCC00)](https://ferrobridge.eu/docs/evaluate/cra-user-information.html)'
  "[![Post-quantum](${ENDPOINT}post-quantum.json)](docs/post-quantum.md)"
)
readonly QUALITY_BADGES=(
  '[![OpenSSF Scorecard](https://img.shields.io/ossf-scorecard/github.com/FerroHEALTH/FerroBRIDGE?label=openssf%20scorecard&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0ibm9uZSIgc3Ryb2tlPSIjZmZmIiBzdHJva2Utd2lkdGg9IjIiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIgZD0iTTEyIDJsOCAzdjZjMCA1LTMuNSA5LjUtOCAxMS00LjUtMS41LTgtNi04LTExVjVaTTguNSAxMmwyLjUgMi41IDQuNS01Ii8%2BPC9zdmc%2B)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroBRIDGE)'
  '[![Quality gate](https://img.shields.io/sonar/quality_gate/FerroHEALTH_FerroBRIDGE?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=quality%20gate)](https://sonarcloud.io/summary/overall?id=FerroHEALTH_FerroBRIDGE)'
  '[![Coverage](https://img.shields.io/sonar/coverage/FerroHEALTH_FerroBRIDGE?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=coverage)](https://sonarcloud.io/summary/new_code?id=FerroHEALTH_FerroBRIDGE)'
  '[![CodeQL](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroBRIDGE/codeql.yml?branch=main&logo=github&label=CodeQL)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml)'
  '[![Signed releases](https://img.shields.io/badge/releases-signed-brightgreen?logo=github)](SECURITY.md#verifying-a-release)'
  '[![SLSA Build L3](https://slsa.dev/images/gh-badge-level3.svg)](SECURITY.md#verifying-a-release)'
)

# The page every corpus badge links to, which breaks its count down.
readonly PAGE=conformance/README.md

# The badge label of a corpus.
label_of() {
  case "$1" in
    fhirconnect-mapping-lib) echo "FHIRconnect mapping library" ;;
    omocl) echo "OMOCL mapping library" ;;
    roundtrip) echo "FHIR round-trip laws" ;;
    rest-api) echo "FHIRconnect REST API" ;;
    hl7v2) echo "message corpora" ;;
    hl7v2-smoke) echo "smoke (NIST, AIRA)" ;;
    fhir-stu3) echo "STU3" ;;
    fhir-r4) echo "R4" ;;
    fhir-r4b) echo "R4B" ;;
    fhir-r5) echo "R5" ;;
    fhir-r6) echo "R6" ;;
    fhir-r4-facade) echo "R4 facade" ;;
    fhirpath-r4) echo "FHIRPath R4 N1" ;;
    fhirpath-r4-stu3) echo "FHIRPath R4 3.0.0 STU" ;;
    validator-agreement) echo "R4 profile validator agreement" ;;
    xtehr-crosswalk) echo "Xt-EHR 1.0.0 crosswalk" ;;
    eehrxf-draft-patient-summary) echo "Patient Summary" ;;
    eehrxf-draft-eprescription) echo "ePrescription" ;;
    eehrxf-draft-edispensation) echo "eDispensation" ;;
    eehrxf-draft-medical-test-report) echo "Medical test report" ;;
    eehrxf-draft-imaging) echo "Imaging report" ;;
    eehrxf-draft-discharge) echo "Discharge report" ;;
    myhealth-eu-core) echo "Core $(ci_build_version myhealth.eu.fhir.core)" ;;
    myhealth-eu-laboratory) echo "Laboratory $(ci_build_version myhealth.eu.fhir.laboratory)" ;;
    myhealth-eu-hdr) echo "HDR $(ci_build_version myhealth.eu.fhir.hdr)" ;;
    myhealth-eu-mvc) echo "MVC $(ci_build_version myhealth.eu.fhir.mvc-package)" ;;
    myhealth-eu-euvac) echo "EuVAC $(pinned_version myhealth.eu.fhir.euvac)" ;;
    eehrxf-annex-x) echo "Annex X groups served" ;;
    *) return 1 ;;
  esac
}

# The badge logo of a corpus: the EU emblem for an EU act, else a pictogram.
logo_of() {
  case "$1" in
    fhirconnect-mapping-lib | roundtrip) echo exchange ;;
    omocl) echo database ;;
    rest-api | fhirpath-r4 | fhirpath-r4-stu3) echo code ;;
    hl7v2 | hl7v2-smoke) echo message ;;
    fhir-stu3 | fhir-r4 | fhir-r4b | fhir-r5 | fhir-r6) echo document ;;
    fhir-r4-facade | validator-agreement) echo shield-check ;;
    eehrxf-annex-x | eehrxf-draft-* | xtehr-crosswalk | myhealth-eu-*) echo eu ;;
    *) return 1 ;;
  esac
}

# What a corpus's N/M counts, one word after the fraction in its badge
# message; empty for a corpus whose label says it.
unit_of() {
  case "$1" in
    myhealth-eu-core | myhealth-eu-laboratory | myhealth-eu-hdr | myhealth-eu-euvac) echo " valid" ;;
    myhealth-eu-mvc) echo " codes" ;;
    xtehr-crosswalk) echo " resolved" ;;
    eehrxf-draft-*) echo " held" ;;
    *) echo "" ;;
  esac
}

# What the N/M of a corpus counts, in full, for conformance/README.md.
measure_of() {
  case "$1" in
    myhealth-eu-*) [[ "$1" = myhealth-eu-mvc ]] && echo "codes that meet their binding" || echo "documents valid" ;;
    xtehr-crosswalk) echo "elements resolved" ;;
    eehrxf-draft-*) echo "annex elements held" ;;
    *) echo "cases passing" ;;
  esac
}

# The edition corpus $1 reads, beyond the version its label names, for
# conformance/README.md; "-" when the label says it all.
edition_of() {
  case "$1" in
    myhealth-eu-euvac) echo "MyHealth@EU, the pinned preview package" ;;
    myhealth-eu-*) echo "MyHealth@EU, the CI build" ;;
    xtehr-crosswalk) echo "Xt-EHR models 1.0.0 against the $(draft_label)" ;;
    eehrxf-draft-*) echo "the $(draft_label)" ;;
    *) echo "-" ;;
  esac
}

# The draft act the crosswalk reads, as eehrxf names it (`eehrxf::draft::DRAFT`).
draft_label() {
  sed -n 's/^pub const DRAFT: &str = "\(.*\)";$/\1/p' crates/eehrxf/src/draft/mod.rs
}

# The version docs/VERSIONS.md pins for the MyHealth@EU CI build of package
# $1, read from its profile package row, so a re-pin moves the badge label.
ci_build_version() {
  awk -F'|' -v want="$1" '
    NF >= 4 && $3 ~ /tarball `https:\/\/build-fhir\.ehdsi\.eu\// {
      k = $2; gsub(/[ `]/, "", k)
      if (k == want) { split($3, t, /[[:space:]]+/); print t[2]; exit }
    }' docs/VERSIONS.md
}

# The version the profile packages table of docs/VERSIONS.md pins for
# package $1, read from its row, so a re-pin moves the badge label; a preview
# keeps its label in the version.
pinned_version() {
  awk -F'|' -v want="$1" '
    /^## / { on = ($0 ~ /^## Profile packages/); next }
    on && NF >= 4 {
      k = $2; gsub(/[ `]/, "", k)
      if (k == want) { split($3, t, /[[:space:]]+/); print t[2]; exit }
    }' docs/VERSIONS.md
}

# The badge colour of k passing out of n, by the shared rule (owner decision,
# 2026-11-25): green when all pass, yellow when some do, red when none do.
colour_of() {
  badge_count_color "$1" "$2" "${3:-0}"
}

# The badge JSON of a label, k passing out of n, and logo $4, the message
# closing on the unit $5 when given, coloured as if the $6 cases an upstream
# holds were out of the count.
fraction_badge() {
  local label="$1" passed="$2" total="$3" logo unit="${5:-}" held="${6:-0}"
  logo="$(badge_logo_fields "$4")"
  jq -cn --arg label "$label" --arg message "$passed/$total$unit" \
    --arg color "$(colour_of "$passed" "$total" "$held")" \
    --argjson logo "$logo" '{schemaVersion: 1, label: $label, message: $message, color: $color} + $logo'
}

# The rows of the upstream file of corpus $1 as case<TAB>issue, none when it
# has no file, or status 2 naming the first malformed line.
upstream_rows() {
  local file="$root/$1/upstream.tsv"
  [[ -f "$file" ]] || return 0
  awk -F'\t' -v header="$UPSTREAM_HEADER" -v file="$file" '
    /^(#|$)/ { next }
    !seen { seen = 1; if ($0 != header) { printf "conformance: %s does not open on the header case<TAB>issue\n", file > "/dev/stderr"; bad = 1; exit } next }
    NF != 2 || $1 == "" || $2 !~ /^[1-9][0-9]*$/ {
      printf "conformance: %s line %d is not a case id and an issue number\n", file, NR > "/dev/stderr"; bad = 1; exit
    }
    ($1 in rows) { printf "conformance: %s line %d repeats the case %s\n", file, NR, $1 > "/dev/stderr"; bad = 1; exit }
    { rows[$1] = 1; print }
    END { if (bad) exit 2 }
  ' "$file"
}

# The passing case ids of the pass list of corpus $1, sorted.
listed_cases() {
  grep -vE "$COUNT_LINE" "$root/$1/pass-list.txt" | LC_ALL=C sort || true
}

# The count of upstream rows of corpus $1 whose case its pass list does not
# record, or status 2 for a malformed file.
held_of() {
  local rows
  rows="$(upstream_rows "$1")" || return 2
  LC_ALL=C comm -23 <(cut -f1 <<<"$rows" | sed '/^$/d' | LC_ALL=C sort) <(listed_cases "$1" | sed '/^$/d') |
    wc -l | tr -d ' '
}

# What count $2 of kind $1 reads as on conformance/README.md.
count_words() {
  case "$1" in
    unmapped) echo "$2 unmapped" ;;
    reviewed) echo "$2 extensible misses reviewed" ;;
    differ) echo "$2 differ from Xt-EHR" ;;
  esac
}

# The passing and total count of the pass list of corpus $1, as "<k> <n>".
fraction_of_list() {
  local list="$root/$1/pass-list.txt" passed total
  passed="$(grep -vEc "$COUNT_LINE" "$list" || true)"
  total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$list")"
  [[ -n "$total" ]] || { echo "conformance: $list has no total line" >&2; return 2; }
  echo "$passed $total"
}

# The badge JSON a pass list implies: the count alone, coloured with the
# cases an upstream holds out of it; conformance/README.md names the rest.
badge_of() {
  local corpus="$1" fraction passed total held
  fraction="$(fraction_of_list "$corpus")" || return 2
  read -r passed total <<<"$fraction"
  held="$(held_of "$corpus")" || return 2
  fraction_badge "$(label_of "$corpus")" "$passed" "$total" "$(logo_of "$corpus")" "$(unit_of "$corpus")" "$held"
}

# The issues that hold the failing upstream rows of corpus $1, as markdown
# links, ascending; "-" for none.
held_issues_of() {
  local rows
  rows="$(upstream_rows "$1")" || return 2
  LC_ALL=C join -t $'\t' -v 1 <(LC_ALL=C sort <<<"$rows" | sed '/^$/d') <(listed_cases "$1" | sed '/^$/d') |
    cut -f2 | sort -nu |
    awk '{ out = out (out == "" ? "" : ", ") "[#" $1 "](https://github.com/FerroHEALTH/FerroBRIDGE/issues/" $1 ")" }
      END { print (out == "" ? "-" : out) }'
}

# The row of corpus $1 on conformance/README.md.
page_row() {
  local corpus="$1" list="$root/$1/pass-list.txt" fraction passed total held issues kind count also=""
  fraction="$(fraction_of_list "$corpus")" || return 2
  read -r passed total <<<"$fraction"
  held="$(held_of "$corpus")" || return 2
  issues="$(held_issues_of "$corpus")" || return 2
  for kind in "${COUNT_KINDS[@]}"; do
    count="$(sed -n "s/^$kind \([0-9][0-9]*\)\$/\1/p" "$list")"
    [[ -z "$count" ]] || also="${also:+$also, }$(count_words "$kind" "$count")"
  done
  if [[ "$held" -eq 0 ]]; then held="-"; else held="$held ($issues)"; fi
  printf '| %s | %s of %s %s | %s | %s | %s | [pass list](%s/pass-list.txt) |\n' "$(label_of "$corpus")" \
    "$passed" "$total" "$(measure_of "$corpus")" "$held" "${also:--}" "$(edition_of "$corpus")" "$corpus"
}

# The corpus badge groups of the README, standards first, each a heading
# and its corpora in order; a label leaves out what its heading says.
readonly BADGE_GROUPS=(mapping fhir hl7v2 ehds myhealth-eu)

# The heading of badge group $1.
group_heading() {
  case "$1" in
    mapping) echo "Mapping languages" ;;
    fhir) echo "FHIR" ;;
    hl7v2) echo "HL7 v2" ;;
    ehds) echo "EHDS, $(draft_label)" ;;
    myhealth-eu) echo "MyHealth@EU" ;;
  esac
}

# The corpora of badge group $1, one per line.
group_corpora() {
  case "$1" in
    mapping) printf '%s\n' fhirconnect-mapping-lib rest-api omocl roundtrip ;;
    fhir) printf '%s\n' fhir-stu3 fhir-r4 fhir-r4b fhir-r5 fhir-r6 fhir-r4-facade fhirpath-r4 fhirpath-r4-stu3 validator-agreement ;;
    hl7v2) printf '%s\n' hl7v2 hl7v2-smoke ;;
    ehds) printf '%s\n' "${DRAFT_CORPORA[@]}" eehrxf-annex-x xtehr-crosswalk ;;
    myhealth-eu) printf '%s\n' myhealth-eu-core myhealth-eu-laboratory myhealth-eu-hdr myhealth-eu-mvc myhealth-eu-euvac ;;
  esac
}

# conformance/README.md: every corpus badge's count broken down.
render_page() {
  local name
  echo '<!-- SPDX-FileCopyrightText: Cadasto B.V. -->'
  echo '<!-- SPDX-License-Identifier: BUSL-1.1 -->'
  echo '<!-- Generated by scripts/checks/conformance.sh from the pass lists and upstream rows. Do not edit. -->'
  echo
  echo '# Conformance'
  echo
  echo 'Each README conformance badge shows one count. This page breaks every'
  echo 'count down, from the same pass lists and upstream rows the badges are'
  echo "rendered from; \`scripts/checks/conformance.sh --check\` fails when it"
  echo 'disagrees with them. The counts are self-tested; none is a'
  echo 'certification.'
  echo
  echo '- **Held upstream:** failing cases an open upstream-report issue holds'
  echo "  (\`upstream.tsv\` beside the pass list). They stay in the total. A badge is"
  echo '  green only when every case passes, and yellowgreen when every case that'
  echo '  does not pass is held upstream.'
  echo '- **Also counted:** unmapped annex elements; codes outside an'
  echo '  extensible value set that a review found no concept for (FHIR R4'
  echo '  terminologies 4.1.5.3), counted as passing; and annex elements whose'
  echo '  Xt-EHR element differs, passing when a decision records where each goes.'
  echo
  local group
  for group in "${BADGE_GROUPS[@]}"; do
    [[ "$group" = mapping ]] || echo
    echo "## $(group_heading "$group")"
    echo
    echo '| Badge | Count | Held upstream | Also counted | Edition | Pass list |'
    echo '|---|---|---|---|---|---|'
    while IFS= read -r name; do
      page_row "$name" || return 2
    done < <(group_corpora "$group")
  done
}

# Prints each badge file of $@ for the shared colour check, as FILE=HELD
# when its corpus's upstream rows hold HELD failing cases, else as FILE.
held_entries() {
  local file corpus held
  for file in "$@"; do
    corpus="$(basename "$file" .json)"
    held=0
    [[ ! -f "$root/$corpus/upstream.tsv" ]] || held="$(held_of "$corpus")" || return 1
    if [[ "$held" -gt 0 ]]; then echo "$file=$held"; else echo "$file"; fi
  done
}

# Holds the upstream rows of corpus $1 to its pass list and, when given, its
# result file $2: status 1 for a malformed file or a case the corpus does not
# hold, status 3 for a held case that passes, whose row --update drops.
upstream_check() {
  local corpus="$1" result="${2:-}" rows status=0 file="$root/$1/upstream.tsv" passing unknown
  rows="$(upstream_rows "$corpus")" || return 1
  [[ -n "$rows" ]] || return 0
  if [[ -n "$result" ]]; then
    unknown="$(LC_ALL=C comm -23 <(cut -f1 <<<"$rows" | LC_ALL=C sort) <(jq -r '.cases[].id' "$result" | LC_ALL=C sort))"
    if [[ -n "$unknown" ]]; then
      echo "conformance: $file names cases $corpus does not hold:" >&2
      printf '  %s\n' "$unknown" >&2
      status=1
    fi
  fi
  passing="$(LC_ALL=C comm -12 <(cut -f1 <<<"$rows" | LC_ALL=C sort) <(
    {
      listed_cases "$corpus"
      [[ -z "$result" ]] || jq -r '.cases[] | select(.passed) | .id' "$result"
    } | sed '/^$/d' | LC_ALL=C sort -u
  ))"
  if [[ -n "$passing" ]]; then
    if [[ "$mode" = update ]]; then
      echo "conformance: $corpus passes cases an upstream held; their rows leave $file:"
      awk -F'\t' 'NR == FNR { pass[$0] = 1; next } !($1 in pass)' <(printf '%s\n' "$passing") "$file" >"$file.new"
      mv "$file.new" "$file"
    else
      echo "conformance: $corpus passes cases $file says an upstream holds; run $0 --update:"
      [[ "$status" -ne 0 ]] || status=3
    fi
    printf '  %s\n' "$passing"
  fi
  return "$status"
}

failed=0

# Writes badge $1 as $2 under --update, else compares it with the file and
# names $3, what the badge is rendered from, when they disagree.
emit_badge() {
  local badge_name="$1" document="$2" source="$3"
  local badge="$BADGES/$badge_name.json"
  if [[ "$mode" = update ]]; then
    mkdir -p "$BADGES"
    printf '%s\n' "$document" >"$badge"
  elif [[ ! -f "$badge" ]] || [[ "$(cat "$badge")" != "$document" ]]; then
    echo "conformance: $badge disagrees with $source; run $0 --update"
    if [[ "$mode" = check ]]; then failed=1; fi
  fi
}

# Runs upstream_check on corpus $1 (and result $2) and records its verdict:
# a malformed file or an unknown case fails every mode, a held case that
# passes fails --check.
upstream_verdict() {
  local status=0
  upstream_check "$@" || status=$?
  case "$status" in
    0) ;;
    3) if [[ "$mode" = check ]]; then failed=1; fi ;;
    *) failed=1 ;;
  esac
}

# The cases of the badge rendering and the upstream rows, over a pass list
# and an upstream file written to a scratch tree.
run_self_test() {
  local work corpus=fhir-r4 status
  work="$(mktemp -d)"
  root="$work"
  mkdir -p "$work/$corpus"
  st_fail() {
    echo "conformance: self-test FAILED: $1" >&2
    rm -rf "$work"
    return 1
  }
  st_expect() {
    local document
    document="$(badge_of "$corpus")" || { st_fail "$3 renders no badge"; return 1; }
    if [[ "$(jq -r '.message + " " + .color' <<<"$document")" != "$1 $2" ]]; then
      st_fail "$3 reads $(jq -r '.message + " " + .color' <<<"$document"), not $1 $2"
      return 1
    fi
  }
  st_upstream() { printf '%s\n' "$UPSTREAM_HEADER" "$@" >"$work/$corpus/upstream.tsv"; }

  st_row() {
    local row
    row="$(page_row "$corpus")" || { st_fail "$3 renders no page row"; return 1; }
    if [[ "$row" != *"| $1 | $2 |"* ]]; then
      st_fail "$3 reads the page row $row, not held '$1' and also '$2'"
      return 1
    fi
  }

  printf '%s\n' a b 'total 4' >"$work/$corpus/pass-list.txt"
  st_expect '2/4' yellow "no upstream file" || return 1
  st_row - - "no upstream file" || return 1
  st_upstream $'c\t632' $'d\t632'
  st_expect '2/4' yellowgreen "every other case held upstream" || return 1
  st_row '2 ([#632](https://github.com/FerroHEALTH/FerroBRIDGE/issues/632))' - "every other case held upstream" || return 1
  st_upstream $'c\t632'
  st_expect '2/4' yellow "one failing case held, one not" || return 1
  printf '%s\n' 'total 2' >"$work/$corpus/pass-list.txt"
  st_upstream $'a\t632'
  st_expect '0/2' red "no case passing" || return 1
  printf '%s\n' a b 'total 4' 'unmapped 3' >"$work/$corpus/pass-list.txt"
  st_upstream $'c\t632' $'d\t2112'
  st_expect '2/4' yellowgreen "unmapped and upstream together" || return 1
  st_row '2 ([#632](https://github.com/FerroHEALTH/FerroBRIDGE/issues/632), [#2112](https://github.com/FerroHEALTH/FerroBRIDGE/issues/2112))' \
    '3 unmapped' "unmapped and upstream named on the page" || return 1
  printf '%s\n' a b 'total 4' 'reviewed 1' >"$work/$corpus/pass-list.txt"
  st_expect '2/4' yellowgreen "reviewed misses and upstream rows" || return 1
  st_row '2 ([#632](https://github.com/FerroHEALTH/FerroBRIDGE/issues/632), [#2112](https://github.com/FerroHEALTH/FerroBRIDGE/issues/2112))' \
    '1 extensible misses reviewed' "reviewed misses named on the page" || return 1
  printf '%s\n' a b 'total 4' 'unmapped 3' >"$work/$corpus/pass-list.txt"

  st_upstream $'c\t632' $'c\t2112'
  if badge_of "$corpus" >/dev/null 2>&1; then st_fail "a repeated case renders"; return 1; fi
  st_upstream $'c\t#632'
  if badge_of "$corpus" >/dev/null 2>&1; then st_fail "an issue that is no number renders"; return 1; fi
  printf '%s\n' 'case issue' $'c\t632' >"$work/$corpus/upstream.tsv"
  if badge_of "$corpus" >/dev/null 2>&1; then st_fail "another header renders"; return 1; fi
  st_upstream $'c\t632\textra'
  if badge_of "$corpus" >/dev/null 2>&1; then st_fail "a row of three columns renders"; return 1; fi

  st_upstream $'b\t632' $'c\t632'
  status=0
  upstream_check "$corpus" >/dev/null 2>&1 || status=$?
  [[ "$status" -eq 3 ]] || { st_fail "a held case that passes answers $status, not 3"; return 1; }
  st_expect '2/4' yellow "a held case that passes counts as passing only" || return 1
  st_row '1 ([#632](https://github.com/FerroHEALTH/FerroBRIDGE/issues/632))' '3 unmapped' "a held case that passes leaves the held count" || return 1
  jq -n '{total: 4, cases: [{id: "a", passed: true}, {id: "b", passed: true}, {id: "c", passed: false}]}' >"$work/result.json"
  st_upstream $'c\t632' $'z\t632'
  status=0
  upstream_check "$corpus" "$work/result.json" >/dev/null 2>&1 || status=$?
  [[ "$status" -eq 1 ]] || { st_fail "a case the corpus does not hold answers $status, not 1"; return 1; }
  st_upstream $'c\t632'
  upstream_check "$corpus" "$work/result.json" >/dev/null 2>&1 || { st_fail "a held failing case is refused"; return 1; }

  printf '%s\n' a b 'total 4' >"$work/$corpus/pass-list.txt"
  st_upstream $'c\t632' $'d\t632'
  jq -n '{message: "2/4", color: "yellowgreen"}' >"$work/$corpus.json"
  [[ "$(held_entries "$work/$corpus.json")" == "$work/$corpus.json=2" ]] || { st_fail "the held count of an upstream badge is not passed on"; return 1; }
  # shellcheck disable=SC2046 # one entry per line, none with a space
  badge_color_check $(held_entries "$work/$corpus.json") 2>/dev/null || { st_fail "a yellowgreen badge whose rest all pass is refused"; return 1; }
  jq -n '{message: "2/4", color: "brightgreen"}' >"$work/$corpus.json"
  # shellcheck disable=SC2046 # one entry per line, none with a space
  if badge_color_check $(held_entries "$work/$corpus.json") 2>/dev/null; then st_fail "a green badge with held cases passes"; return 1; fi
  jq -n '{message: "2/4", color: "yellowgreen"}' >"$work/$corpus.json"
  st_upstream $'c\t632'
  # shellcheck disable=SC2046 # one entry per line, none with a space
  if badge_color_check $(held_entries "$work/$corpus.json") 2>/dev/null; then st_fail "a yellowgreen badge with an unheld failing case passes"; return 1; fi
  rm "$work/$corpus/upstream.tsv"
  [[ "$(held_entries "$work/$corpus.json")" == "$work/$corpus.json" ]] || { st_fail "a plain badge carries a held count"; return 1; }

  local name
  for name in "${CORPORA[@]}" "${EXTERNAL_CORPORA[@]}"; do
    label_of "$name" >"$work/label" || { st_fail "$name has no label"; return 1; }
    jq -n --rawfile label "$work/label" '{label: ($label | rtrimstr("\n")), message: "9999/9999 codes"}' >"$work/badge.json"
    badge_length_check "$work/badge.json" || { st_fail "the badge of $name runs past the length limits"; return 1; }
  done
  jq -n '{label: "FHIR R4", message: "5295/5309, 14 upstream"}' >"$work/badge.json"
  if badge_length_check "$work/badge.json" 2>/dev/null; then st_fail "a message past 20 characters passes"; return 1; fi
  badge_length_self_test >/dev/null || { st_fail "the shared length check"; return 1; }
  badge_count_color_self_test >/dev/null || { st_fail "the shared colour rule"; return 1; }
  printf '[![x](https://img.shields.io/badge/CRA-information%%20for%%20users-blue?logo=x)](x)\n' >"$work/README.md"
  if badge_length_readme "$work/README.md" 2>/dev/null; then st_fail "a static badge past 20 characters passes"; return 1; fi
  printf '%s\n' "${COMPLIANCE_BADGES[@]}" "${PROJECT_BADGES[@]}" "${QUALITY_BADGES[@]}" >"$work/README.md"
  badge_length_readme "$work/README.md" || { st_fail "a static badge of the README block runs long"; return 1; }

  rm -rf "$work"
  root=conformance
  echo "conformance: self-test OK"
}

if [[ "$self_test" -eq 1 ]]; then
  run_self_test
  exit
fi

# Runs the corpus tests the run selects, compares each result with its pass
# list, and emits its badge.
measure_and_compare() {
  # A MyHealth@EU corpus named alone reads the result the validator lane left.
  [[ "$external_only" -eq 1 ]] || rm -rf "$OUT"
  export FERROBRIDGE_CONFORMANCE_OUT="$PWD/$OUT"
  if [[ "$mode" = update ]]; then
    export FERROBRIDGE_CONFORMANCE_UPDATE=1
  fi
  # The hl7v2-smoke corpus reads the HL7 v2 sets fetched at build time; the
  # script keeps a tree already at its pinned digest without a download. A run
  # of one corpus fetches only what that corpus reads.
  if [[ "$external_only" -eq 0 ]] && [[ -z "$only" || "$only" = hl7v2* ]] && ! scripts/vendor/hl7v2-samples.sh --build-time; then
    echo "conformance: the build-time HL7 v2 sets could not be fetched" >&2
    exit 2
  fi

  # The FHIR corpora read the examples packages fetched at build time, kept the
  # same way at their pinned count and digest.
  if [[ "$external_only" -eq 0 ]] && [[ -z "$only" || "$only" = fhir-* ]] && ! scripts/vendor/fhir-packages.sh --build-time; then
    echo "conformance: the FHIR examples packages could not be fetched" >&2
    exit 2
  fi

  # The R4 profile validator's package set (#435), fetched at build time and
  # kept the same way, so the validator corpus finds it on disk.
  if [[ -z "$only" || "$only" = validator-agreement ]] && ! scripts/vendor/profile-packages.sh; then
    echo "conformance: the profile packages could not be fetched" >&2
    exit 2
  fi
  # Runs the conformance tests that filter $1 selects in the packages after it.
  measure() {
    local test_filter="$1" package
    shift
    local args=()
    for package in "$@"; do
      args+=(-p "$package")
    done
    if ! cargo nextest run --locked "${args[@]}" --no-tests=fail --no-fail-fast -E "$test_filter"; then
      echo "conformance: a corpus test of $* failed; a case its list records no longer passes"
      failed=1
    fi
  }

  # Runs the one test that measures corpus $1.
  measure_corpus() {
    local corpus_name="$1"
    local package test
    read -r package test < <(test_of "$corpus_name")
    measure "test(/(^|::)${test}\$/)" "$package"
  }

  # Every package in one invocation, so each workspace crate compiles once with
  # the features of the whole set. The CI test lane runs the same tests one
  # package at a time, with each package's own features.
  if [[ -z "$only" ]]; then
    measure 'test(/conformance_/)' "${PACKAGES[@]}"
  fi
  selected=("${CORPORA[@]}" "${EXTERNAL_CORPORA[@]}")
  if [[ "$external_only" -eq 1 ]]; then
    selected=("$only")
  elif [[ -n "$only" ]]; then
    selected=("$only")
    measure_corpus "$only"
    # The other HL7 v2 corpus is measured for the family and version badges,
    # with its own list left as it is.
    case "$only" in
      hl7v2) sibling=hl7v2-smoke ;;
      hl7v2-smoke) sibling=hl7v2 ;;
      *) sibling="" ;;
    esac
    if [[ -n "$sibling" ]]; then
      FERROBRIDGE_CONFORMANCE_UPDATE=0 measure_corpus "$sibling"
    fi
  fi

  for corpus in "${selected[@]}"; do
    result="$OUT/$corpus.json"
    list="conformance/$corpus/pass-list.txt"
    if [[ ! -f "$result" ]] && contains "$corpus" "${EXTERNAL_CORPORA[@]}" && [[ -z "$only" ]]; then
      echo "conformance: $corpus is measured outside this script, not in this run"
      upstream_verdict "$corpus"
      emit_badge "$corpus" "$(badge_of "$corpus")" "$list"
      continue
    fi
    if [[ ! -f "$result" ]]; then
      echo "conformance: no test reported $corpus ($result is missing)" >&2
      exit 2
    fi
    # The validator lane writes no pass list, so --update writes it here.
    if [[ "$mode" = update ]] && contains "$corpus" "${EXTERNAL_CORPORA[@]}"; then
      mkdir -p "conformance/$corpus"
      {
        jq -r '.cases[] | select(.passed) | .id' "$result" | LC_ALL=C sort
        echo "total $(jq -r '.total' "$result")"
        for kind in "${COUNT_KINDS[@]}"; do
          jq -r --arg kind "$kind" 'select(.[$kind] != null) | "\($kind) \(.[$kind])"' "$result"
        done
      } >"$list"
    fi
    passing="$(jq -r '.cases[] | select(.passed) | .id' "$result" | LC_ALL=C sort)"
    measured_total="$(jq -r '.total' "$result")"
    listed="$(grep -vE "$COUNT_LINE" "$list" | LC_ALL=C sort || true)"
    listed_total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$list")"
    regressed="$(LC_ALL=C comm -23 <(printf '%s\n' "$listed" | sed '/^$/d') <(printf '%s\n' "$passing" | sed '/^$/d'))"
    new="$(LC_ALL=C comm -13 <(printf '%s\n' "$listed" | sed '/^$/d') <(printf '%s\n' "$passing" | sed '/^$/d'))"
    count="$(printf '%s\n' "$passing" | sed '/^$/d' | wc -l | tr -d ' ')"
    echo "conformance: $corpus $count / $measured_total"
    if [[ -n "$regressed" ]]; then
      echo "conformance: $corpus regressed:"
      while IFS= read -r id; do
        echo "  $id: $(jq -r --arg id "$id" '(.cases[] | select(.id == $id) | .failure) // "the corpus no longer holds it"' "$result")"
      done <<<"$regressed"
      failed=1
    fi
    if [[ -n "$new" ]]; then
      echo "conformance: $corpus passes cases its list does not record; run $0 --update:"
      printf '  %s\n' "$new"
      if [[ "$mode" = check ]]; then failed=1; fi
    fi
    for kind in "${COUNT_KINDS[@]}"; do
      measured_count="$(jq -r --arg kind "$kind" '.[$kind] // empty' "$result")"
      listed_count="$(sed -n "s/^$kind \([0-9][0-9]*\)\$/\1/p" "$list")"
      if [[ "$measured_count" != "$listed_count" ]]; then
        echo "conformance: $corpus measures ${measured_count:-no} $kind and its list records ${listed_count:-none}; run $0 --update"
        if [[ "$mode" = check ]]; then failed=1; fi
      fi
    done
    if [[ "$measured_total" != "$listed_total" ]]; then
      echo "conformance: $corpus holds $measured_total cases and its list records $listed_total; run $0 --update"
      if [[ "$mode" = check ]]; then failed=1; fi
    fi
    upstream_verdict "$corpus" "$result"
    emit_badge "$corpus" "$(badge_of "$corpus")" "$list"
  done
}

# Emits every corpus badge from its committed pass list and upstream rows,
# running no test: the render-only mode after a change to the badge format.
render_from_lists() {
  local corpus
  for corpus in "${CORPORA[@]}" "${EXTERNAL_CORPORA[@]}"; do
    upstream_verdict "$corpus"
    emit_badge "$corpus" "$(badge_of "$corpus")" "conformance/$corpus/pass-list.txt"
  done
}

if [[ "$render_only" -eq 1 ]]; then
  render_from_lists
else
  measure_and_compare
fi

families=()
versions=()

# The family and version badges already on disk, in the order a measured run
# sorts them, for a run of one corpus other than the HL7 v2 two.
badges_on_disk() {
  local name
  while IFS= read -r name; do
    families+=("$name")
  done < <(cd "$BADGES" && find . -name 'hl7v2-family-*.json' | sed 's#^\./##; s#\.json$##' | LC_ALL=C sort)
  while IFS= read -r name; do
    versions+=("hl7v2-version-$name")
  done < <(cd "$BADGES" && find . -name 'hl7v2-version-*.json' |
    sed 's#^\./hl7v2-version-##; s#\.json$##' | sort -t. -k1,1n -k2,2n -k3,3n)
}

# The family and version badges of both measured HL7 v2 corpora, emitted, and
# a badge no case counts any more removed under --update.
badges_measured() {
  local hl7v2_cases family version name passed total stale
  # Every case of both HL7 v2 corpora, and the passing and total count of the
  # cases whose field $1 is $2.
  hl7v2_cases="$(jq -c -s '[.[].cases[]]' "$OUT/hl7v2.json" "$OUT/hl7v2-smoke.json")"
  fraction_of() {
    local field="$1" value="$2"
    jq -r --arg field "$field" --arg value "$value" \
      '[.[] | select(.[$field] == $value)] | "\([.[] | select(.passed)] | length) \(length)"' \
      <<<"$hl7v2_cases"
  }
  while IFS= read -r family; do
    [[ -n "$family" ]] || continue
    read -r passed total < <(fraction_of family "$family")
    name="hl7v2-family-$(tr '[:upper:]' '[:lower:]' <<<"$family")"
    families+=("$name")
    emit_badge "$name" "$(fraction_badge "$family" "$passed" "$total" message)" "the HL7 v2 results"
  done < <(jq -r '[.[] | .family | select(. != null and test("^[A-Z0-9]+$"))] | unique | .[]' \
    <<<"$hl7v2_cases" | LC_ALL=C sort)
  while IFS= read -r version; do
    [[ -n "$version" ]] || continue
    read -r passed total < <(fraction_of version "$version")
    name="hl7v2-version-$version"
    versions+=("$name")
    emit_badge "$name" "$(fraction_badge "v$version" "$passed" "$total" message)" "the HL7 v2 results"
  done < <(jq -r '[.[] | .version | select(. != null and test("^[0-9]+(\\.[0-9]+)+$"))]
    | unique | sort_by(split(".") | map(tonumber)) | .[]' <<<"$hl7v2_cases")

  for stale in "$BADGES"/hl7v2-family-*.json "$BADGES"/hl7v2-version-*.json; do
    [[ -f "$stale" ]] || continue
    if ! contains "$(basename "$stale" .json)" ${families[@]+"${families[@]}"} \
      ${versions[@]+"${versions[@]}"}; then
      if [[ "$mode" = update ]]; then
        rm "$stale"
      else
        echo "conformance: $stale counts no case any more; run $0 --update"
        if [[ "$mode" = check ]]; then failed=1; fi
      fi
    fi
  done
}

if [[ -f "$OUT/hl7v2.json" && -f "$OUT/hl7v2-smoke.json" ]]; then
  badges_measured
else
  badges_on_disk
fi

# The markdown of one badge: its alt text, its badge file, the list it links to.
badge_link() {
  echo "[![$1](${ENDPOINT}${2}.json)]($3)"
}

# The README block with its markers, in a fixed order: the project and quality
# rows, then the conformance rows linked to conformance/README.md, closing on
# the HL7 v2 family and version badges.
render_block() {
  local name group
  echo '<!-- badges:begin -->'
  echo '**Project**<br>'
  printf '%s\n' "${PROJECT_BADGES[@]}"
  echo
  echo '**Quality and security**<br>'
  printf '%s\n' "${QUALITY_BADGES[@]}"
  echo
  echo "$badge_caption<br>"
  for group in "${BADGE_GROUPS[@]}"; do
    [[ "$group" = mapping ]] || echo
    echo "**$(group_heading "$group")**<br>"
    while IFS= read -r name; do
      badge_link "$(label_of "$name")" "$name" "$PAGE"
    done < <(group_corpora "$group")
    # The per-family and per-version badges fold under the HL7 v2 group, so
    # the per-corpus badges stay in view; GitHub renders Markdown inside
    # <details> only after a blank line.
    if [[ "$group" = hl7v2 ]] && [[ ${#families[@]} -gt 0 || ${#versions[@]} -gt 0 ]]; then
      echo
      echo '<details>'
      echo '<summary>By message family and by version, across both corpora</summary>'
      echo
      for name in ${families[@]+"${families[@]}"}; do
        badge_link "HL7 v2 $(tr '[:lower:]' '[:upper:]' <<<"${name#hl7v2-family-}")" "$name" conformance/hl7v2/pass-list.txt
      done
      for name in ${versions[@]+"${versions[@]}"}; do
        badge_link "HL7 v${name#hl7v2-version-}" "$name" conformance/hl7v2/pass-list.txt
      done
      echo
      echo '</details>'
    fi
  done
  echo
  echo '**Security and compliance**<br>'
  printf '%s\n' "${COMPLIANCE_BADGES[@]}"
  echo '<!-- badges:end -->'
}

# No badge claims more than a self-test measures (#2090): a label or message
# that says certified, compliant or conformant fails every mode.
for badge in "$BADGES"/*.json; do
  [[ -f "$badge" ]] || continue
  if jq -r '.label, .message' "$badge" | grep -qiE 'certified|compliant|conformant'; then
    echo "conformance: $badge claims a certification; a badge names what it measures and its N / M"
    failed=1
  fi
done

block="$(render_block)"
if ! grep -qx '<!-- badges:begin -->' README.md || ! grep -qx '<!-- badges:end -->' README.md; then
  echo "conformance: README.md has no badges:begin and badges:end markers"
  exit 1
fi
if [[ "$mode" = update ]]; then
  rendered="$(mktemp)"
  trap 'rm -f "$rendered" "$rendered.readme"' EXIT
  printf '%s\n' "$block" >"$rendered"
  awk -v block="$rendered" '
    $0 == "<!-- badges:begin -->" { while ((getline line < block) > 0) print line; skipping = 1; next }
    $0 == "<!-- badges:end -->" { skipping = 0; next }
    !skipping { print }
  ' README.md >"$rendered.readme"
  cat "$rendered.readme" >README.md
elif [[ "$(sed -n '/^<!-- badges:begin -->$/,/^<!-- badges:end -->$/p' README.md)" != "$block" ]]; then
  echo "conformance: the README.md badge block disagrees with the badge files; run $0 --update"
  if [[ "$mode" = check ]]; then failed=1; fi
fi

page="$(render_page)" || exit 2
if [[ "$mode" = update ]]; then
  printf '%s\n' "$page" >"$PAGE"
elif [[ "$(cat "$PAGE" 2>/dev/null)" != "$page" ]]; then
  echo "conformance: $PAGE disagrees with the pass lists and upstream rows; run $0 --update"
  if [[ "$mode" = check ]]; then failed=1; fi
fi

# A long badge fails every mode: its breakdown belongs on the page.
badge_length_check "$BADGES"/*.json || failed=1
badge_length_readme README.md || failed=1
badge_block_check README.md || failed=1
badge_render_check README.md || failed=1

mapfile -t colour_entries < <(held_entries "$BADGES"/*.json)
if ! badge_logo_check "$BADGES"/*.json || ! badge_logo_readme README.md || ! badge_caption_check README.md ||
  ! badge_color_check "${colour_entries[@]}"; then
  if [[ "$mode" = check ]]; then failed=1; fi
fi

exit "$failed"
