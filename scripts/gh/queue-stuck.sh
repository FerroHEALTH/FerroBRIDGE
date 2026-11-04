#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/gh/queue-stuck.sh: finds merge-queue entries that stopped moving,
# and with --apply dequeues each and arms auto-merge on it again.
#
# WHY THIS EXISTS: an entry can sit QUEUED with no merge-group commit, or
# with a commit no workflow ran on, until someone dequeues it and arms it
# again by hand. An entry is stuck when it is QUEUED, stands within the
# entries the queue builds at once (the ruleset's max_entries_to_build),
# has waited longer than --minutes, and has no head commit or a head
# commit with no check suite. An entry further back waits for a build slot
# and is never stuck.
#
# It runs by hand. A re-arm made with a workflow's GITHUB_TOKEN would start
# no merge_group run ("events triggered by the GITHUB_TOKEN ... will not
# create a new workflow run", GitHub docs, "Triggering a workflow from a
# workflow"), so a scheduled lane would leave the entry stuck again.
#
# Official docs (durable references):
#   Merge queue ......... https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue
#   GraphQL ............. https://docs.github.com/en/graphql/reference/objects#mergequeueentry
#                         https://docs.github.com/en/graphql/reference/mutations#dequeuepullrequest
#   Rules for a branch .. https://docs.github.com/en/rest/repos/rules#get-rules-for-a-branch
#
# Policy: no specification governs it: our own design.
#
# Usage:
#   scripts/gh/queue-stuck.sh [--apply] [--minutes <n>] [--branch <name>]
#       Lists the stuck entries of the queue of <name> (default main) that
#       have waited more than <n> minutes (default 20). Without --apply it
#       is a dry run: it prints what it would do and exits 3 when it found
#       one, 0 when it found none. With --apply it dequeues each and runs
#       `gh pr merge <n> --auto`, and exits 1 when one of them failed.
#   scripts/gh/queue-stuck.sh --self-test
#       Drives the script against a stub gh: a healthy queue, a stuck entry
#       as a dry run and applied, and a refused dequeue.
#   Any other argument prints this usage and exits 2 before any gh call.

# shellcheck disable=SC2016 # every $o/$n/$b/$i in a query is a GraphQL variable, and $build/$now/$wait in a jq filter is a jq binding
set -euo pipefail

die() {
  echo "queue-stuck: $*" >&2
  exit 1
}

usage() {
  sed -n '/^# Usage:/,/^$/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

self_test() {
  command -v jq >/dev/null 2>&1 || die "jq is not installed"
  local work stub calls entries dequeue now
  work="$(mktemp -d)"
  stub="$work/bin"
  calls="$work/calls"
  mkdir -p "$stub"
  cat > "$stub/gh" <<'STUB'
#!/usr/bin/env bash
# The gh a self-test run of scripts/gh/queue-stuck.sh speaks to. It writes
# every call where the test reads it and answers from the environment.
set -euo pipefail
printf '%s\n' "$*" >> "$GH_STUB_CALLS"
case "${1:-} ${2:-}" in
  "repo view")
    printf '%s\n' '{"nameWithOwner":"Example-Org/Example"}'
    ;;
  "api repos/Example-Org/Example/rules/branches/main")
    printf '%s\n' '[{"type":"merge_queue","parameters":{"max_entries_to_build":2}}]'
    ;;
  "api graphql")
    case "$*" in
      *dequeuePullRequest*)
        if [[ "$GH_STUB_DEQUEUE" != "ok" ]]; then
          echo "stub: the dequeue was refused" >&2
          exit 1
        fi
        printf '%s\n' '{"data":{"dequeuePullRequest":{"mergeQueueEntry":{"id":"MQE_stub"}}}}'
        ;;
      *mergeQueue*)
        printf '%s\n' "$GH_STUB_ENTRIES"
        ;;
      *)
        echo "stub: an unexpected query: $*" >&2
        exit 90
        ;;
    esac
    ;;
  "pr merge")
    printf '%s\n' "armed"
    ;;
  *)
    echo "stub: an unexpected call: $*" >&2
    exit 90
    ;;
esac
STUB
  chmod 0755 "$stub/gh"

  # run NAME CODE ARGS…: this program through the stub, landing on CODE.
  run() {
    local name=$1 code=$2 rc=0
    shift 2
    : > "$calls"
    PATH="$stub:$PATH" GH_STUB_CALLS="$calls" GH_STUB_ENTRIES="$entries" \
      GH_STUB_DEQUEUE="$dequeue" QUEUE_STUCK_NOW="$now" \
      bash "$0" "$@" > "$work/out" 2> "$work/err" || rc=$?
    if [[ "$rc" -ne "$code" ]]; then
      echo "queue-stuck: self-test failed: $name exited $rc, wanted $code." >&2
      cat "$work/out" "$work/err" "$calls" >&2
      exit 1
    fi
  }
  # said NAME FILE NEEDLE: the case left that text in FILE.
  said() {
    if ! grep -qF -- "$3" "$2"; then
      echo "queue-stuck: self-test failed: $1 did not say '$3'." >&2
      cat "$work/out" "$work/err" "$calls" >&2
      exit 1
    fi
  }
  # never NAME NEEDLE: the case made no gh call holding NEEDLE.
  never() {
    if grep -qF -- "$2" "$calls"; then
      echo "queue-stuck: self-test failed: $1 called gh with '$2'." >&2
      cat "$calls" >&2
      exit 1
    fi
  }

  # 2026-01-01T01:00:00Z, an hour after every entry below was queued.
  now=1767229200
  dequeue=ok
  # A queue that moves: the first entry builds, the second has a commit and
  # a suite, and the third waits behind the two the queue builds at once.
  entries='{"data":{"repository":{"mergeQueue":{"entries":{"nodes":[
    {"position":1,"state":"AWAITING_CHECKS","enqueuedAt":"2026-01-01T00:00:00Z","headCommit":{"oid":"a1","checkSuites":{"totalCount":3}},"pullRequest":{"id":"PR_1","number":11}},
    {"position":2,"state":"QUEUED","enqueuedAt":"2026-01-01T00:00:00Z","headCommit":{"oid":"b2","checkSuites":{"totalCount":1}},"pullRequest":{"id":"PR_2","number":12}},
    {"position":3,"state":"QUEUED","enqueuedAt":"2026-01-01T00:00:00Z","headCommit":null,"pullRequest":{"id":"PR_3","number":13}}]}}}}}'
  run "a queue that moves" 0
  said "a queue that moves" "$work/out" "no stuck entry"
  never "a queue that moves" "dequeuePullRequest"

  # The second entry has waited an hour with no merge-group commit.
  entries='{"data":{"repository":{"mergeQueue":{"entries":{"nodes":[
    {"position":1,"state":"AWAITING_CHECKS","enqueuedAt":"2026-01-01T00:00:00Z","headCommit":{"oid":"a1","checkSuites":{"totalCount":3}},"pullRequest":{"id":"PR_1","number":11}},
    {"position":2,"state":"QUEUED","enqueuedAt":"2026-01-01T00:00:00Z","headCommit":null,"pullRequest":{"id":"PR_42","number":42}}]}}}}}'
  run "a stuck entry, dry run" 3
  said "a stuck entry, dry run" "$work/out" "stuck: #42"
  said "a stuck entry, dry run" "$work/out" "would dequeue #42 and arm auto-merge again"
  never "a stuck entry, dry run" "dequeuePullRequest"
  never "a stuck entry, dry run" "pr merge"

  run "a stuck entry younger than the threshold" 0 --minutes 90
  said "a stuck entry younger than the threshold" "$work/out" "no stuck entry"

  run "a stuck entry, applied" 0 --apply
  said "a stuck entry, applied" "$work/out" "re-armed: #42"
  said "a stuck entry, applied" "$calls" "dequeuePullRequest"
  said "a stuck entry, applied" "$calls" "pr merge 42 --auto"

  dequeue=fail
  run "a refused dequeue" 1 --apply
  said "a refused dequeue" "$work/err" "dequeuing #42 failed"
  never "a refused dequeue" "pr merge"
  dequeue=ok

  # A usage error prints the usage and exits 2 before a single gh call.
  local argument
  for argument in --help -h --bogus bogus "--minutes" "--minutes x" "--branch"; do
    # shellcheck disable=SC2086 # the case splits "--minutes x" into two arguments on purpose
    run "the argument $argument" 2 $argument
    said "the argument $argument" "$work/err" "Usage:"
    if [[ -s "$calls" ]]; then
      echo "queue-stuck: self-test failed: the argument $argument called gh." >&2
      exit 1
    fi
  done

  rm -r "$work"
  echo "queue-stuck: self-test OK."
}

APPLY=no
MINUTES=20
BRANCH=main
case "$#:${1:-}" in
  1:--self-test)
    self_test
    exit 0
    ;;
  *) ;;
esac
while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply) APPLY=yes ;;
    --minutes)
      [[ $# -ge 2 && "$2" =~ ^[0-9]+$ ]] || usage
      MINUTES="$2"
      shift
      ;;
    --branch)
      [[ $# -ge 2 && -n "$2" ]] || usage
      BRANCH="$2"
      shift
      ;;
    *) usage ;;
  esac
  shift
done

command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is not installed"
command -v jq >/dev/null 2>&1 || die "jq is not installed"

REPO="$(gh repo view --json nameWithOwner | jq -r .nameWithOwner)" ||
  die "could not resolve the current repository (run inside a gh-authenticated clone)"
OWNER="${REPO%%/*}"
NAME="${REPO##*/}"

BUILD="$(gh api "repos/$REPO/rules/branches/$BRANCH" |
  jq -r '[.[] | select(.type == "merge_queue") | .parameters.max_entries_to_build] | first // empty')" ||
  die "could not read the rules of $BRANCH"
[[ -n "$BUILD" ]] || die "$BRANCH has no merge queue"

ANSWER="$(gh api graphql -f query='query($o:String!,$n:String!,$b:String!){ repository(owner:$o,name:$n){ mergeQueue(branch:$b){ entries(first:100){ nodes{ position state enqueuedAt headCommit{ oid checkSuites(first:1){ totalCount } } pullRequest{ id number } } } } } }' \
  -f o="$OWNER" -f n="$NAME" -f b="$BRANCH")" || die "could not read the merge queue of $BRANCH"

NOW="${QUEUE_STUCK_NOW:-$(date -u +%s)}"
# One line per stuck entry: "<pr number> <pr node id> <minutes waited> <head>".
STUCK="$(printf '%s' "$ANSWER" | jq -r --argjson build "$BUILD" --argjson now "$NOW" --argjson wait "$MINUTES" '
  (.data.repository.mergeQueue.entries.nodes // [])[]
  | select(.state == "QUEUED" and .position <= $build)
  | (($now - (.enqueuedAt | fromdateiso8601)) / 60 | floor) as $waited
  | select($waited >= $wait)
  | select(.headCommit == null or .headCommit.checkSuites.totalCount == 0)
  | "\(.pullRequest.number) \(.pullRequest.id) \($waited) \(.headCommit.oid // "none")"')" ||
  die "could not read the merge queue answer"

if [[ -z "$STUCK" ]]; then
  echo "queue-stuck: no stuck entry in the merge queue of $BRANCH"
  exit 0
fi

failed=0
while read -r number id waited head; do
  if [[ "$head" == "none" ]]; then
    echo "stuck: #$number has waited $waited minutes with no merge-group commit"
  else
    echo "stuck: #$number has waited $waited minutes and no workflow ran on $head"
  fi
  if [[ "$APPLY" != "yes" ]]; then
    echo "  would dequeue #$number and arm auto-merge again (--apply)"
    continue
  fi
  if ! gh api graphql -f query='mutation($i:ID!){ dequeuePullRequest(input:{id:$i}){ mergeQueueEntry{ id } } }' \
    -f i="$id" >/dev/null; then
    echo "queue-stuck: dequeuing #$number failed" >&2
    failed=1
    continue
  fi
  if ! gh pr merge "$number" --auto >/dev/null; then
    echo "queue-stuck: #$number is dequeued, and arming auto-merge failed; run gh pr merge $number --auto" >&2
    failed=1
    continue
  fi
  echo "re-armed: #$number"
done <<<"$STUCK"

if [[ "$APPLY" == "yes" ]]; then
  exit "$failed"
fi
exit 3
