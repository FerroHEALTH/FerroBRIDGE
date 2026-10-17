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

# Runs the one test that measures corpus $1.
measure_corpus() {
  local package test
  read -r package test < <(test_of "$1")
  measure "$package" "test(/(^|::)${test}\$/)"
}

# One package per invocation, as the CI test lane runs them.
for package in "${PACKAGES[@]}"; do
  [[ -z "$only" ]] || break
  measure "$package" 'test(/conformance_/)'
done
selected=("${CORPORA[@]}")
if [[ -n "$only" ]]; then
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
  if [[ ! -f "$result" ]]; then
    echo "conformance: no test reported $corpus ($result is missing)" >&2
    exit 2
  fi
  passing="$(jq -r '.cases[] | select(.passed) | .id' "$result" | LC_ALL=C sort)"
  measured_total="$(jq -r '.total' "$result")"
  listed="$(grep -v '^total ' "$list" | LC_ALL=C sort || true)"
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
  if [[ "$measured_total" != "$listed_total" ]]; then
    echo "conformance: $corpus holds $measured_total cases and its list records $listed_total; run $0 --update"
    if [[ "$mode" = check ]]; then failed=1; fi
  fi
  emit_badge "$corpus" "$(badge_of "$corpus")" "$list"
done

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
  local hl7v2_cases family version name passed total derived stale
  # Every case of both HL7 v2 corpora, and the passing and total count of the
  # cases whose field $1 is $2.
  hl7v2_cases="$(jq -c -s '[.[].cases[]]' "$OUT/hl7v2.json" "$OUT/hl7v2-smoke.json")"
  fraction_of() {
    jq -r --arg field "$1" --arg value "$2" \
      '[.[] | select(.[$field] == $value)] | "\([.[] | select(.passed)] | length) \(length)"' \
      <<<"$hl7v2_cases"
  }
  while IFS= read -r family; do
    [[ -n "$family" ]] || continue
    read -r passed total < <(fraction_of family "$family")
    name="hl7v2-family-$(tr '[:upper:]' '[:lower:]' <<<"$family")"
    families+=("$name")
    emit_badge "$name" "$(fraction_badge "$family" "$passed" "$total")" "the HL7 v2 results"
  done < <(jq -r '[.[] | .family | select(. != null and test("^[A-Z0-9]+$"))] | unique | .[]' \
    <<<"$hl7v2_cases" | LC_ALL=C sort)
  while IFS= read -r version; do
    [[ -n "$version" ]] || continue
    read -r passed total < <(fraction_of version "$version")
    name="hl7v2-version-$version"
    versions+=("$name")
    emit_badge "$name" "$(fraction_badge "v$version" "$passed" "$total")" "the HL7 v2 results"
  done < <(jq -r '[.[] | .version | select(. != null and test("^[0-9]+(\\.[0-9]+)+$"))]
    | unique | sort_by(split(".") | map(tonumber)) | .[]' <<<"$hl7v2_cases")

  derived=" ${families[*]-} ${versions[*]-} "
  for stale in "$BADGES"/hl7v2-family-*.json "$BADGES"/hl7v2-version-*.json; do
    [[ -f "$stale" ]] || continue
    if [[ "$derived" != *" $(basename "$stale" .json) "* ]]; then
      if [[ "$mode" = update ]]; then
        rm "$stale"
      else
        echo "conformance: $stale counts no case any more; run $0 --update"
        if [[ "$mode" = check ]]; then failed=1; fi
      fi
    fi
  done
}
