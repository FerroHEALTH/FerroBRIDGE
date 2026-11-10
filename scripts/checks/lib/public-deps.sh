# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# shellcheck shell=bash
# The public dependencies of a published crate and the compatibility line each
# is required on, shared by crate-version-guard.sh, semver.sh and
# public-surface.sh (#1777, #1851).
#
# Moving a public dependency to an incompatible version is a breaking change.
# The Cargo Book, "SemVer Compatibility"
# (<https://doc.rust-lang.org/cargo/reference/semver.html>), has no entry for
# it; its intro counts as breaking the "changes that will cause cargo and rustc
# to fail to build something that previously worked", and a type of the
# dependency in the crate's API becomes another type on the new line. The
# cargo-semver-checks README (v0.51.0) lists "breaking type changes, for
# example in the type of a field or function parameter" among what the judge
# does not catch, so the two scripts compare these lines themselves.
#
# A public dependency is one of the families whose types the crates of this
# line expose (`openehr-*`, `fhir-types`, `hl7v2-types`, `ferrohealth-*`), a
# sibling of this workspace, or a third-party crate the member manifest
# declares in the `public-dependencies` array of its
# `[package.metadata.ferrohealth]` table. Every family and sibling requirement
# counts, optional ones included, since the rule assumes a crate exposes them
# all, our own design. The declaration names the third-party crates whose
# items the crate's public API names, and scripts/checks/public-surface.sh
# derives that set from the rustdoc JSON of every crate and refuses a
# declaration that differs from it.

# The names in the `public-dependencies` array of the
# `[package.metadata.ferrohealth]` table of the manifest on stdin, one per
# line. The array may span lines; a `#` comment is left out.
declared_public() {
  awk '
    /^[[:space:]]*\[/ {
      table = ($0 ~ /^[[:space:]]*\[package\.metadata\.ferrohealth\][[:space:]]*(#.*)?$/)
      on = 0
      next
    }
    table && /^[[:space:]]*public-dependencies[[:space:]]*=/ { on = 1; sub(/^[^=]*=/, "") }
    on {
      line = $0
      sub(/#.*/, "", line)
      while (match(line, /"[^"]*"/)) {
        print substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      }
      if (line ~ /\]/) on = 0
    }
  '
}

# Whether crate $1 is a public dependency of the crates of this line, given
# the declared third-party names $2, one per line.
public_dependency() {
  case "$1" in
    openehr-* | fhir-types | hl7v2-types | ferrohealth-*) return 0 ;;
  esac
  [[ -f "crates/$1/Cargo.toml" ]] || grep -qxF -- "$1" <<<"${2-}"
}

# The compatibility line of requirement $1, as Cargo reads a caret or an exact
# requirement: the major, `0.Y` while the major is 0, and `0.0.Z` while the
# minor is 0 too.
compat_line() {
  local version="${1#[=^~]}" major minor patch
  IFS=. read -r major minor patch <<<"$version"
  if [[ "$major" != "0" ]]; then
    echo "$major"
  elif [[ "${minor:-0}" != "0" ]]; then
    echo "0.$minor"
  else
    echo "0.0.${patch:-0}"
  fi
}

# The `name requirement` pairs of the normal dependencies (target-specific
# ones included) of the member manifest $2, a `workspace = true` entry read
# from the `[workspace.dependencies]` table of the root manifest $1. An entry
# without a version (path-only) is left out.
manifest_requirements() {
  awk '
    function requirement(line, s) {
      if (match(line, /version[[:space:]]*=[[:space:]]*"[^"]*"/) ||
          match(line, /^[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*"[^"]*"/)) {
        s = substr(line, RSTART, RLENGTH)
        sub(/^[^"]*"/, "", s)
        sub(/"$/, "", s)
        return s
      }
      return ""
    }
    FNR == 1 { file++ }
    /^\[/ { table = $0; next }
    !match($0, /^[A-Za-z0-9_-]+/) { next }
    { name = substr($0, 1, RLENGTH) }
    file == 1 && table == "[workspace.dependencies]" { root[name] = requirement($0) }
    file == 2 && (table == "[dependencies]" || table ~ /^\[target\..*\.dependencies\]$/) {
      if ($0 ~ /^[A-Za-z0-9_-]+\.workspace[[:space:]]*=[[:space:]]*true/ ||
          $0 ~ /^[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*\{.*workspace[[:space:]]*=[[:space:]]*true/) {
        used[name] = root[name]
      } else {
        used[name] = requirement($0)
      }
    }
    END { for (n in used) if (used[n] != "") print n, used[n] }
  ' "$1" "$2"
}

# The sorted `name line` pairs of the public dependencies among the
# `name requirement` pairs on stdin, given the declared third-party names $1.
public_lines() {
  local declared="${1-}" name requirement
  while read -r name requirement; do
    public_dependency "$name" "$declared" || continue
    echo "$name $(compat_line "$requirement")"
  done | LC_ALL=C sort
}

# The public dependencies both `name line` lists $1 (before) and $2 (after)
# name with a different line, as `name before->after` words. A dependency
# added or removed is left out: an added one breaks nothing, and a removed
# one leaves items cargo-semver-checks sees go.
moved_lines() {
  LC_ALL=C join <(printf '%s\n' "$1") <(printf '%s\n' "$2") |
    awk '$2 != $3 { printf "%s%s %s->%s", sep, $1, $2, $3; sep = ", " }'
}
