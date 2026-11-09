#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The SemVer judge of the published crates (.claude/rules/crates-publishing.md,
# #1745): cargo-semver-checks compares a crate's public API with a baseline and
# refuses a breaking change that the version does not answer with a minor bump
# while the crate is 0.x (0.1.7 to 0.2.0), the Cargo SemVer rules of
# https://doc.rust-lang.org/cargo/reference/semver.html. A minor bump with no
# breaking change is refused too, so a compatible change moves the patch and
# one caret requirement keeps every consumer on one release. No specification
# governs where the judge runs: our own design. crate-version-guard.sh runs it
# on a pull request and in the merge queue against the target branch, and the
# release plan runs it against crates.io over the crates the release
# publishes (scripts/release/publish-crates.sh semver).
#
#   semver.sh [--baseline-rev <rev>] [<crate>...]
#       Judge the named crates, or every crate past 0.0.0, against <rev> or,
#       without it, against each crate's newest release on crates.io. A crate
#       at its baseline's version, or new since it, is left out.
#   semver.sh --pin
#       Print the cargo-semver-checks version docs/VERSIONS.md pins.
#
# The installed cargo-semver-checks must be the pinned version. Exit 0 when
# every crate's version answers its change, 1 when one does not, 2 on a usage
# or setup error.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

usage() {
  echo "usage: semver.sh [--baseline-rev <rev>] [<crate>...] | --pin" >&2
  exit 2
}

# The Pin cell of the cargo-semver-checks row, its backticks stripped.
pin="$(awk -F'|' '{ k = $2; gsub(/[` ]/, "", k) } k == "cargo-semver-checks" { v = $3; gsub(/[^0-9.]/, "", v); print v; exit }' docs/VERSIONS.md)"
if [[ -z "$pin" ]]; then
  echo "semver: docs/VERSIONS.md pins no cargo-semver-checks version" >&2
  exit 2
fi
if [[ "${1:-}" == "--pin" ]]; then
  [[ "$#" -eq 1 ]] || usage
  echo "$pin"
  exit 0
fi

baseline_rev=""
if [[ "${1:-}" == "--baseline-rev" ]]; then
  [[ "$#" -ge 2 ]] || usage
  baseline_rev="$2"
  shift 2
fi

if ! installed="$(cargo semver-checks --version 2>/dev/null)"; then
  echo "semver: cargo-semver-checks is not installed; install $pin (cargo binstall cargo-semver-checks@$pin)" >&2
  exit 2
fi
if [[ "$installed" != "cargo-semver-checks $pin" ]]; then
  echo "semver: found '$installed', docs/VERSIONS.md pins $pin" >&2
  exit 2
fi

# The `[package]` table's own `version` in the manifest on stdin.
package_version() {
  awk -F'"' '/^\[package\]/{p=1; next} /^\[/{p=0} p && /^version[[:space:]]*=/{print $2; exit}'
}

# The version crate $1 has at the baseline: its manifest at the baseline
# revision, or its newest stable release on crates.io; empty when it has none.
baseline_version() {
  local crate="$1"
  if [[ -n "$baseline_rev" ]]; then
    git show "$baseline_rev:crates/$crate/Cargo.toml" 2>/dev/null | package_version || true
  else
    curl --proto '=https' --tlsv1.2 -sSL --fail \
      -H 'User-Agent: ferrobridge-semver (scripts/checks/semver.sh)' \
      "https://crates.io/api/v1/crates/$crate" 2>/dev/null |
      jq -r '.crate.max_stable_version // empty' || true
  fi
}

# Whether $1 and $2 share their compatibility line: the major, and the minor
# too while the major is 0 (the Cargo caret rule).
same_line() {
  local a="$1" b="$2"
  if [[ "${a%%.*}" == "0" ]]; then
    [[ "$(cut -d. -f1-2 <<<"$a")" == "$(cut -d. -f1-2 <<<"$b")" ]]
  else
    [[ "${a%%.*}" == "${b%%.*}" ]]
  fi
}

crates=("$@")
if [[ "${#crates[@]}" -eq 0 ]]; then
  for manifest in crates/*/Cargo.toml; do
    crates+=("$(basename "$(dirname "$manifest")")")
  done
fi

log="$(mktemp)"
trap 'rm -f "$log"' EXIT
against="${baseline_rev:-crates.io}"
failed=""
for crate in "${crates[@]}"; do
  [[ -f "crates/$crate/Cargo.toml" ]] || { echo "semver: no crate crates/$crate" >&2; exit 2; }
  current="$(package_version < "crates/$crate/Cargo.toml")"
  if [[ "$current" == "0.0.0" ]]; then
    echo "semver: $crate is at 0.0.0, outside the line"
    continue
  fi
  previous="$(baseline_version "$crate")"
  if [[ -z "$previous" ]] || [[ "$previous" == "0.0.0" ]]; then
    echo "semver: $crate has no release at $against to compare with"
    continue
  fi
  if [[ "$previous" == "$current" ]]; then
    echo "semver: $crate is at $current at $against too, nothing to judge"
    continue
  fi
  echo "semver: $crate $previous -> $current against $against"
  # The array is never empty, so bash 3.2 under `set -u` expands it.
  args=(check-release --package "$crate")
  [[ -z "$baseline_rev" ]] || args+=(--baseline-rev "$baseline_rev")
  # The judge enables every feature by default; fhir-types with `resources`
  # does not fit a hosted runner, so it is judged on its default features,
  # every FHIR version's terminology surface.
  [[ "$crate" != "fhir-types" ]] || args+=(--default-features)
  status=0
  cargo semver-checks "${args[@]}" > "$log" 2>&1 || status=$?
  cat "$log"
  # Only a summary the judge printed counts as a judgement; a build that
  # fails is named apart from a break.
  if [[ "$status" -ne 0 ]] && grep -q 'semver requires new' "$log"; then
    echo "::error::semver: $crate $current breaks its public API against $previous; a breaking change bumps the minor while the crate is 0.x (.claude/rules/crates-publishing.md)." >&2
    failed="$failed $crate"
    continue
  fi
  if [[ "$status" -ne 0 ]] || ! grep -q 'Summary' "$log"; then
    echo "::error::semver: cargo-semver-checks could not judge $crate $current against $previous (exit $status); see its output above." >&2
    failed="$failed $crate"
    continue
  fi
  # The version left the line, so the change must break: judged as a patch,
  # it has to fail.
  if ! same_line "$previous" "$current" &&
    cargo semver-checks "${args[@]}" --release-type patch > /dev/null 2>&1; then
    echo "::error::semver: $crate moves from $previous to $current with no breaking change; a compatible change bumps the patch (.claude/rules/crates-publishing.md)." >&2
    failed="$failed $crate"
  fi
done

if [[ -n "$failed" ]]; then
  echo "semver: refused:$failed" >&2
  exit 1
fi
echo "semver: every judged version answers its change"
