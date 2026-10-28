#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The exchange-format validator lane (#1318; no specification governs the
# lane: our own design). The eehrxf suites that write a patient summary
# document and hold it to the vendored HL7 Europe profiles (`claimed`, `own`)
# also write it to the directory EEHRXF_EMIT_DIR names; this script runs HL7's
# FHIR Validator over every one of them and fails on an error. Its results
# are the evidence the component's page cites for the exchange format
# (Regulation (EU) 2025/327 Art 15, Annex II 2.2); the validator is never
# shipped.
#
# The validator runs offline: no terminology server (-tx n/a) and no HTTP
# access at all (-no-http-access). Its jar and its package cache are the ones
# scripts/vendor/hl7-validator-agreement.sh --fetch prepares from the
# "HL7 FHIR Validator" table of docs/VERSIONS.md, each checked against its
# sha256, over the profile packages scripts/vendor/profile-packages.sh
# fetched (hl7.fhir.eu.eps 1.0.0-ballot and hl7.fhir.eu.base 2.0.1 with their
# dependency closure). This script adds the vendored MyHealth@EU Master Value
# Sets Catalogue, whose value sets the patient summary profiles bind, and
# loads the three with -ig.
#
# A run fails on an issue of severity error or fatal, on a document the
# output names no outcome for, and on a claimed profile the validator could
# not find (VALIDATION_VAL_PROFILE_UNKNOWN and its kin, whatever severity it
# reports them at), so a claim is never left unchecked. Warnings and
# information never fail a run: among them are the terminology messages of an
# offline run, a code or value set no loaded package defines, which the
# validator cannot check without a terminology server. Every issue is listed
# in summary.tsv and counted in summary.md, so none is hidden. Example URLs
# are admitted (-allow-example-urls true) because the test documents name
# their entries under example hosts, which RFC 6761 section 6.5 reserves.
#
# Usage:
#   scripts/checks/fhir-validator.sh DOCUMENTS OUT
#   scripts/checks/fhir-validator.sh --judge OUTPUT DOCUMENTS OUT
#   scripts/checks/fhir-validator.sh --self-test
#
# DOCUMENTS is the directory the tests wrote (EEHRXF_EMIT_DIR), OUT the
# directory the results go to: validation.json (the validator's
# OperationOutcome output), validation.html, validator.log, summary.tsv and
# summary.md, beside a copy of every document. --judge reads an existing
# validator output alone. Needs Java 17 or later (as $JAVA or java on PATH),
# curl, jq, tar and shasum. Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The validator's working tree and package cache, as
# scripts/vendor/hl7-validator-agreement.sh prepares them.
readonly WORK="target/hl7-validator"
readonly CACHE="$WORK/home/.fhir/packages"

# The vendored MyHealth@EU Master Value Sets Catalogue.
readonly MVC="docs/specs/ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz"

fail=0

# issues OUTPUT: one "file<TAB>severity<TAB>message-id<TAB>where<TAB>text"
# line per issue of the validator output OUTPUT, a single OperationOutcome or
# a Bundle of them, each naming its file in the operationoutcome-file
# extension. The file is its base name.
issues() {
  local output="$1"
  jq -r '
    def ext($name): (.extension // []) | map(select((.url // "") | endswith("/" + $name))) | .[0];
    (if .resourceType == "Bundle" then [.entry[]?.resource] else [.] end)[]
    | (ext("operationoutcome-file") | .valueString // "") as $file
    | ($file | split("/") | last) as $base
    | .issue[]?
    | [ $base,
        (.severity // ""),
        (ext("operationoutcome-message-id") | (.valueCode // .valueString // "")),
        (((.expression // .location // []) | .[0]) // ""),
        ((.details.text // .diagnostics // "") | gsub("[\t\r\n]"; " "))
      ]
    | @tsv' "$output"
}

# outcome_files OUTPUT: the base name of every file the validator output
# OUTPUT holds an outcome for, one per line.
outcome_files() {
  local output="$1"
  jq -r '
    (if .resourceType == "Bundle" then [.entry[]?.resource] else [.] end)[]
    | (.extension // []) | map(select((.url // "") | endswith("/operationoutcome-file"))) | .[0].valueString // empty
    | split("/") | last' "$output"
}

# judge OUTPUT DOCUMENTS OUT: holds the validator output OUTPUT for the
# documents in DOCUMENTS to the rules in the header, writes summary.tsv and
# summary.md to OUT, and returns 1 when a run fails.
judge() {
  local output="$1" documents="$2" out="$3" status=0 doc base n=0 outcomes failing
  mkdir -p "$out"
  if ! jq -e '.resourceType == "Bundle" or .resourceType == "OperationOutcome"' "$output" > /dev/null 2>&1; then
    echo "::error::$output is no OperationOutcome and no Bundle of them" >&2
    return 1
  fi
  {
    printf 'file\tseverity\tmessage-id\twhere\ttext\n'
    issues "$output"
  } > "$out/summary.tsv"
  outcomes="$(outcome_files "$output" | sort -u)"
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] || continue
    n=$((n + 1))
    base="${doc##*/}"
    # A single OperationOutcome may carry no file extension: it is the one
    # document's outcome.
    if [[ -z "$outcomes" ]] && jq -e '.resourceType == "OperationOutcome"' "$output" > /dev/null; then
      continue
    fi
    if ! grep -qxF "$base" <<< "$outcomes"; then
      echo "::error::the validator output names no outcome for $base" >&2
      status=1
    fi
  done
  if [[ "$n" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with EEHRXF_EMIT_DIR set?" >&2
    status=1
  fi
  failing="$(awk -F '\t' '
    NR > 1 {
      unknown = ($3 ~ /^(VALIDATION_VAL_PROFILE_UNKNOWN|Validation_VAL_Profile_Unknown|Validation_VAL_Unknown_Profile|VALIDATION_VAL_GLOBAL_PROFILE_UNKNOWN)/) \
        || ($5 ~ /has not been checked because it could not be found/)
      if ($2 == "error" || $2 == "fatal" || unknown) print
    }' "$out/summary.tsv")"
  if [[ -n "$failing" ]]; then
    while IFS=$'\t' read -r file severity id where text; do
      echo "::error::$file: $severity ${id:+[$id] }at ${where:-?}: $text" >&2
    done <<< "$failing"
    status=1
  fi
  {
    echo "## HL7 FHIR Validator over the exchange-format documents"
    echo
    echo "$n document(s); a run fails on an error, a fatal issue or an unchecked profile claim."
    echo
    echo "| Document | fatal | error | warning | information |"
    echo "|---|---|---|---|---|"
    awk -F '\t' '
      NR > 1 { c[$1 SUBSEP $2]++; f[$1] = 1 }
      END {
        for (file in f) {
          printf "| %s | %d | %d | %d | %d |\n", file, c[file SUBSEP "fatal"], c[file SUBSEP "error"], c[file SUBSEP "warning"], c[file SUBSEP "information"]
        }
      }' "$out/summary.tsv" | sort
    echo
    echo "### Warnings by message id"
    echo
    awk -F '\t' 'NR > 1 && $2 == "warning" { c[($3 == "" ? "(none)" : $3)]++ } END { for (id in c) printf "- %s: %d\n", id, c[id] }' "$out/summary.tsv" | sort
    if [[ -n "$failing" ]]; then
      echo
      echo "### Failing issues"
      echo
      while IFS=$'\t' read -r file severity id where text; do
        echo "- \`$file\` $severity ${id:+\`$id\` }at \`${where:-?}\`: $text"
      done <<< "$failing"
    fi
  } > "$out/summary.md"
  cat "$out/summary.md"
  return "$status"
}

# run DOCUMENTS OUT: prepares the jar and the package cache, runs the
# validator offline over every document in DOCUMENTS and judges its output.
run() {
  local documents="$1" out="$2" java="${JAVA:-java}"
  command -v "$java" > /dev/null 2>&1 || {
    echo "::error::java is not on PATH; the validator needs Java 17 or later" >&2
    return 1
  }
  bash scripts/vendor/hl7-validator-agreement.sh --fetch
  local name
  name="$(tar -xzOf "$MVC" package/package.json | jq -r '"\(.name)#\(.version)"')"
  if [[ ! -d "$CACHE/$name/package" ]]; then
    mkdir -p "$CACHE/$name"
    tar -xzf "$MVC" -C "$CACHE/$name"
  fi
  local docs=() doc
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] && docs+=("$doc")
  done
  if [[ "${#docs[@]}" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with EEHRXF_EMIT_DIR set?" >&2
    return 1
  fi
  mkdir -p "$out"
  local code=0
  "$java" -Xmx6g -Duser.home="$PWD/$WORK/home" -jar "$WORK/validator_cli.jar" \
    "${docs[@]}" \
    -version 4.0.1 \
    -tx n/a \
    -no-http-access \
    -show-message-ids \
    -allow-example-urls true \
    -ig 'hl7.fhir.eu.eps#1.0.0-ballot' \
    -ig 'hl7.fhir.eu.base#2.0.1' \
    -ig "$name" \
    -output "$out/validation.json" \
    -html-output "$out/validation.html" \
    > "$out/validator.log" 2>&1 || code=$?
  tail -n 40 "$out/validator.log"
  if [[ ! -f "$out/validation.json" ]]; then
    echo "::error::the validator wrote no output (exit $code); see validator.log" >&2
    return 1
  fi
  cp "${docs[@]}" "$out/"
  judge "$out/validation.json" "$documents" "$out"
}

# self_test: holds judge to its rules over synthetic validator outputs.
self_test() {
  local scratch
  scratch="$(mktemp -d)"
  # shellcheck disable=SC2064 # the path is fixed now, on purpose
  trap "rm -rf '$scratch'" EXIT
  mkdir -p "$scratch/docs"
  echo '{}' > "$scratch/docs/a.json"
  echo '{}' > "$scratch/docs/b.json"
  local file='http://hl7.org/fhir/StructureDefinition/operationoutcome-file'
  local mid='http://hl7.org/fhir/StructureDefinition/operationoutcome-message-id'
  outcome() {
    local name="$1" severity="$2" id="$3"
    printf '{"resourceType":"OperationOutcome","extension":[{"url":"%s","valueString":"/x/%s"}],"issue":[{"extension":[{"url":"%s","valueString":"%s"}],"severity":"%s","code":"processing","details":{"text":"t"},"expression":["Bundle"]}]}' \
      "$file" "$name" "$mid" "$id" "$severity"
  }
  bundle() {
    printf '{"resourceType":"Bundle","type":"collection","entry":[{"resource":%s},{"resource":%s}]}' "$1" "$2"
  }
  local cases=0 wrong=0
  check() {
    local label="$1" want="$2" json="$3" got=0
    printf '%s' "$json" > "$scratch/out.json"
    judge "$scratch/out.json" "$scratch/docs" "$scratch/result" > /dev/null 2>&1 || got=1
    cases=$((cases + 1))
    if [[ "$got" != "$want" ]]; then
      echo "::error::self-test: $label: judged $got, expected $want" >&2
      wrong=1
    fi
  }
  check "information alone passes" 0 \
    "$(bundle "$(outcome a.json information All_OK)" "$(outcome b.json information All_OK)")"
  check "a warning passes" 0 \
    "$(bundle "$(outcome a.json warning UNKNOWN_CODESYSTEM)" "$(outcome b.json information All_OK)")"
  check "an error fails" 1 \
    "$(bundle "$(outcome a.json error Validation_VAL_Profile_Minimum)" "$(outcome b.json information All_OK)")"
  check "a fatal issue fails" 1 \
    "$(bundle "$(outcome a.json fatal Error_parsing_JSON)" "$(outcome b.json information All_OK)")"
  check "an unknown profile fails at any severity" 1 \
    "$(bundle "$(outcome a.json warning VALIDATION_VAL_PROFILE_UNKNOWN_NOT_POLICY)" "$(outcome b.json information All_OK)")"
  check "a document with no outcome fails" 1 \
    "$(bundle "$(outcome a.json information All_OK)" "$(outcome c.json information All_OK)")"
  check "no validator output fails" 1 '{"resourceType":"Parameters"}'
  rm -f "$scratch/docs/"*.json
  check "no document fails" 1 '{"resourceType":"OperationOutcome","issue":[]}'
  if [[ "$wrong" -ne 0 ]]; then
    return 1
  fi
  echo "self-test: $cases cases judged as expected"
}

case "${1:-}" in
--self-test)
  self_test
  ;;
--judge)
  [[ "$#" -eq 4 ]] || {
    echo "usage: $0 --judge OUTPUT DOCUMENTS OUT" >&2
    exit 2
  }
  judge "$2" "$3" "$4"
  ;;
-* | '')
  echo "usage: $0 DOCUMENTS OUT | --judge OUTPUT DOCUMENTS OUT | --self-test" >&2
  exit 2
  ;;
*)
  [[ "$#" -eq 2 ]] || {
    echo "usage: $0 DOCUMENTS OUT" >&2
    exit 2
  }
  run "$1" "$2"
  ;;
esac
[[ "$fail" -eq 0 ]] || exit 1
