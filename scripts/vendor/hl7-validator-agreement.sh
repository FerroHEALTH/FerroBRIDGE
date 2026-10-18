#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/hl7-validator-agreement.sh
#
# Regenerates the reference outcomes of the profile validator's agreement
# corpus (#442) by running the HL7 FHIR Validator over the synthetic cases in
# crates/ferrobridge-validator/tests/agreement/cases/ and writing one
# normalized outcome per case into
# crates/ferrobridge-validator/tests/agreement/reference/. The HL7 FHIR
# Validator is prior art and the agreement target, never the oracle.
#
# The "HL7 FHIR Validator" table of docs/VERSIONS.md pins validator_cli.jar
# and the five packages the validator loads for FHIR 4.0.1 beyond the ones
# this repository already fetches, each by version and sha256; this script
# reads its pins from that table. The jar and the packages are fetched into
# the ignored target/hl7-validator/, verified, and never committed. The
# validator then runs offline (-no-http-access) against a private package
# cache holding R4 core from tools/fhir-codegen/vendor/, the profile packages
# scripts/vendor/profile-packages.sh fetched into docs/specs/fhir-packages/,
# and those five, with no terminology server (-tx n/a).
#
# A normalized outcome keeps the error and fatal issues alone, each as its
# severity, its location with the validator's /*...*/ comments removed, its
# rule (constraint:<key> for a failed invariant, whose message id is
# <definition>#<key>, the message id otherwise) and its text, sorted and
# without repeats.
#
# Usage:
#   scripts/vendor/hl7-validator-agreement.sh          # fetch what is missing, run, rewrite the outcomes
#   scripts/vendor/hl7-validator-agreement.sh --fetch  # fetch and verify the jar and the packages only
#
# Requires: curl, jq, tar, shasum, and a Java 17 or later runtime as $JAVA or
# java on PATH. Run scripts/vendor/profile-packages.sh first.
#
# No specification governs this file; it is FerroBRIDGE's own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

pins="docs/VERSIONS.md"
work="target/hl7-validator"
cache="$work/home/.fhir/packages"
corpus="crates/ferrobridge-validator/tests/agreement"
registry="https://packages.fhir.org"
ua="ferrobridge-vendor (scripts/vendor/hl7-validator-agreement.sh)"
java="${JAVA:-java}"

# The rows of the HL7 FHIR Validator table, one per line as
# `<item> <version> <sha256>`.
rows() {
  awk '
    /^## HL7 FHIR Validator/ { inside = 1; next }
    /^## / { inside = 0 }
    inside && /^\| `/ {
      item = $0; sub(/^\| `/, "", item); sub(/`.*/, "", item)
      pin = $0; sub(/^\| `[^`]*` \| /, "", pin); sub(/ \|.*/, "", pin)
      split(pin, part, " ")
      digest = part[3]; gsub(/`/, "", digest)
      print item, part[1], digest
    }
  ' "$pins"
}

# Fails unless the file $1 has the sha256 $2.
verify() {
  local found
  found="$(shasum -a 256 "$1" | cut -d' ' -f1)"
  if [[ "$found" != "$2" ]]; then
    echo "hl7-validator-agreement: $1 has sha256 $found, the pin is $2" >&2
    exit 1
  fi
}

# Downloads $1 to $2 unless $2 is already there at the sha256 $3.
fetch() {
  if [[ -f "$2" ]] && [[ "$(shasum -a 256 "$2" | cut -d' ' -f1)" == "$3" ]]; then
    return
  fi
  curl -fsSL --retry 3 -A "$ua" -o "$2" "$1"
  verify "$2" "$3"
}

# Fetches the jar and the pinned packages, and fills the private cache.
prepare() {
  mkdir -p "$work" "$cache"
  local item version digest
  while read -r item version digest; do
    if [[ "$item" == "validator_cli.jar" ]]; then
      fetch "https://github.com/hapifhir/org.hl7.fhir.core/releases/download/$version/validator_cli.jar" \
        "$work/validator_cli.jar" "$digest"
      continue
    fi
    fetch "$registry/$item/$version" "$work/$item#$version.tgz" "$digest"
    if [[ ! -d "$cache/$item#$version/package" ]]; then
      mkdir -p "$cache/$item#$version"
      tar -xzf "$work/$item#$version.tgz" -C "$cache/$item#$version"
    fi
  done < <(rows)
  if [[ ! -d "$cache/hl7.fhir.r4.core#4.0.1/package" ]]; then
    mkdir -p "$cache/hl7.fhir.r4.core#4.0.1"
    cp -R tools/fhir-codegen/vendor/hl7.fhir.r4.core/package "$cache/hl7.fhir.r4.core#4.0.1/"
  fi
  local tree name
  for tree in docs/specs/fhir-packages/*#*; do
    name="$(basename "$tree")"
    if [[ ! -d "$tree/package" ]]; then
      echo "hl7-validator-agreement: $tree is not fetched; run scripts/vendor/profile-packages.sh" >&2
      exit 1
    fi
    if [[ ! -d "$cache/$name/package" ]]; then
      mkdir -p "$cache/$name"
      cp -R "$tree/package" "$cache/$name/"
    fi
  done
}

# Runs the validator over every case and rewrites the normalized outcomes.
run() {
  local raw="$work/outcomes.json"
  "$java" -Duser.home="$root/$work/home" -jar "$work/validator_cli.jar" "$corpus/cases" \
    -version 4.0.1 -ig 'hl7.fhir.eu.eps#1.0.0-ballot' -ig 'hl7.fhir.eu.base#2.0.1' \
    -tx n/a -no-http-access -show-message-ids -output "$raw" >"$work/run.log" 2>&1 || true
  if [[ ! -s "$raw" ]]; then
    echo "hl7-validator-agreement: the validator wrote no outcome; see $work/run.log" >&2
    exit 1
  fi
  mkdir -p "$corpus/reference"
  rm -f "$corpus/reference/"*.json
  local case file
  for case in "$corpus/cases/"*.json; do
    file="$(basename "$case")"
    jq --arg file "$file" '
      [.entry[].resource
       | select([.extension[]? | select(.url | endswith("operationoutcome-file")) | .valueString
                 | split("/") | last] | index($file))
       | .issue[]
       | select(.severity == "error" or .severity == "fatal")
       | ([.extension[]? | select(.url | endswith("operationoutcome-message-id")) | .valueCode][0] // "") as $id
       | {severity,
          location: ((.expression // [""])[0] | gsub("/\\*[^*]*\\*/"; "")),
          rule: (if ($id | test("^https?://.*#")) then "constraint:" + ($id | sub("^.*#"; "")) else $id end),
          text: (.details.text // "")}]
      | unique
      | {validator: "HL7 FHIR Validator", issues: .}
    ' "$work/outcomes.json" >"$corpus/reference/$file"
  done
}

case "${1:-}" in
  --fetch) prepare ;;
  "") prepare; run ;;
  *) echo "usage: $0 [--fetch]" >&2; exit 2 ;;
esac
