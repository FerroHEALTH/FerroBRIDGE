#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/gh/watch-prs.sh: watches pull requests until each is merged.
#
# Usage: scripts/gh/watch-prs.sh <pr> [<pr>...]   (run it in the background)
#
# - BEHIND: updates the branch.
# - DIRTY in CHANGELOG.md or only in version lines: merges origin/main
#   (both changelog sides, the branch's higher versions, a fresh Cargo.lock,
#   the crate bump guard passing), pushes, and keeps watching.
# - DIRTY elsewhere, or a red required check with nothing pending: stops
#   (exit 2 or 3) so the session fixes it. Advisory analyzers never stop it.
set -u
if [ $# -eq 0 ]; then
  echo "usage: scripts/gh/watch-prs.sh <pr> [<pr>...]" >&2
  exit 64
fi
repo=$(git rev-parse --show-toplevel) || exit 1
# A conflict this script resolves on its own: CHANGELOG.md keeps both sides,
# and a version line (a crate manifest, the root manifest, the pin tables)
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
    git merge --no-edit origin/main >/dev/null 2>&1
    for f in $(git diff --name-only --diff-filter=U); do
      case $f in
        CHANGELOG.md)
          perl -0pi -e 's/<<<<<<< HEAD\n(.*?)=======\n(.*?)>>>>>>> origin\/main\n/$1$2/gs' "$f" ;;
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
    case $ms in
      BEHIND) gh pr update-branch "$p" >/dev/null 2>&1 && echo "updated #$p" ;;
      DIRTY)
        if resolve_merge "$p"; then echo "resolved merge #$p"; next+=("$p"); continue
        else echo "CONFLICT #$p"; exit 2; fi ;;
    esac
    c=$(gh pr checks "$p" 2>&1)
    pend=$(printf '%s\n' "$c" | awk -F'\t' '$2=="pending"' | wc -l | tr -d ' ')
    fails=$(printf '%s\n' "$c" | awk -F'\t' '$2=="fail" && $1!="conclusion" && $1!="analyze" && $1!="CodeQL" && $1!="Analyze (rust)" {print $1}' | tr '\n' ' ')
    if [ "$pend" = 0 ] && [ -n "$fails" ]; then echo "RED #$p: $fails"; exit 3; fi
    next+=("$p")
  done
  left=("${next[@]+"${next[@]}"}")
  [ ${#left[@]} -gt 0 ] && sleep 60
done
echo "ALL MERGED"
