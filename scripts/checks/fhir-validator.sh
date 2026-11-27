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
# hl7.fhir.eu.laboratory 2.1.0, hl7.fhir.eu.mpd 1.0.0, hl7.fhir.eu.hdr 1.0.0,
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
# reports them at), so a claim is never left unchecked. Warnings and
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
# against that Core profile, in a run that also loads the HL7 Europe guide of
# the document's category so an extension that guide defines resolves (#2372),
# and every coded element a Core profile binds
# to a Master Value Sets Catalogue value set against it. A case passes when the
# validator reports no error, no fatal issue and no unchecked profile for it;
# the mode fails only when it cannot measure.
#
# A case asks whether a document, in the form eehrxf wrote it, is also a
# MyHealth@EU document (#2325). A document in the MyHealth@EU form is measured
# as written. A document in the HL7 Europe form claims the MyHealth@EU Bundle
# profile of its guide instead, and each entry of a type exactly one entry
# slice of that Bundle matches by type and profile claims the slice's profile,
# the way the Core corpus holds each entry to the Core profile of its type.
# FHIR R4 profiling.html 5.1.0.11 matches a `profile` discriminator by the
# entry's conformance to the profile, never by its meta.profile, so the claim
# changes no verdict: without it the validator reports only that the slice
# found no entry, and with it the validator names what the entry lacks. No
# specification governs the corpora: our own design.
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
# summary.md, beside a copy of every document. A DOCUMENTS with no document
# at its top level holds one lane per subdirectory, the documents one CI job
# wrote, as the fhir-validator job downloads each artifact into a directory
# of its own (#2315): two lanes may write one name with different bytes. Each
# lane is then validated on its own into OUT/LANE, and OUT/summary.tsv and
# OUT/summary.md name the lane of every document. --judge reads an existing
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

# The codings outside an extensible MyHealth@EU binding that a human review
# found no concept of the value set for (FHIR R4 terminologies §4.1.5.3).
readonly MVC_REVIEWED="conformance/myhealth-eu-mvc/extensible-reviewed.tsv"

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

# The laboratory report document a run validates over the Laboratory Report
# guide.
readonly LAB_BUNDLE="http://hl7.eu/fhir/laboratory/StructureDefinition/Bundle-eu-lab"
readonly LAB_IG="hl7.fhir.eu.laboratory#2.1.0"

# The imaging report document a run validates over the Imaging Report guide.
readonly IMAGING_BUNDLE="http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging"
readonly IMAGING_IG="hl7.fhir.eu.imaging#1.0.0-ballot"

# The imaging study manifest a run validates over the Imaging Manifest guide.
readonly MANIFEST_BUNDLE="http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoBundle"
readonly MANIFEST_IG="hl7.fhir.eu.imaging-manifest#1.0.0-ballot"

# The discharge report document a run validates over the Hospital Discharge
# Report guide.
readonly HDR_BUNDLE="http://hl7.eu/fhir/hdr/StructureDefinition/bundle-eu-hdr"

# The discharge report in the MyHealth@EU form, which a run validates over
# the MyHealth@EU Hospital Discharge Report CI build docs/VERSIONS.md pins.
readonly MYHEALTHEU_HDR_BUNDLE="http://fhir.ehdsi.eu/hdr/StructureDefinition/bundle-hdr-myhealtheu"
readonly HDR_IG="hl7.fhir.eu.hdr#1.0.0"

# The laboratory report in the MyHealth@EU form, the profile an HL7 Europe
# laboratory report claims in the MyHealth@EU Laboratory corpus.
readonly MYHEALTHEU_LAB_BUNDLE="http://fhir.ehdsi.eu/laboratory/StructureDefinition/Bundle-lab-myhealtheu"

# The MyHealth@EU vaccination history a run validates over the MyHealth@EU
# Vaccination guide, a preview (#2168).
readonly EUVAC_BUNDLE="http://fhir.ehdsi.eu/euvac/StructureDefinition/bundle-euvac-myhealtheu"
readonly EUVAC_IG="myhealth.eu.fhir.euvac#10.0.0-preview"

# The patient summary documents, and the guide and profile the xShare Yellow
# Button holds a Patient Summary to (xShare Yellow Button 0.3.0, Information
# view; CapabilityStatement capabilityStatement-yb-provider).
readonly IPS_BUNDLE="http://hl7.org/fhir/uv/ips/StructureDefinition/Bundle-uv-ips"
readonly EPS_BUNDLE="http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps"
readonly EPS_IG="hl7.fhir.eu.eps#1.0.0-ballot"
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

# lanes DOCUMENTS: the lane directories of DOCUMENTS, one per line, sorted:
# DOCUMENTS itself when it holds a document at its top level, otherwise each
# subdirectory that holds one.
lanes() {
  local documents="$1" dir docs
  docs=("$documents"/*.json)
  if [[ -f "${docs[0]}" ]]; then
    printf '%s\n' "$documents"
    return
  fi
  for dir in "$documents"/*/; do
    docs=("$dir"*.json)
    [[ -f "${docs[0]}" ]] && printf '%s\n' "${dir%/}"
  done | LC_ALL=C sort
}

# run DOCUMENTS OUT: validates DOCUMENTS as one lane, or each of its lanes on
# its own into OUT/LANE, and writes OUT/summary.tsv and OUT/summary.md with
# the lane of every document (no specification governs this: our own design).
run() {
  local documents="$1" out="$2" status=0 dir lane
  local found=()
  while IFS= read -r dir; do
    [[ -n "$dir" ]] && found+=("$dir")
  done < <(lanes "$documents")
  if [[ "${#found[@]}" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with EEHRXF_EMIT_DIR set?" >&2
    return 1
  fi
  if [[ "${found[0]}" == "$documents" ]]; then
    run_lane "$documents" "$out"
    return
  fi
  mkdir -p "$out"
  for dir in "${found[@]}"; do
    lane="${dir##*/}"
    echo "lane $lane"
    run_lane "$dir" "$out/$lane" || status=1
  done
  {
    printf 'lane\tfile\tseverity\tmessage-id\twhere\ttext\n'
    for dir in "${found[@]}"; do
      lane="${dir##*/}"
      [[ -f "$out/$lane/summary.tsv" ]] || continue
      awk -F '\t' -v lane="$lane" 'NR > 1 { print lane "\t" $0 }' "$out/$lane/summary.tsv"
    done
  } > "$out/summary.tsv"
  {
    echo "## HL7 FHIR Validator over the exchange-format documents"
    echo
    echo "${#found[@]} lane(s), each validated on its own: a lane is the documents one CI job wrote."
    for dir in "${found[@]}"; do
      lane="${dir##*/}"
      echo
      if [[ -f "$out/$lane/summary.md" ]]; then
        sed -e "1s/^## .*/## Lane \`$lane\`/" -e '2,$s/^### /#### /' "$out/$lane/summary.md"
      else
        echo "## Lane \`$lane\`"
        echo
        echo "The validator wrote no result for this lane; see its log."
      fi
    done
  } > "$out/summary.md"
  return "$status"
}

# run_lane DOCUMENTS OUT: prepares the jar and the package cache, runs the
# validator offline over every document in DOCUMENTS and judges its output.
# An imaging report, an imaging study manifest, a discharge report and a
# vaccination history are each validated in a run of their own, over their guide's closure: one validator context holding
# several closures read the laboratory report against the imaging guide's
# dependencies too (no specification governs this: our own design).
run_lane() {
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
  local docs=() general=() imaging=() manifest=() discharge=() myhealtheu=() euvac=() summaries=() doc copies
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
    elif jq -e --arg bundle "$MYHEALTHEU_HDR_BUNDLE" '(.meta.profile // []) | index($bundle) != null' "$doc" > /dev/null 2>&1; then
      myhealtheu+=("$doc")
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
      'hl7.fhir.uv.ips#2.0.1' "$EPS_IG" 'hl7.fhir.eu.base#2.0.1' \
      "$LAB_IG" 'hl7.fhir.eu.mpd#1.0.0' "$REMAINING" "$name" \
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
  if [[ "${#myhealtheu[@]}" -gt 0 ]]; then
    local hdr_ci
    hdr_ci="$(ci_build_version myhealth.eu.fhir.hdr)"
    [[ -n "$hdr_ci" ]] || {
      echo "::error::docs/VERSIONS.md pins no MyHealth@EU CI build of myhealth.eu.fhir.hdr" >&2
      return 1
    }
    validate "$out" myhealtheu-discharge "${myhealtheu[@]}" -- "myhealth.eu.fhir.hdr#$hdr_ci" "$name" \
      > "$out/myhealtheu-discharge.bundle.json" || return 1
    parts+=("$out/myhealtheu-discharge.bundle.json")
  fi
  if [[ "${#euvac[@]}" -gt 0 ]]; then
    validate "$out" euvac "${euvac[@]}" -- "$EUVAC_IG" > "$out/euvac.bundle.json" || return 1
    parts+=("$out/euvac.bundle.json")
  fi
  if [[ "${#summaries[@]}" -gt 0 ]]; then
    validate "$out" xshare-yb "${summaries[@]}" -- "$XSHARE_IG" "$EPS_IG" "$name" \
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
  : > "$out/core.guides.tsv"
  local profiles
  profiles="$(jq -s -c '[.[] | select(.resourceType == "StructureDefinition" and .kind == "resource" and .derivation == "constraint")]
    | group_by(.type) | map(select(length == 1) | {key: .[0].type, value: .[0].url}) | from_entries' \
    "docs/specs/fhir-packages/$core/package/"StructureDefinition-*.json)"
  local lab_slices hdr_slices
  lab_slices="$(profile_slices "docs/specs/fhir-packages/$lab/package" "$MYHEALTHEU_LAB_BUNDLE")"
  hdr_slices="$(profile_slices "docs/specs/fhir-packages/$hdr/package" "$MYHEALTHEU_HDR_BUNDLE")"
  local dir seen=""
  while IFS= read -r dir; do
    for doc in "$dir"/eehrxf-*.json; do
      [[ -f "$doc" ]] || continue
      stem="$(basename "$doc" .json)"
      if grep -qxF "$stem" <<< "$seen"; then
        echo "::error::two lanes of $documents write $stem.json; a corpus case is named by its document" >&2
        return 1
      fi
      seen+="$stem"$'\n'
      measure_document "$doc" "$stem" "$out" "$profiles" "$lab_slices" "$hdr_slices"
    done
  done < <(lanes "$documents")
  measure_core myhealth-eu-core "$out" "$out/core" "$core" "$REMAINING" || return 1
  measure_guide myhealth-eu-laboratory "$out" "$out/laboratory" "$lab" || return 1
  measure_guide myhealth-eu-hdr "$out" "$out/hdr" "$hdr" || return 1
  measure_guide myhealth-eu-euvac "$out" "$out/euvac" "$EUVAC_IG" || return 1
  mvc_codes "$out/core" "docs/specs/fhir-packages/$core/package" "docs/specs/fhir-packages/$mvc/package" "$out"
}

# profile_slices PACKAGE BUNDLE: the entry profile by resource type of the
# Bundle profile BUNDLE in PACKAGE, for each type exactly one entry slice
# matches when that slice is required and the slicing discriminates by
# profile; {} when it does not, since a slice matched by type alone is
# validated against its profile anyway.
profile_slices() {
  local file
  file="$(grep -l "\"url\" *: *\"$2\"" "$1"/StructureDefinition-*.json | head -n 1)"
  [[ -n "$file" ]] || {
    echo "::error::$1 holds no StructureDefinition $2" >&2
    return 1
  }
  jq -c '[.snapshot.element[]] as $elements
    | if any($elements[] | select(.id == "Bundle.entry") | .slicing.discriminator[]?;
        .type == "profile" and .path == "resource")
      then ([$elements[] | select(.id | test("^Bundle\\.entry:[^.]+$")) | {key: .id, value: .min}] | from_entries) as $min
        | [$elements[] | select(.id | test("^Bundle\\.entry:[^.]+\\.resource$"))
          | {slice: (.id | rtrimstr(".resource")), types: .type}]
        | group_by(.types[0].code)
        | map(select(length == 1 and (.[0].types | length) == 1 and (.[0].types[0].profile // [] | length) == 1
            and ($min[.[0].slice] // 0) >= 1)
          | {key: .[0].types[0].code, value: .[0].types[0].profile[0]})
        | from_entries
      else {} end' "$file"
}

# retag DOC BUNDLE SLICES: the document DOC claiming the Bundle profile
# BUNDLE, and the one entry of each type SLICES names claiming that profile.
# Only the sole entry of its type must meet a required slice, so the claim
# changes no verdict; a type the document holds twice keeps its claims.
retag() {
  jq --arg p "$2" --argjson slices "$3" '.meta.profile = [$p]
    | ([.entry[]?.resource.resourceType] | group_by(.) | map({key: .[0], value: length}) | from_entries) as $count
    | .entry |= map(.resource.resourceType as $t
        | if $t != null and $slices[$t] != null and $count[$t] == 1
          then .resource.meta.profile = [$slices[$t]] else . end)' "$1"
}

# measure_document DOC STEM OUT PROFILES LAB HDR: writes the cases the
# document DOC gives each MyHealth@EU corpus under OUT, its Core entries held
# to the profile PROFILES names for their type, an HL7 Europe laboratory or
# discharge report retagged by the slices LAB or HDR.
measure_document() {
  local doc="$1" stem="$2" out="$3" profiles="$4" lab_slices="$5" hdr_slices="$6" line
  if jq -e --arg b "$LAB_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
    retag "$doc" "$MYHEALTHEU_LAB_BUNDLE" "$lab_slices" > "$out/laboratory/$stem.json"
  elif jq -e --arg b "$HDR_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
    retag "$doc" "$MYHEALTHEU_HDR_BUNDLE" "$hdr_slices" > "$out/hdr/$stem.json"
  elif jq -e --arg b "$MYHEALTHEU_HDR_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
    cp "$doc" "$out/hdr/$stem.json"
  elif jq -e --arg b "$EUVAC_BUNDLE" '(.meta.profile // []) | index($b) != null' "$doc" > /dev/null 2>&1; then
    cp "$doc" "$out/euvac/$stem.json"
  fi
  local guide file
  guide="$(category_guide "$doc")"
  jq -c --argjson profiles "$profiles" '
    select(.resourceType == "Bundle") | .entry // [] | to_entries[]
    | select(.value.resource.resourceType as $t | $profiles[$t] != null)
    | [.key, .value.resource.resourceType, (.value.resource | .meta.profile = [$profiles[.resourceType]])]' "$doc" |
    while IFS= read -r line; do
      file="$stem.$(jq -r '"\(.[0]).\(.[1])"' <<< "$line").json"
      jq '.[2]' <<< "$line" > "$out/core/$file"
      printf '%s\t%s\n' "$file" "$guide" >> "$out/core.guides.tsv"
    done
}

# category_guide DOC: the HL7 Europe guide of the category whose Bundle
# profile the document DOC claims, which defines the extensions its entries
# carry, or nothing for a document of no such category.
category_guide() {
  local doc="$1" bundle guide
  for bundle in "$IMAGING_BUNDLE" "$MANIFEST_BUNDLE" "$HDR_BUNDLE" "$LAB_BUNDLE" "$EPS_BUNDLE"; do
    if jq -e --arg b "$bundle" 'any((.meta.profile // [])[]; split("|")[0] == $b)' "$doc" > /dev/null 2>&1; then
      case "$bundle" in
        "$IMAGING_BUNDLE") guide="$IMAGING_IG" ;;
        "$MANIFEST_BUNDLE") guide="$MANIFEST_IG" ;;
        "$HDR_BUNDLE") guide="$HDR_IG" ;;
        "$LAB_BUNDLE") guide="$LAB_IG" ;;
        *) guide="$EPS_IG" ;;
      esac
      printf '%s\n' "$guide"
      return 0
    fi
  done
}

# measure_core NAME OUT DIR IG...: measures the Core entries under DIR as
# measure_guide does, each in a run that also loads the category guide
# OUT/core.guides.tsv names for its file, so an extension that guide defines
# resolves; the entries of one guide share a run.
measure_core() {
  local corpus="$1" out="$2" dir="$3"
  shift 3
  local guides="$out/core.guides.tsv" guide group n=0 file parts=()
  [[ -s "$guides" ]] || {
    echo "::error::$corpus: no document to measure" >&2
    return 1
  }
  while IFS= read -r guide; do
    local files=()
    while IFS=$'\t' read -r file group; do
      [[ "$group" = "$guide" ]] && files+=("$dir/$file")
    done < "$guides"
    n=$((n + 1))
    if [[ -n "$guide" ]]; then
      validate "$out" "$corpus.$n" "${files[@]}" -- "$@" "$guide" > "$out/$corpus.$n.bundle.json" || return 1
    else
      validate "$out" "$corpus.$n" "${files[@]}" -- "$@" > "$out/$corpus.$n.bundle.json" || return 1
    fi
    parts+=("$out/$corpus.$n.bundle.json")
  done < <(cut -f2 "$guides" | LC_ALL=C sort -u)
  jq -s '{resourceType: "Bundle", type: "collection", entry: [.[].entry[]]}' "${parts[@]}" \
    > "$out/$corpus.bundle.json"
  {
    printf 'file\tseverity\tmessage-id\twhere\ttext\n'
    issues "$out/$corpus.bundle.json"
  } > "$out/$corpus.tsv"
  corpus_result "$corpus" "$dir" "$out/$corpus.tsv"
}

# mvc_codes CASES CORE MVC OUT [REVIEWED]: one case per coded element of
# each Core entry under CASES at which its Core profile (from package CORE)
# binds a value set required or extensible, when that value set and every
# value set it includes come from the catalogue MVC or the Core guide. The
# case passes when a coding of the element is in the value set's members.
# Text alone fails at a required binding and is no case at an extensible one,
# which admits it (FHIR R4 terminologies §4.1.1.3). An element no catalogue
# value set binds is no case. A coding outside an extensible value set passes
# only when the table REVIEWED (MVC_REVIEWED by default) names it for that
# value set with the reason no concept of the set applies, the human review
# FHIR R4 terminologies §4.1.5.3 asks for; the result counts those cases as
# `reviewed`, which the badge names. Every case carries its binding strength.
mvc_codes() {
  local cases="$1" core="$2" mvc="$3" out="$4" reviewed_table="${5:-$MVC_REVIEWED}"
  local results="${FERROBRIDGE_CONFORMANCE_OUT:-target/conformance}" reviewed
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
  reviewed="$(awk -F'\t' '/^(#|$)/ || !seen++ { next }
    NF != 3 || $3 == "" { printf "::error::%s line %d is not a value set, a coding and a reason\n", FILENAME, NR > "/dev/stderr"; bad = 1; exit }
    { print $1 "\t" $2 }
    END { exit bad }' "$reviewed_table" | jq -R -s -c 'split("\n") | map(select(length > 0) | {key: ., value: true}) | from_entries')" || return 1
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
         codings: $codings, valueSet: $binding.valueSet, strength: $binding.strength}' "$file"
  done | jq -s -c --argjson reviewed "$reviewed" 'unique_by(.id)
    | map(.valueSet as $vs | . + {reviewed: ((.passed | not) and .strength == "extensible"
        and any(.codings[]; $reviewed["\($vs)\t\(.)"] // false))})
    | {corpus: "myhealth-eu-mvc", total: length, reviewed: ([.[] | select(.reviewed)] | length),
       cases: map(if .passed then {id, passed, strength}
         elif .reviewed then {id, passed: true, strength, reviewed: true}
         elif .strength == "extensible" then {id, passed, strength,
           failure: "no coding is in \(.valueSet), and no review in the extensible table names one"}
         else {id, passed, strength, failure: "no coding is in \(.valueSet)"} end)}' \
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
  check "a dangling profile reference fails" 1 \
    "$(bundle "$(outcome a.json error Reference_REF_CantResolveProfile "Unable to resolve the profile reference 'http://example.org/other'")" "$(outcome b.json information All_OK)")"
  rm -f "$scratch/docs/"*.json
  check "no document fails" 1 '{"resourceType":"OperationOutcome","issue":[]}'
  lane() {
    local label="$1" want="$2" got
    got="$(lanes "$scratch/lanes" | sed "s|^$scratch/lanes|.|" | paste -sd ' ' -)"
    cases=$((cases + 1))
    if [[ "$got" != "$want" ]]; then
      echo "::error::self-test: $label: lanes '$got', expected '$want'" >&2
      wrong=1
    fi
  }
  mkdir -p "$scratch/lanes/test-1" "$scratch/lanes/e2e" "$scratch/lanes/empty"
  echo '{}' > "$scratch/lanes/test-1/a.json"
  echo '{}' > "$scratch/lanes/e2e/a.json"
  lane "each subdirectory holding a document is a lane, sorted" "./e2e ./test-1"
  echo '{}' > "$scratch/lanes/b.json"
  lane "a document at the top level makes the directory one lane" "."
  mkdir -p "$scratch/pkg"
  jq -n '{resourceType: "StructureDefinition", url: "urn:b", snapshot: {element: [
      {id: "Bundle.entry", slicing: {discriminator: [{type: "type", path: "resource"}, {type: "profile", path: "resource"}]}},
      {id: "Bundle.entry:c", min: 1}, {id: "Bundle.entry:c.resource", type: [{code: "Composition", profile: ["urn:c"]}]},
      {id: "Bundle.entry:d", min: 0}, {id: "Bundle.entry:d.resource", type: [{code: "Device", profile: ["urn:d"]}]},
      {id: "Bundle.entry:p", min: 1}, {id: "Bundle.entry:p.resource", type: [{code: "Patient", profile: ["urn:p"]}]}]}}' \
    > "$scratch/pkg/StructureDefinition-b.json"
  jq -n '{resourceType: "Bundle", meta: {profile: ["urn:eu"]}, entry: [
      {resource: {resourceType: "Composition"}}, {resource: {resourceType: "Device"}},
      {resource: {resourceType: "Patient", meta: {profile: ["urn:x"]}}}, {resource: {resourceType: "Patient"}}]}' \
    > "$scratch/doc.json"
  local claims
  claims="$(retag "$scratch/doc.json" urn:mhe "$(profile_slices "$scratch/pkg" urn:b)" |
    jq -c '[.meta.profile[0], (.entry[] | .resource.meta.profile[0])]')"
  cases=$((cases + 1))
  if [[ "$claims" != '["urn:mhe","urn:c",null,"urn:x",null]' ]]; then
    echo "::error::self-test: only the sole entry of a required profile slice is retagged: got $claims" >&2
    wrong=1
  fi
  mkdir -p "$scratch/mvc/core" "$scratch/mvc/catalogue" "$scratch/mvc/cases" "$scratch/mvc/out"
  jq -n '{resourceType: "StructureDefinition", url: "urn:p", snapshot: {element: [
      {id: "Composition.type", path: "Composition.type", binding: {strength: "required", valueSet: "urn:types"}},
      {id: "Composition.section.code", path: "Composition.section.code", binding: {strength: "extensible", valueSet: "urn:sections"}}]}}' \
    > "$scratch/mvc/core/StructureDefinition-p.json"
  jq -n '{resourceType: "ValueSet", url: "urn:types", compose: {include: [{system: "urn:s", concept: [{code: "t"}]}]}}' \
    > "$scratch/mvc/catalogue/ValueSet-types.json"
  jq -n '{resourceType: "ValueSet", url: "urn:sections", compose: {include: [{system: "urn:s", concept: [{code: "a"}]}]}}' \
    > "$scratch/mvc/catalogue/ValueSet-sections.json"
  jq -n '{resourceType: "ValueSet", url: "urn:core", compose: {include: [{system: "urn:s", concept: [{code: "z"}]}]}}' \
    > "$scratch/mvc/core/ValueSet-core.json"
  jq -n '{resourceType: "Composition", meta: {profile: ["urn:p"]}, type: {coding: [{system: "urn:s", code: "x"}]},
      section: [{code: {coding: [{system: "urn:s", code: "a"}]}}, {code: {coding: [{system: "urn:s", code: "b"}]}},
        {code: {coding: [{system: "urn:s", code: "c"}]}}]}' > "$scratch/mvc/cases/d.0.Composition.json"
  printf 'value set\tcoding\treason\nurn:sections\turn:s|b\tno concept applies\nurn:types\turn:s|x\tnot admitted at a required binding\n' \
    > "$scratch/mvc/reviewed.tsv"
  FERROBRIDGE_CONFORMANCE_OUT="$scratch/mvc/out" mvc_codes "$scratch/mvc/cases" "$scratch/mvc/core" \
    "$scratch/mvc/catalogue" "$scratch/mvc/out" "$scratch/mvc/reviewed.tsv" > /dev/null
  local verdicts
  verdicts="$(jq -c '[.reviewed, (.cases[] | [(.id | split("|")[1]), .passed, .strength])]' "$scratch/mvc/out/myhealth-eu-mvc.json")"
  cases=$((cases + 1))
  if [[ "$verdicts" != '[1,["a",true,"extensible"],["b",true,"extensible"],["c",false,"extensible"],["x",false,"required"]]' ]]; then
    echo "::error::self-test: a reviewed extensible miss passes, an unreviewed one and a required miss fail: got $verdicts" >&2
    wrong=1
  fi
  mkdir -p "$scratch/core/core"
  : > "$scratch/core/core.guides.tsv"
  jq -n --arg b "$IMAGING_BUNDLE" '{resourceType: "Bundle", meta: {profile: [$b]}, entry: [
      {resource: {resourceType: "Composition", section: [{extension: [{url: "urn:category-extension"}]}]}},
      {resource: {resourceType: "Endpoint"}}]}' > "$scratch/imaging.json"
  jq -n '{resourceType: "Bundle", meta: {profile: ["urn:no-category"]}, entry: [{resource: {resourceType: "Patient"}}]}' \
    > "$scratch/other.json"
  measure_document "$scratch/imaging.json" imaging "$scratch/core" '{"Composition":"urn:core-c","Patient":"urn:core-p"}' '{}' '{}'
  measure_document "$scratch/other.json" other "$scratch/core" '{"Composition":"urn:core-c","Patient":"urn:core-p"}' '{}' '{}'
  local guided
  guided="$(tr '\t' '=' < "$scratch/core/core.guides.tsv" | tr '\n' ' ')"
  cases=$((cases + 1))
  if [[ "$guided" != "imaging.0.Composition.json=$IMAGING_IG other.0.Patient.json= " ]]; then
    echo "::error::self-test: a Core entry is measured with its document's category guide: got $guided" >&2
    wrong=1
  fi
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
