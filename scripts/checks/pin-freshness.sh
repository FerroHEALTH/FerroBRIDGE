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
# Exit 0 when every pin is current, 1 when at least one is behind (each such
# line starts with STALE), 2 when a release could not be read, so a network
# failure never reads as a fresh pin.
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
mdBook	rust-lang/mdBook
mdbook-toc	badboy/mdbook-toc
mdbook-mermaid	badboy/mdbook-mermaid
cargo-leptos	leptos-rs/cargo-leptos
Binaryen	WebAssembly/binaryen"

# The second cell of the matrix row whose first cell is $1, with the backticks
# stripped and only the first token kept, the same shape versions.sh reads.
matrix_pin() {
  awk -F'|' -v want="$1" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (label == want) { split(value, f, " "); print f[1]; exit }
    }' "$MATRIX"
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
# registry publishes for it (the npm registry shape both registries speak).
# Of the profile set only the two roots with a `latest` tag are read: every
# other row is pinned at the version a dependent names, and the third root,
# hl7.fhir.eu.eps, has one ballot version and no `latest` tag.
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
hl7.fhir.r6.examples	https://packages2.fhir.org/packages/hl7.fhir.r6.examples"

while IFS=$'\t' read -r label url; do
  [ -n "$label" ] || continue

  pinned="$(matrix_pin "$label")"
  if [ -z "$pinned" ]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$url" | jq -er '."dist-tags".latest' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the latest version at %s (%s)\n' "$label" "$url" "$latest"
    unreadable=1
    continue
  fi

  if [ "$pinned" = "$latest" ]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$url"
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
