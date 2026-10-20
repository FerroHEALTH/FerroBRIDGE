#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/block_dangerous.sh
#
# Claude Code PreToolUse hook (matcher: Bash). Blocks destructive commands:
#   - a recursive forced delete (delete specific files, use git rm, or work
#     under /tmp)
#   - a force push whose target ref is main or master, and a force push that
#     names no conventional-type branch
#   - deletion of LICENSE or CLAUDE.md (the licence and the working discipline)
#
# Reads the tool-call JSON on stdin. Exit 2 blocks; exit 0 allows.
#
#   .claude/hooks/block_dangerous.sh --self-test   # run the built-in cases
#
# The force-push rule reads only an actual `git push` invocation: the command
# is split into simple commands at ; & | and newlines, and only one whose
# words are `git [global options] push …` is checked, by its own arguments.
# A protected name elsewhere (a sed string, an issue body) is never read.

set -euo pipefail

# The conventional branch types a force push may name.
readonly BRANCH_TYPES='feat|fix|chore|docs|refactor|perf|test|ci|build|release'

# Prints the reason and returns 2 when the simple command $1 is a force push
# that targets main or master or names no conventional-type branch.
check_push() {
  local -a words
  local i=0 word force=0 named=0 ref dst
  read -r -a words <<< "$1" || true
  # Leading environment assignments and a subshell parenthesis.
  while [ "$i" -lt "${#words[@]}" ] && [[ "${words[$i]}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; do
    i=$((i + 1))
  done
  [ "$i" -lt "${#words[@]}" ] || return 0
  word="${words[$i]#(}"
  [ "$word" = "git" ] || return 0
  i=$((i + 1))
  # Global options before the subcommand; -C and -c take a value.
  while [ "$i" -lt "${#words[@]}" ] && [[ "${words[$i]}" == -* ]]; do
    case "${words[$i]}" in
      -C | -c) i=$((i + 2)) ;;
      *) i=$((i + 1)) ;;
    esac
  done
  [ "$i" -lt "${#words[@]}" ] && [ "${words[$i]}" = "push" ] || return 0
  i=$((i + 1))

  local -a refs=()
  for word in "${words[@]:$i}"; do
    word="${word//\"/}"
    word="${word//\'/}"
    word="${word%)}"
    case "$word" in
      --force | -f | --force-if-includes) force=1 ;;
      --force-with-lease) force=1 ;;
      --force-with-lease=*)
        force=1
        ref="${word#--force-with-lease=}"
        refs+=("${ref%%:*}")
        ;;
      -*) ;;
      +*)
        force=1
        refs+=("${word#+}")
        ;;
      *) refs+=("$word") ;;
    esac
  done
  [ "$force" -eq 1 ] || return 0

  for ref in "${refs[@]+"${refs[@]}"}"; do
    dst="${ref##*:}"
    dst="${dst#refs/heads/}"
    case "$dst" in
      main | master)
        echo "BLOCKED: force-push touching main or master is forbidden (CLAUDE.md hard rule)."
        return 2
        ;;
    esac
    [[ "$dst" =~ ^($BRANCH_TYPES)/ ]] && named=1
  done
  if [ "$named" -eq 0 ]; then
    echo "BLOCKED: bare force-push refused. Force-push (prefer --force-with-lease) only an explicit conventional-type branch (feat/, fix/, chore/, docs/, refactor/, perf/, test/, ci/, build/, release/)."
    return 2
  fi
  return 0
}

# Prints the reason and returns 2 when the command $1 must be blocked.
verdict() {
  local cmd="$1" segment status
  # A delete carrying both -r and -f (combined or separate flags), unless it
  # is scoped under /tmp.
  if printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])rm[[:space:]]+-[a-zA-Z]*([rR][a-zA-Z]*f|f[a-zA-Z]*[rR])' ||
    printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])rm[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*-[a-zA-Z]*[rR][a-zA-Z]*([[:space:]]+-[a-zA-Z]+)*[[:space:]]+-[a-zA-Z]*f'; then
    if ! printf '%s' "$cmd" | grep -qE 'rm[[:space:]]+-[a-zA-Z]+[[:space:]]+"?(/private)?/tmp/'; then
      echo "BLOCKED: a recursive forced delete is not allowed (block_dangerous hook). Delete specific files with 'git rm' or a plain 'rm <file>', or operate under /tmp."
      return 2
    fi
  fi

  while IFS= read -r segment; do
    status=0
    check_push "$segment" || status=$?
    [ "$status" -eq 0 ] || return "$status"
  done < <(printf '%s\n' "$cmd" | tr ';&|' '\n')

  # Never delete the licence or the working discipline.
  if printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])(git[[:space:]]+rm|rm)[^;|&]*(LICENSE|CLAUDE\.md)([[:space:]]|$|["'"'"';&|])'; then
    echo "BLOCKED: LICENSE (the Business Source License 1.1, the project licence) and CLAUDE.md (the working discipline) must not be deleted."
    return 2
  fi
  return 0
}

# Runs every case: a command and the exit status the hook must give it.
self_test() {
  local failed=0 count=0 expected cmd got
  while IFS=$'\t' read -r expected cmd; do
    [ -n "$expected" ] || continue
    count=$((count + 1))
    got=0
    verdict "$cmd" > /dev/null || got=$?
    if [ "$got" != "$expected" ]; then
      echo "block_dangerous: FAIL: expected $expected, got $got: $cmd" >&2
      failed=1
    fi
  done << 'CASES'
2	git push --force origin main
2	git push --force-with-lease origin main
2	git push -f origin master
2	git push origin +main
2	git push --force-with-lease origin feat/x:main
2	git push --force-with-lease origin refs/heads/main
2	cd /repo && git push --force-with-lease origin main
2	git -C /repo push --force origin HEAD:main
2	git push --force-with-lease=main origin feat/x:main
2	git push --force
2	git push --force-with-lease origin some-branch
2	rm -rf build
2	rm -r -f build
2	git rm LICENSE
2	rm CLAUDE.md
0	git push --force-with-lease origin feat/maintenance
0	git push --force-with-lease origin fix/main-menu
0	git push --force-with-lease origin feat/x && sed -i 's/main/trunk/' notes.txt
0	gh issue create --title "force push" --body "never git push --force-with-lease origin main"
0	sed -n '/--force-with-lease/p' notes.md; echo main
0	git push origin main
0	git push -u origin feat/x
0	git log --oneline main
0	rm -rf /tmp/scratch
0	rm notes.txt
CASES
  if [ "$failed" -ne 0 ]; then
    exit 1
  fi
  echo "block_dangerous: self-test OK ($count cases)."
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit 0
fi

payload="$(cat)"

if command -v jq > /dev/null 2>&1; then
  cmd="$(printf '%s' "$payload" | jq -r '.tool_input.command // empty' 2> /dev/null || true)"
else
  cmd="$payload"
fi
[ -n "${cmd:-}" ] || exit 0

status=0
reason="$(verdict "$cmd")" || status=$?
if [ "$status" -ne 0 ]; then
  echo "$reason" >&2
  exit 2
fi
exit 0
