#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The upstream-row guard (#2334). A row of conformance/<corpus>/upstream.tsv
# says an open upstream-report issue holds a failing case, and the corpus
# badge then names it as `, k upstream` and colours by the other cases
# (scripts/checks/conformance.sh). This guard asks the tracker for every
# issue such a row names and fails when one is closed, unknown, or carries no
# upstream-report label, so a row whose issue closes leaves the file and counts
# against the badge colour again. No specification governs this: our own
# design.
#
#   scripts/checks/upstream-rows.sh              # check every upstream file
#   scripts/checks/upstream-rows.sh --self-test  # the cases, against a stub gh
#
# The badges are rendered from the committed rows, so conformance.sh needs no
# tracker; this guard is where the tracker is read. It needs a `gh` that can
# read the repository's issues: locally your login, in CI the job's
# GITHUB_TOKEN with `issues: read`. When `gh` is absent, unauthenticated or
# offline, the guard says so and exits 0, so it never blocks offline work; CI
# is where it gates.
#
# Exit 0 = every row names an open upstream-report issue (or the guard
# skipped), 1 = a row names another issue, 2 = a usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly LABEL=upstream-report

# One "issue<TAB>file" record per distinct issue and upstream file under $1.
issues_under() {
  local file
  for file in "$1"/*/upstream.tsv; do
    [[ -f "$file" ]] || continue
    awk -F'\t' -v file="$file" '/^(#|$)/ || !seen++ { next } { print $2 "\t" file }' "$file"
  done | sort -t $'\t' -k1,1n -k2,2 -u
}

# Checks every issue the upstream files under $1 name; prints one error line
# per file that names an issue the tracker does not answer as an open
# upstream-report issue.
check_rows() {
  local records number answer state labels fail=0 checked=0
  records="$(issues_under "$1")"
  if [[ -z "$records" ]]; then
    echo "upstream-rows: no upstream rows under $1."
    return 0
  fi
  while IFS= read -r number; do
    checked=$((checked + 1))
    if ! answer="$(gh issue view "$number" --json state,labels \
      --jq '.state + " " + ([.labels[].name] | join(","))' 2>/dev/null </dev/null)"; then
      answer=UNKNOWN
    fi
    state="${answer%% *}"
    labels=",${answer#* },"
    if [[ "$state" = OPEN && "$labels" == *",$LABEL,"* ]]; then
      continue
    fi
    fail=1
    while IFS=$'\t' read -r n file; do
      [[ "$n" = "$number" ]] || continue
      case "$state" in
        CLOSED) echo "::error file=$file::$file names #$number, which is closed: drop its rows, which count against the badge colour again, or name the open issue that holds them." >&2 ;;
        OPEN) echo "::error file=$file::$file names #$number, which carries no $LABEL label." >&2 ;;
        *) echo "::error file=$file::$file names #$number, which the tracker does not answer as an open issue." >&2 ;;
      esac
    done <<<"$records"
  done < <(cut -f1 <<<"$records" | sort -un)
  if [[ "$fail" -eq 0 ]]; then
    echo "upstream-rows: $checked issue(s) named by upstream rows, all open and labelled $LABEL."
  fi
  return "$fail"
}

# The cases of check_rows over a scratch tree and a stub gh that answers #1
# open and labelled, #2 closed, #3 open without the label, and nothing else.
self_test() {
  local work
  work="$(mktemp -d)"
  mkdir -p "$work/bin" "$work/tree/a" "$work/tree/b"
  cat >"$work/bin/gh" <<'STUB'
#!/usr/bin/env bash
case "$3" in
  1) echo "OPEN documentation,upstream-report" ;;
  2) echo "CLOSED upstream-report" ;;
  3) echo "OPEN documentation" ;;
  *) exit 1 ;;
esac
STUB
  chmod +x "$work/bin/gh"
  rows() { printf 'case\tissue\n' >"$work/tree/$1/upstream.tsv"; shift_rows "$@"; }
  shift_rows() {
    local corpus="$1"
    shift
    printf '%s\n' "$@" >>"$work/tree/$corpus/upstream.tsv"
  }
  st_fail() {
    echo "upstream-rows: self-test FAILED: $1" >&2
    rm -rf "$work"
    return 1
  }
  rows a $'x\t1' $'y\t1'
  rows b $'z\t1'
  PATH="$work/bin:$PATH" check_rows "$work/tree" >/dev/null 2>&1 || { st_fail "open labelled issues are refused"; return 1; }
  local issue
  for issue in 2 3 4; do
    rows b $'z\t1' "w	$issue"
    if PATH="$work/bin:$PATH" check_rows "$work/tree" >/dev/null 2>&1; then
      st_fail "a row naming #$issue passes"
      return 1
    fi
  done
  rm -rf "$work"
  echo "upstream-rows: self-test OK"
}

case "${1:-}" in
  --self-test)
    self_test
    exit
    ;;
  "") ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac

if ! command -v gh >/dev/null 2>&1; then
  echo "upstream-rows: skipped: gh is not installed, so the tracker cannot be read."
  exit 0
fi
if ! gh repo view --json name >/dev/null 2>&1; then
  echo "upstream-rows: skipped: gh cannot read the repository (unauthenticated or offline)."
  exit 0
fi
check_rows conformance
