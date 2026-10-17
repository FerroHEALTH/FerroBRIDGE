#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The conformance gate (#24): runs the thirteen corpus tests, compares what they
# measured with the committed pass lists under conformance/, and renders one
# shields.io endpoint badge per corpus (https://shields.io/badges/endpoint-badge),
# one per HL7 v2 message family and one per HL7 v2 version (#366), and the
# README block between badges:begin and badges:end from those badges.
#
#   scripts/checks/conformance.sh            # run and compare; warn on drift
#   scripts/checks/conformance.sh --update   # run and rewrite lists, badges, README block
#   scripts/checks/conformance.sh --check    # run and fail on any drift (CI)
#   scripts/checks/conformance.sh --update fhir-r4b   # one corpus: its test, list, badge
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
# the list is what the ratchet protects.
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
# package set is fetched the same way through scripts/vendor/profile-packages.sh.
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
  'draft-rest-api fhirconnect conformance_the_draft_operation_definitions_hold_their_pass_list'
  'hl7v2 ferrobridge-hl7v2 conformance_the_vendored_hl7v2_corpora_hold_their_pass_list'
  'hl7v2-smoke ferrobridge-hl7v2 conformance_the_fetched_hl7v2_corpora_hold_their_pass_list'
  'fhir-stu3 ferrobridge-testkit conformance_the_fhir_stu3_examples_hold_their_pass_list'
  'fhir-r4 ferrobridge-testkit conformance_the_fhir_r4_examples_hold_their_pass_list'
  'fhir-r4b ferrobridge-testkit conformance_the_fhir_r4b_examples_hold_their_pass_list'
  'fhir-r5 ferrobridge-testkit conformance_the_fhir_r5_examples_hold_their_pass_list'
  'fhir-r6 ferrobridge-testkit conformance_the_fhir_r6_examples_hold_their_pass_list'
  'fhir-r4-facade ferrobridge-server conformance_the_fhir_r4_examples_hold_their_facade_pass_list'
  'fhirpath-r4 ferrobridge-fhirpath conformance_fhirpath_r4'
)
corpora=()
packages=()
for row in "${CORPUS_TESTS[@]}"; do
  corpora+=("${row%% *}")
  package="${row#* }"
  package="${package%% *}"
  [[ " ${packages[*]-} " = *" $package "* ]] || packages+=("$package")
done
readonly CORPORA=("${corpora[@]}")
# The packages of the table, each once, in the order it first names them.
readonly PACKAGES=("${packages[@]}")

# The package and the test name of corpus $1, or status 1 for no such corpus.
test_of() {
  local row
  for row in "${CORPUS_TESTS[@]}"; do
    if [[ "${row%% *}" = "$1" ]]; then
      echo "${row#* }"
      return 0
    fi
  done
  return 1
}

usage() {
  echo "usage: $0 [--update|--check] [corpus]" >&2
  echo "corpora: ${CORPORA[*]}" >&2
  exit 2
}

mode=compare
only=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --update) mode=update ;;
    --check) mode=check ;;
    -*) usage ;;
    *)
      [[ -z "$only" ]] || usage
      test_of "$1" >/dev/null || { echo "conformance: no corpus is named $1" >&2; usage; }
      only="$1"
      ;;
  esac
  shift
done

# A missing cargo would otherwise read as every corpus test failing.
if ! command -v cargo >/dev/null 2>&1; then
  echo "conformance: cargo not found on PATH" >&2
  exit 1
fi
cd "$(dirname "$0")/../.."

readonly OUT=target/conformance
readonly BADGES=conformance/badges
# The shields.io endpoint prefix every conformance badge file is read through.
readonly ENDPOINT='https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2F'
# The build badges, the first row of the README block.
readonly BUILD_BADGES=(
  '[![CI](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml/badge.svg)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml)'
  '[![CodeQL](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml/badge.svg)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml)'
  '[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroBRIDGE/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroBRIDGE)'
  '[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=FerroHEALTH_FerroBRIDGE&metric=alert_status)](https://sonarcloud.io/summary/overall?id=FerroHEALTH_FerroBRIDGE)'
  '[![Coverage](https://sonarcloud.io/api/project_badges/measure?project=FerroHEALTH_FerroBRIDGE&metric=coverage)](https://sonarcloud.io/summary/new_code?id=FerroHEALTH_FerroBRIDGE)'
  '[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)'
  '[![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/FerroHEALTH/FerroBRIDGE?sort=semver)](https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest)'
)

# The badge label of a corpus.
label_of() {
  case "$1" in
    fhirconnect-mapping-lib) echo "FHIRconnect mapping library" ;;
    omocl) echo "OMOCL mapping library" ;;
    roundtrip) echo "FHIR round-trip laws" ;;
    draft-rest-api) echo "FHIRconnect REST API (draft)" ;;
    hl7v2) echo "HL7 v2 message corpora" ;;
    hl7v2-smoke) echo "HL7 v2 smoke corpora (NIST, AIRA)" ;;
    fhir-stu3) echo "FHIR STU3" ;;
    fhir-r4) echo "FHIR R4" ;;
    fhir-r4b) echo "FHIR R4B" ;;
    fhir-r5) echo "FHIR R5" ;;
    fhir-r6) echo "FHIR R6" ;;
    fhir-r4-facade) echo "FHIR R4 facade" ;;
    fhirpath-r4) echo "FHIRPath R4 test suite" ;;
    *) return 1 ;;
  esac
}

# The badge colour of k passing out of n: the share in quarters.
colour_of() {
  local passed="$1" total="$2" share
  if [[ "$total" -eq 0 ]]; then
    echo lightgrey
    return
  fi
  share=$((passed * 100 / total))
  if [[ "$passed" -eq "$total" ]]; then echo brightgreen
  elif [[ "$share" -ge 75 ]]; then echo green
  elif [[ "$share" -ge 50 ]]; then echo yellow
  elif [[ "$share" -ge 25 ]]; then echo orange
  else echo red
  fi
}

# The badge JSON of a label and k passing out of n.
fraction_badge() {
  jq -cn --arg label "$1" --arg message "$2 / $3" --arg color "$(colour_of "$2" "$3")" \
    '{schemaVersion: 1, label: $label, message: $message, color: $color}'
}

# The badge JSON a pass list implies.
badge_of() {
  local corpus="$1" list="conformance/$1/pass-list.txt" passed total
  passed="$(grep -vc '^total ' "$list" || true)"
  total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$list")"
  [[ -n "$total" ]] || { echo "conformance: $list has no total line" >&2; return 2; }
  fraction_badge "$(label_of "$corpus")" "$passed" "$total"
}

failed=0

# Writes badge $1 as $2 under --update, else compares it with the file and
# names $3, what the badge is rendered from, when they disagree.
emit_badge() {
  local badge="$BADGES/$1.json"
  if [[ "$mode" = update ]]; then
    mkdir -p "$BADGES"
    printf '%s\n' "$2" >"$badge"
  elif [[ ! -f "$badge" ]] || [[ "$(cat "$badge")" != "$2" ]]; then
    echo "conformance: $badge disagrees with $3; run $0 --update"
    if [[ "$mode" = check ]]; then failed=1; fi
  fi
}

rm -rf "$OUT"
export FERROBRIDGE_CONFORMANCE_OUT="$PWD/$OUT"
if [[ "$mode" = update ]]; then
  export FERROBRIDGE_CONFORMANCE_UPDATE=1
fi
# The hl7v2-smoke corpus reads the HL7 v2 sets fetched at build time; the
# script keeps a tree already at its pinned digest without a download. A run
# of one corpus fetches only what that corpus reads.
if [[ -z "$only" || "$only" = hl7v2* ]] && ! scripts/vendor/hl7v2-samples.sh --build-time; then
  echo "conformance: the build-time HL7 v2 sets could not be fetched" >&2
  exit 2
fi

# The FHIR corpora read the examples packages fetched at build time, kept the
# same way at their pinned count and digest.
if [[ -z "$only" || "$only" = fhir-* ]] && ! scripts/vendor/fhir-packages.sh --build-time; then
  echo "conformance: the FHIR examples packages could not be fetched" >&2
  exit 2
fi

# The R4 profile validator's package set (#435), fetched at build time and
# kept the same way, so the validator corpus finds it on disk.
if [[ -z "$only" ]] && ! scripts/vendor/profile-packages.sh; then
  echo "conformance: the profile packages could not be fetched" >&2
  exit 2
fi
# Runs the conformance tests of package $1 that filter $2 selects.
measure() {
  if ! cargo nextest run --locked -p "$1" --no-tests=fail --no-fail-fast -E "$2"; then
    echo "conformance: a $1 corpus test failed; a case its list records no longer passes"
    failed=1
  fi
}
