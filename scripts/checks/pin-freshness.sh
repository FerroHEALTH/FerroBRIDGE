#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every CI tool pin no ecosystem watches, compared with its newest upstream
# release (#35). Dependabot's `github-actions` ecosystem reads `uses:`
# references; it does not read an image named in a `run:` block, a version
# handed to an installer as an input, or a commit a vendor script fetches from.
# scripts/checks/versions.sh catches drift between two files we control and
# cannot tell you that upstream published a newer release, which is what this
# reads.
#
#   scripts/checks/pin-freshness.sh
#   scripts/checks/pin-freshness.sh --self-test
#
# Reads each pin from docs/VERSIONS.md and each newest release tag from the
# upstream project's GitHub releases, which is the tag the container image and
# the installer both carry. A corpus pinned by commit on a repository with no
# releases is read against the newest commit of the branch it follows, and a
# FHIR package (a core package, THO, the v2-to-FHIR guide, an examples package
# or a root of the profile set) against the registry's `latest` version, and
# the Chrome for Testing release against the newest Stable version its JSON
# endpoint lists. Needs an authenticated `gh`, curl and jq. `--self-test` runs
# the Chrome for Testing reader over fixtures and needs no network.
#
# A pin cell that ends in `held: <reason>` records why its pin stays behind
# the newest release; such a pin is reported HELD with its reason and never
# counts as stale. The version of a cell is the word after the package name
# when the cell reads `package <name> <version>`, else its first word.
#
# Exit 0 when every pin is current or held, 1 when at least one is behind
# (each such line starts with STALE), 2 when a release could not be read, so a
# network failure never reads as a fresh pin.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly MATRIX=docs/VERSIONS.md

# One "matrix label<TAB>upstream repository" record per line. The label is the
# first cell of the row in docs/VERSIONS.md, backticks and all.
readonly WATCHED="\
zizmor	zizmorcore/zizmor
actionlint	rhysd/actionlint
shellcheck	koalaman/shellcheck
hadolint	hadolint/hadolint
trivy	aquasecurity/trivy
mdBook	rust-lang/mdBook
mdbook-toc	badboy/mdbook-toc
mdbook-mermaid	badboy/mdbook-mermaid
cargo-leptos	leptos-rs/cargo-leptos
Binaryen	WebAssembly/binaryen"

# The second cell of the matrix row whose first cell is $1, with the backticks
# stripped.
matrix_cell() {
  awk -F'|' -v want="$1" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      gsub(/`/, "", value); gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (label == want) { print value; exit }
    }' "$MATRIX"
}

# The version a pin cell on stdin names: the word after the package name when
# the cell reads `package <name> <version>`, else its first word, the same
# token versions.sh reads.
cell_version() {
  awk '{
    if ($1 == "package" && NF >= 3) { v = $3; sub(/,$/, "", v); print v }
    else { print $1 }
    exit
  }'
}

# The reason a pin cell on stdin records for a pin held behind the newest
# release, the text after `held:` up to the end of the cell, or nothing.
cell_held() {
  sed -n 's/.*held: *\(.*\)$/\1/p' | sed 's/[[:space:]]*$//'
}

# The version the matrix row whose first cell is $1 pins.
matrix_pin() {
  matrix_cell "$1" | cell_version
}

# The newest version a registry metadata document on stdin names: its
# `latest` dist-tag, or its highest version by version order when it has
# none, as a package published only as a ballot is.
registry_newest() {
  local document tagged
  document="$(cat)"
  if tagged="$(jq -er '."dist-tags".latest' <<< "$document" 2>/dev/null)"; then
    printf '%s\n' "$tagged"
    return
  fi
  jq -er '.versions | keys[]' <<< "$document" | sort -V | tail -n 1 | grep .
}

# The Chrome for Testing release the browser journeys run Chrome and
# chromedriver at, read against the newest Stable release the Chrome for
# Testing JSON endpoints publish (#821).
readonly CHROME_LABEL="Chrome for Testing (Chrome and chromedriver)"
readonly CHROME_FEED=https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions.json

# The version of the Stable channel in the last-known-good-versions JSON on
# stdin; fails when the document names no Stable version.
chrome_stable() {
  jq -er '.channels.Stable.version | select(type == "string" and length > 0)'
}

# The line one pin LABEL at PINNED gets against the newest release LATEST read
# from SOURCE: `current` when they are equal, `STALE` when they differ.
verdict() {
  if [ "$2" = "$3" ]; then
    printf 'current    %s %s (%s)\n' "$1" "$2" "$4"
  else
    printf 'STALE      %s: pinned %s, newest upstream release %s (%s)\n' "$1" "$2" "$3" "$4"
  fi
}

# Runs the Chrome for Testing reader and the verdict over fixtures, and the
# matrix reader over the Chrome row, with no network.
self_test() {
  local failures=0 got
  expect() {
    if [ "$2" != "$1" ]; then
      printf 'pin-freshness: self-test: %s gave "%s", wanted "%s"\n' "$3" "$2" "$1" >&2
      failures=$((failures + 1))
    fi
  }
  got="$(chrome_stable <<< '{"timestamp":"2026-10-17T00:00:00Z","channels":{"Stable":{"channel":"Stable","version":"155.0.8059.84","revision":"1"},"Beta":{"channel":"Beta","version":"156.0.8100.10","revision":"2"}}}')"
  expect "155.0.8059.84" "$got" "the Stable channel of a feed"
  if got="$(chrome_stable <<< '{"channels":{"Beta":{"version":"156.0.8100.10"}}}' 2>/dev/null)"; then
    expect "a failure" "$got" "a feed with no Stable channel"
  fi
  if got="$(chrome_stable <<< 'not json' 2>/dev/null)"; then
    expect "a failure" "$got" "a feed that is no JSON"
  fi
  got="$(verdict "$CHROME_LABEL" 1.2.3.4 1.2.3.4 feed | cut -c1-10)"
  expect "current   " "$got" "an equal pin"
  got="$(verdict "$CHROME_LABEL" 1.2.3.4 1.2.3.5 feed | cut -c1-10)"
  expect "STALE     " "$got" "a pin behind the newest Stable"
  got="$(cell_version <<< 'package hl7.fhir.eu.laboratory 2.0.0, pin-set digest ab, tarball sha256 cd')"
  expect "2.0.0" "$got" "the version of a package cell"
  got="$(cell_version <<< 'v1.0.0 (grammar)')"
  expect "v1.0.0" "$got" "the first word of another cell"
  got="$(cell_held <<< 'package a 2.0.0, pin-set digest ab, held: 2.1.0 needs THO 7.4.0')"
  expect "2.1.0 needs THO 7.4.0" "$got" "the reason of a held pin"
  got="$(cell_held <<< 'package a 2.0.0, pin-set digest ab')"
  expect "" "$got" "a pin with no reason"
  got="$(registry_newest <<< '{"dist-tags":{"latest":"2.1.0"},"versions":{"2.0.0":{},"2.1.0":{}}}')"
  expect "2.1.0" "$got" "the latest tag of a package"
  got="$(registry_newest <<< '{"versions":{"0.9.0-ballot":{},"1.0.0-ballot":{}}}')"
  expect "1.0.0-ballot" "$got" "the highest version of a ballot-only package"
  got="$(matrix_pin "$CHROME_LABEL")"
  case "$got" in
    [0-9]*.[0-9]*.[0-9]*.[0-9]*) ;;
    *) expect "a four-part version" "$got" "the Chrome for Testing row of $MATRIX" ;;
  esac
  if [ "$failures" -gt 0 ]; then
    echo "pin-freshness: self-test: $failures check(s) failed" >&2
    exit 1
  fi
  echo "pin-freshness: self-test passed"
}

case "${1:-}" in
  "") ;;
  --self-test)
    self_test
    exit 0
    ;;
  *)
    echo "usage: scripts/checks/pin-freshness.sh [--self-test]" >&2
    exit 2
    ;;
esac

stale=0
unreadable=0
while IFS=$'\t' read -r label repo; do
  [ -n "$label" ] || continue

  pinned="$(matrix_pin "$label")"
  if [ -z "$pinned" ]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! tag="$(gh api "repos/$repo/releases/latest" --jq '.tag_name' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the newest release of %s (%s)\n' "$label" "$repo" "$tag"
    unreadable=1
    continue
  fi
  # A leading v is a version prefix only before a digit (v1.2.3, never
  # Binaryen's version_133).
  latest="$tag"
  [[ $tag =~ ^v[0-9] ]] && latest="${tag#v}"

  if [ "$pinned" = "$latest" ]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$repo"
  else
    printf 'STALE      %s: pinned %s, newest upstream release %s (https://github.com/%s/releases/tag/%s)\n' \
      "$label" "$pinned" "$latest" "$repo" "$tag"
    stale=1
  fi
done <<< "$WATCHED"

# One "matrix label<TAB>upstream repository<TAB>branch" record per line: a
# corpus pinned by commit on a repository that publishes no releases, read
# against the newest commit of the branch the pin follows. Each KDS fixture
# row pins one file, and the UCUM row a release tag, so none of them is read
# here.
readonly WATCHED_COMMITS="\
FHIRconnect specification source	SevKohler/FHIRconnect-spec	main
FHIRconnect mapping library (corpus, never an oracle)	SevKohler/FHIRconnect-mapping-lib	main
OMOCL corpus	SevKohler/OMOCL	main
HL7 v2 definitions (v2ig source of truth, never committed)	HL7/v2ig	main
FHIRPath test suite (R4)	FHIR/fhir-test-cases	master
HL7 v2+ licence page	HL7/v2plus	master
HL7 v2 samples: Microsoft FHIR-Converter	microsoft/FHIR-Converter	main
HL7 v2 samples: CDC ReportStream data tests	CDCgov/prime-reportstream	main
HL7 v2 samples: HL7 v2-to-FHIR benchmark messages	HL7/v2-to-fhir	master
HL7 v2 samples: NIST LRI (build time, never committed)	usnistgov/hit-mu-tools-resource-bundles	lri-r2
HL7 v2 samples: NIST LOI (build time, never committed)	usnistgov/hit-mu-tools-resource-bundles	loi-r1
HL7 v2 samples: NIST syndromic surveillance (build time, never committed)	usnistgov/hit-mu-tools-resource-bundles	ss-r2
HL7 v2 samples: AIRA MQE (build time, never committed)	immregistries/mqe	master
HL7 v2 legacy tables (NIST IGAMT export, never committed)	usnistgov/igamt-hl7Tools-service	master"

# The first 40-hex commit in the second cell of the matrix row whose first cell
# is $1.
matrix_commit() {
  awk -F'|' -v want="$1" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      if (label == want && match(value, /[0-9a-f]{40}/)) { print substr(value, RSTART, RLENGTH); exit }
    }' "$MATRIX"
}

while IFS=$'\t' read -r label repo branch; do
  [ -n "$label" ] || continue

  pinned="$(matrix_commit "$label")"
  if [ -z "$pinned" ]; then
    printf 'UNREADABLE %s: no commit pin in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! head="$(gh api "repos/$repo/commits/$branch" --jq '.sha' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the head of %s %s (%s)\n' "$label" "$repo" "$branch" "$head"
    unreadable=1
    continue
  fi

  if [ "$pinned" = "$head" ]; then
    printf 'current    %s %s (%s %s)\n' "$label" "$pinned" "$repo" "$branch"
  else
    printf 'STALE      %s: pinned %s, newest commit on %s %s (https://github.com/%s/commit/%s)\n' \
      "$label" "$pinned" "$branch" "$head" "$repo" "$head"
    stale=1
  fi
done <<< "$WATCHED_COMMITS"

# One "matrix label<TAB>registry metadata URL" record per line: a FHIR package
# pinned by version, read against the `latest` dist-tag the FHIR package
# registry publishes for it (the npm registry shape both registries speak),
# or, for a package with ballot versions only and no `latest` tag, against its
# highest version. Of the profile set only the two roots with a `latest` tag
# are read: every other row is pinned at the version a dependent names. The
# corpora the eehrxf crate reads are read by their package rows (#1335).
readonly WATCHED_PACKAGES="\
hl7.fhir.r3.core	https://packages.fhir.org/hl7.fhir.r3.core
hl7.fhir.r4.core	https://packages.fhir.org/hl7.fhir.r4.core
hl7.fhir.r4b.core	https://packages.fhir.org/hl7.fhir.r4b.core
hl7.fhir.r5.core	https://packages.fhir.org/hl7.fhir.r5.core
hl7.fhir.r6.core	https://packages2.fhir.org/packages/hl7.fhir.r6.core
hl7.terminology (THO)	https://packages.fhir.org/hl7.terminology
hl7.fhir.uv.v2mappings (the v2-to-FHIR IG)	https://packages.fhir.org/hl7.fhir.uv.v2mappings
hl7.fhir.eu.base	https://packages.fhir.org/hl7.fhir.eu.base
hl7.fhir.uv.ips	https://packages.fhir.org/hl7.fhir.uv.ips
hl7.fhir.r3.examples	https://packages.fhir.org/hl7.fhir.r3.examples
hl7.fhir.r4.examples	https://packages.fhir.org/hl7.fhir.r4.examples
hl7.fhir.r4b.examples	https://packages.fhir.org/hl7.fhir.r4b.examples
hl7.fhir.r5.examples	https://packages.fhir.org/hl7.fhir.r5.examples
hl7.fhir.r6.examples	https://packages2.fhir.org/packages/hl7.fhir.r6.examples
Xt-EHR EHDS Logical Information Models	https://packages.fhir.org/xtehr.eu.ehds.models
HL7 Europe Base and Core	https://packages.fhir.org/hl7.fhir.eu.base
HL7 Europe Patient Summary	https://packages.fhir.org/hl7.fhir.eu.eps
HL7 Europe Medication Prescription and Dispense	https://packages.fhir.org/hl7.fhir.eu.mpd
HL7 Europe Laboratory Report	https://packages.fhir.org/hl7.fhir.eu.laboratory
HL7 Europe Imaging Report	https://packages.fhir.org/hl7.fhir.eu.imaging
HL7 Europe Hospital Discharge Report	https://packages.fhir.org/hl7.fhir.eu.hdr
MyHealth@EU Master Value Sets Catalogue	https://packages.fhir.org/myhealth.eu.fhir.mvc-package
HL7 France FR Core	https://packages.fhir.org/hl7.fhir.fr.core"


while IFS=$'\t' read -r label url; do
  [ -n "$label" ] || continue

  pinned="$(matrix_pin "$label")"
  if [ -z "$pinned" ]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$url" | registry_newest 2>&1)"; then
    printf 'UNREADABLE %s: could not read the latest version at %s (%s)\n' "$label" "$url" "$latest"
    unreadable=1
    continue
  fi

  held="$(matrix_cell "$label" | cell_held)"
  if [ "$pinned" = "$latest" ]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$url"
  elif [ -n "$held" ]; then
    printf 'HELD       %s: pinned %s, the registry names %s latest, held: %s (%s)\n' "$label" "$pinned" "$latest" "$held" "$url"
  else
    printf 'STALE      %s: pinned %s, the registry names %s latest (%s)\n' "$label" "$pinned" "$latest" "$url"
    stale=1
  fi
done <<< "$WATCHED_PACKAGES"

pinned="$(matrix_pin "$CHROME_LABEL")"
if [ -z "$pinned" ]; then
  printf 'UNREADABLE %s: no pin row in %s\n' "$CHROME_LABEL" "$MATRIX"
  unreadable=1
elif ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$CHROME_FEED" | chrome_stable 2>&1)"; then
  printf 'UNREADABLE %s: could not read the newest Stable release from %s (%s)\n' \
    "$CHROME_LABEL" "$CHROME_FEED" "$latest"
  unreadable=1
else
  verdict "$CHROME_LABEL" "$pinned" "$latest" "$CHROME_FEED"
  [ "$pinned" = "$latest" ] || stale=1
fi

[ "$unreadable" -eq 0 ] || exit 2
exit "$stale"
