#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The post-quantum inventory and its badge (#2177).
#
# conformance/post-quantum.tsv lists every cryptographic use, one row each,
# with five tab-separated columns: use, algorithm, safe (yes or no),
# pq_standard (the standard that defines the post-quantum form, or
# `no standard yet`) and location. The badge conformance/badges/post-quantum.json
# reads `post-quantum` with the message <safe>/<total>, coloured by
# badge_count_color of the shared badge helper: green when every row is safe,
# yellow when some are, red when none. A row whose pq_standard is
# `no standard yet` counts as not safe whatever its safe column says.
# docs/post-quantum.md carries the same rows as a table between its
# inventory:begin and inventory:end markers. No specification governs the
# badge: our own design.
#
#   scripts/checks/post-quantum.sh              # fail when the badge or table disagrees
#   scripts/checks/post-quantum.sh --update     # rewrite the badge and the table
#   scripts/checks/post-quantum.sh --self-test  # the count over fixtures
#
# A header other than the five columns, a row with other than five non-empty
# cells, a safe other than yes or no, a repository path in a location that
# does not exist, or a badge that says certified, compliant or conformant
# fails every mode. Needs jq.
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/badge-logos/logo.sh
source scripts/checks/badge-logos/logo.sh

readonly TSV=conformance/post-quantum.tsv
readonly BADGE=conformance/badges/post-quantum.json
readonly DOC=docs/post-quantum.md
readonly HEADER=$'use\talgorithm\tsafe\tpq_standard\tlocation'

# Prints "<safe> <total>" of the inventory $1, or names each defect on
# stderr and fails.
count() {
  awk -F'\t' -v header="$HEADER" '
    NR == 1 {
      if ($0 != header) { print FILENAME ": the header is not use, algorithm, safe, pq_standard, location" > "/dev/stderr"; bad = 1 }
      next
    }
    NF != 5 { printf "%s:%d: %d columns, not 5\n", FILENAME, NR, NF > "/dev/stderr"; bad = 1; next }
    $1 == "" || $2 == "" || $4 == "" || $5 == "" { printf "%s:%d: an empty cell\n", FILENAME, NR > "/dev/stderr"; bad = 1; next }
    $3 != "yes" && $3 != "no" { printf "%s:%d: safe is \"%s\", not yes or no\n", FILENAME, NR, $3 > "/dev/stderr"; bad = 1; next }
    { total++; if ($3 == "yes" && $4 != "no standard yet") safe++ }
    END {
      if (total == 0) { print FILENAME ": no rows" > "/dev/stderr"; bad = 1 }
      if (bad) exit 1
      print safe + 0, total
    }
  ' "$1"
}

# The badge JSON of the inventory $1.
badge_of() {
  local counted safe total
  counted="$(count "$1")" || return 1
  read -r safe total <<<"$counted"
  jq -cn --arg message "$safe/$total" --arg color "$(badge_count_color "$safe" "$total")" \
    --argjson logo "$(badge_logo_fields key)" \
    '{schemaVersion: 1, label: "post-quantum", message: $message, color: $color} + $logo'
}

# The markdown table of the inventory $1, each location in code spans.
table_of() {
  echo '| Use | Algorithm | Safe | Post-quantum standard | Location |'
  echo '|---|---|---|---|---|'
  tail -n +2 "$1" | awk -F'\t' '{ printf "| %s | %s | %s | %s | `%s` |\n", $1, $2, $3, $4, $5 }'
}

# Fails for each repository path a location of the inventory $1 names that
# does not exist; a family crate or a note in parentheses is no path.
locations_exist() {
  local location entry path failures=0
  while IFS= read -r location; do
    IFS=',' read -ra entries <<<"$location"
    for entry in "${entries[@]}"; do
      path="${entry# }"
      path="${path%% *}"
      if [[ "$path" == */* || "$path" == Cargo.toml ]] && [[ ! -e "$path" ]]; then
        echo "post-quantum: $1 names $path, which does not exist" >&2
        failures=$((failures + 1))
      fi
    done
  done < <(tail -n +2 "$1" | cut -f5)
  [[ "$failures" -eq 0 ]]
}

# The text of file $1 with the lines between its inventory markers replaced
# by the file $2.
with_table() {
  awk -v table="$2" '
    $0 == "<!-- inventory:begin -->" { print; while ((getline line < table) > 0) print line; skipping = 1; next }
    $0 == "<!-- inventory:end -->" { skipping = 0 }
    !skipping { print }
  ' "$1"
}

self_test() {
  local scratch failures=0 name want got
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' RETURN
  fixture() {
    { echo "$HEADER"; printf '%s\n' "${@:2}"; } >"$scratch/$1.tsv"
  }
  fixture all $'a\tA\tyes\tRFC 10024\tx' $'b\tB\tyes\tNIST IR 8547 4.1.3\tx'
  fixture some $'a\tA\tyes\tRFC 10024\tx' $'b\tB\tno\tRFC 9964\tx'
  fixture none $'a\tA\tno\tRFC 9964\tx' $'b\tB\tno\tno standard yet\tx'
  fixture unstandardised $'a\tA\tyes\tRFC 10024\tx' $'b\tB\tyes\tno standard yet\tx'
  for name in all:2/2:brightgreen some:1/2:yellow none:0/2:red unstandardised:1/2:yellow; do
    want="${name#*:}"
    got="$(badge_of "$scratch/${name%%:*}.tsv" | jq -r '"\(.message):\(.color)"')"
    if [[ "$got" != "$want" ]]; then
      echo "post-quantum self-test: ${name%%:*} gives $got, not $want" >&2
      failures=$((failures + 1))
    fi
  done
  fixture columns $'a\tA\tyes\tRFC 10024'
  fixture verdict $'a\tA\tmaybe\tRFC 10024\tx'
  fixture empty $'a\t\tyes\tRFC 10024\tx'
  printf 'use\tsafe\n' >"$scratch/header.tsv"
  for name in columns verdict empty header; do
    if count "$scratch/$name.tsv" >/dev/null 2>&1; then
      echo "post-quantum self-test: the $name fixture is accepted" >&2
      failures=$((failures + 1))
    fi
  done
  fixture located $'a\tA\tyes\tRFC 10024\tCargo.toml (rustls), scripts/checks/post-quantum.sh, ferrohealth-server tls'
  fixture missing $'a\tA\tyes\tRFC 10024\tcrates/no-such-crate/src/lib.rs'
  if ! locations_exist "$scratch/located.tsv" 2>/dev/null; then
    echo "post-quantum self-test: existing locations are refused" >&2
    failures=$((failures + 1))
  fi
  if locations_exist "$scratch/missing.tsv" 2>/dev/null; then
    echo "post-quantum self-test: a missing location is accepted" >&2
    failures=$((failures + 1))
  fi
  [[ "$failures" -eq 0 ]] && echo "post-quantum self-test: ok"
  [[ "$failures" -eq 0 ]]
}

mode=check
case "${1:-}" in
  "") ;;
  --update) mode=update ;;
  --self-test) self_test; exit $? ;;
  *) echo "usage: $0 [--update|--self-test]" >&2; exit 2 ;;
esac

failed=0
document="$(badge_of "$TSV")" || exit 1
locations_exist "$TSV" || failed=1
table="$(mktemp)"
trap 'rm -f "$table"' EXIT
table_of "$TSV" >"$table"
if ! grep -qx '<!-- inventory:begin -->' "$DOC" || ! grep -qx '<!-- inventory:end -->' "$DOC"; then
  echo "post-quantum: $DOC has no inventory:begin and inventory:end markers" >&2
  exit 1
fi
rendered="$(with_table "$DOC" "$table")"
if [[ "$mode" = update ]]; then
  printf '%s\n' "$document" >"$BADGE"
  printf '%s\n' "$rendered" >"$DOC"
else
  if [[ ! -f "$BADGE" ]] || [[ "$(cat "$BADGE")" != "$document" ]]; then
    echo "post-quantum: $BADGE disagrees with $TSV; run $0 --update" >&2
    failed=1
  fi
  if [[ "$(cat "$DOC")" != "$rendered" ]]; then
    echo "post-quantum: the table in $DOC disagrees with $TSV; run $0 --update" >&2
    failed=1
  fi
fi
if jq -r '.label, .message' "$BADGE" | grep -qiE 'certified|compliant|conformant'; then
  echo "post-quantum: $BADGE claims a certification; the badge counts safe rows" >&2
  failed=1
fi
badge_logo_check "$BADGE" || failed=1
badge_color_check "$BADGE" || failed=1
exit "$failed"
