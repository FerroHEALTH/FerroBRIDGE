#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The post-quantum inventory and its badge (#2177).
#
# conformance/post-quantum.tsv lists every cryptographic use, one row each,
# in the shape the shared badge helper reads (scripts/checks/badge-logos/logo.sh):
# use, algorithm, safe, pq_standard and location, tab-separated. The helper's
# badge_post_quantum writes conformance/badges/post-quantum.json from it and
# badge_post_quantum_check holds the two together. This script adds what is
# FerroBRIDGE's own: every repository path a location names exists, and
# docs/post-quantum.md carries the same rows as a table between its
# inventory:begin and inventory:end markers. No specification governs the
# badge: our own design.
#
#   scripts/checks/post-quantum.sh              # fail when the badge or table disagrees
#   scripts/checks/post-quantum.sh --update     # rewrite the badge and the table
#   scripts/checks/post-quantum.sh --self-test  # the helper's cases and the location check
#
# A malformed row, a repository path in a location that does not exist, or a
# badge that says certified, compliant or conformant fails every mode. Needs
# jq.
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/checks/badge-logos/logo.sh
source scripts/checks/badge-logos/logo.sh

readonly TSV=conformance/post-quantum.tsv
readonly BADGE=conformance/badges/post-quantum.json
readonly DOC=docs/post-quantum.md

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
  local scratch failures=0
  badge_post_quantum_self_test || return 1
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' RETURN
  printf '%s\n%s\n' "$badge_post_quantum_header" \
    $'a\tA\tyes\tRFC 10024\tCargo.toml (rustls), scripts/checks/post-quantum.sh, ferrohealth-server tls' >"$scratch/located.tsv"
  printf '%s\n%s\n' "$badge_post_quantum_header" \
    $'a\tA\tyes\tRFC 10024\tcrates/no-such-crate/src/lib.rs' >"$scratch/missing.tsv"
  if ! locations_exist "$scratch/located.tsv" 2>/dev/null; then
    echo "post-quantum self-test: existing locations are refused" >&2
    failures=$((failures + 1))
  fi
  if locations_exist "$scratch/missing.tsv" 2>/dev/null; then
    echo "post-quantum self-test: a missing location is accepted" >&2
    failures=$((failures + 1))
  fi
  [[ "$failures" -eq 0 ]] && echo "post-quantum self-test: locations ok"
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
document="$(badge_post_quantum "$TSV")" || exit 1
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
  if [[ ! -f "$BADGE" ]] || ! badge_post_quantum_check "$TSV" "$BADGE"; then
    echo "post-quantum: run $0 --update" >&2
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
