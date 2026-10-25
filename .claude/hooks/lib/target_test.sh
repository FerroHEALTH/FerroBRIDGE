#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/lib/target_test.sh
#
# Feeds sample tool calls to the hooks that act on this repository and checks
# each judges only a checkout of this repository (lib/target.sh): a commit
# or an edit in another repository passes untouched, one here (or in a
# worktree of here) is judged. The guards each hook runs are stand-ins that
# always fail, so "judged" reads as exit 2.
#
# Usage: bash .claude/hooks/lib/target_test.sh
set -euo pipefail

hooks="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

repo() {
  git init -q "$1"
  git -C "$1" -c user.email=t@example.invalid -c user.name=t commit -q --allow-empty -m base
  git -C "$1" update-ref refs/remotes/origin/main HEAD
}
repo "$work/ours"
repo "$work/theirs"
mkdir -p "$work/ours/crates" "$work/ours/scripts/checks"
for guard in crate-version-guard.sh versions.sh comment-style.sh; do
  printf '#!/usr/bin/env bash\necho "%s ran"\nexit 1\n' "$guard" >"$work/ours/scripts/checks/$guard"
  chmod +x "$work/ours/scripts/checks/$guard"
done
git -C "$work/ours" worktree add -q "$work/ours-wt" 2>/dev/null
mkdir -p "$work/ours-wt/crates" "$work/ours-wt/scripts/checks"
cp "$work/ours/scripts/checks/"*.sh "$work/ours-wt/scripts/checks/"
for tree in ours ours-wt theirs; do
  touch "$work/$tree/README.md" "$work/$tree/lib.rs"
done

failures=0
# expect <hook> <want-exit> <payload>
expect() {
  local got=0
  CLAUDE_PROJECT_DIR="$work/ours" bash "$hooks/$1" <<<"$3" >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$2" ]; then
    echo "FAIL: $1 exited $got, want $2, for: $3" >&2
    failures=$((failures + 1))
  fi
}
bash_call() { jq -cn --arg c "$1" --arg d "$2" '{cwd: $d, tool_input: {command: $c}}'; }
edit_call() { jq -cn --arg f "$1" '{tool_input: {file_path: $f}}'; }

g=crate_version_bump_guard.sh
expect "$g" 2 "$(bash_call 'git commit -m x' "$work/ours")"
expect "$g" 0 "$(bash_call 'git commit -m x' "$work/theirs")"
expect "$g" 0 "$(bash_call "git -C $work/theirs commit -m x" "$work/ours")"
expect "$g" 0 "$(bash_call "cd $work/theirs && git commit -m x" "$work/ours")"
expect "$g" 0 "$(bash_call "R=$work/theirs; git -C \$R push origin x" "$work/ours")"
expect "$g" 2 "$(bash_call "git -C \"$work/ours\" commit -m x" "$work/theirs")"
expect "$g" 2 "$(bash_call "git -C $work/ours-wt commit -m x" "$work/theirs")"
# shellcheck disable=SC2016 # the variable is the command's, unset on purpose
expect "$g" 0 "$(bash_call 'git -C $NOWHERE_SET commit -m x' "$work/ours")"
expect "$g" 0 "$(bash_call 'git status' "$work/ours")"

# The token is assembled so this file does not carry it.
token="Gene""rated with Cla""ude Co""de"
expect no_attribution_guard.sh 2 "$(bash_call "git -C $work/theirs commit -m 'x $token'" "$work/ours")"
expect no_attribution_guard.sh 0 "$(bash_call "git -C $work/theirs commit -m 'x'" "$work/ours")"

expect versions_guard.sh 2 "$(edit_call "$work/ours/README.md")"
expect versions_guard.sh 2 "$(edit_call "$work/ours-wt/README.md")"
expect versions_guard.sh 0 "$(edit_call "$work/theirs/README.md")"

expect rust_fmt_clippy.sh 2 "$(edit_call "$work/ours/lib.rs")"
expect rust_fmt_clippy.sh 0 "$(edit_call "$work/theirs/lib.rs")"

if [ "$failures" -ne 0 ]; then
  echo "target_test: $failures case(s) failed" >&2
  exit 1
fi
echo "target_test: OK"
