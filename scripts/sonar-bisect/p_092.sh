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
