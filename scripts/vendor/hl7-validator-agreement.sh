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
#   scripts/vendor/hl7-validator-agreement.sh --check-warm  # fail unless every pinned download is cached
#   scripts/vendor/hl7-validator-agreement.sh --cache-key   # the key the CI cache of the downloads is stored under
#   scripts/vendor/hl7-validator-agreement.sh --self-test   # a warm download makes no request, a cold failure names its host
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

# Prints the host of the URL $1.
host_of() {
  local rest="${1#*://}"
  printf '%s\n' "${rest%%/*}"
}

# Downloads $1 to $2 unless $2 is already there at the sha256 $3, so a warm
# cache makes no request. A download retries transient failures, connection
# refusals included, and a failure that outlasts them names the host.
fetch() {
  if [[ -f "$2" ]] && [[ "$(shasum -a 256 "$2" | cut -d' ' -f1)" == "$3" ]]; then
    return
  fi
  if ! curl -fsSL --retry 5 --retry-all-errors --retry-connrefused --retry-delay 2 \
    -A "$ua" -o "$2" "$1"; then
    rm -f "$2"
    echo "hl7-validator-agreement: cannot fetch $1 from $(host_of "$1") after 5 retries" >&2
    exit 1
  fi
  verify "$2" "$3"
}

# Prints every pinned download as `<url> <file> <sha256>`, one per line.
downloads() {
  local item version digest
  while read -r item version digest; do
    if [[ "$item" == "validator_cli.jar" ]]; then
      printf '%s %s %s\n' \
        "https://github.com/hapifhir/org.hl7.fhir.core/releases/download/$version/validator_cli.jar" \
        "$work/validator_cli.jar" "$digest"
    else
      printf '%s %s %s\n' "$registry/$item/$version" "$work/$item#$version.tgz" "$digest"
    fi
  done < <(rows)
}

# Fails, naming each, when a pinned download is missing from $work or not at
# its sha256, so a warm cache is shown to need no request.
check_warm() {
  local url file digest missing=0
  while read -r url file digest; do
    if [[ ! -f "$file" ]] || [[ "$(shasum -a 256 "$file" | cut -d' ' -f1)" != "$digest" ]]; then
      echo "hl7-validator-agreement: $file is not cached at its pin; a run would fetch $url" >&2
      missing=1
    fi
  done < <(downloads)
  [[ "$missing" -eq 0 ]] || exit 1
  echo "hl7-validator-agreement: every pinned download is cached; a run makes no request"
}

# Fetches the jar and the pinned packages, and fills the private cache.
prepare() {
  mkdir -p "$work" "$cache"
  local url file item version digest
  while read -r url file digest; do
    fetch "$url" "$file" "$digest"
  done < <(downloads)
  while read -r item version digest; do
    [[ "$item" != "validator_cli.jar" ]] || continue
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

# Checks that a cached download makes no request and a cold one names its
# host, with `curl` replaced on PATH by a stub that records each call and
# fails as an unreachable host does (exit 6, name not resolved).
self_test() {
  local scratch stubs file digest status
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' RETURN
  stubs="$scratch/bin"
  mkdir -p "$stubs"
  printf '#!/usr/bin/env bash\necho called >> "%s/calls"\nexit 6\n' "$scratch" >"$stubs/curl"
  chmod +x "$stubs/curl"
  file="$scratch/cached.tgz"
  printf 'pinned bytes\n' >"$file"
  digest="$(shasum -a 256 "$file" | cut -d' ' -f1)"
  ( PATH="$stubs:$PATH" fetch "https://packages.fhir.org/example/1.0.0" "$file" "$digest" )
  if [[ -f "$scratch/calls" ]]; then
    echo "hl7-validator-agreement: self-test: a cached download made a request" >&2
    exit 1
  fi
  status=0
  ( PATH="$stubs:$PATH" fetch "https://packages.fhir.org/example/1.0.0" "$scratch/cold.tgz" "$digest" ) \
    2>"$scratch/err" || status=$?
  if [[ "$status" -eq 0 ]] || ! grep -q 'from packages.fhir.org after 5 retries' "$scratch/err"; then
    echo "hl7-validator-agreement: self-test: a cold fetch that fails did not name its host: $(cat "$scratch/err")" >&2
    exit 1
  fi
  if [[ -f "$scratch/cold.tgz" ]]; then
    echo "hl7-validator-agreement: self-test: a failed fetch left a partial file" >&2
    exit 1
  fi
  echo "hl7-validator-agreement: self-test OK (a warm download makes no request, a cold failure names its host)"
}

case "${1:-}" in
  --fetch) prepare ;;
  --check-warm) check_warm ;;
  --cache-key) rows | shasum -a 256 | cut -c1-16 ;;
  --self-test) self_test ;;
  "") prepare; run ;;
  *) echo "usage: $0 [--fetch | --check-warm | --cache-key | --self-test]" >&2; exit 2 ;;
esac
