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
#
# Reads each pin from docs/VERSIONS.md and each newest release tag from the
# upstream project's GitHub releases, which is the tag the container image and
# the installer both carry. A corpus pinned by commit on a repository with no
# releases is read against the newest commit of the branch it follows, and a
# FHIR package (a core package, THO, the v2-to-FHIR guide, an examples package
# or a root of the profile set) against the registry's `latest` version. Needs
# an authenticated `gh`, curl and jq.
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
Tailwind CSS	tailwindlabs/tailwindcss
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
# against the newest commit of the branch the pin follows. The REST API
# chapter is pinned at the head of an upstream pull request and is read
# against the default branch it merges into. The two KDS fixture rows pin one
# file each, and the UCUM row a release tag, so neither is read here.
readonly WATCHED_COMMITS="\
FHIRconnect REST API chapter (draft, unmerged)	SevKohler/FHIRconnect-spec	main
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


[ "$unreadable" -eq 0 ] || exit 2
exit "$stale"
