#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The release facts the `plan` job of .github/workflows/release.yml verifies
# at the tagged commit, as ONE ordered list (no specification governs the list
# or its order: our own design). The plan job runs it in CI and
# scripts/release/preflight.sh runs it on a release branch before the tag is
# pushed, so the two cannot check different things.
#
#   plan.sh checks <tag> [--notes <file>]
#       Runs every check below at the working tree, in order, and writes the
#       release notes to <file> (default release-notes.md). A failing check is
#       named and the list goes on, so one run reports every finding.
#       Exit 1 when any check failed.
#   plan.sh prev-tag <tag>
#       Prints the release tag before <tag> by version order, read from
#       `git ls-remote` of origin: a pushed tag counts whether or not its
#       release published, and ancestry plays no part.
#   plan.sh notes <tag> <file>
#       Writes the release notes for <tag>: its changelog section, the
#       support period and the link to the CRA user information in the book.
#   plan.sh support-end <YYYY-MM-DD>
#       Prints the month the support period of a release of that date ends.
#   plan.sh --self-test
#       Proves prev-tag's ordering, support-end's arithmetic and the notes
#       composition offline.
#
# The milestone check reads the tracker through gh (GH_TOKEN in CI, the
# logged-in gh locally). Exit 2 on a usage error.
set -euo pipefail

readonly TAG_RE='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

# The CRA Annex II user information, as a path under website/book/src and as
# the address it is published at: the book is served under /docs/
# (website/book/book.toml site-url) from main, not frozen per tag, so every
# release links the same page.
readonly CRA_PAGE='evaluate/cra-user-information.md'
readonly CRA_URL='https://ferrobridge.eu/docs/evaluate/cra-user-information.html'

usage() {
  echo "usage: plan.sh checks <tag> [--notes <file>] | prev-tag <tag> | notes <tag> <file> | support-end <YYYY-MM-DD> | --self-test" >&2
  exit 2
}

valid_tag() {
  [[ "$1" =~ $TAG_RE ]] || {
    echo "::error::plan: '$1' is not a vX.Y.Z tag" >&2
    exit 2
  }
}

# ── The previous release ─────────────────────────────────────────────────────

# order_prev TAG: reads tag names on stdin and prints the release tag directly
# below TAG's release (its pre-release suffix dropped) by version order.
order_prev() {
  local base="${1%%-*}"
  grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | grep -vxF "$base" |
    { cat; echo "$base"; } | sort -V | awk -v t="$base" '$0==t{print p; exit} {p=$0}'
}

prev_tag() {
  local prev
  prev="$(git ls-remote --tags --refs origin 'v*' | sed 's#.*refs/tags/##' | order_prev "$1")"
  [[ -n "$prev" ]] || {
    echo "::error::plan: no release tag below ${1%%-*}" >&2
    return 1
  }
  printf '%s\n' "$prev"
}

# ── The support period ───────────────────────────────────────────────────────

# support_end YYYY-MM-DD: the month and year five years after the date, as GNU
# `date -d "<date> +5 years" '+%B %Y'` gives it (a 29 February rolls into
# March), computed here so the preflight runs on BSD date too.
support_end() {
  local d="$1" y m day ey em
  local -a months=(January February March April May June July August September October November December)
  local -a mdays=(31 28 31 30 31 30 31 31 30 31 30 31)
  [[ "$d" =~ ^([0-9]{4})-([0-9]{2})-([0-9]{2})$ ]] || return 1
  y=$((10#${BASH_REMATCH[1]}))
  m=$((10#${BASH_REMATCH[2]}))
  day=$((10#${BASH_REMATCH[3]}))
  ((m >= 1 && m <= 12 && day >= 1)) || return 1
  leap() { (($1 % 4 == 0 && ($1 % 100 != 0 || $1 % 400 == 0))); }
  if ((m == 2)) && leap "$y"; then ((day <= 29)) || return 1; else ((day <= mdays[m - 1])) || return 1; fi
  ey=$((y + 5))
  em=$m
  if ((m == 2 && day == 29)) && ! leap "$ey"; then em=3; fi
  printf '%s %s\n' "${months[em - 1]}" "$ey"
}

# ── The release notes ────────────────────────────────────────────────────────

notes() {
  local version="${1#v}" out="$2" released end
  if ! grep -q "^## \[$version\]" CHANGELOG.md; then
    echo "::error::CHANGELOG.md has no '## [$version]' section. Cut the release before tagging: rename [Unreleased] to $version with its date." >&2
    return 1
  fi
  # Everything under the version heading, stopping at the next version
  # heading or at the trailing link-reference block, whichever comes first.
  awk -v ver="$version" '
    $0 ~ "^## \\[" ver "\\]" {found=1; next}
    found && /^## \[/ {exit}
    found && /^\[[^]]+\]:/ {exit}
    found {print}
  ' CHANGELOG.md >"$out"
  if ! grep -q '[^[:space:]]' "$out"; then
    echo "::error::The [$version] changelog section is empty." >&2
    return 1
  fi
  # CRA Art 13(19) and Annex II point 7: the end of the support commitment, at
  # least month and year, five years from the release date.
  released="$(sed -n "s/^## \[$version\] - \([0-9-]*\).*/\1/p" CHANGELOG.md | head -n1)"
  if ! end="$(support_end "$released")"; then
    echo "::error::The [$version] heading carries no release date to count the support period from." >&2
    return 1
  fi
  {
    echo
    echo "### Support period"
    echo
    echo "Only the newest release is supported. When a new release is published, every earlier release stops receiving fixes: there are no maintenance branches, no long-term-support line and no backports. If you are not on the newest release, upgrade; upgrading is free. Under the Cyber Resilience Act, Cadasto B.V. commits to keep publishing security fixes for at least five years after each release (Art 13(8)) and ships them only in the newest release (Art 13(10)). For this release that commitment runs until the end of $end (Regulation (EU) 2024/2847 Art 13(19)); see [SECURITY.md](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/SECURITY.md#supported-versions)."
  } >>"$out"
  # CRA Art 13(18): the Annex II information and instructions to the user. The
  # notes link the page only while the book builds it.
  if [[ ! -f "website/book/src/$CRA_PAGE" ]] || ! grep -qF "]($CRA_PAGE)" website/book/src/SUMMARY.md 2>/dev/null; then
    echo "::error::website/book/src/$CRA_PAGE is missing or website/book/src/SUMMARY.md does not list it, so the notes would link a page the book does not publish (Regulation (EU) 2024/2847 Art 13(18))." >&2
    return 1
  fi
  {
    echo
    echo "### Information and instructions to the user (CRA Annex II)"
    echo
    echo "[CRA information and instructions to the user for $1]($CRA_URL): the manufacturer, the vulnerability contact, the support period, secure use, updates and decommissioning (Regulation (EU) 2024/2847 Art 13(18) and Annex II)."
  } >>"$out"
}

# ── The checks, one function each, run in the order of `list` ───────────────

# A release is cut only when its milestone is empty
# (.claude/rules/issue-workflow.md).
check_milestone() { bash scripts/release/milestone.sh check "$TAG"; }

# Every file that declares the product version agrees with the tag: the root
# Cargo.toml, CITATION.cff and the product row of docs/VERSIONS.md.
check_version() {
  local required manifest cff matrix fail=0
  for required in Cargo.toml CITATION.cff docs/VERSIONS.md CHANGELOG.md compose.yaml; do
    if [[ ! -f "$required" ]]; then
      echo "::error::${required} is missing at ${TAG}, so this release cannot be cut." >&2
      return 1
    fi
  done
  declared() {
    if [[ -z "$2" ]]; then
      echo "::error::$1 declares no product version." >&2
      fail=1
    elif [[ "$2" != "$VERSION" ]]; then
      echo "::error::tag $TAG declares version $VERSION, but $1 says $2." >&2
      fail=1
    else
      echo "OK: $1 declares $VERSION"
    fi
  }
  manifest="$(awk '
    /^[[:space:]]*\[/ { h = $0; gsub(/[[:space:]]/, "", h); f = (h == "[workspace.package]"); next }
    f && $0 ~ /^[[:space:]]*version[[:space:]]*=/ {
      if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit }
    }
  ' Cargo.toml)"
  declared "the root Cargo.toml [workspace.package] version" "$manifest"
  cff="$(sed -nE 's/^version:[[:space:]]*//p' CITATION.cff | head -n1 | tr -d '"'\''[:space:]')"
  declared "CITATION.cff" "$cff"
  # The first token of the second cell of the row whose first cell is
  # "Product version", as scripts/checks/versions.sh reads it.
  matrix="$(awk -F'|' '
    NF >= 3 {
      k = $2; v = $3
      gsub(/`/, "", k); gsub(/`/, "", v)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == "Product version") { split(v, w, /[[:space:]]/); print w[1]; exit }
    }
  ' docs/VERSIONS.md)"
  declared "the docs/VERSIONS.md product-version row" "$matrix"
  [[ "$fail" -eq 0 ]]
}

check_changelog_structure() { bash scripts/checks/changelog-structure.sh; }

# CRA Annex I Part II(2), (4) and (8): the section being cut says why a
# security fix ships with functional changes and names every advisory.
check_changelog_security() {
  bash scripts/checks/changelog-security.sh --self-test
  bash scripts/checks/changelog-security.sh --section "$VERSION"
}

# The previous release, by version order from `git ls-remote`, named in the
# log so the cut reads what it is measured against.
check_prev_tag() {
  local prev
  prev="$(prev_tag "$TAG")"
  echo "the previous release of $TAG is $prev"
}

check_notes() {
  notes "$TAG" "$NOTES"
  echo "Release notes for $VERSION:"
  cat "$NOTES"
}

# The list: "<function> <title>", in the order the plan job runs them.
list() {
  cat <<'EOF'
check_milestone The milestone titled as the tag holds no open item
check_version The tag matches the declared product version
check_changelog_structure Changelog structure (no duplicate subsections, canonical types)
check_changelog_security The Security section of the cut names its advisories and its reason
check_prev_tag The previous release resolves from the pushed tags
check_notes The changelog section for this tag becomes the release notes, with the support period and the CRA user information link
EOF
}

checks() {
  TAG="$1"
  VERSION="${TAG#v}"
  NOTES="$2"
  local fn title rc
  local -a failed=()
  while read -r fn title; do
    if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then echo "::group::$title"; else echo "── $title"; fi
    set +e
    (
      set -e
      "$fn"
    )
    rc=$?
    set -e
    [[ "${GITHUB_ACTIONS:-}" == "true" ]] && echo "::endgroup::"
    if [[ "$rc" -ne 0 ]]; then
      echo "::error::plan: refused: $title" >&2
      failed+=("$title")
    fi
  done < <(list)
  if [[ "${#failed[@]}" -gt 0 ]]; then
    echo "plan: ${#failed[@]} check(s) refused $TAG:" >&2
    printf '  %s\n' "${failed[@]}" >&2
    return 1
  fi
  echo "plan: every release fact for $TAG verified at this tree"
}

# The notes composed in a scratch tree: the changelog section and the support
# period exactly as before, the CRA user information link after them, and a
# refusal when the book does not publish the page.
self_test_notes() {
  local tmp want
  tmp="$(mktemp -d)"
  mkdir -p "$tmp/website/book/src/evaluate"
  printf '%s\n' "# Changelog" "" "## [Unreleased]" "" "## [1.2.3] - 2026-10-23" "" "### Fixed" "" "- a fix" "" \
    "## [1.2.2] - 2026-10-01" "" "- older" "" "[1.2.3]: https://example.org" >"$tmp/CHANGELOG.md"
  printf '%s\n' "# Summary" "" "- [CRA information for users]($CRA_PAGE)" >"$tmp/website/book/src/SUMMARY.md"
  printf '%s\n' "# CRA information for users" >"$tmp/website/book/src/$CRA_PAGE"
  want="$(printf '%s\n' "" "### Fixed" "" "- a fix" "" "" "### Support period" "")"
  if ! (cd "$tmp" && notes v1.2.3 "$tmp/notes.md") 2>/dev/null; then
    expect "the notes compose" "refused" "composed"
    rm -rf "$tmp"
    return
  fi
  expect "the changelog section and the support period are unchanged" \
    "$(sed -n '1,/^### Support period$/p' "$tmp/notes.md"; echo)" "$want"
  expect "the support period names its end month" \
    "$(grep -c 'until the end of October 2031' "$tmp/notes.md")" "1"
  expect "the CRA section follows the support period" \
    "$(grep '^### ' "$tmp/notes.md" | tail -n2 | tr '\n' '|')" "### Support period|### Information and instructions to the user (CRA Annex II)|"
  expect "the notes end with the CRA user information link" \
    "$(tail -n1 "$tmp/notes.md" | grep -cF "[CRA information and instructions to the user for v1.2.3]($CRA_URL)")" "1"
  : >"$tmp/website/book/src/SUMMARY.md"
  expect "a page the book does not list is refused" \
    "$( (cd "$tmp" && notes v1.2.3 "$tmp/notes.md") 2>/dev/null && echo composed || echo refused)" "refused"
  printf '%s\n' "- [CRA information for users]($CRA_PAGE)" >"$tmp/website/book/src/SUMMARY.md"
  rm "$tmp/website/book/src/$CRA_PAGE"
  expect "a missing page is refused" \
    "$( (cd "$tmp" && notes v1.2.3 "$tmp/notes.md") 2>/dev/null && echo composed || echo refused)" "refused"
  rm -rf "$tmp"
}

self_test() {
  local fails=0 got
  expect() {
    if [[ "$2" != "$3" ]]; then
      echo "self-test: $1: got '$2', want '$3'" >&2
      fails=$((fails + 1))
    fi
  }
  got="$(printf '%s\n' v5.3.2 v5.10.0 v5.3.10 v4.9.9 v5.3.3-rc1 v5.4.0 | order_prev v5.4.0)"
  expect "version order, not lexical" "$got" "v5.3.10"
  got="$(printf '%s\n' v5.3.2 v5.3.3 v5.3.4 | order_prev v5.3.3)"
  expect "the tag itself is skipped" "$got" "v5.3.2"
  got="$(printf '%s\n' v5.3.2 v5.3.3 | order_prev v5.3.3-rc1)"
  expect "a pre-release compares as its release" "$got" "v5.3.2"
  got="$(printf '%s\n' v5.3.2 v5.3.3 | order_prev v5.4.0)"
  expect "an unpushed tag" "$got" "v5.3.3"
  got="$(printf '%s\n' v5.3.3 | order_prev v1.0.0)"
  expect "no tag below" "$got" ""
  expect "a plain date" "$(support_end 2026-10-23)" "October 2031"
  expect "a leap day into a common year" "$(support_end 2028-02-29)" "March 2033"
  expect "a leap day into a leap year" "$(support_end 2031-02-29 || echo refused)" "refused"
  expect "the last day of a month" "$(support_end 2026-12-31)" "December 2031"
  expect "an empty date" "$(support_end "" || echo refused)" "refused"
  expect "a month out of range" "$(support_end 2026-13-01 || echo refused)" "refused"
  expect "a day out of range" "$(support_end 2026-04-31 || echo refused)" "refused"
  self_test_notes
  if [[ "$fails" -gt 0 ]]; then
    echo "plan.sh self-test FAILED ($fails case(s))" >&2
    exit 1
  fi
  echo "plan.sh self-test OK"
}

main() {
  case "${1:-}" in
  checks)
    [[ "$#" -eq 2 || ("$#" -eq 4 && "$3" == "--notes" && -n "$4") ]] || usage
    valid_tag "$2"
    local notes_file="${4:-release-notes.md}"
    [[ "$notes_file" == /* ]] || notes_file="$PWD/$notes_file"
    cd "$(dirname "$0")/../.."
    checks "$2" "$notes_file"
    ;;
  prev-tag)
    [[ "$#" -eq 2 ]] || usage
    valid_tag "$2"
    prev_tag "$2"
    ;;
  notes)
    [[ "$#" -eq 3 ]] || usage
    valid_tag "$2"
    local out="$3"
    [[ "$out" == /* ]] || out="$PWD/$out"
    cd "$(dirname "$0")/../.."
    notes "$2" "$out"
    ;;
  support-end)
    [[ "$#" -eq 2 ]] || usage
    support_end "$2" || {
      echo "::error::plan: '$2' is not a YYYY-MM-DD date" >&2
      exit 1
    }
    ;;
  --self-test)
    [[ "$#" -eq 1 ]] || usage
    self_test
    ;;
  *) usage ;;
  esac
}

main "$@"
