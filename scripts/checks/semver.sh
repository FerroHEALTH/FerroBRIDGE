#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The SemVer judge of the published crates (.claude/rules/crates-publishing.md,
# #1745, #1830): cargo-semver-checks compares a crate's public API with a
# baseline and refuses a breaking change that the version does not answer with
# a minor bump while the crate is 0.x (0.1.7 to 0.2.0). The rule is The Cargo
# Book, "SemVer Compatibility"
# (https://doc.rust-lang.org/cargo/reference/semver.html), alone: a 0.y.z
# release "can treat changes in "y" as a major release, and "z" as a minor
# release". A public dependency (scripts/checks/lib/public-deps.sh) moved to
# another compatibility line breaks the crate where the judge cannot see it,
# so it needs the minor (#1777). No specification governs where the judge
# runs: our own design. crate-version-guard.sh runs it on a pull request and
# in the merge queue against the target branch, and the release plan runs it
# against crates.io over the crates the release publishes
# (scripts/release/publish-crates.sh semver).
#
# Each crate is judged on more than one feature set, because the
# cargo-semver-checks README (v0.51.0) says it does not catch "breaking
# changes that exist when only a subset of all crate features are activated":
# the judge's own default (every feature but the unstable-named ones), and,
# for a crate with features, --only-explicit-features, what a dependant built
# with default-features = false compiles against. fhir-types is judged on its
# default set, on no feature, and on each FHIR version module with the
# terminology set and with its own resources set (<version>-resources), which
# peaks near 1.3 GB and runs in under a minute on ubuntu-latest (#1834). Every
# resource sits behind its release's resources set or the all-release
# `resources` feature, so the five sets hold every item `resources` enables.
# ferrobridge-term refuses to build with no release feature, so in place of
# no feature it is judged on each release alone, the least a dependant with
# default-features = false can build (#1840).
#
#   semver.sh [--baseline-rev <rev>] [<crate>...]
#       Judge the named crates, or every crate past 0.0.0, against <rev> or,
#       without it, against each crate's newest release on crates.io. A crate
#       at its baseline's version, or new since it, is left out.
#   semver.sh --pin
#       Print the cargo-semver-checks version docs/VERSIONS.md pins.
#   semver.sh --self-test
#       Run the public dependency verdict over fixtures, with no judge and no
#       network.
#
# The installed cargo-semver-checks must be the pinned version. Exit 0 when
# every crate's version answers its change, 1 when one does not, 2 on a usage
# or setup error.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. scripts/checks/lib/public-deps.sh

usage() {
  echo "usage: semver.sh [--baseline-rev <rev>] [<crate>...] | --pin | --self-test" >&2
  exit 2
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

# The third-party crates crate $1 declares public in the tree and, when $2
# names a revision, at that revision too, one per line (#1851).
declared_of() {
  {
    declared_public < "crates/$1/Cargo.toml"
    [[ -z "${2:-}" ]] || git show "$2:crates/$1/Cargo.toml" 2>/dev/null | declared_public
  } | LC_ALL=C sort -u
}

# The public dependency lines (scripts/checks/lib/public-deps.sh) of crate $1
# in the tree, given the declared third-party names $2.
tree_lines() {
  manifest_requirements Cargo.toml "crates/$1/Cargo.toml" | public_lines "$2"
}

# The public dependency lines of the crates.io dependencies document on stdin,
# given the declared third-party names $1.
document_lines() {
  jq -r '.dependencies[] | select(.kind == "normal") | "\(.crate_id) \(.req)"' | public_lines "$1"
}

# The verdict on crate $1 moving from $2 to $3 while the public dependencies
# `moved` ($4) left their lines: the move breaks its consumers, so it needs
# another line of its own.
dependency_verdict() {
  local crate="$1" previous="$2" current="$3" moved="$4"
  [[ -n "$moved" ]] || return 0
  if same_line "$previous" "$current"; then
    echo "::error::semver: $crate moves from $previous to $current while a public dependency moves to another compatibility line ($moved); that breaks its consumers, so it bumps the minor (.claude/rules/crates-publishing.md)." >&2
    return 1
  fi
  echo "semver: $crate leaves its line for its public dependencies ($moved)"
}

# The verdict over a scratch tree and a crates.io document, with no judge and
# no network: a declared serde_json and jiff count as family crates do (#1851).
self_test() {
  local tmp failures=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  mkdir -p "$tmp/crates/demo"
  printf '{"dependencies": [
    {"crate_id": "openehr-rm", "req": "^0.0.135", "kind": "normal"},
    {"crate_id": "serde_json", "req": "^1.0.0", "kind": "normal"},
    {"crate_id": "jiff", "req": "^0.2.0", "kind": "normal"},
    {"crate_id": "regex", "req": "^1.0.0", "kind": "normal"},
    {"crate_id": "tokio", "req": "^1.0.0", "kind": "dev"}]}' > "$tmp/release.json"
  # The tree requiring openehr-rm $1, serde_json $2, jiff $3 and regex $4,
  # declaring $5 public.
  tree() {
    printf '[workspace]\nmembers = ["crates/demo"]\n\n[workspace.dependencies]\nopenehr-rm = "%s"\nserde_json = "%s"\njiff = { version = "%s", default-features = false }\nregex = "%s"\n' \
      "$1" "$2" "$3" "$4" > "$tmp/Cargo.toml"
    printf '[package]\nname = "demo"\nversion = "0.1.4"\n\n[package.metadata.ferrohealth]\npublic-dependencies = [%s]\n\n[dependencies]\nopenehr-rm.workspace = true\nserde_json.workspace = true\njiff.workspace = true\nregex.workspace = true\n' \
      "$5" > "$tmp/crates/demo/Cargo.toml"
  }
  expect() {
    local want="$1" label="$2" got=0
    (
      cd "$tmp"
      declared="$(declared_of demo)"
      dependency_verdict demo "$4" "$3" \
        "$(moved_lines "$(document_lines "$declared" < release.json)" "$(tree_lines demo "$declared")")"
    ) > /dev/null 2>&1 || got=1
    if [[ "$got" -ne "$want" ]]; then
      echo "semver: self-test: $label: exit $got, wanted $want" >&2
      failures=$((failures + 1))
    fi
  }

  tree 0.0.135 1.0.9 0.2.5 1.11.0 '"serde_json", "jiff"'
  expect 0 "patch steps on every line" 0.1.4 0.1.3
  tree 0.0.135 1.0.0 0.2.0 2.0.0 '"serde_json", "jiff"'
  expect 0 "an undeclared regex on another line with a patch step" 0.1.4 0.1.3
  tree 0.0.136 1.0.0 0.2.0 1.0.0 '"serde_json", "jiff"'
  expect 1 "a family dependency on another line with a patch step" 0.1.4 0.1.3
  tree 0.0.135 2.0.0 0.2.0 1.0.0 '"serde_json", "jiff"'
  expect 1 "a declared serde_json on another line with a patch step" 0.1.4 0.1.3
  expect 0 "a declared serde_json on another line with the next minor" 0.2.0 0.1.3
  tree 0.0.135 1.0.0 0.3.0 1.0.0 '"serde_json", "jiff"'
  expect 1 "a declared jiff on another line with a patch step" 0.1.4 0.1.3
  expect 0 "a declared jiff on another line with a minor that skips one" 0.3.0 0.1.3
  tree 0.0.135 1.0.0 0.3.0 1.0.0 '"serde_json"'
  expect 0 "an undeclared jiff on another line with a patch step" 0.1.4 0.1.3

  if [[ "$failures" -ne 0 ]]; then
    echo "semver: self-test: $failures case(s) failed" >&2
    return 1
  fi
  echo "semver: self-test passed"
}

if [[ "${1:-}" == "--self-test" ]]; then
  [[ "$#" -eq 1 ]] || usage
  self_test
  exit
fi

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

# The crates.io API document at path $1 under /api/v1/crates/.
crates_io() {
  curl --proto '=https' --tlsv1.2 -sSL --fail \
    -H 'User-Agent: ferrobridge-semver (scripts/checks/semver.sh)' \
    "https://crates.io/api/v1/crates/$1" 2>/dev/null
}

# The newest stable release of crate $1 on crates.io; empty when it has none.
newest_release() {
  crates_io "$1" | jq -r '.crate.max_stable_version // empty' || true
}

# The version crate $1 has at the baseline: its manifest at the baseline
# revision, or its newest stable release on crates.io; empty when it has none.
baseline_version() {
  local crate="$1"
  if [[ -n "$baseline_rev" ]]; then
    git show "$baseline_rev:crates/$crate/Cargo.toml" 2>/dev/null | package_version || true
  else
    newest_release "$crate"
  fi
}

revision_lines() {
  manifest_requirements <(git show "$2:Cargo.toml") <(git show "$2:crates/$1/Cargo.toml") |
    public_lines "$3"
}
release_lines() {
  crates_io "$1/$2/dependencies" | document_lines "$3"
}

# The public dependencies of crate $1 that moved to another compatibility line
# since version $2 at the baseline (#1777). Against a revision, the newest
# release on crates.io counts too while it shares the line of $2: a move that
# landed under an unpublished patch still breaks the consumers of that release.
# A third-party crate the tree or the baseline declares counts on both (#1851).
moved_since() {
  local crate="$1" previous="$2" current published moved="" declared
  declared="$(declared_of "$crate" "$baseline_rev")"
  current="$(tree_lines "$crate" "$declared")"
  if [[ -n "$baseline_rev" ]]; then
    moved="$(moved_lines "$(revision_lines "$crate" "$baseline_rev" "$declared")" "$current")"
    published="$(newest_release "$crate")"
    if [[ -n "$published" ]] && ! same_line "$published" "$previous"; then published=""; fi
  else
    published="$previous"
  fi
  if [[ -z "$moved" ]] && [[ -n "$published" ]]; then
    moved="$(moved_lines "$(release_lines "$crate" "$published" "$declared")" "$current")"
  fi
  echo "$moved"
}

# The feature selections crate $1 is judged on, one line of
# cargo-semver-checks flags each; an empty line is the judge's own default.
feature_sets() {
  if [[ "$1" == "fhir-types" ]]; then
    echo "--default-features"
    echo "--only-explicit-features"
    local version
    for version in stu3 r4 r4b r5 r6; do
      echo "--only-explicit-features --features $version,terminology"
      echo "--only-explicit-features --features $version-resources"
    done
  elif [[ "$1" == "ferrobridge-term" ]]; then
    echo ""
    local release
    for release in stu3 r4 r4b r5 r6; do
      echo "--only-explicit-features --features $release"
    done
  else
    echo ""
    # A crate with no features has one feature set.
    if grep -q '^\[features\]' "crates/$1/Cargo.toml"; then
      echo "--only-explicit-features"
    fi
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
    echo "semver: $crate is a 0.0.0 name reservation with no release, skipped"
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
  judged=true
  while IFS= read -r selection; do
    run=("${args[@]}")
    # The selection is a list of flags that splits on whitespace by design.
    # shellcheck disable=SC2206
    [[ -z "$selection" ]] || run+=($selection)
    label="${selection:-the default feature set}"
    echo "semver: $crate on $label"
    status=0
    cargo semver-checks "${run[@]}" < /dev/null > "$log" 2>&1 || status=$?
    cat "$log"
    # Only a summary the judge printed counts as a judgement; a build that
    # fails is named apart from a break.
    if [[ "$status" -ne 0 ]] && grep -q 'semver requires new' "$log"; then
      echo "::error::semver: $crate $current breaks its public API against $previous on $label; a breaking change bumps the minor while the crate is 0.x (.claude/rules/crates-publishing.md)." >&2
      judged=false
      break
    fi
    if [[ "$status" -ne 0 ]] || ! grep -q 'Summary' "$log"; then
      echo "::error::semver: cargo-semver-checks could not judge $crate $current against $previous on $label (exit $status); see its output above." >&2
      judged=false
      break
    fi
  done < <(feature_sets "$crate")
  if [[ "$judged" != true ]]; then
    failed="$failed $crate"
    continue
  fi
  # A public dependency on another line breaks the crate where the judge
  # cannot see it (#1777), so it needs the minor.
  dependency_verdict "$crate" "$previous" "$current" "$(moved_since "$crate" "$previous")" ||
    failed="$failed $crate"
done

if [[ -n "$failed" ]]; then
  echo "semver: refused:$failed" >&2
  exit 1
fi
echo "semver: every judged version answers its change"
