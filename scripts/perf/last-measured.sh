#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The base of the performance gate on a push to main (#2313): the newest
# commit of main whose own CI push run measured it and passed the gate. A
# push run that GitHub cancelled or dropped while it waited in the
# concurrency group measured nothing, so measuring from the commit before the
# push left that commit's change between two measurements nobody compared.
# Measuring from the last commit that passed puts every change after it into
# this run, and a breach stays visible on every later push until it is fixed.
#
#   scripts/perf/last-measured.sh
#       Reads GITHUB_REPOSITORY and GITHUB_RUN_ID and asks the GitHub API,
#       through gh, for the newest $LIMIT push runs of the CI workflow on
#       main. Prints the head commit of the newest run started before this
#       one whose "perf / perf verdict" job succeeded, or nothing when none
#       did, in which case the gate takes the previous release. The reason
#       goes to stderr. Exit 1 when the API cannot be read.
#   scripts/perf/last-measured.sh --self-test
#       Proves the choice over stubbed API answers, offline.
#
# No specification governs this: our own design.
set -euo pipefail

readonly LIMIT=50
readonly WORKFLOW=ci.yml
readonly VERDICT="perf / perf verdict"

die() {
  echo "last-measured: $*" >&2
  exit 1
}

# runs: "<id> <head sha>" of the newest push runs of the CI workflow on main
# started before this one, newest first.
runs() {
  local answer
  answer="$(gh api --method GET "repos/$GITHUB_REPOSITORY/actions/workflows/$WORKFLOW/runs" \
    -f branch=main -f event=push -f per_page="$LIMIT")" || die "the CI runs on main cannot be read"
  jq --raw-output --argjson run "$GITHUB_RUN_ID" \
    '.workflow_runs | sort_by(-.id) | .[] | select(.id < $run) | "\(.id) \(.head_sha)"' <<< "$answer" ||
    die "the CI runs on main are not the API's shape"
}

# verdict RUN: the conclusion of RUN's perf verdict job in its latest
# attempt, or nothing when the run has none.
verdict() {
  local answer
  answer="$(gh api --paginate "repos/$GITHUB_REPOSITORY/actions/runs/$1/jobs?filter=latest&per_page=100")" ||
    die "the jobs of run $1 cannot be read"
  jq --raw-output --arg name "$VERDICT" '.jobs[] | select(.name == $name) | .conclusion // ""' <<< "$answer" ||
    die "the jobs of run $1 are not the API's shape"
}

last_measured() {
  [[ -n "${GITHUB_REPOSITORY:-}" && -n "${GITHUB_RUN_ID:-}" ]] || die "GITHUB_REPOSITORY and GITHUB_RUN_ID are not set"
  local list id sha conclusion
  list="$(runs)" || exit 1
  while read -r id sha; do
    [[ -n "$id" ]] || continue
    conclusion="$(verdict "$id")" || exit 1
    if [[ "$conclusion" == success ]]; then
      echo "$sha"
      echo "last-measured: run $id passed the gate on $sha, the newest commit of main that did" >&2
      return 0
    fi
  done <<< "$list"
  echo "last-measured: none of the $LIMIT newest push runs on main before run $GITHUB_RUN_ID passed the gate, so the gate takes the previous release" >&2
}

self_test() {
  local work n=0 failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now
  trap "rm -r -- '$work'" EXIT
  mkdir -p "$work/bin" "$work/api"
  # A gh that answers each API path from a file under $work/api, and fails
  # on any other.
  cat > "$work/bin/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
for arg in "$@"; do
  case "$arg" in
    repos/*/actions/workflows/*/runs) cat "$STUB_API/runs.json"; exit 0 ;;
    repos/*/actions/runs/*/jobs*)
      id="${arg#*/actions/runs/}"
      id="${id%%/*}"
      [[ -f "$STUB_API/jobs-$id.json" ]] || exit 1
      cat "$STUB_API/jobs-$id.json"
      exit 0
      ;;
  esac
done
exit 1
GH
  chmod 0755 "$work/bin/gh"
  check() {
    local label="$1" want="$2" got="$3"
    n=$((n + 1))
    if [[ "$want" != "$got" ]]; then
      echo "  FAIL: $label: expected [$want], got [$got]" >&2
      failed=1
    fi
  }
  jobs() {
    local id="$1" conclusion="$2"
    if [[ -z "$conclusion" ]]; then
      printf '{"jobs": [{"name": "detect", "conclusion": "success"}, {"name": "perf", "conclusion": "skipped"}]}\n'
    else
      printf '{"jobs": [{"name": "detect", "conclusion": "success"}, {"name": "%s", "conclusion": "%s"}]}\n' \
        "$VERDICT" "$conclusion"
    fi > "$work/api/jobs-$id.json"
  }
  measure() {
    PATH="$work/bin:$PATH" STUB_API="$work/api" GITHUB_REPOSITORY=o/r GITHUB_RUN_ID=500 last_measured
  }
  # Run 600 started after this one, 400 was cancelled before its verdict,
  # 300 ran no perf, 200 breached, 100 passed.
  printf '{"workflow_runs": [%s]}\n' \
    '{"id": 300, "head_sha": "c300"}, {"id": 600, "head_sha": "c600"}, {"id": 400, "head_sha": "c400"}, {"id": 500, "head_sha": "c500"}, {"id": 200, "head_sha": "c200"}, {"id": 100, "head_sha": "c100"}' \
    > "$work/api/runs.json"
  jobs 600 success
  jobs 500 success
  jobs 400 cancelled
  jobs 300 ""
  jobs 200 failure
  jobs 100 success
  check "the newest earlier run that passed the gate names the base" c100 "$(measure 2> /dev/null)"
  check "the reason names that run" "last-measured: run 100 passed the gate on c100, the newest commit of main that did" \
    "$(measure 2>&1 > /dev/null)"
  jobs 300 success
  check "a later run that passed is the base once it is the newest" c300 "$(measure 2> /dev/null)"
  jobs 300 ""
  jobs 100 failure
  check "with no earlier run that passed the base is empty" "" "$(measure 2> /dev/null)"
  check "and the reason says the previous release is taken" 1 \
    "$(measure 2>&1 > /dev/null | grep -c 'takes the previous release')"
  rm "$work/api/jobs-200.json"
  check "a run whose jobs cannot be read fails" 1 "$( (measure) > /dev/null 2>&1; echo $?)"
  rm "$work/api/runs.json"
  check "runs that cannot be read fail" 1 "$( (measure) > /dev/null 2>&1; echo $?)"
  check "a run with no repository fails" 1 "$( (GITHUB_REPOSITORY='' last_measured) > /dev/null 2>&1; echo $?)"
  [[ "$failed" -eq 0 ]] || exit 1
  echo "last-measured: self-test OK ($n checks)."
}

case "${1:-}" in
  "") last_measured ;;
  --self-test) self_test ;;
  *) die "usage: last-measured.sh [--self-test]" ;;
esac
