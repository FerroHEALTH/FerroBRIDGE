#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The EHDS system documents a FerroBRIDGE release relies on: the pin, the
# pages at the pinned commit of the FerroHEALTH book, the copies the release
# attaches with its own facts appended, and the notes section naming the
# pinned revisions.
#
# The EHR system is FerroEHR with FerroBRIDGE, and its information sheet and
# instructions for use live version-free in the FerroHEALTH book. EHDS Art.
# 30(1)(d) has the system "accompanied, free of charge for the user, by the
# information sheet provided for in Article 38 and clear and complete
# instructions for use". By the owner's rulings on FerroHEALTH/FerroEHR#3705,
# each product's release lane attaches a copy of both pages taken at the
# FerroHEALTH commit it pins, and replaces the page's last section, "This
# copy", with the release's facts: the version of the EHR system, "the
# release of the product this copy accompanies, with the oldest release of
# the other product it is tested against", the release date (Art. 38(2)(b)),
# and the versions of the standards (Art. 38(2)(e)), read from the release's
# pins. For FerroBRIDGE the other product's release is the FerroEHR release
# its end-to-end tests run against (the FerroEHR CDR image row of
# docs/VERSIONS.md). No specification governs the files or their layout: our
# own design.
#
# Usage:
#   system-documents.sh pin <KEY>               print one pin value
#   system-documents.sh fetch <page> <file>     the page at the pinned commit
#   system-documents.sh facts <tag> <page>      the "This copy" section
#   system-documents.sh attach <tag> <out-dir>  write the two attached copies
#   system-documents.sh notes-section           the release notes section
#   system-documents.sh check-notes <file>      refuse notes without it
#   system-documents.sh --self-test             prove the readers offline
# <page> is information-sheet or instructions-for-use. EHDS_PAGES_DIR names a
# directory to read the pages from in place of the network, for a test.
set -euo pipefail
# shellcheck source=scripts/lib/guard-args.sh
. "$(dirname "$0")/../lib/guard-args.sh"
caller_dir="$PWD"
cd "$(dirname "$0")/../.."

PIN="docs/ehds/system-documents.env"
STANDARDS="docs/ehds/standards.tsv"
VERSIONS="docs/VERSIONS.md"
CHANGELOG="CHANGELOG.md"
readonly FAMILY="https://ferrohealth.eu/docs"
readonly GRAMMAR="pin <KEY> | fetch <page> <file> | facts <tag> <page> | attach <tag> <out-dir> | notes-section | check-notes <file> | --self-test"
readonly TAG_RE='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

die() {
  echo "system-documents: $*" >&2
  exit 1
}

# pin KEY: the value of KEY in the pin file.
pin() {
  local key="$1" value
  value="$(sed -n "s/^$key=//p" "$PIN" | sed -n 1p)"
  [[ -n "$value" ]] || die "$PIN has no $key"
  printf '%s\n' "$value"
}

# The pin key of a page's revision date.
revised_key() {
  case "$1" in
    information-sheet) echo INFORMATION_SHEET_REVISED ;;
    instructions-for-use) echo INSTRUCTIONS_FOR_USE_REVISED ;;
    *) die "unknown page '$1' (information-sheet or instructions-for-use)" ;;
  esac
}

# fetch PAGE FILE: the page at the pinned commit, refused when its Revised
# line is not the pinned date or it has no "This copy" heading.
fetch() {
  local page="$1" file="$2" commit want got
  commit="$(pin FERROHEALTH_COMMIT)"
  want="$(pin "$(revised_key "$page")")"
  [[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "FERROHEALTH_COMMIT in $PIN is not a full commit id"
  if [[ -n "${EHDS_PAGES_DIR:-}" ]]; then
    cp "$EHDS_PAGES_DIR/$page.md" "$file"
  else
    curl --proto =https --fail --silent --show-error --location \
      "https://raw.githubusercontent.com/FerroHEALTH/FerroHEALTH/$commit/website/book/src/ehds/$page.md" \
      -o "$file" || die "ehds/$page.md could not be fetched at FerroHEALTH $commit"
  fi
  got="$(sed -n '3s/^Revised \([0-9-]*\)\.$/\1/p' "$file")"
  [[ "$got" == "$want" ]] ||
    die "ehds/$page.md at FerroHEALTH $commit is revised ${got:-<no Revised line>}, and $PIN pins $want"
  grep -qx '## This copy' "$file" || die "ehds/$page.md at FerroHEALTH $commit has no '## This copy' heading"
}

# cell ITEM: the Pin cell of the docs/VERSIONS.md row whose first cell is
# ITEM, backticks kept out.
cell() {
  local item="$1" found
  found="$(awk -F'|' -v item="$item" '
    NF >= 4 {
      k = $2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      if (k == item) { v = $3; gsub(/`/, "", v); gsub(/^[[:space:]]+|[[:space:]]+$/, "", v); print v; exit }
    }' "$VERSIONS")"
  [[ -n "$found" ]] || die "$VERSIONS has no row '$item'"
  printf '%s\n' "$found"
}

# version SOURCE: the version a standards.tsv source names.
version() {
  local source="$1" text value
  case "$source" in
    text:*) value="${source#text:}" ;;
    cell:*) value="$(cell "${source#cell:}")" ;;
    first:*)
      text="$(cell "${source#first:}")"
      value="${text%% *}"
      ;;
    package:*)
      text="$(cell "${source#package:}")"
      value="$(sed -nE 's/^package [^ ]+ ([^ ,]+).*/\1/p' <<<"$text")"
      ;;
    tag:*)
      text="$(cell "${source#tag:}")"
      value="$(sed -nE 's/.* tag ([^ ,]+).*/\1/p' <<<" $text")"
      ;;
    image:*)
      text="$(cell "${source#image:}")"
      value="$(sed -nE 's/^[^ ]+:([^:@ ]+)@sha256:.*/\1/p' <<<"$text")"
      ;;
    *) die "$STANDARDS: unknown source '$source'" ;;
  esac
  [[ -n "$value" ]] || die "$STANDARDS: '$source' names no version in $VERSIONS"
  printf '%s\n' "$value"
}

# The FerroEHR release the end-to-end tests run against.
ehr_version() {
  local v
  v="$(version 'image:FerroEHR CDR image')"
  [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "the FerroEHR CDR image row of $VERSIONS pins no release ($v)"
  printf '%s\n' "$v"
}

# The release date of a version, from its changelog heading.
release_date() {
  local version="$1" date
  date="$(grep -E "^## \[${version//./\\.}\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" "$CHANGELOG" |
    sed -E 's/.* - ([0-9-]+)$/\1/' | sed -n 1p)"
  [[ -n "$date" ]] || die "$CHANGELOG has no '## [$version] - YYYY-MM-DD' heading"
  printf '%s\n' "$date"
}

# facts TAG PAGE: the "This copy" section of PAGE for TAG.
facts() {
  local tag="$1" page="$2" version date ehr commit revised standard source
  [[ "$tag" =~ $TAG_RE ]] || die "$tag is not a vX.Y.Z tag"
  version="${tag#v}"
  date="$(release_date "${version%%-*}")"
  ehr="$(ehr_version)"
  commit="$(pin FERROHEALTH_COMMIT)"
  revised="$(pin "$(revised_key "$page")")"
  echo "## This copy"
  echo
  echo "This copy accompanies FerroBRIDGE ${version}, released ${date}. It is the FerroHEALTH book's page revised ${revised}, taken at commit \`${commit}\` of <https://github.com/FerroHEALTH/FerroHEALTH>, with these facts of the release appended."
  echo
  echo "- **The EHR system and its version (EHDS Art. 38(2)(b)):** FerroBRIDGE ${version} with FerroEHR ${ehr}, the oldest FerroEHR release this FerroBRIDGE release is tested against: the CDR its end-to-end tests run against."
  echo "- **Release date:** ${date}, the date of the release's heading in FerroBRIDGE's changelog."
  case "$page" in
    information-sheet)
      echo
      echo "The versions of the standards, formats and specifications FerroBRIDGE ${version} supports (EHDS Art. 38(2)(e)), read from the release's pins. FerroEHR's are on the copy each FerroEHR release attaches."
      echo
      echo "| Standard or specification | Version |"
      echo "|---|---|"
      while IFS=$'\t' read -r standard source; do
        [[ -z "$standard" || "$standard" == \#* ]] && continue
        echo "| $standard | $(version "$source") |"
      done <"$STANDARDS"
      ;;
    instructions-for-use)
      echo "- **PostgreSQL:** $(version 'image:PostgreSQL image'), the database the OMOP CDM is tested on."
      echo "- **The openEHR CDR:** any CDR that serves openEHR ITS-REST $(version 'first:openEHR ITS-REST'); FerroEHR ${ehr} is the one this release is tested against."
      ;;
  esac
}

# render PAGE TAG SOURCE TARGET: the attached copy of PAGE for TAG. The
# family book's relative links are made absolute, the release's version and
# date are stamped under the title, and the page's "This copy" section is
# replaced by the release's facts. FerroBRIDGE's book is served from main,
# never frozen per tag, so its absolute links stay as they are.
render() {
  local page="$1" tag="$2" source="$3" target="$4" date revised
  date="$(release_date "$(sed -E 's/^v//; s/-.*//' <<<"$tag")")"
  revised="$(sed -n '3s/^Revised \([0-9-]*\)\.$/\1/p' "$source")"
  awk '/^## This copy$/ { exit } /^<!-- toc -->$/ { next } { print }' "$source" |
    sed -E \
      -e "s#\]\(\.\./([a-z0-9-]+)\.md#](${FAMILY}/\1.html#g" \
      -e "s#\]\(([a-z0-9][a-z0-9-]*)\.md#](${FAMILY}/ehds/\1.html#g" |
    awk -v stamp="**This copy:** FerroBRIDGE ${tag#v}, released ${date}, manufactured by Cadasto B.V. It is the FerroHEALTH book's ${FAMILY}/ehds/${page}.html, revised ${revised}, with the facts of this release under \"This copy\" at the end." \
      'NR == 1 && /^# / { print; print ""; print stamp; next } { print }' >"$target"
  grep -qF "**This copy:**" "$target" || die "ehds/$page.md does not start with a '# ' title"
  facts "$tag" "$page" >>"$target"
}

# check_sheet FILE: the information sheet carries every point of Art. 38(2).
check_sheet() {
  local file="$1" point
  for point in a b c d e; do
    grep -qE "^## \(${point}\) " "$file" || die "the information sheet has no section for Art. 38(2)(${point})"
  done
}

# attach TAG OUT: the two copies the release attaches, their paths printed.
attach() {
  local tag="$1" out="$2" work page
  [[ "$tag" =~ $TAG_RE ]] || die "$tag is not a vX.Y.Z tag"
  mkdir -p "$out"
  work="$(mktemp -d)"
  for page in information-sheet instructions-for-use; do
    (fetch "$page" "$work/$page.md") || {
      rm -rf "$work"
      exit 1
    }
  done
  (check_sheet "$work/information-sheet.md") || {
    rm -rf "$work"
    exit 1
  }
  for page in information-sheet instructions-for-use; do
    (render "$page" "$tag" "$work/$page.md" "$out/ferrobridge-${tag}-${page}.md") || {
      rm -rf "$work"
      exit 1
    }
    echo "$out/ferrobridge-${tag}-${page}.md"
  done
  rm -rf "$work"
}

# notes_section: the release notes section naming the pinned revisions.
notes_section() {
  local commit sheet ifu readiness techdoc
  commit="$(pin FERROHEALTH_COMMIT)"
  sheet="$(pin INFORMATION_SHEET_REVISED)"
  ifu="$(pin INSTRUCTIONS_FOR_USE_REVISED)"
  readiness="$(pin READINESS_REVISED)"
  techdoc="$(pin TECHNICAL_DOCUMENTATION_REVISED)"
  echo "### The EHDS system documents"
  echo
  echo "The EHR system is FerroEHR with FerroBRIDGE, and its documents are pages of the FerroHEALTH book. This release relies on them at commit \`${commit}\` of <https://github.com/FerroHEALTH/FerroHEALTH>: the [information sheet](${FAMILY}/ehds/information-sheet.html) revised ${sheet} and the [instructions for use](${FAMILY}/ehds/instructions-for-use.html) revised ${ifu}, attached to this release with its versions appended (EHDS Art. 30(1)(d), Art. 38), the [readiness record](${FAMILY}/ehds/readiness.html) revised ${readiness} and the [technical documentation](${FAMILY}/ehds/technical-documentation.html) revised ${techdoc} (EHDS Art. 37)."
}

# check_notes FILE: refuse notes that do not carry the section verbatim.
check_notes() {
  local file="$1" section
  [[ -f "$file" ]] || die "$file does not exist"
  section="$(notes_section)"
  [[ "$(cat "$file")" == *"$section"* ]] ||
    die "$file does not name the pinned revisions of the EHDS system documents ('system-documents.sh notes-section' prints the section)"
  echo "system-documents: $file names the EHDS system documents at FerroHEALTH $(pin FERROHEALTH_COMMIT)"
}

self_test() {
  local tmp fails=0 tag out revised_sheet revised_ifu
  expect() {
    if [[ "$2" != "$3" ]]; then
      echo "self-test: $1: got '$2', want '$3'" >&2
      fails=$((fails + 1))
    fi
  }
  refused() { if ("$@") >/dev/null 2>&1; then echo accepted; else echo refused; fi; }
  tmp="$(mktemp -d)"
  # The readers, over this tree's own pins: every standards row reads.
  while IFS=$'\t' read -r standard source; do
    [[ -z "$standard" || "$standard" == \#* ]] && continue
    [[ -n "$(version "$source")" ]] || die "self-test: $standard reads no version"
  done <"$STANDARDS"
  [[ "$(ehr_version)" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "self-test: no FerroEHR release"
  # The readers, over a fixed matrix.
  printf '%s\n' "| Item | Pin | Repeated in |" "|---|---|---|" \
    "| FHIR | R4 (4.0.1) | x |" \
    "| OMOCL | v1.0.0 | x |" \
    "| HL7 Europe Laboratory Report | package \`hl7.fhir.eu.laboratory\` 2.0.0, pin-set digest \`ab\` | x |" \
    "| UCUM essence | \`ucum-org/ucum\` tag \`v2.2\` commit \`d0\` | x |" \
    "| PostgreSQL image | \`postgres:18.6@sha256:4e\` | x |" \
    "| FerroEHR CDR image | \`ghcr.io/ferrohealth/ferroehr:5.5.0@sha256:e1\` | x |" >"$tmp/VERSIONS.md"
  VERSIONS="$tmp/VERSIONS.md"
  expect "a whole cell" "$(version cell:FHIR)" "R4 (4.0.1)"
  expect "a first word" "$(version first:OMOCL)" "v1.0.0"
  expect "a package version" "$(version 'package:HL7 Europe Laboratory Report')" "2.0.0"
  expect "a git tag" "$(version 'tag:UCUM essence')" "v2.2"
  expect "an image tag" "$(version 'image:PostgreSQL image')" "18.6"
  expect "the FerroEHR release" "$(ehr_version)" "5.5.0"
  expect "a missing row" "$(refused version first:Nothing)" refused
  expect "an unknown source" "$(refused version other:FHIR)" refused
  VERSIONS="docs/VERSIONS.md"
  # The pages: a fetch at the pinned revision passes, another revision and a
  # page without "This copy" are refused.
  revised_sheet="$(pin INFORMATION_SHEET_REVISED)"
  revised_ifu="$(pin INSTRUCTIONS_FOR_USE_REVISED)"
  mkdir -p "$tmp/pages" "$tmp/other"
  {
    printf '# The EHDS EHR system: information sheet\n\nRevised %s.\n\n<!-- toc -->\n\n' "$revised_sheet"
    printf 'See the [instructions for use](instructions-for-use.md) and [security](../security.md#x).\n\n'
    printf 'The [facade](https://ferrobridge.eu/docs/integrate/fhir-facade.html).\n\n'
    for point in a b c d e; do printf '## (%s) Point\n\nText.\n\n' "$point"; done
    printf '## This copy\n\nThe book carries no facts here.\n'
  } >"$tmp/pages/information-sheet.md"
  printf '# The EHDS EHR system: instructions for use\n\nRevised %s.\n\nText.\n\n## This copy\n\nNone.\n' \
    "$revised_ifu" >"$tmp/pages/instructions-for-use.md"
  printf '# B\n\nRevised 1999-01-01.\n\n## This copy\n' >"$tmp/other/information-sheet.md"
  expect "a page at the pinned revision" "$(EHDS_PAGES_DIR="$tmp/pages" refused fetch information-sheet "$tmp/f.md")" accepted
  expect "a page at another revision" "$(EHDS_PAGES_DIR="$tmp/other" refused fetch information-sheet "$tmp/f.md")" refused
  printf '# A\n\nRevised %s.\n' "$revised_sheet" >"$tmp/other/information-sheet.md"
  expect "a page without 'This copy'" "$(EHDS_PAGES_DIR="$tmp/other" refused fetch information-sheet "$tmp/f.md")" refused
  # The copies, for the newest dated changelog heading as the tag.
  tag="v$(grep -m 1 -oE '^## \[[0-9]+\.[0-9]+\.[0-9]+\] - [0-9-]+$' "$CHANGELOG" | sed -E 's/^## \[([^]]+)\].*/\1/')"
  EHDS_PAGES_DIR="$tmp/pages" attach "$tag" "$tmp/out" >/dev/null
  out="$tmp/out/ferrobridge-${tag}-information-sheet.md"
  expect "the stamp under the title" "$(grep -cF "**This copy:** FerroBRIDGE ${tag#v}, released" "$out")" 1
  expect "a sibling page link made absolute" "$(grep -cF "](${FAMILY}/ehds/instructions-for-use.html)" "$out")" 1
  expect "a family root link made absolute" "$(grep -cF "](${FAMILY}/security.html#x)" "$out")" 1
  expect "a FerroBRIDGE link kept" "$(grep -cF "](https://ferrobridge.eu/docs/integrate/fhir-facade.html)" "$out")" 1
  expect "the page's own 'This copy' text dropped" "$(grep -cF "The book carries no facts here." "$out")" 0
  expect "the toc marker dropped" "$(grep -cF "<!-- toc -->" "$out")" 0
  expect "one 'This copy' section" "$(grep -c '^## This copy$' "$out")" 1
  expect "the standards table" "$(grep -cF "| FHIRconnect | " "$out")" 1
  expect "the system version" "$(grep -cF "FerroBRIDGE ${tag#v} with FerroEHR $(ehr_version)" "$out")" 1
  expect "the platform facts" "$(grep -cF "**PostgreSQL:**" "$tmp/out/ferrobridge-${tag}-instructions-for-use.md")" 1
  sed -i.bak '/^## (c) /d' "$tmp/pages/information-sheet.md"
  expect "a sheet without Art. 38(2)(c)" "$(EHDS_PAGES_DIR="$tmp/pages" refused attach "$tag" "$tmp/out2")" refused
  expect "a bad tag" "$(EHDS_PAGES_DIR="$tmp/pages" refused attach 0.0.1 "$tmp/out3")" refused
  # The notes: the section passes, a missing or older revision is refused.
  notes_section >"$tmp/notes.md"
  expect "notes naming the pinned revisions" "$(refused check_notes "$tmp/notes.md")" accepted
  sed -i.bak "s/revised $revised_sheet/revised 1999-01-01/" "$tmp/notes.md"
  expect "notes naming another revision" "$(refused check_notes "$tmp/notes.md")" refused
  printf '### Fixed\n\n- a fix\n' >"$tmp/notes.md"
  expect "notes without the section" "$(refused check_notes "$tmp/notes.md")" refused
  rm -rf "$tmp"
  if [[ "$fails" -gt 0 ]]; then
    echo "system-documents: self-test FAILED ($fails case(s))" >&2
    exit 1
  fi
  echo "system-documents: self-test OK (the pins read, a pinned page fetched, another revision and a page without 'This copy' refused, the copies stamped with absolute links and the facts, the notes section checked)."
}

# A relative path argument is the caller's, so it is resolved against the
# directory the script was started from.
absolute() {
  if [[ "$1" == /* ]]; then printf '%s\n' "$1"; else printf '%s\n' "$caller_dir/$1"; fi
}

case "${1:-}" in
  pin)
    [[ "$#" -eq 2 ]] || guard_usage "$GRAMMAR"
    pin "$2"
    ;;
  fetch)
    [[ "$#" -eq 3 ]] || guard_usage "$GRAMMAR"
    fetch "$2" "$(absolute "$3")"
    ;;
  facts)
    [[ "$#" -eq 3 ]] || guard_usage "$GRAMMAR"
    facts "$2" "$3"
    ;;
  attach)
    [[ "$#" -eq 3 ]] || guard_usage "$GRAMMAR"
    attach "$2" "$(absolute "$3")"
    ;;
  notes-section)
    shift
    guard_no_args "$@"
    notes_section
    ;;
  check-notes)
    [[ "$#" -eq 2 ]] || guard_usage "$GRAMMAR"
    check_notes "$(absolute "$2")"
    ;;
  --self-test)
    shift
    guard_no_args "$@"
    self_test
    ;;
  *) guard_usage "$GRAMMAR" ;;
esac
