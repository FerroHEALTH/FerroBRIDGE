#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Released-section guard: every `## [X.Y.Z]` section of CHANGELOG.md reads
# exactly as it does in CHANGELOG.md at tag vX.Y.Z.
#
# A release's notes are its changelog section at the tag, and the published
# release is immutable, so a section that changes after its cut makes the
# changelog disagree with what shipped. The usual cause is a merge that files
# a later entry under the version cut before it (#903 under [0.0.9]). A
# section whose tag does not exist yet (the bump pull request, before the tag
# is pushed) is skipped with a note. Sections cut before v0.0.9 predate this
# guard and are not compared. No specification governs this: our own design.
#
# A tag missing from a shallow checkout is fetched from origin; a tag origin
# has and the fetch cannot read fails the guard.
#
# Usage:
#   scripts/checks/changelog-released.sh
#
# Exit 0 = every tagged section agrees. Exit 1 = a section drifted.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly FIRST_CHECKED=0.0.9

# The section of version $2 in file $1: its heading through the line before
# the next version heading or the first link reference.
section() {
  awk -v v="$2" 'BEGIN { h = "## [" v "]" }
    index($0, h) == 1 { p = 1; print; next }
    p && (/^## \[/ || /^\[[^]]+\]: /) { exit }
    p' "$1"
}

# 0 when version $1 is at or after FIRST_CHECKED.
checked() {
  [ "$(printf '%s\n%s\n' "$FIRST_CHECKED" "$1" | sort -V | head -n1)" = "$FIRST_CHECKED" ]
}

fail=0
tagged=0
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
while IFS= read -r v; do
  checked "$v" || continue
  tag="v$v"
  if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    rc=0
    git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 || rc=$?
    if [ "$rc" = 2 ]; then
      echo "  $tag: not tagged yet, skipped"
      continue
    elif [ "$rc" != 0 ]; then
      echo "  DRIFT: cannot ask origin for $tag (git ls-remote exit $rc)" >&2
      fail=1
      continue
    fi
    if ! git fetch -q --depth=1 origin "refs/tags/$tag:refs/tags/$tag"; then
      echo "  DRIFT: origin has $tag but the fetch failed" >&2
      fail=1
      continue
    fi
  fi
  git show "$tag:CHANGELOG.md" >"$work/tagged"
  if diff -u --label "$tag:CHANGELOG.md" --label CHANGELOG.md \
    <(section "$work/tagged" "$v") <(section CHANGELOG.md "$v") >"$work/diff"; then
    echo "  OK: [$v] reads as it does at $tag"
    tagged=$((tagged + 1))
  else
    echo "  DRIFT: [$v] differs from its text at $tag:" >&2
    cat "$work/diff" >&2
    fail=1
  fi
done < <(sed -nE 's/^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*/\1/p' CHANGELOG.md)

if [ "$fail" != 0 ]; then
  echo "changelog-released: a released section changed after its tag; move the entry to [Unreleased]" >&2
  exit 1
fi
echo "changelog-released: OK ($tagged tagged section(s) agree)"
