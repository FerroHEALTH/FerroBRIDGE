#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Release-window guard: while the newest `## [X.Y.Z]` section of CHANGELOG.md
# has no pushed tag vX.Y.Z, changelog.d/ holds no fragment.
#
# The window opens when the release pull request assembles the fragments into
# the version's section and closes when vX.Y.Z is pushed. A fragment that
# reaches a tree inside the window would sit unassembled on the commit to be
# tagged, which the release plan refuses. So the guard fails the release pull
# request whose merge group carries a fragment that landed after its assembly
# (merge main in and assemble again), and every fragment-bearing pull request
# merged while the cut is untagged (it waits for the tag). A pull request
# without a fragment passes either way. No specification governs this: our
# own design (#1652).
#
# A tag missing locally is asked of origin; a failure to ask fails the guard.
#
# Usage:
#   scripts/checks/release-window.sh
#   scripts/checks/release-window.sh --self-test
#
# Exit 0 = the window is closed, or open with no fragment. Exit 1 = a
# fragment sits in an open window, or the tags could not be read.
set -euo pipefail

# The fragments under changelog.d/ of the tree at $1, one per line.
fragments() {
  local root="$1"
  find "$root/changelog.d" -maxdepth 1 -type f ! -name README.md 2>/dev/null | sort
}

# The version of the newest released section of $1/CHANGELOG.md, or nothing.
newest() {
  local root="$1"
  sed -nE 's/^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*/\1/p' "$root/CHANGELOG.md" | head -n1
}

# 0 when tag $2 exists in the repository at $1 or at its origin, 1 when origin
# has none, 2 when origin cannot be asked.
tagged() {
  local root="$1" tag="$2" rc=0
  git -C "$root" rev-parse -q --verify "refs/tags/$tag" >/dev/null && return 0
  git -C "$root" ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 || rc=$?
  case "$rc" in
    0) return 0 ;;
    2) return 1 ;;
    *) return 2 ;;
  esac
}

# Runs the guard over the repository at $1.
guard() {
  local root="$1" version tag rc=0 left
  version="$(newest "$root")"
  if [[ -z "$version" ]]; then
    echo "release-window: no released section, so no window is open"
    return 0
  fi
  tag="v$version"
  tagged "$root" "$tag" || rc=$?
  case "$rc" in
    0)
      echo "release-window: [$version] is tagged $tag, so the window is closed"
      return 0
      ;;
    1) ;;
    *)
      echo "release-window: cannot ask origin whether $tag exists" >&2
      return 1
      ;;
  esac
  left="$(fragments "$root")"
  if [[ -z "$left" ]]; then
    echo "release-window: the $tag cut is untagged and changelog.d/ holds no fragment"
    return 0
  fi
  {
    echo "release-window: the $tag cut is assembled and not tagged yet, so the release window is open, and changelog.d/ holds:"
    printf '  %s\n' "${left//$root\//}"
    echo "A fragment waits until $tag is pushed; the release pull request merges main in and runs scripts/release/changelog.sh --assemble again (docs/release.md, the release window)."
  } >&2
  return 1
}

self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  # git writes its objects read-only, so the tree is made writable before it
  # is removed.
  trap 'chmod -R u+w "$work" && rm -r -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  stub_git() {
    git -C "$work/clone" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c tag.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  # changelog VERSION: a CHANGELOG.md whose newest section is VERSION.
  changelog() {
    printf '# Changelog\n\n## [Unreleased]\n\n## [%s] - 2026-11-01\n\n### Added\n\n- A release.\n' \
      "$1" >"$work/clone/CHANGELOG.md"
  }
  commit() {
    stub_git add -A
    stub_git commit -q -m "$1"
  }
  expect() {
    local name="$1" want="$2"
    status=0
    (cd "$work/clone" && bash "$script" >"$work/out" 2>&1) || status=$?
    if [[ "$status" != "$want" ]]; then
      echo "release-window: self-test failed: $name exited $status, wanted $want." >&2
      cat "$work/out" >&2
      exit 1
    fi
    echo "  ok: $name"
  }
  git init -q --bare "$work/origin.git"
  git clone -q "$work/origin.git" "$work/clone" 2>/dev/null
  mkdir -p "$work/clone/changelog.d" "$work/clone/scripts/checks"
  echo '# Changelog fragments' >"$work/clone/changelog.d/README.md"

  changelog 1.0.0
  commit "release: v1.0.0"
  stub_git tag v1.0.0
  stub_git push -q origin HEAD:refs/heads/main refs/tags/v1.0.0
  echo '- A change.' >"$work/clone/changelog.d/2-change.added.md"
  commit "feat: a change after the tag"
  expect "a fragment after a pushed tag passes" 0

  stub_git rm -q "changelog.d/2-change.added.md"
  changelog 1.1.0
  commit "release: v1.1.0, the fragments assembled"
  stub_git push -q origin HEAD:refs/heads/main
  expect "an assembled, untagged cut with no fragment passes" 0

  echo '- A change that lands between the cut and the tag.' >"$work/clone/changelog.d/3-late.added.md"
  commit "feat: a fragment between the cut and the tag"
  expect "a fragment between the cut and the tag is refused" 1

  stub_git tag v1.1.0 HEAD~1
  stub_git push -q origin refs/tags/v1.1.0
  stub_git tag -d v1.1.0 >/dev/null
  expect "a tag pushed to origin and missing locally closes the window" 0

  git -C "$work/clone" remote set-url origin "$work/missing.git"
  changelog 1.2.0
  commit "release: v1.2.0"
  expect "an origin that cannot be asked fails" 1
  echo "release-window: self-test OK"
}

case "${1:-}" in
  --self-test) self_test ;;
  "") guard "$(git rev-parse --show-toplevel)" ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
