#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The CPU instructions of each hot path's in-process work (#2249): the
# facade's FHIR to openEHR and openEHR to FHIR mapping of the synthetic
# Condition and the HL7 v2 face's parse and acknowledgment of an ORU^R01,
# each counted by callgrind in a release build of the example
# `hot_path_instructions` and held to its ceiling in perf/instructions.ceilings.
#
# The lane runs on Linux, where valgrind runs. `--toggle-collect` counts
# inside the example's `measured` function and nowhere else. Two things make
# one binary's count the same in every run (#2305): the example runs the
# measured call on a thread of its own, so it allocates from a fresh heap and
# never from the one loading the mappings left behind, and the script
# preloads perf/getrandom.c, so every standard library hash seed is the same.
# Each path is measured twice, and the lane fails when the two counts
# differ, so a count that stops being repeatable is reported as such. No
# specification governs the cost of a path: our own design.
#
# Usage:
#   scripts/perf/instructions.sh            build, measure, fail on a ceiling
#   scripts/perf/instructions.sh --no-build measure the example already built
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

ceilings="perf/instructions.ceilings"
example="target/release/examples/hot_path_instructions"

for tool in valgrind cc; do
  command -v "$tool" >/dev/null || {
    echo "instructions: $tool is not installed" >&2
    exit 2
  }
done
if [[ "${1:-}" != "--no-build" ]]; then
  cargo build --locked --release -p ferrobridge-server --example hot_path_instructions
fi

out="$(mktemp -d)"
trap 'rm -f "$out"/*.out "$out"/getrandom.so; rmdir "$out"' EXIT
cc -shared -fPIC -O2 -o "$out/getrandom.so" perf/getrandom.c

# Prints the instructions one run of `$1` executes inside `measured`.
measure() {
  local trace="$out/$1.$2.out"
  LD_PRELOAD="$out/getrandom.so" valgrind --tool=callgrind \
    --toggle-collect='*measured*' --callgrind-out-file="$trace" \
    "$example" "$1" 2>/dev/null
  awk '/^summary:/ { print $2 }' "$trace"
}

over=0
printf '%-16s %15s %15s\n' path instructions ceiling
while read -r path ceiling; do
  [[ -z "$path" || "$path" == \#* ]] && continue
  count="$(measure "$path" 1)"
  again="$(measure "$path" 2)"
  [[ -n "$count" && -n "$again" ]] || {
    echo "instructions: no count for $path" >&2
    exit 2
  }
  [[ "$count" == "$again" ]] || {
    echo "instructions: $path counted $count, then $again: the count is not repeatable" >&2
    exit 2
  }
  verdict=""
  if [[ "$ceiling" != "report" && "$count" -gt "$ceiling" ]]; then
    verdict="  OVER"
    over=1
  fi
  printf '%-16s %15s %15s%s\n' "$path" "$count" "$ceiling" "$verdict"
done <"$ceilings"

if [[ "$over" -ne 0 ]]; then
  echo "instructions: a hot path exceeds its ceiling in $ceilings" >&2
  exit 1
fi
