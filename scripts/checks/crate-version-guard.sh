#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crate bump rule (.claude/rules/crates-publishing.md): a change that alters
# the PACKAGED content of a `crates/*` member (what its `include` ships:
# `src/**`, `README.md`, `LICENSE`, `Cargo.toml`) bumps THAT member's version in
# the same change, because a published version is immutable. A root
# `[workspace.dependencies]` entry a member consumes is packaged content too:
# `cargo package` renders the concrete requirement.
#
# Versions are per crate here, not lockstep: the members on the crate line move
# one patch per change and every other member still holds its name at the 0.0.0
# placeholder (docs/VERSIONS.md).
#
#   crate-version-guard.sh <base-ref> [head-ref]
#   crate-version-guard.sh --self-test
#
# The diff runs from the merge base of the base ref and the head, never from
# the base ref itself, so a crate change the base gained after the head forked
# is not counted against the head (#969). CI passes the current tip of the
# pull request's target branch as the base ref.
#
# The head ref may be the literal WORKTREE, which compares the base with the
# tree as it stands rather than with a commit. That is what the pre-commit hook
# passes: the manifests and Cargo.lock are read from disk either way, so the
# changed set has to come from the same place to agree with them.
#
# Exit 0 when no packaged content changed, or every member whose packaged
# content changed also moved its version, with the root requirement and
# Cargo.lock following. Exit 1 otherwise. The `no-crate-bump` pull-request label
# is the CI escape for a diff that provably does not alter packaged bytes; this
# script does not read labels. `--self-test` runs the guard over synthetic
# histories in a scratch repository.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Each case builds a one-crate workspace in a scratch repository, forks a
# branch from it, moves main on, and runs a copy of this script there.
self_test() {
  local guard tmp failures=0
  guard="$PWD/scripts/checks/crate-version-guard.sh"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  t_git() { git -C "$tmp/repo" -c user.name=self-test -c user.email=self-test@example.invalid -c commit.gpgsign=false "$@"; }
  set_version() {
    printf '[package]\nname = "demo"\nversion = "%s"\n' "$1" > "$tmp/repo/crates/demo/Cargo.toml"
    printf '[[package]]\nname = "demo"\nversion = "%s"\n' "$1" > "$tmp/repo/Cargo.lock"
  }
  expect() {
    local want="$1" label="$2" got=0
    shift 2
    (cd "$tmp/repo" && bash scripts/checks/crate-version-guard.sh "$@" > /dev/null 2>&1) || got=1
    if [[ "$got" -ne "$want" ]]; then
      echo "crate-version-guard: self-test: $label: exit $got, wanted $want" >&2
      failures=$((failures + 1))
    fi
  }

  mkdir -p "$tmp/repo/crates/demo/src" "$tmp/repo/scripts/checks" "$tmp/repo/docs"
  git init -q -b main "$tmp/repo"
  cp "$guard" "$tmp/repo/scripts/checks/"
  printf '[workspace]\nmembers = ["crates/demo"]\n' > "$tmp/repo/Cargo.toml"
  echo 'pub fn a() {}' > "$tmp/repo/crates/demo/src/lib.rs"
  set_version 0.1.0
  t_git add -A
  t_git commit -qm base
  t_git branch pr

  # Main gains a bumped crate change after the branch forked.
  echo 'pub fn b() {}' >> "$tmp/repo/crates/demo/src/lib.rs"
  set_version 0.1.1
  t_git commit -qam 'main moves'

  # The #963 case: the branch touches no crate, and CI checks out the merge of
  # the branch into main while naming the branch head.
  t_git checkout -q pr
  echo 'notes' > "$tmp/repo/docs/notes.md"
  t_git add -A
  t_git commit -qm 'docs only'
  expect 0 "the branch head alone" main pr
  expect 0 "the same, against the worktree" main WORKTREE
  t_git checkout -q --detach main
  t_git merge -q --no-ff -m 'pull request merge' pr > /dev/null 2>&1
  expect 0 "a crate change main gained after the fork" main pr
  t_git checkout -q pr

  # The branch with main merged in, as watch-prs merges it, before and after
  # the merge commit.
  t_git merge -q --no-commit --no-ff main > /dev/null 2>&1
  expect 0 "an uncommitted merge of main" main WORKTREE
  t_git commit -qm merge
  expect 0 "a committed merge of main" main HEAD

  # The guard still bites: the branch changes the crate without a bump.
  echo 'pub fn c() {}' >> "$tmp/repo/crates/demo/src/lib.rs"
  t_git commit -qam 'unbumped change'
  expect 1 "a crate change without a bump" main HEAD
  set_version 0.1.2
  t_git commit -qam bump
  expect 0 "a crate change with its bump" main HEAD

  if [[ "$failures" -ne 0 ]]; then
    echo "crate-version-guard: self-test: $failures case(s) failed" >&2
    return 1
  fi
  echo "crate-version-guard: self-test passed"
}

if [[ "${1:-}" = "--self-test" ]]; then
  self_test
  exit
fi

base_ref="${1:?usage: crate-version-guard.sh <base-ref> [head-ref|WORKTREE] | --self-test}"
head="${2:-HEAD}"

# During an uncommitted merge the worktree already holds MERGE_HEAD, so the
# merge base is taken against both parents.
head_commits=("$head")
if [[ "$head" = "WORKTREE" ]]; then
  head_commits=(HEAD)
  if merge_head="$(git rev-parse -q --verify MERGE_HEAD)"; then head_commits+=("$merge_head"); fi
fi
base="$(git merge-base "$base_ref" "${head_commits[@]}")" || {
  echo "::error::crate-version-guard: no merge base between $base_ref and ${head_commits[*]}." >&2
  exit 1
}

# `git diff <base> -- …` with no second ref reads the working tree, which is
# the WORKTREE head; everything else names two commits.
changed_paths() {
  if [[ "$head" = "WORKTREE" ]]; then git diff --name-only "$base" --; else git diff --name-only "$base" "$head" --; fi
}
diff_text() {
  if [[ "$head" = "WORKTREE" ]]; then git diff "$base" -- "$@"; else git diff "$base" "$head" -- "$@"; fi
}
head_file() {
  if [[ "$head" = "WORKTREE" ]]; then cat "$1"; else git show "$head:$1"; fi
}

changed="$(changed_paths)"

# Only the `[workspace.dependencies]` table renders into a packaged manifest;
# the `[workspace.package]` keys (the product `version`, edition, licence) are
# read through `key.workspace = true` and change no crate's packaged bytes.
workspace_dependencies() {
  awk '/^\[workspace\.dependencies\]/{p=1; next} /^\[/{p=0} p && /^[A-Za-z0-9_-]+[[:space:]]*=/{sub(/[[:space:]]*=.*/, ""); print}'
}

# The workspace dependency names this change touched, restricted to names the
# table actually declares on either side.
touched_dependencies=""
if grep -qx 'Cargo.toml' <<<"$changed"; then
  diff_names="$(diff_text Cargo.toml |
    grep -E '^[+-][A-Za-z0-9_-]+[[:space:]]*=' |
    sed -E 's/^[+-]//; s/[[:space:]]*=.*//' | sort -u || true)"
  dependency_names="$( { head_file Cargo.toml; git show "$base:Cargo.toml"; } | workspace_dependencies)"
  for name in $diff_names; do
    grep -qx "$name" <<<"$dependency_names" || continue
    touched_dependencies="$touched_dependencies $name"
  done
fi

package_field() {
  awk -F'"' -v key="$1" '/^\[package\]/{p=1} p && $0 ~ "^" key " = " {print $2; exit}'
}

fail=0
bumped=""
clean=""
reserved=""
for manifest in crates/*/Cargo.toml; do
  crate="$(dirname "$manifest")"
  name="$(package_field name < "$manifest")"

  # A here-string, not a pipe: `grep -q` closes its input on the first match,
  # and under `pipefail` the SIGPIPE'd writer would fail the whole test.
  packaged=0
  # Everything a manifest's `include` can ship: src, the schemas a crate embeds,
  # the README, the licence and the manifest itself.
  if grep -qE "^${crate}/(src/|schemas/|README\.md$|LICENSE$|Cargo\.toml$)" <<<"$changed"; then
    packaged=1
  fi
  if [[ "$packaged" -eq 0 ]]; then
    for dep in $touched_dependencies; do
      if grep -qE "^${dep}(\.workspace)?[[:space:]]*=" "$manifest"; then
        echo "crate-version-guard: $name consumes workspace dependency '$dep', which this change moved."
        packaged=1
        break
      fi
    done
  fi
  if [[ "$packaged" -eq 0 ]]; then
    clean="$clean $name"
    continue
  fi

  new_ver="$(package_field version < "$manifest")"
  old_ver="$(git show "$base:$manifest" 2>/dev/null | package_field version || true)"
  # A 0.0.0 manifest is a crates.io name reservation outside the crate line
  # (docs/VERSIONS.md); its content changes until the first real version.
  if [[ "$new_ver" = "0.0.0" ]] && [[ "${old_ver:-0.0.0}" = "0.0.0" ]]; then
    reserved="$reserved $name"
    continue
  fi
  if [[ -n "$old_ver" ]] && [[ "$old_ver" = "$new_ver" ]]; then
    echo "::error::packaged content of $name changed but its version is still $new_ver. Bump $manifest, move any internal requirement in the root Cargo.toml with it, refresh Cargo.lock, or apply the 'no-crate-bump' label when the diff provably does not alter packaged bytes." >&2
    fail=1
    continue
  fi

  # An internal requirement only exists for a crate other members depend on;
  # when it does, it must name the same version.
  req="$(grep -E "^${name} = \{ path = \"${crate}\", version = \"" Cargo.toml |
    sed -E 's/.*version = "([^"]+)".*/\1/' || true)"
  if [[ -n "$req" ]] && [[ "$req" != "$new_ver" ]]; then
    echo "::error::root Cargo.toml requires $name $req, its manifest is at $new_ver." >&2
    fail=1
  fi
  locked="$(awk -v n="\"$name\"" '$1 == "name" && $3 == n { hit = 1; next } hit && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
  if [[ "$locked" != "$new_ver" ]]; then
    echo "::error::Cargo.lock records $name ${locked:-nothing}, its manifest is at $new_ver; run cargo update -w and commit the lock." >&2
    fail=1
  fi
  bumped="$bumped $name@${old_ver:-<new>}->$new_ver"
done

[[ "$fail" -eq 0 ]] || exit 1
if [[ -z "$bumped" ]]; then
  echo "crate-version-guard: no packaged content of crates/* changed."
else
  echo "crate-version-guard: packaged content changed and the version moved:$bumped"
fi
[[ -z "$clean" ]] || echo "crate-version-guard: unchanged packaged content:$clean"
[[ -z "$reserved" ]] || echo "crate-version-guard: 0.0.0 name reservations, outside the line:$reserved"
