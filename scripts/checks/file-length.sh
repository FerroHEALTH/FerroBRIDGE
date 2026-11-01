#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# File-length guard: a hand-written Rust file is at most $HARD lines, and a
# file over $SOFT lines is split into a module folder before it grows further.
#
# Generated files (a `@generated` banner on the first line) and vendored trees
# are outside the rule. A file listed in scripts/checks/file-length-allow.txt
# is a known breach with the issue that splits it: it passes while it does not
# grow past the line count recorded beside it. A file over $SOFT and under
# $HARD that is not listed is reported as a warning, never a failure.
#
# The list is a ratchet that only tightens (#1359). A listed file that shrank,
# fell to $SOFT lines or under, or is gone passes with a one-line note, so a
# pull request that splits a file leaves the list alone and conflicts with no
# other one; the scheduled ratchet-tighten workflow runs --update on main and
# opens the one pull request that lowers or removes the entries (#1363).
#
# Usage:
#   scripts/checks/file-length.sh              check every tracked .rs file
#   scripts/checks/file-length.sh --files F…   check the named files
#   scripts/checks/file-length.sh --update     lower each entry to its file's
#                                              length and drop the entries
#                                              back within $SOFT or gone; it
#                                              never raises or adds one
#   scripts/checks/file-length.sh --record --issue N
#                                              --update, and add each new
#                                              breach with issue N
#   scripts/checks/file-length.sh --self-test  a growth, a shrink, a split,
#                                              a gone file and --update
# Exit 1 on a hard breach or a listed file that grew; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

SOFT=750
HARD=1000
ALLOW=scripts/checks/file-length-allow.txt

note() { printf '  %s\n' "$*"; }

# Builds a scratch repository with one listed file and runs a copy of this
# script there; a listed file's length is set by the line count it is given.
self_test() {
  local guard tmp failures=0
  guard="$PWD/scripts/checks/file-length.sh"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  mkdir -p "$tmp/repo/scripts/checks" "$tmp/repo/src"
  cp "$guard" "$tmp/repo/scripts/checks/file-length.sh"
  git -C "$tmp/repo" init -q
  lines() { seq "$2" | sed 's/^/\/\/ /' >"$tmp/repo/src/$1"; }
  allow() {
    printf '# header\n' >"$tmp/repo/$ALLOW"
    [ "$#" -eq 0 ] || printf '%s\n' "$@" >>"$tmp/repo/$ALLOW"
  }
  track() { git -C "$tmp/repo" add -A >/dev/null; }
  expect() {
    local want="$1" label="$2" got=0
    shift 2
    (cd "$tmp/repo" && bash scripts/checks/file-length.sh "$@" >"$tmp/out" 2>&1) || got=$?
    if [ "$got" -ne "$want" ]; then
      echo "file-length: self-test: $label: exit $got, wanted $want" >&2
      sed 's/^/    /' "$tmp/out" >&2
      failures=$((failures + 1))
    fi
  }
  entry() { awk -v p="src/$1" '$1 !~ /^#/ && $2 == p { print $1 }' "$tmp/repo/$ALLOW"; }

  lines big.rs 900; allow "900 src/big.rs #1"; track
  expect 0 "a listed file at its count passes"
  lines big.rs 901; track
  expect 1 "a listed file that grew fails"
  expect 1 "--update refuses a listed file that grew" --update
  [ "$(entry big.rs)" = 900 ] || { echo "file-length: self-test: --update raised an entry" >&2; failures=$((failures + 1)); }
  expect 1 "--record refuses a listed file that grew" --record --issue 2
  [ "$(entry big.rs)" = 900 ] || { echo "file-length: self-test: --record raised an entry" >&2; failures=$((failures + 1)); }
  lines big.rs 850; track
  expect 0 "a listed file that shrank passes"
  grep -q 'note:' "$tmp/out" || { echo "file-length: self-test: a shrink passed with no note" >&2; failures=$((failures + 1)); }
  [ "$(entry big.rs)" = 900 ] || { echo "file-length: self-test: a check wrote the list" >&2; failures=$((failures + 1)); }
  expect 0 "--update lowers a shrunk entry" --update
  [ "$(entry big.rs)" = 850 ] || { echo "file-length: self-test: --update did not lower the entry to 850" >&2; failures=$((failures + 1)); }
  lines big.rs 700; track
  expect 0 "a listed file back within the soft limit passes"
  grep -q 'note:' "$tmp/out" || { echo "file-length: self-test: a split passed with no note" >&2; failures=$((failures + 1)); }
  expect 0 "--update drops an entry back within the limit" --update
  [ -z "$(entry big.rs)" ] || { echo "file-length: self-test: --update kept an entry within the limit" >&2; failures=$((failures + 1)); }
  allow "900 src/gone.rs #1"
  expect 0 "a listed file that is gone passes"
  expect 0 "--update drops the entry of a gone file" --update
  [ -z "$(entry gone.rs)" ] || { echo "file-length: self-test: --update kept the entry of a gone file" >&2; failures=$((failures + 1)); }
  lines new.rs 1100; allow; track
  expect 1 "an unlisted file over the hard limit fails"
  expect 2 "--update never adds a new breach" --update
  [ -z "$(entry new.rs)" ] || { echo "file-length: self-test: --update added a breach" >&2; failures=$((failures + 1)); }
  expect 0 "--record adds a new breach with its issue" --record --issue 7
  [ "$(entry new.rs)" = 1100 ] || { echo "file-length: self-test: --record did not add the breach" >&2; failures=$((failures + 1)); }

  if [ "$failures" -ne 0 ]; then
    echo "file-length: self-test: $failures case(s) failed" >&2
    return 1
  fi
  echo "file-length: self-test OK (a growth, a shrink, a split, a gone file, --update and --record)"
}

mode=check
issue=""
files=()
while [ $# -gt 0 ]; do
  case "$1" in
    --files) mode=files; shift; while [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; do files+=("$1"); shift; done ;;
    --update) mode=update; shift ;;
    --record) mode=record; shift ;;
    --issue) issue="${2:-}"; shift 2 ;;
    --self-test) self_test; exit $? ;;
    *) printf 'file-length: unknown argument %s\n' "$1" >&2; exit 2 ;;
  esac
done
if [ "$mode" = record ] && [ -z "$issue" ]; then
  printf 'file-length: --record needs --issue N\n' >&2
  exit 2
fi

if [ "$mode" != files ]; then
  while IFS= read -r f; do files+=("$f"); done < <(git ls-files -- '*.rs' ':(glob,exclude)**/vendor/**')
fi

# A template under a generator's src/templates/ is the text of one generated
# file, embedded verbatim, so it is generated output in source form.
hand_written() {
  case "$1" in */vendor/*|*/src/templates/*) return 1 ;; esac
  ! head -n1 "$1" | grep -q '@generated'
}

# The allow-list entry of a path, as "<lines> <tag>", or nothing (bash 3.2 has
# no associative arrays, so the list is read per lookup).
listed() {
  [ -f "$ALLOW" ] || return 0
  awk -v p="$1" '$1 !~ /^#/ && $2 == p { print $1, $3 }' "$ALLOW"
}

fail=0
grew=0
warn=0
fell=0
breaches=()
# A listed path that no longer exists was split away: a fall.
if [ "$mode" != files ] && [ -f "$ALLOW" ]; then
  while IFS=' ' read -r first path _; do
    case "$first" in ''|\#*) continue ;; esac
    [ -f "$path" ] && continue
    fell=$((fell + 1))
  done < "$ALLOW"
fi
for f in ${files[@]+"${files[@]}"}; do
  [ -f "$f" ] || continue
  hand_written "$f" || continue
  n=$(wc -l < "$f" | tr -d " ")
  if [ "$n" -gt "$SOFT" ]; then
    breaches+=("$n $f")
  fi
  entry=$(listed "$f")
  if [ -n "$entry" ]; then
    recorded=${entry%% *}
    tag=${entry#* }
    if [ "$n" -gt "$recorded" ]; then
      note "FAIL $f: $n lines, listed at $recorded ($tag); a listed file may not grow, split it"
      fail=1
      grew=1
    elif [ "$n" -lt "$recorded" ]; then
      fell=$((fell + 1))
    fi
    continue
  fi
  if [ "$n" -gt "$HARD" ]; then
    note "FAIL $f: $n lines, over the hard limit of $HARD; split it into a module folder"
    fail=1
  elif [ "$n" -gt "$SOFT" ]; then
    note "warn $f: $n lines, over $SOFT; split it before it grows"
    warn=$((warn + 1))
  fi
done

if [ "$mode" = update ] || [ "$mode" = record ]; then
  if [ "$grew" -ne 0 ]; then
    note "--update never raises an entry: split the file that grew"
    exit 1
  fi
  out=()
  for b in ${breaches[@]+"${breaches[@]}"}; do
    n=${b%% *}; p=${b#* }
    entry=$(listed "$p")
    if [ -n "$entry" ]; then
      recorded=${entry%% *}
      if [ "$n" -gt "$recorded" ]; then
        printf 'file-length: %s grew to %s lines, listed at %s; the list never rises, split it\n' "$p" "$n" "$recorded" >&2
        exit 1
      fi
      out+=("$n $p ${entry#* }")
    elif [ "$mode" = record ]; then
      out+=("$n $p #$issue")
    elif [ "$n" -gt "$HARD" ]; then
      printf 'file-length: %s is a new breach; split it, or record it with --record --issue N\n' "$p" >&2
      exit 2
    fi
  done
  {
    printf '# Known file-length breaches: "<lines> <path> #<issue>". A listed file passes\n'
    printf '# while it does not grow past its recorded count; the split removes the entry.\n'
    if [ "${#out[@]}" -gt 0 ]; then printf '%s\n' "${out[@]}" | sort -k2; fi
  } > "$ALLOW.tmp" && mv "$ALLOW.tmp" "$ALLOW"
  note "recorded ${#out[@]} breaches in $ALLOW"
  exit 0
fi

if [ "$fail" -eq 0 ]; then
  note "OK: no hand-written .rs file over $HARD lines outside the allow-list ($warn over $SOFT unlisted)"
  if [ "$fell" -gt 0 ]; then
    note "note: $fell listed file(s) shrank, split or went; the scheduled ratchet-tighten workflow lowers $ALLOW."
  fi
fi
exit "$fail"
