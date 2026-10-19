#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/gh/watch-prs.sh: watches pull requests until each is merged.
#
# Usage: scripts/gh/watch-prs.sh <pr> [<pr>...]   (run it in the background)
#
# - BEHIND: updates the branch.
# - DIRTY in CHANGELOG.md or only in version lines: merges origin/main
#   (the branch's changelog entries by scripts/gh/changelog-merge.sh, the
#   branch's higher versions, a fresh Cargo.lock, the crate bump guard
#   passing), pushes, and keeps watching.
# - A clean merge that would file the branch's changelog entries under a
#   released version (a release cut after the branch): merges the same way.
# - Open and not queued: arms auto-merge again, since a push to the branch
#   (an update or a resolved merge) drops the arming.
# - DIRTY elsewhere, or a red required check with nothing pending: stops
#   (exit 2 or 3) so the session fixes it. Advisory analyzers never stop it.
set -u
if [ $# -eq 0 ]; then
  echo "usage: scripts/gh/watch-prs.sh <pr> [<pr>...]" >&2
  exit 64
fi
repo=$(git rev-parse --show-toplevel) || exit 1
merge_changelog="$(cd "$(dirname "$0")" && pwd)/changelog-merge.sh"
# A conflict this script resolves on its own: CHANGELOG.md takes the branch's
# entries into main's [Unreleased] whatever main released meanwhile, and a
# version line (a crate manifest, the root manifest, the pin tables)
# keeps the branch's side when the two sides differ only in their version
# numbers. Cargo.lock is regenerated, and the crate bump guard must pass.
resolve_merge() {
  local p=$1 b tmp
  b=$(gh pr view "$p" --json headRefName --jq .headRefName)
  tmp=$(mktemp -d)
  git -C "$repo" fetch -q origin || return 1
  git -C "$repo" worktree add -q "$tmp" "origin/$b" 2>/dev/null || return 1
  (
    cd "$tmp" || exit 1
    git merge --no-commit --no-ff origin/main >/dev/null 2>&1
    mb=$(git merge-base HEAD origin/main) || { git merge --abort; exit 1; }
    if ! git diff --quiet "$mb" HEAD -- CHANGELOG.md; then
      "$merge_changelog" <(git show "$mb:CHANGELOG.md") <(git show HEAD:CHANGELOG.md) \
        <(git show origin/main:CHANGELOG.md) >CHANGELOG.md.merged ||
        { rm -f CHANGELOG.md.merged; git merge --abort 2>/dev/null; exit 1; }
      mv CHANGELOG.md.merged CHANGELOG.md
      git add CHANGELOG.md
    fi
    for f in $(git diff --name-only --diff-filter=U); do
      case $f in
        Cargo.lock)
          git checkout --theirs Cargo.lock ;;
        Cargo.toml | crates/*/Cargo.toml | docs/VERSIONS.md | docs/architecture.md)
          perl -0pi -e 's{<<<<<<< HEAD\n(.*?)=======\n(.*?)>>>>>>> origin/main\n}{ (my $o = $1) =~ s/\d+\.\d+\.\d+/V/g; (my $t = $2) =~ s/\d+\.\d+\.\d+/V/g; $o eq $t ? $1 : $& }gse' "$f" ;;
        *) git merge --abort; exit 1 ;;
      esac
      if grep -q '^<<<<<<<\|^>>>>>>>' "$f"; then git merge --abort; exit 1; fi
    done
    cargo update -w -q || { git merge --abort; exit 1; }
    git add -A
    scripts/checks/crate-version-guard.sh origin/main WORKTREE >/dev/null 2>&1 || { git merge --abort; exit 1; }
    git commit -q --no-edit && git push -q origin "HEAD:$b"
  )
  local rc=$?
  git -C "$repo" worktree remove --force "$tmp" >/dev/null 2>&1
  return $rc
}
# Whether a clean merge with main would change main's released sections: the
# branch's changelog entries would land under a version cut after it.
changelog_leaks() {
  local p=$1 b tree
  gh pr view "$p" --json files --jq '.files[].path' | grep -qx CHANGELOG.md || return 1
  b=$(gh pr view "$p" --json headRefName --jq .headRefName)
  git -C "$repo" fetch -q origin main "$b" || return 1
  # A branch that already holds main has nothing left to merge.
  git -C "$repo" merge-base --is-ancestor origin/main "origin/$b" && return 1
  tree=$(git -C "$repo" merge-tree --write-tree origin/main "origin/$b" 2>/dev/null | head -n1) || return 1
  ! cmp -s <("$merge_changelog" --released <(git -C "$repo" show "$tree:CHANGELOG.md")) \
    <("$merge_changelog" --released <(git -C "$repo" show origin/main:CHANGELOG.md))
}
left=("$@")
# A run started by a fresh push takes a moment to register; until it does,
# gh pr checks still shows the previous run.
sleep 90
while [ ${#left[@]} -gt 0 ]; do
  next=()
  for p in "${left[@]}"; do
    s=$(gh pr view "$p" --json state,mergeStateStatus --jq '.state + " " + .mergeStateStatus')
    st=${s% *}
    ms=${s#* }
    if [ "$st" = MERGED ]; then echo "MERGED #$p"; continue; fi
    if [ "$st" = CLOSED ]; then echo "CLOSED #$p"; continue; fi
    # A push drops the auto-merge arming; re-arming a queued PR is refused
    # harmlessly, so this runs on every pass.
    gh pr merge "$p" --auto >/dev/null 2>&1 || true
    case $ms in
      BEHIND) gh pr update-branch "$p" >/dev/null 2>&1 && echo "updated #$p" ;;
      DIRTY)
        if resolve_merge "$p"; then echo "resolved merge #$p"; next+=("$p"); continue
        else echo "CONFLICT #$p"; exit 2; fi ;;
      *)
        if changelog_leaks "$p"; then
          if resolve_merge "$p"; then echo "kept the changelog entries of #$p unreleased"; next+=("$p"); continue
          else echo "CONFLICT #$p: its changelog entries would land under a released version"; exit 2; fi
        fi ;;
    esac
    c=$(gh pr checks "$p" 2>&1)
    pend=$(printf '%s\n' "$c" | awk -F'\t' '$2=="pending"' | wc -l | tr -d ' ')
    fails=$(printf '%s\n' "$c" | awk -F'\t' '$2=="fail" && $1!="conclusion" && $1!="analyze" && $1!="CodeQL" && $1!="Analyze (rust)" && $1!="SonarCloud Code Analysis" {print $1}' | tr '\n' ' ')
    if [ "$pend" = 0 ] && [ -n "$fails" ]; then echo "RED #$p: $fails"; exit 3; fi
    next+=("$p")
  done
  left=("${next[@]+"${next[@]}"}")
  [ ${#left[@]} -gt 0 ] && sleep 60
done
echo "ALL MERGED"
