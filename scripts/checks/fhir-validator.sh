#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The exchange-format validator lane (#1318; no specification governs the
# lane: our own design). The eehrxf suites that write a patient summary, a
# laboratory report, an imaging report or a discharge report document and hold
# it to the vendored HL7 Europe profiles (`claimed`, `own`, `lab_mapped`,
# `imaging_mapped`, `discharge_mapped`), and the one that maps a prescription and
# a dispensation to the MPD resources (`medication::mapped`), also write it
# to the directory EEHRXF_EMIT_DIR names, and so do the server's
# Patient/$summary and SMART Health Link suites with each document they
# serve, and the one that writes a MyHealth@EU vaccination history (`euvac`);
# this script runs HL7's FHIR Validator over every one of them and fails on an
# error. Its results
# are the evidence the component's page cites for the exchange format
# (Regulation (EU) 2025/327 Art 15, Annex II 2.2); the validator is never
# shipped.
#
# The validator runs offline: no terminology server (-tx n/a) and no HTTP
# access at all (-no-http-access). Its jar and its package cache are the ones
# scripts/vendor/hl7-validator-agreement.sh --fetch prepares from the
# "HL7 FHIR Validator" table of docs/VERSIONS.md, each checked against its
# sha256, over the profile packages scripts/vendor/profile-packages.sh
# fetched (hl7.fhir.uv.ips 2.0.1, hl7.fhir.eu.eps 1.0.0-ballot, hl7.fhir.eu.base 2.0.1,
# hl7.fhir.eu.laboratory 2.0.0, hl7.fhir.eu.mpd 1.0.0, hl7.fhir.eu.hdr 1.0.0,
# hl7.fhir.eu.imaging 1.0.0-ballot, hl7.fhir.eu.imaging-manifest 1.0.0-ballot
# hl7.eu.fhir.xshare-yb 0.3.0 and myhealth.eu.fhir.euvac 10.0.0-preview with
# their dependency closure). This
# script adds the vendored MyHealth@EU Master Value Sets Catalogue, whose
# value sets the patient summary profiles bind, and the remaining-dispensations
# extension eehrxf publishes. Each guide's documents run on their own, so no
# closure reads another guide's documents: a discharge report with
# hl7.fhir.eu.hdr 1.0.0 and the catalogue loaded, an imaging report with
# hl7.fhir.eu.imaging 1.0.0-ballot loaded, an imaging study manifest with
# hl7.fhir.eu.imaging-manifest 1.0.0-ballot loaded, a vaccination history with
# the MyHealth@EU Vaccination guide 10.0.0-preview loaded, and every other document with the
# other four guides, IPS 2.0.1, the catalogue and the extension loaded. Every
# patient summary also runs once more with the xShare Yellow Button guide
# 0.3.0 and the catalogue loaded, held to the profile the guide's provider
# CapabilityStatement names for the Patient Summary content, Bundle-uv-ips of
# IPS 2.0.1 (#2029). The runs' outcomes are judged together; a document's
# second outcome is named `xshare-yb.<document>`.
#
# A run fails on an issue of severity error or fatal, on a document the
# output names no outcome for, and on a claimed profile the validator could
# not find (VALIDATION_VAL_PROFILE_UNKNOWN and its kin, whatever severity it
# reports them at), so a claim is never left unchecked. The one exception is
# the known upstream defect KNOWN_DEFECT_ID names (#1454), admitted for that
# one profile only and required to appear on every laboratory report. Warnings and
# information never fail a run: among them are the terminology messages of an
# offline run, a code or value set no loaded package defines, which the
# validator cannot check without a terminology server. Every issue is listed
# in summary.tsv and counted in summary.md, so none is hidden. Example URLs
# are admitted (-allow-example-urls true) because the test documents name
# their entries under example hosts, which RFC 6761 section 6.5 reserves.
#
# The --myhealth-eu mode measures the same documents against the MyHealth@EU
# guides on their CI line (#2083), one conformance corpus per guide, whose
# results scripts/checks/conformance.sh holds to the pass lists under
# conformance/myhealth-eu-*/. It reads the component's own documents, those
# the eehrxf suites write (`eehrxf-*.json`): each laboratory report against the guide's
# laboratory Bundle profile, each discharge report against its discharge
# Bundle profile, each vaccination history against the Vaccination guide's
# Bundle profile it claims, every document entry of a type the Core guide profiles
# against that Core profile, and every coded element a Core profile binds
# to a Master Value Sets Catalogue value set against it. A case passes when the
# validator reports no error, no fatal issue and no unchecked profile for it;
# the mode fails only when it cannot measure.
#
# Usage:
#   scripts/checks/fhir-validator.sh DOCUMENTS OUT
#   scripts/checks/fhir-validator.sh --myhealth-eu DOCUMENTS OUT
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

# The remaining-dispensations extension a mapped prescription carries.
readonly REMAINING="crates/eehrxf/src/medication/StructureDefinition-remaining-dispensations.json"

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

# The one known upstream defect the lane admits (#1454; no specification
# governs this: our own design over an upstream defect). HL7 Europe
# Laboratory Report 2.0.0 DiagnosticReport-eu-lab names the subject target
# profile below, which no package of its closure defines, so the validator
# reports this message id naming it on every laboratory report. Only that id
# naming exactly that profile is admitted; it must appear for every
# laboratory report document, so the entry fails the lane once upstream
# fixes the package and is then removed.
readonly KNOWN_DEFECT_ID="Reference_REF_CantResolveProfile"
readonly KNOWN_DEFECT_PROFILE="http://hl7.eu/fhir/base/StructureDefinition/patient-animal-eu-core"
readonly LAB_BUNDLE="http://hl7.eu/fhir/laboratory/StructureDefinition/Bundle-eu-lab"

# The imaging report document a run validates over the Imaging Report guide.
readonly IMAGING_BUNDLE="http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging"
readonly IMAGING_IG="hl7.fhir.eu.imaging#1.0.0-ballot"

# The imaging study manifest a run validates over the Imaging Manifest guide.
readonly MANIFEST_BUNDLE="http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoBundle"
readonly MANIFEST_IG="hl7.fhir.eu.imaging-manifest#1.0.0-ballot"

# The discharge report document a run validates over the Hospital Discharge
# Report guide.
readonly HDR_BUNDLE="http://hl7.eu/fhir/hdr/StructureDefinition/bundle-eu-hdr"
readonly HDR_IG="hl7.fhir.eu.hdr#1.0.0"

# The MyHealth@EU vaccination history a run validates over the MyHealth@EU
# Vaccination guide, a preview (#2168).
readonly EUVAC_BUNDLE="http://fhir.ehdsi.eu/euvac/StructureDefinition/bundle-euvac-myhealtheu"
readonly EUVAC_IG="myhealth.eu.fhir.euvac#10.0.0-preview"

# The patient summary documents, and the guide and profile the xShare Yellow
# Button holds a Patient Summary to (xShare Yellow Button 0.3.0, Information
# view; CapabilityStatement capabilityStatement-yb-provider).
readonly IPS_BUNDLE="http://hl7.org/fhir/uv/ips/StructureDefinition/Bundle-uv-ips"
readonly EPS_BUNDLE="http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps"
readonly XSHARE_IG="hl7.eu.fhir.xshare-yb#0.3.0"
readonly XSHARE_PROFILE="$IPS_BUNDLE|2.0.1"

# The extension a validator outcome names its file in.
readonly OUTCOME_FILE="http://hl7.org/fhir/StructureDefinition/operationoutcome-file"

# judge OUTPUT DOCUMENTS OUT: holds the validator output OUTPUT for the
# documents in DOCUMENTS to the rules in the header, writes summary.tsv and
# summary.md to OUT, and returns 1 when a run fails.
judge() {
  local output="$1" documents="$2" out="$3" status=0 doc base n=0 outcomes failing
  local file severity id where text
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
  failing="$(awk -F '\t' -v id="$KNOWN_DEFECT_ID" -v profile="'$KNOWN_DEFECT_PROFILE'" '
    NR > 1 {
      unknown = ($3 ~ /^(VALIDATION_VAL_PROFILE_UNKNOWN|Validation_VAL_Profile_Unknown|Validation_VAL_Unknown_Profile|VALIDATION_VAL_GLOBAL_PROFILE_UNKNOWN)/) \
        || ($5 ~ /has not been checked because it could not be found/)
      known = ($3 == id && index($5, profile) > 0)
      if (($2 == "error" || $2 == "fatal" || unknown) && !known) print
    }' "$out/summary.tsv")"
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] || continue
    jq -e --arg lab "$LAB_BUNDLE" '(.meta.profile // []) | index($lab) != null' "$doc" > /dev/null 2>&1 || continue
    base="${doc##*/}"
    if ! awk -F '\t' -v file="$base" -v id="$KNOWN_DEFECT_ID" -v profile="'$KNOWN_DEFECT_PROFILE'" \
      'NR > 1 && $1 == file && $3 == id && index($5, profile) > 0 { found = 1 } END { exit !found }' \
      "$out/summary.tsv"; then
      echo "::error::$base: the known upstream defect (#1454) no longer appears; remove its entry" >&2
      status=1
    fi
  done
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

# validate OUT GROUP DOCUMENT... -- IG...: runs the validator offline over the
# DOCUMENTs with the IGs loaded (an IG written `profile=<url>` is a profile
# every DOCUMENT is held to), writing OUT/GROUP.json, OUT/GROUP.html and
# OUT/GROUP.log, and answers its output as a Bundle of OperationOutcomes, each
# naming its file. A single document's outcome may name none, so it takes the
# document's name.
validate() {
  local out="$1" group="$2" java="${JAVA:-java}"
  shift 2
  local docs=() igs=()
  while [[ $# -gt 0 && "$1" != "--" ]]; do
    docs+=("$1")
    shift
  done
  shift
  local ig
  while [[ $# -gt 0 ]]; do
    ig="$1"
    if [[ "$ig" = profile=* ]]; then
      igs+=(-profile "${ig#profile=}")
    else
      igs+=(-ig "$ig")
    fi
    shift
  done
  local code=0
  "$java" -Xmx6g -Duser.home="$PWD/$WORK/home" -jar "$WORK/validator_cli.jar" \
    "${docs[@]}" \
    -version 4.0.1 \
    -tx n/a \
    -no-http-access \
    -show-message-ids \
    -allow-example-urls true \
    "${igs[@]}" \
    -output "$out/$group.json" \
    -html-output "$out/$group.html" \
    > "$out/$group.log" 2>&1 || code=$?
  tail -n 40 "$out/$group.log" >&2
  if [[ ! -f "$out/$group.json" ]]; then
    echo "::error::the validator wrote no output for the $group documents (exit $code); see $group.log" >&2
    return 1
  fi
  jq --arg file "${docs[0]}" --arg url "$OUTCOME_FILE" '
    if .resourceType == "Bundle" then .
    else {resourceType: "Bundle", type: "collection", entry: [{resource:
      (if ((.extension // []) | map(select(.url == $url)) | length) > 0 then .
       else .extension = ((.extension // []) + [{url: $url, valueString: $file}]) end)}]}
    end' "$out/$group.json"
}

# run DOCUMENTS OUT: prepares the jar and the package cache, runs the
# validator offline over every document in DOCUMENTS and judges its output.
# An imaging report, an imaging study manifest, a discharge report and a
# vaccination history are each validated in a run of their own, over their guide's closure: one validator context holding
# several closures read the laboratory report against the imaging guide's
# dependencies too (no specification governs this: our own design).
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
  mkdir -p "$out/xshare-yb"
  local docs=() general=() imaging=() manifest=() discharge=() euvac=() summaries=() doc copies
  copies="$(cd "$out/xshare-yb" && pwd)"
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] || continue
    docs+=("$doc")
    if jq -e --arg ips "$IPS_BUNDLE" --arg eps "$EPS_BUNDLE" \
      'any((.meta.profile // [])[]; split("|")[0] == $ips or split("|")[0] == $eps)' "$doc" > /dev/null 2>&1; then
      cp "$doc" "$copies/xshare-yb.${doc##*/}"
      summaries+=("$copies/xshare-yb.${doc##*/}")
    fi
    if jq -e --arg bundle "$IMAGING_BUNDLE" '(.meta.profile // []) | index($bundle) != null' "$doc" > /dev/null 2>&1; then
      imaging+=("$doc")
    elif jq -e --arg bundle "$MANIFEST_BUNDLE" '(.meta.profile // []) | index($bundle) != null' "$doc" > /dev/null 2>&1; then
      manifest+=("$doc")
    elif jq -e --arg bundle "$HDR_BUNDLE" '(.meta.profile // []) | index($bundle) != null' "$doc" > /dev/null 2>&1; then
      discharge+=("$doc")
    elif jq -e --arg bundle "$EUVAC_BUNDLE" '(.meta.profile // []) | index($bundle) != null' "$doc" > /dev/null 2>&1; then
      euvac+=("$doc")
    else
      general+=("$doc")
    fi
  done
  if [[ "${#docs[@]}" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with EEHRXF_EMIT_DIR set?" >&2
    return 1
  fi
  local parts=()
  if [[ "${#general[@]}" -gt 0 ]]; then
    validate "$out" general "${general[@]}" -- \
      'hl7.fhir.uv.ips#2.0.1' 'hl7.fhir.eu.eps#1.0.0-ballot' 'hl7.fhir.eu.base#2.0.1' \
      'hl7.fhir.eu.laboratory#2.0.0' 'hl7.fhir.eu.mpd#1.0.0' "$REMAINING" "$name" \
      > "$out/general.bundle.json" || return 1
    parts+=("$out/general.bundle.json")
  fi
  if [[ "${#imaging[@]}" -gt 0 ]]; then
    validate "$out" imaging "${imaging[@]}" -- "$IMAGING_IG" > "$out/imaging.bundle.json" || return 1
    parts+=("$out/imaging.bundle.json")
  fi
  if [[ "${#manifest[@]}" -gt 0 ]]; then
    validate "$out" manifest "${manifest[@]}" -- "$MANIFEST_IG" > "$out/manifest.bundle.json" || return 1
    parts+=("$out/manifest.bundle.json")
  fi
  if [[ "${#discharge[@]}" -gt 0 ]]; then
    validate "$out" discharge "${discharge[@]}" -- "$HDR_IG" "$name" > "$out/discharge.bundle.json" || return 1
    parts+=("$out/discharge.bundle.json")
  fi
  if [[ "${#euvac[@]}" -gt 0 ]]; then
    validate "$out" euvac "${euvac[@]}" -- "$EUVAC_IG" > "$out/euvac.bundle.json" || return 1
    parts+=("$out/euvac.bundle.json")
  fi
  if [[ "${#summaries[@]}" -gt 0 ]]; then
    validate "$out" xshare-yb "${summaries[@]}" -- "$XSHARE_IG" 'hl7.fhir.eu.eps#1.0.0-ballot' "$name" \
      "profile=$XSHARE_PROFILE" > "$out/xshare-yb.bundle.json" || return 1
    for doc in "${summaries[@]}"; do
      if ! outcome_files "$out/xshare-yb.bundle.json" | grep -qxF "${doc##*/}"; then
        echo "::error::the xShare Yellow Button run names no outcome for ${doc##*/}" >&2
        return 1
      fi
    done
    parts+=("$out/xshare-yb.bundle.json")
  fi
  jq -s '{resourceType: "Bundle", type: "collection", entry: [.[].entry[]]}' "${parts[@]}" \
    > "$out/validation.json"
  cp "${docs[@]}" "$out/"
  judge "$out/validation.json" "$documents" "$out"
}

# The version the profile packages table of docs/VERSIONS.md pins for the
# MyHealth@EU CI build of package $1, or nothing.
ci_build_version() {
  awk -F'|' -v want="$1" '
    NF >= 4 && $3 ~ /tarball `https:\/\/build-fhir\.ehdsi\.eu\// {
      k = $2; gsub(/[ `]/, "", k)
      if (k == want) { split($3, t, /[[:space:]]+/); print t[2]; exit }
    }' docs/VERSIONS.md
}

# failing_cases TSV: one "file<TAB>first failing issue" line per file of the
# summary TSV that holds an error, a fatal issue or an unchecked profile.
failing_cases() {
  awk -F '\t' '
    NR > 1 && !($1 in seen) {
      unknown = ($3 ~ /^(VALIDATION_VAL_PROFILE_UNKNOWN|Validation_VAL_Profile_Unknown|Validation_VAL_Unknown_Profile|VALIDATION_VAL_GLOBAL_PROFILE_UNKNOWN)/) \
        || ($5 ~ /has not been checked because it could not be found/)
      if ($2 == "error" || $2 == "fatal" || unknown) { seen[$1] = 1; print $1 "\t" $2 " [" $3 "] at " $4 ": " $5 }
    }' "$1"
}

# corpus_result NAME DIR TSV: writes the conformance result of the corpus
# NAME, one case per document in DIR judged by the summary TSV, to
# $FERROBRIDGE_CONFORMANCE_OUT/NAME.json.
corpus_result() {
  local corpus="$1" dir="$2" tsv="$3" results="${FERROBRIDGE_CONFORMANCE_OUT:-target/conformance}"
  mkdir -p "$results"
  failing_cases "$tsv" > "$dir.failing.tsv"
  find "$dir" -maxdepth 1 -name '*.json' -exec basename {} .json \; | LC_ALL=C sort |
    jq -R -s --arg corpus "$corpus" --rawfile failing "$dir.failing.tsv" '
      ($failing | split("\n") | map(select(length > 0) | split("\t") | {key: (.[0] | sub("\\.json$"; "")), value: .[1]}) | from_entries) as $f
      | (split("\n") | map(select(length > 0))) as $ids
      | {corpus: $corpus, total: ($ids | length),
         cases: [$ids[] | if $f[.] then {id: ., passed: false, failure: $f[.]} else {id: ., passed: true} end]}' \
      > "$results/$corpus.json"
  echo "myhealth-eu: $corpus $(jq '[.cases[] | select(.passed)] | length' "$results/$corpus.json") / $(jq .total "$results/$corpus.json")"
}

# measure_guide NAME OUT DIR IG...: validates every document in DIR with the
# IGs loaded and writes the corpus result NAME.
measure_guide() {
  local corpus="$1" out="$2" dir="$3"
  shift 3
  local docs=("$dir"/*.json)
  [[ -f "${docs[0]}" ]] || {
    echo "::error::$corpus: no document to measure" >&2
    return 1
  }
  validate "$out" "$corpus" "${docs[@]}" -- "$@" > "$out/$corpus.bundle.json" || return 1
  {
    printf 'file\tseverity\tmessage-id\twhere\ttext\n'
    issues "$out/$corpus.bundle.json"
  } > "$out/$corpus.tsv"
  corpus_result "$corpus" "$dir" "$out/$corpus.tsv"
}

# myhealth_eu DOCUMENTS OUT: measures the documents against the MyHealth@EU
# CI guides as the header describes, writing one conformance result per guide.
myhealth_eu() {
  local documents="$1" out="$2" java="${JAVA:-java}" pkg ver doc stem
  command -v "$java" > /dev/null 2>&1 || {
    echo "::error::java is not on PATH; the validator needs Java 17 or later" >&2
    return 1
  }
  bash scripts/vendor/hl7-validator-agreement.sh --fetch
  local core lab hdr mvc
  for pkg in core laboratory hdr mvc-package; do
    ver="$(ci_build_version "myhealth.eu.fhir.$pkg")"
    [[ -n "$ver" ]] || {
      echo "::error::docs/VERSIONS.md pins no MyHealth@EU CI build of myhealth.eu.fhir.$pkg" >&2
      return 1
    }
    case "$pkg" in
      core) core="myhealth.eu.fhir.core#$ver" ;;
      laboratory) lab="myhealth.eu.fhir.laboratory#$ver" ;;
      hdr) hdr="myhealth.eu.fhir.hdr#$ver" ;;
      *) mvc="myhealth.eu.fhir.mvc-package#$ver" ;;
    esac
  done
  mkdir -p "$out/core" "$out/laboratory" "$out/hdr" "$out/euvac"
  local profiles
  profiles="$(jq -s -c '[.[] | select(.resourceType == "StructureDefinition" and .kind == "resource" and .derivation == "constraint")]
    | group_by(.type) | map(select(length == 1) | {key: .[0].type, value: .[0].url}) | from_entries' \
    "docs/specs/fhir-packages/$core/package/"StructureDefinition-*.json)"
  for doc in "$documents"/eehrxf-*.json; do
    [[ -f "$doc" ]] || continue
    stem="$(basename "$doc" .json)"
    if jq -e --arg b "$LAB_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
      jq --arg p "http://fhir.ehdsi.eu/laboratory/StructureDefinition/Bundle-lab-myhealtheu" '.meta.profile = [$p]' "$doc" > "$out/laboratory/$stem.json"
    elif jq -e --arg b "$HDR_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
      jq --arg p "http://fhir.ehdsi.eu/hdr/StructureDefinition/bundle-hdr-myhealtheu" '.meta.profile = [$p]' "$doc" > "$out/hdr/$stem.json"
    elif jq -e --arg b "$EUVAC_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
      cp "$doc" "$out/euvac/$stem.json"
    fi
    jq -c --argjson profiles "$profiles" '
      select(.resourceType == "Bundle") | .entry // [] | to_entries[]
      | select(.value.resource.resourceType as $t | $profiles[$t] != null)
      | [.key, .value.resource.resourceType, (.value.resource | .meta.profile = [$profiles[.resourceType]])]' "$doc" |
      while IFS= read -r line; do
        jq '.[2]' <<< "$line" > "$out/core/$stem.$(jq -r '"\(.[0]).\(.[1])"' <<< "$line").json"
      done
  done
  measure_guide myhealth-eu-core "$out" "$out/core" "$core" "$REMAINING" || return 1
  measure_guide myhealth-eu-laboratory "$out" "$out/laboratory" "$lab" || return 1
  measure_guide myhealth-eu-hdr "$out" "$out/hdr" "$hdr" || return 1
  measure_guide myhealth-eu-euvac "$out" "$out/euvac" "$EUVAC_IG" || return 1
  mvc_codes "$out/core" "docs/specs/fhir-packages/$core/package" "docs/specs/fhir-packages/$mvc/package" "$out"
}

# mvc_codes CASES CORE MVC OUT: one case per coded element of each Core
# entry under CASES at which its Core profile (from package CORE) binds a
# value set required or extensible, when that value set and every value set
# it includes come from the catalogue MVC or the Core guide. The case passes
# when a coding of the element is in the value set's members. Text alone
# fails at a required binding and is no case at an extensible one, which
# admits it (FHIR R4 terminologies §4.1.1.3). An element no catalogue value
# set binds is no case.
mvc_codes() {
  local cases="$1" core="$2" mvc="$3" out="$4" results="${FERROBRIDGE_CONFORMANCE_OUT:-target/conformance}"
  mkdir -p "$results"
  jq -s -c '
    map(select(.resourceType == "ValueSet"))
    | map({key: .url, value: [.compose.include[]? | {system, codes: [.concept[]?.code],
        sets: [.valueSet[]? | sub("\\|.*$"; "")], filtered: (.filter != null)}]})
    | from_entries' "$mvc"/ValueSet-*.json "$core"/ValueSet-*.json > "$out/mvc-valuesets.json"
  jq -s -c --slurpfile sets "$out/mvc-valuesets.json" '
    $sets[0] as $m
    | def members($url; $seen):
        if ($seen | index($url)) != null then []
        elif $m[$url] == null then null
        else [$m[$url][] as $i
          | if ($i.sets | length) > 0 then ($i.sets[] | members(.; $seen + [$url]))
            elif $i.filtered then null
            elif ($i.codes | length) > 0 then ($i.codes | map("\($i.system)|\(.)"))
            else ["\($i.system)|*"] end]
          | if any(.[]; . == null) then null else add // [] end
        end;
      map(select(.resourceType == "StructureDefinition"))
      | map({key: .url, value: [.snapshot.element[]
          | select(.sliceName == null and (.id | test(":|\\.extension|\\.modifierExtension") | not))
          | select(.binding.valueSet != null and (.binding.strength == "required" or .binding.strength == "extensible"))
          | (.binding.valueSet | sub("\\|.*$"; "")) as $vs
          | members($vs; []) as $members
          | select($members != null)
          | {path: .path, valueSet: $vs, strength: .binding.strength, members: ($members | map({key: ., value: true}) | from_entries)}]})
      | from_entries' "$core"/StructureDefinition-*.json > "$out/mvc-bindings.json"
  local file
  for file in "$cases"/*.json; do
    [[ -f "$file" ]] || continue
    jq -c --arg stem "$(basename "$file" .json)" --slurpfile bindings "$out/mvc-bindings.json" '
      def at($parts):
        if type == "array" then .[] | at($parts)
        elif ($parts | length) == 0 then .
        elif type == "object" then
          ($parts[0]) as $p
          | (if ($p | endswith("[x]")) then ($p | rtrimstr("[x]")) as $pre
              | to_entries[] | select((.key | startswith($pre)) and ((.key | ltrimstr($pre) | .[0:1]) | test("^[A-Z]$"))) | .value
            else .[$p] // empty end)
          | at($parts[1:])
        else empty end;
      . as $resource
      | ($bindings[0][(.meta.profile // [])[0] // ""] // [])[]
      | . as $binding
      | $resource | at($binding.path | split(".") | .[1:])
      | select(type == "object")
      | (if has("coding") then [.coding[] | select(.system != null and .code != null) | "\(.system)|\(.code)"]
         elif (.system | type) == "string" and (.code | type) == "string" then ["\(.system)|\(.code)"]
         else [] end) as $codings
      | select(($codings | length) > 0 or (has("text") and $binding.strength == "required"))
      | {id: "\($stem) \($binding.path) \(if ($codings | length) > 0 then ($codings | join(",")) else "(text only)" end)",
         passed: any($codings[]; . as $c | $binding.members[$c] or $binding.members[($c | split("|")[0]) + "|*"]),
         valueSet: $binding.valueSet}' "$file"
  done | jq -s -c 'unique_by(.id)
    | {corpus: "myhealth-eu-mvc", total: length,
       cases: map(if .passed then {id, passed} else {id, passed, failure: "no coding is in \(.valueSet)"} end)}' \
    > "$results/myhealth-eu-mvc.json"
  echo "myhealth-eu: myhealth-eu-mvc $(jq '[.cases[] | select(.passed)] | length' "$results/myhealth-eu-mvc.json") / $(jq .total "$results/myhealth-eu-mvc.json")"
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
    local name="$1" severity="$2" id="$3" text="${4:-t}"
    printf '{"resourceType":"OperationOutcome","extension":[{"url":"%s","valueString":"/x/%s"}],"issue":[{"extension":[{"url":"%s","valueString":"%s"}],"severity":"%s","code":"processing","details":{"text":"%s"},"expression":["Bundle"]}]}' \
      "$file" "$name" "$mid" "$id" "$severity" "$text"
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
  local known="Unable to resolve the profile reference '$KNOWN_DEFECT_PROFILE'"
  check "the known upstream defect passes" 0 \
    "$(bundle "$(outcome a.json error "$KNOWN_DEFECT_ID" "$known")" "$(outcome b.json information All_OK)")"
  check "the same message naming another profile fails" 1 \
    "$(bundle "$(outcome a.json error "$KNOWN_DEFECT_ID" "Unable to resolve the profile reference 'http://example.org/other'")" "$(outcome b.json information All_OK)")"
  check "another message naming the known profile fails" 1 \
    "$(bundle "$(outcome a.json error Reference_REF_CantMatchChoice "$known")" "$(outcome b.json information All_OK)")"
  printf '{"resourceType":"Bundle","meta":{"profile":["%s"]}}' "$LAB_BUNDLE" > "$scratch/docs/a.json"
  check "a laboratory report the known defect stopped naming fails" 1 \
    "$(bundle "$(outcome a.json information All_OK)" "$(outcome b.json information All_OK)")"
  check "a laboratory report with the known defect passes" 0 \
    "$(bundle "$(outcome a.json error "$KNOWN_DEFECT_ID" "$known")" "$(outcome b.json information All_OK)")"
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
--myhealth-eu)
  [[ "$#" -eq 3 ]] || {
    echo "usage: $0 --myhealth-eu DOCUMENTS OUT" >&2
    exit 2
  }
  myhealth_eu "$2" "$3"
  ;;
--judge)
  [[ "$#" -eq 4 ]] || {
    echo "usage: $0 --judge OUTPUT DOCUMENTS OUT" >&2
    exit 2
  }
  judge "$2" "$3" "$4"
  ;;
-* | '')
  echo "usage: $0 DOCUMENTS OUT | --myhealth-eu DOCUMENTS OUT | --judge OUTPUT DOCUMENTS OUT | --self-test" >&2
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
