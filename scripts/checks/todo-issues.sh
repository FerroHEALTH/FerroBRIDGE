#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Stale pending-work guard (#614). A `TODO(#N):` comment and the issue a
# not-yet-running subcommand names (`Command::pending_issue` in the server's
# cli.rs) both promise that issue N carries the missing work. When N closes
# without it, the pointer lies and nothing else notices. This guard collects
# every such number from the tracked tree (the vendored corpora under
# docs/specs excluded) and fails when the tracker reports one closed or
# missing.
#
#   scripts/checks/todo-issues.sh
#
# It needs a `gh` that can read the repository's issues: locally your login,
# in CI the job's GITHUB_TOKEN with `issues: read`. When `gh` is absent,
# unauthenticated or offline, the guard says so and exits 0, so it never
# blocks offline work; CI is where it gates.
#
# Exit 0 = every pointer names an open issue (or the guard skipped),
# 1 = a pointer names a closed or unknown issue.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly PENDING_SOURCE=app/ferrobridge-server/src/cli.rs

# One "number<TAB>file:line" record per pointer.
pointers() {
  git grep -nE 'TODO\(#[0-9]+\)' -- . ':!docs/specs' |
    while IFS= read -r hit; do
      local location=${hit%%:*}
      local rest=${hit#*:}
      location="$location:${rest%%:*}"
      grep -oE 'TODO\(#[0-9]+\)' <<< "$hit" | tr -dc '0-9\n' |
        while IFS= read -r number; do
          printf '%s\t%s\n' "$number" "$location"
        done
    done
  if [ -f "$PENDING_SOURCE" ]; then
    # The arms of `pending_issue`, from its signature to the closing brace of
    # the function at the impl's indentation.
    awk -v file="$PENDING_SOURCE" '
      /fn pending_issue\(/ { inside = 1; next }
      inside && /^    }/ { inside = 0 }
      inside {
        line = $0
        while (match(line, /Some\([0-9]+\)/)) {
          printf "%s\t%s:%d\n", substr(line, RSTART + 5, RLENGTH - 6), file, NR
          line = substr(line, RSTART + RLENGTH)
        }
      }
    ' "$PENDING_SOURCE"
  fi
}

if ! command -v gh > /dev/null 2>&1; then
  echo "todo-issues: skipped: gh is not installed, so the tracker cannot be read."
  exit 0
fi
if ! gh repo view --json name > /dev/null 2>&1; then
  echo "todo-issues: skipped: gh cannot read the repository (unauthenticated or offline)."
  exit 0
fi

records=$(pointers | sort -t $'\t' -k1,1n -k2,2)
if [ -z "$records" ]; then
  echo "todo-issues: no pending-work pointers in the tree."
  exit 0
fi

fail=0
checked=0
while IFS= read -r number; do
  checked=$((checked + 1))
  if ! state=$(gh issue view "$number" --json state --jq .state 2> /dev/null < /dev/null); then
    state=UNKNOWN
  fi
  [ "$state" = OPEN ] && continue
  fail=1
  while IFS=$'\t' read -r n location; do
    [ "$n" = "$number" ] || continue
    if [ "$state" = CLOSED ]; then
      echo "::error file=${location%%:*},line=${location##*:}::$location names #$number, which is closed: point it at the open issue that carries the work, or remove it."
    else
      echo "::error file=${location%%:*},line=${location##*:}::$location names #$number, which the tracker does not answer as an open issue."
    fi
  done <<< "$records"
done < <(cut -f1 <<< "$records" | sort -un)

if [ "$fail" -eq 0 ]; then
  echo "todo-issues: $checked issue(s) named by pending-work pointers, all open."
fi
exit "$fail"
