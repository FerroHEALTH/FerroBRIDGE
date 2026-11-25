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
#   scripts/checks/pin-freshness.sh --family
#   scripts/checks/pin-freshness.sh --self-test
#
# Reads each pin from docs/VERSIONS.md and each newest release tag from the
# upstream project's GitHub releases, which is the tag the container image and
# the installer both carry: from the REST API, else from the redirect of the
# project's releases/latest page, which an organisation's IP allow list does
# not guard (#2007). A corpus pinned by commit on a repository with no
# releases is read against the newest commit of the branch it follows, and a
# FHIR package (a core package, THO, the v2-to-FHIR guide, an examples package
# or a root of the profile set) against the registry's `latest` version, or
# against the current release its guide's `package-list.json` names when the
# pin comes from the guide's canonical site, and
# the Chrome for Testing release against the newest Stable version its JSON
# endpoint lists. Needs an authenticated `gh`, curl and jq. `--self-test` runs
# the Chrome for Testing reader and the product image readers over fixtures
# and needs no network.
#
# `--family` reads every `ghcr.io/ferrohealth/*` image another FerroHEALTH
# product publishes, pinned by tag and digest in docs/VERSIONS.md, against
# the product's latest stable GitHub release and the digest of the image index
# GHCR serves for it, so a pin never falls behind a product release unseen.
# A release whose image is not published yet is reported PENDING.
#
# A pin cell that ends in `held: <reason>` records why its pin stays behind
# the newest release; such a pin is reported HELD with its reason and never
# counts as stale. The version of a cell is the word after the package name
# when the cell reads `package <name> <version>`, else its first word.
#
# Every pin is read, so one unreadable source never hides another pin's line.
# Exit 0 when every pin is current or held, 1 when at least one is behind
# (each such line starts with STALE), 2 when a release could not be read (each
# such line starts with UNREADABLE), so a network failure never reads as a
# fresh pin.
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
cargo-semver-checks	obi1kenobi/cargo-semver-checks
trivy	aquasecurity/trivy
mdBook	rust-lang/mdBook
mdbook-toc	badboy/mdbook-toc
mdbook-mermaid	badboy/mdbook-mermaid
Standard Webhooks	standard-webhooks/standard-webhooks"

# The second cell of the matrix row whose first cell is $1, with the backticks
# stripped.
matrix_cell() {
  local row_label="$1"
  awk -F'|' -v want="$row_label" '
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
  local row_label="$1"
  matrix_cell "$row_label" | cell_version
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

# The release a guide's `package-list.json` on stdin marks `current`, the one
# its canonical site publishes as the current version; fails unless exactly one
# entry is marked.
package_list_current() {
  jq -er '[.list[]? | select(.current == true) | .version] | select(length == 1) | .[0]'
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

# The release tag the releases/latest redirect URL on stdin names for the
# GitHub repository $1; fails on any other URL, such as the releases page a
# repository with no release redirects to.
release_url_tag() {
  local prefix="https://github.com/$1/releases/tag/" url tag
  url="$(cat)"
  [[ "$url" == "$prefix"?* ]] || return 1
  tag="${url#"$prefix"}"
  [[ "$tag" != */* ]] || return 1
  printf '%s\n' "$tag"
}

# The tag of the newest release of the GitHub repository $1. On failure it
# prints what each source answered.
newest_release_tag() {
  local repo="$1" api page
  if api="$(gh api "repos/$repo/releases/latest" --jq '.tag_name' 2>&1)" && [[ -n "$api" ]]; then
    printf '%s\n' "$api"
    return
  fi
  if page="$(curl --proto '=https' --tlsv1.2 -fsS -o /dev/null -w '%{redirect_url}' \
    "https://github.com/$repo/releases/latest" 2>&1)" && release_url_tag "$repo" <<< "$page"; then
    return
  fi
  printf 'the API answered %s; the releases/latest page answered %s\n' "$api" "${page:-nothing}"
  return 1
}

# The line one pin LABEL at PINNED gets against the newest release LATEST read
# from SOURCE: `current` when they are equal, `STALE` when they differ.
verdict() {
  local pin_name="$1" pinned_version="$2" newest_version="$3" source="$4"
  if [[ "$pinned_version" = "$newest_version" ]]; then
    printf 'current    %s %s (%s)\n' "$pin_name" "$pinned_version" "$source"
  else
    printf 'STALE      %s: pinned %s, newest upstream release %s (%s)\n' "$pin_name" "$pinned_version" "$newest_version" "$source"
  fi
}

# The images the other FerroHEALTH products publish, one
# "image<TAB>repository<TAB>release tag prefix" record per line: every image
# ships with a release of its repository, tagged the prefix and the version.
readonly FAMILY_IMAGES="\
ferrobridge	FerroBRIDGE	v
ferrobridge-console	FerroBRIDGE	v
ferroehr	FerroEHR	v
ferroehr-postgres	FerroEHR	v
ferroehr-viewer	FerroEHR	v
ferrofed	FerroFED	v
ferrofed-console	FerroFED	v
ferropix	FerroPIX	v
ferropix-console	FerroPIX	v
ferrosmart	FerroSMART	ferrosmart-v
ferroterm	FerroTERM	v
ferroterm-console	FerroTERM	v
ferroterm-sync	FerroTERM	v"
readonly GHCR_INDEX='application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json'

# family_pins FILE...: one "image<TAB>tag<TAB>digest<TAB>files<TAB>held reason"
# record per distinct `ghcr.io/ferrohealth/<image>:<tag>@sha256:<digest>`
# reference in FILE, sorted, with the files that carry it joined by ", " and
# the text after a `held:` on a line that carries it, up to a cell boundary.
family_pins() {
  awk '
    {
      line = $0; held = ""
      if (match(line, /held: *[^|]*/)) {
        held = substr(line, RSTART, RLENGTH); sub(/^held: */, "", held); sub(/[ \t]+$/, "", held)
      }
      while (match(line, /ghcr\.io\/ferrohealth\/[a-z0-9-]+:[A-Za-z0-9._-]+@sha256:[0-9a-f]{64}/)) {
        ref = substr(line, RSTART + 20, RLENGTH - 20); line = substr(line, RSTART + RLENGTH)
        name = ref; sub(/:.*$/, "", name)
        tag = ref; sub(/^[^:]*:/, "", tag); sub(/@.*$/, "", tag)
        digest = ref; sub(/^.*@/, "", digest)
        key = name "\t" tag "\t" digest
        if (!(key in files)) { files[key] = FILENAME; keys[++n] = key }
        else if (index(", " files[key] ", ", ", " FILENAME ", ") == 0) { files[key] = files[key] ", " FILENAME }
        if (held != "" && !(key in reason)) reason[key] = held
      }
    }
    END { for (i = 1; i <= n; i++) print keys[i] "\t" files[keys[i]] "\t" reason[keys[i]] }
  ' "$@" | LC_ALL=C sort
}

# family_newest PREFIX: the highest stable version among the release tags on
# stdin that read PREFIX followed by a dotted version, without the prefix.
family_newest() {
  awk -v prefix="$1" '
    index($0, prefix) == 1 {
      version = substr($0, length(prefix) + 1)
      if (version ~ /^[0-9]+(\.[0-9]+)*$/) print version
    }' | sort -V | tail -n 1
}

# family_verdict IMAGE PINNED_TAG PINNED_DIGEST FILES HELD REPO RELEASE_TAG
# VERSION DIGEST: the line one product image pin gets against the latest
# release of REPO, tagged RELEASE_TAG for VERSION, whose image index GHCR
# serves at DIGEST. A pin at the latest version and digest is current; a pin
# behind it is STALE, or HELD when its line records a reason.
family_verdict() {
  local image="$1" pinned_tag="$2" pinned_digest="$3" files="$4" held="$5"
  local repo="$6" release_tag="$7" version="$8" digest="$9"
  local release="https://github.com/FerroHEALTH/$repo/releases/tag/$release_tag"
  if [[ "$pinned_tag" = "$version" && "$pinned_digest" = "$digest" ]]; then
    printf 'current    ghcr.io/ferrohealth/%s %s (%s %s; %s)\n' "$image" "$pinned_tag" "$repo" "$release_tag" "$files"
  elif [[ -n "$held" ]]; then
    printf 'HELD       ghcr.io/ferrohealth/%s: pinned %s in %s, latest release %s %s, held: %s\n' \
      "$image" "$pinned_tag" "$files" "$repo" "$release_tag" "$held"
  elif [[ "$pinned_tag" = "$version" ]]; then
    printf 'STALE      ghcr.io/ferrohealth/%s: pinned %s@%s in %s, but GHCR serves the index of %s at %s now (%s)\n' \
      "$image" "$pinned_tag" "$pinned_digest" "$files" "$version" "$digest" "$release"
  else
    printf 'STALE      ghcr.io/ferrohealth/%s: pinned %s in %s, latest release %s %s (%s) at %s@%s; check the Upgrade notes, Changed and Removed sections of every %s release after %s for configuration the pin moves with (https://github.com/FerroHEALTH/%s/releases)\n' \
      "$image" "$pinned_tag" "$files" "$repo" "$release_tag" "$release" "$version" "$digest" "$repo" "$pinned_tag" "$repo"
  fi
}

# ghcr_digest IMAGE TAG: the digest of the image index GHCR serves for
# ghcr.io/ferrohealth/IMAGE:TAG, read anonymously, or nothing when the tag is
# not published; fails when the registry could not be read.
ghcr_digest() {
  local image="$1" tag="$2" token headers status
  token="$(curl --proto '=https' --tlsv1.2 -fsS \
    "https://ghcr.io/token?scope=repository:ferrohealth/$image:pull" | jq -er .token)" || return 1
  headers="$(curl --proto '=https' --tlsv1.2 -sS -I -H "Authorization: Bearer $token" \
    -H "Accept: $GHCR_INDEX" "https://ghcr.io/v2/ferrohealth/$image/manifests/$tag" | tr -d '\r')" || return 1
  status="$(awk 'NR == 1 { print $2 }' <<< "$headers")"
  case "$status" in
    200) awk 'tolower($1) == "docker-content-digest:" { print $2 }' <<< "$headers" | grep -E '^sha256:[0-9a-f]{64}$' ;;
    404) ;;
    *) return 1 ;;
  esac
}

# family_check FILE...: compares every product image pin of FILE with the
# latest release of the product that publishes it; images of SELF_REPO are
# this repository's own and are skipped. Sets stale and unreadable.
family_check() {
  local pins image tag digest files held record repo prefix tags version newest line
  pins="$(family_pins "$@")"
  while IFS=$'\t' read -r image tag digest files held; do
    [[ -n "$image" ]] || continue
    record="$(awk -F'\t' -v want="$image" '$1 == want { print $2 "\t" $3; exit }' <<< "$FAMILY_IMAGES")"
    repo="${record%%$'\t'*}"
    prefix="${record#*$'\t'}"
    if [[ -z "$repo" ]]; then
      printf 'UNREADABLE ghcr.io/ferrohealth/%s: pinned in %s, but no product of FAMILY_IMAGES publishes it\n' "$image" "$files"
      unreadable=1
      continue
    fi
    [[ "$repo" != "$SELF_REPO" ]] || continue
    if ! tags="$(gh api --paginate "repos/FerroHEALTH/$repo/releases?per_page=100" \
      --jq '.[] | select(.draft == false and .prerelease == false) | .tag_name' 2>&1)"; then
      printf 'UNREADABLE ghcr.io/ferrohealth/%s: could not read the releases of FerroHEALTH/%s (%s)\n' "$image" "$repo" "$tags"
      unreadable=1
      continue
    fi
    version="$(family_newest "$prefix" <<< "$tags")"
    if [[ -z "$version" ]]; then
      printf 'UNREADABLE ghcr.io/ferrohealth/%s: FerroHEALTH/%s has no release tagged %s<version>\n' "$image" "$repo" "$prefix"
      unreadable=1
      continue
    fi
    if ! newest="$(ghcr_digest "$image" "$version")"; then
      printf 'UNREADABLE ghcr.io/ferrohealth/%s: could not read the digest of %s from GHCR\n' "$image" "$version"
      unreadable=1
      continue
    fi
    if [[ -z "$newest" ]]; then
      printf 'PENDING    ghcr.io/ferrohealth/%s: pinned %s in %s, latest release %s %s%s has no published image yet\n' \
        "$image" "$tag" "$files" "$repo" "$prefix" "$version"
      continue
    fi
    line="$(family_verdict "$image" "$tag" "$digest" "$files" "$held" "$repo" "$prefix$version" "$version" "$newest")"
    printf '%s\n' "$line"
    [[ "$line" != STALE* ]] || stale=1
  done <<< "$pins"
}

# The files whose product image pins `--family` reads, and the repository
# whose own images it skips.
readonly FAMILY_SOURCES=("$MATRIX")
readonly SELF_REPO=FerroBRIDGE

# Runs the Chrome for Testing reader, the product image readers and the
# verdicts over fixtures, and the matrix reader over the Chrome row, with no
# network.
self_test() {
  local failures=0 got
  expect() {
    local expected="$1" actual="$2" case_label="$3"
    if [[ "$actual" != "$expected" ]]; then
      printf 'pin-freshness: self-test: %s gave "%s", wanted "%s"\n' "$case_label" "$actual" "$expected" >&2
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
  got="$(package_list_current <<< '{"list":[{"version":"current"},{"version":"0.3.0","current":true},{"version":"0.2.0"}]}')"
  expect "0.3.0" "$got" "the current release of a package list"
  got="$(package_list_current <<< '{"list":[{"version":"0.2.0"}]}' || echo none)"
  expect "none" "$got" "a package list that marks no current release"
  local fixture d1 d2
  fixture="$(mktemp)"
  d1="sha256:$(printf '1%.0s' {1..64})"
  d2="sha256:$(printf '2%.0s' {1..64})"
  printf '%s\n' \
    "| A | \`ghcr.io/ferrohealth/ferroterm:0.4.2@$d1\` | x |" \
    "    image: \${FERROPIX_IMAGE:-ghcr.io/ferrohealth/ferropix:0.0.26@$d2}  # held: the PIX configuration" \
    "| B | \`ghcr.io/ferrohealth/ferroterm:0.4.2@$d1\` | y |" \
    "    image: ghcr.io/ferrohealth/ferrobridge:\${FERROBRIDGE_VERSION:-0.0.27}" > "$fixture"
  got="$(family_pins "$fixture" | cut -f1,2,5 | tr '\t' ' ' | paste -sd ';' -)"
  expect "ferropix 0.0.26 the PIX configuration;ferroterm 0.4.2 " "$got" "the product image pins of a file"
  got="$(family_pins "$fixture" "$fixture" | awk -F'\t' '$1 == "ferroterm" { print $4 }')"
  expect "$fixture" "$got" "a file named once for a pin it carries twice"
  rm -f "$fixture"
  got="$(printf '%s\n' v0.0.9 v0.0.10 v0.0.11-rc1 ferrohealth-smart-v0.33.1 | family_newest v)"
  expect "0.0.10" "$got" "the newest stable release of a v-tagged product"
  got="$(printf '%s\n' ferrohealth-smart-v0.33.1 ferrosmart-v0.3.0 ferrosmart-v0.2.0 | family_newest ferrosmart-v)"
  expect "0.3.0" "$got" "the newest server release among crate releases"
  got="$(family_verdict ferroterm 0.4.7 "$d1" f "" FerroTERM v0.4.7 0.4.7 "$d1" | cut -c1-10)"
  expect "current   " "$got" "a pin at the latest release and digest"
  got="$(family_verdict ferroterm 0.4.7 "$d1" f "" FerroTERM v0.4.7 0.4.7 "$d2" | cut -c1-10)"
  expect "STALE     " "$got" "a pin whose tag now serves another digest"
  got="$(family_verdict ferroterm 0.4.2 "$d1" f "" FerroTERM v0.4.7 0.4.7 "$d2")"
  case "$got" in
    "STALE      ghcr.io/ferrohealth/ferroterm: pinned 0.4.2 in f, latest release FerroTERM v0.4.7 "*"Upgrade notes"*) ;;
    *) expect "a STALE line naming the release and the upgrade notes" "$got" "a pin behind the latest release" ;;
  esac
  got="$(family_verdict ferroterm 0.4.2 "$d1" f "a reason" FerroTERM v0.4.7 0.4.7 "$d2" | cut -c1-10)"
  expect "HELD      " "$got" "a held pin behind the latest release"
  got="$(release_url_tag aquasecurity/trivy <<< 'https://github.com/aquasecurity/trivy/releases/tag/v0.76.2')"
  expect "v0.76.2" "$got" "the tag of a releases/latest redirect"
  if got="$(release_url_tag aquasecurity/trivy <<< 'https://github.com/aquasecurity/trivy/releases')"; then
    expect "a failure" "$got" "a redirect to a repository with no release"
  fi
  if got="$(release_url_tag aquasecurity/trivy <<< 'https://github.com/rhysd/actionlint/releases/tag/v1.7.12')"; then
    expect "a failure" "$got" "a redirect to another repository"
  fi
  got="$(matrix_pin "$CHROME_LABEL")"
  case "$got" in
    [0-9]*.[0-9]*.[0-9]*.[0-9]*) ;;
    *) expect "a four-part version" "$got" "the Chrome for Testing row of $MATRIX" ;;
  esac
  if [[ "$failures" -gt 0 ]]; then
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
  --family)
    stale=0
    unreadable=0
    family_check "${FAMILY_SOURCES[@]}"
    [[ "$unreadable" -eq 0 ]] || exit 2
    exit "$stale"
    ;;
  *)
    echo "usage: scripts/checks/pin-freshness.sh [--family | --self-test]" >&2
    exit 2
    ;;
esac

stale=0
unreadable=0
while IFS=$'\t' read -r label repo; do
  [[ -n "$label" ]] || continue

  pinned="$(matrix_pin "$label")"
  if [[ -z "$pinned" ]]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! tag="$(newest_release_tag "$repo")"; then
    printf 'UNREADABLE %s: could not read the newest release of %s (%s)\n' "$label" "$repo" "$tag"
    unreadable=1
    continue
  fi
  # A leading v is a version prefix only before a digit (v1.2.3, never a
  # tag such as version_133).
  latest="$tag"
  [[ $tag =~ ^v[0-9] ]] && latest="${tag#v}"

  if [[ "$pinned" = "$latest" ]]; then
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
HL7 v2 legacy tables (NIST IGAMT export, never committed)	usnistgov/igamt-hl7Tools-service	master
Standard Webhooks v2a	standard-webhooks/standard-webhooks	main"

# The first 40-hex commit in the second cell of the matrix row whose first cell
# is $1.
matrix_commit() {
  local row_label="$1"
  awk -F'|' -v want="$row_label" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      if (label == want && match(value, /[0-9a-f]{40}/)) { print substr(value, RSTART, RLENGTH); exit }
    }' "$MATRIX"
}

while IFS=$'\t' read -r label repo branch; do
  [[ -n "$label" ]] || continue

  pinned="$(matrix_commit "$label")"
  if [[ -z "$pinned" ]]; then
    printf 'UNREADABLE %s: no commit pin in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! head="$(gh api "repos/$repo/commits/$branch" --jq '.sha' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the head of %s %s (%s)\n' "$label" "$repo" "$branch" "$head"
    unreadable=1
    continue
  fi

  if [[ "$pinned" = "$head" ]]; then
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
# corpora the eehrxf crate reads are read by their package rows (#1335). A
# guide no registry lists at its pinned version is read at its canonical
# site's `package-list.json`, the source its pin is fetched from (#2029).
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
HL7 France FR Core	https://packages.fhir.org/hl7.fhir.fr.core
hl7.eu.fhir.xshare-yb	https://hl7.eu/fhir/ig/xshare-yb/package-list.json"


while IFS=$'\t' read -r label url; do
  [[ -n "$label" ]] || continue

  pinned="$(matrix_pin "$label")"
  if [[ -z "$pinned" ]]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  reader=registry_newest
  [[ "$url" != */package-list.json ]] || reader=package_list_current
  if ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$url" | "$reader" 2>&1)"; then
    printf 'UNREADABLE %s: could not read the latest version at %s (%s)\n' "$label" "$url" "$latest"
    unreadable=1
    continue
  fi

  held="$(matrix_cell "$label" | cell_held)"
  if [[ "$pinned" = "$latest" ]]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$url"
  elif [[ -n "$held" ]]; then
    printf 'HELD       %s: pinned %s, the registry names %s latest, held: %s (%s)\n' "$label" "$pinned" "$latest" "$held" "$url"
  else
    printf 'STALE      %s: pinned %s, the registry names %s latest (%s)\n' "$label" "$pinned" "$latest" "$url"
    stale=1
  fi
done <<< "$WATCHED_PACKAGES"

pinned="$(matrix_pin "$CHROME_LABEL")"
if [[ -z "$pinned" ]]; then
  printf 'UNREADABLE %s: no pin row in %s\n' "$CHROME_LABEL" "$MATRIX"
  unreadable=1
elif ! latest="$(curl --proto '=https' --tlsv1.2 -fsSL "$CHROME_FEED" | chrome_stable 2>&1)"; then
  printf 'UNREADABLE %s: could not read the newest Stable release from %s (%s)\n' \
    "$CHROME_LABEL" "$CHROME_FEED" "$latest"
  unreadable=1
else
  verdict "$CHROME_LABEL" "$pinned" "$latest" "$CHROME_FEED"
  [[ "$pinned" = "$latest" ]] || stale=1
fi

[[ "$unreadable" -eq 0 ]] || exit 2
exit "$stale"
