#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/deploy-probe.sh: probe a running FerroBRIDGE deployment, lane by lane.
#
# Each lane gets one of four findings, and the four never merge:
#   on         the lane is mounted and answers as served
#   off        the lane is not mounted (404), or says it is not configured
#   failing    the lane answers a 5xx, or an answer the lane never gives
#   uncovered  the suite could not observe the lane (no answer, 401 or 403
#              without a token, a status no rule classifies)
# An uncovered lane is never counted as passed: it fails the run with its own
# exit status. The report also names, per lane, the findings this run did not
# observe, because one run sees only the state the deployment is in.
#
# The lanes and the probes, each against a route the server mounts:
#   health      GET /health/liveness, GET /health/readiness, GET /health/info
#   facade      GET /fhir/metadata (a CapabilityStatement when on)
#   operations  POST /fhir/$tofhir with an empty Parameters: the engine
#               refuses it with a 4xx when the lane is on, and the route
#               answers 503 with the issue code not-supported when no mapping
#               set is configured. The probe writes nothing to the CDR.
#
# No specification governs this suite or its findings: our own design.
#
# Usage:
#   scripts/deploy-probe.sh [options] <base-url>
#       Probes the deployment at <base-url> (for example
#       http://localhost:8080) and prints one finding per lane.
#     --expect <lane>=<on|off>
#       Fails the run when <lane> is not found in that state. Repeatable.
#     --json <file>
#       Also writes the findings to <file> as JSON.
#     --token-file <file>
#       Sends the bearer token read from <file> on the /fhir probes, for a
#       facade behind SMART authorization. Without it a 401 or 403 there is
#       reported as uncovered.
#     --timeout <seconds>
#       The time each request may take (default 10).
#   scripts/deploy-probe.sh --self-test
#       Drives this program against a stub curl on PATH through every
#       finding of every lane.
#   Any other argument, --help included, prints this usage and exits 2.
#
# Exit 0 = every lane observed, none failing, every expectation met;
#      1 = a lane failing or an expectation missed;
#      2 = usage;
#      3 = a lane uncovered (and none failing).

set -euo pipefail

die() {
  echo "deploy-probe: $*" >&2
  exit 2
}

usage() {
  sed -n '/^# Usage:/,/^$/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

LANES="health facade operations"
FINDINGS="on off failing uncovered"
BASE=""
JSON_OUT=""
TOKEN_FILE=""
TIMEOUT=10
EXPECT=""
WORK=""
ROWS=""

# One JSON string body: backslashes, quotes and control characters escaped
# (RFC 8259 §7), newlines folded to spaces.
json_string() {
  printf '%s' "$1" | tr '\r\n\t' '   ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

# request <method> <path> [curl args...]: prints the status code, or 000 when
# no HTTP answer came back; the body lands in $WORK/body.
request() {
  local method="$1" path="$2"
  shift 2
  local args=(-s -o "$WORK/body" -w '%{http_code}' --max-time "$TIMEOUT" -X "$method")
  if [ -n "$TOKEN_FILE" ] && [ "${path#/fhir}" != "$path" ]; then
    args+=(-H "Authorization: Bearer $(tr -d '\r\n' < "$TOKEN_FILE")")
  fi
  : > "$WORK/body"
  curl "${args[@]}" "$@" -- "$BASE$path" || true
}

body_has() {
  grep -q -- "$1" "$WORK/body"
}

# The part of a classification every probe shares: no answer, a refused
# token, a 5xx, a 404. Prints "<finding> <evidence>" and returns 0 when it
# decided, 1 when the probe's own rule must decide.
common() {
  local code="$1"
  case "$code" in
    000) echo "uncovered no HTTP answer within ${TIMEOUT}s" ;;
    401 | 403)
      if [ -n "$TOKEN_FILE" ]; then
        echo "failing $code with the token supplied"
      else
        echo "uncovered $code: the lane is guarded and no token was supplied"
      fi
      ;;
    404) echo "off 404" ;;
    5??) return 1 ;;
    *) return 1 ;;
  esac
}

classify_health() {
  local code="$1"
  common "$code" && return 0
  case "$code" in
    200) echo "on 200" ;;
    503) echo "failing 503 $(head -c 300 "$WORK/body")" ;;
    5??) echo "failing $code" ;;
    *) echo "uncovered $code is not an answer this route gives" ;;
  esac
}

classify_facade() {
  local code="$1"
  common "$code" && return 0
  case "$code" in
    200)
      if body_has '"resourceType" *: *"CapabilityStatement"'; then
        echo "on 200 CapabilityStatement"
      else
        echo "failing 200 without a CapabilityStatement"
      fi
      ;;
    5??) echo "failing $code" ;;
    *) echo "uncovered $code is not an answer this route gives" ;;
  esac
}

classify_operations() {
  local code="$1"
  common "$code" && return 0
  case "$code" in
    503)
      if body_has '"not-supported"'; then
        echo "off 503 not-supported: no mapping set is configured"
      else
        echo "failing 503"
      fi
      ;;
    5??) echo "failing $code" ;;
    200) echo "on 200" ;;
    400 | 422)
      if body_has '"resourceType" *: *"OperationOutcome"'; then
        echo "on $code: the engine refused the empty Parameters"
      else
        echo "failing $code without an OperationOutcome"
      fi
      ;;
    *) echo "uncovered $code is not an answer this route gives" ;;
  esac
}

# probe <lane> <method> <path> [curl args...]: runs one probe and records its
# row as "<lane>\t<finding>\t<method> <path>\t<evidence>".
probe() {
  local lane="$1" method="$2" path="$3" code verdict
  shift 3
  code="$(request "$method" "$path" "$@")"
  verdict="$("classify_$lane" "$code")"
  ROWS="$ROWS$lane"$'\t'"${verdict%% *}"$'\t'"$method $path"$'\t'"${verdict#* }"$'\n'
}

# The finding of a lane from its probes: failing beats uncovered; probes that
# agree give their finding; an on and an off together mean a half-mounted
# lane, which is failing.
lane_finding() {
  local findings
  findings="$(printf '%s' "$ROWS" | awk -F'\t' -v l="$1" '$1 == l { print $2 }' | sort -u)"
  case "$findings" in
    *failing*) echo failing ;;
    *uncovered*) echo uncovered ;;
    on) echo on ;;
    off) echo off ;;
    *) echo failing ;;
  esac
}

expected_of() {
  printf '%s\n' "$EXPECT" | awk -F= -v l="$1" '$1 == l { v = $2 } END { print v }'
}

report() {
  local lane finding want status=0 any_uncovered=0 json_lanes="" unseen f
  echo "deploy-probe: $BASE"
  for lane in $LANES; do
    finding="$(lane_finding "$lane")"
    want="$(expected_of "$lane")"
    printf '  %-11s %s' "$lane" "$finding"
    [ -n "$want" ] && [ "$want" != "$finding" ] && printf '  (expected %s)' "$want"
    printf '\n'
    printf '%s' "$ROWS" | awk -F'\t' -v l="$lane" \
      '$1 == l { printf "      %-9s %-26s %s\n", $2, $3, $4 }'
    case "$finding" in
      failing) status=1 ;;
      uncovered) any_uncovered=1 ;;
      *) ;;
    esac
    if [ -n "$want" ] && [ "$want" != "$finding" ]; then
      status=1
    fi
    unseen=""
    for f in $FINDINGS; do
      [ "$f" = "$finding" ] || unseen="$unseen${unseen:+, }$f"
    done
    json_lanes="$json_lanes${json_lanes:+,}{\"lane\":\"$lane\",\"finding\":\"$finding\",\"expected\":\"$want\",\"not_observed\":\"$unseen\",\"probes\":[$(probe_json "$lane")]}"
  done
  echo "  not observed by this run (a finding the deployment was not in is not covered):"
  for lane in $LANES; do
    finding="$(lane_finding "$lane")"
    unseen=""
    for f in $FINDINGS; do
      [ "$f" = "$finding" ] || unseen="$unseen${unseen:+, }$f"
    done
    printf '    %-11s %s\n' "$lane" "$unseen"
  done
  if [ "$status" -eq 0 ] && [ "$any_uncovered" -eq 1 ]; then
    status=3
  fi
  if [ -n "$JSON_OUT" ]; then
    printf '{"base":"%s","exit":%d,"lanes":[%s]}\n' \
      "$(json_string "$BASE")" "$status" "$json_lanes" > "$JSON_OUT"
  fi
  case "$status" in
    0) echo "deploy-probe: every lane observed, none failing." ;;
    1) echo "deploy-probe: FAILED: a lane is failing or not in its expected state." ;;
    *) echo "deploy-probe: UNCOVERED: a lane could not be observed; nothing about it passed." ;;
  esac
  return "$status"
}

probe_json() {
  local first=1 l f p e
  while IFS=$'\t' read -r l f p e; do
    [ "$l" = "$1" ] || continue
    [ "$first" -eq 1 ] || printf ','
    first=0
    printf '{"probe":"%s","finding":"%s","evidence":"%s"}' \
      "$(json_string "$p")" "$f" "$(json_string "$e")"
  done <<< "$ROWS"
}

run() {
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  probe health GET /health/liveness
  probe health GET /health/readiness
  probe health GET /health/info
  probe facade GET /fhir/metadata -H 'Accept: application/fhir+json'
  probe operations POST "/fhir/\$tofhir" \
    -H 'Content-Type: application/fhir+json' \
    -H 'Accept: application/fhir+json' \
    --data '{"resourceType":"Parameters"}'
  local status=0
  report || status=$?
  return "$status"
}

self_test() {
  local work stub
  work="$(mktemp -d)"
  stub="$work/bin"
  mkdir -p "$stub"
  # The stub answers from $STUB_ROUTES, one "METHOD path code body" line per
  # route; a route it does not list answers 404, and STUB_DOWN=1 answers
  # nothing at all.
  cat > "$stub/curl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
out="" method=GET url="" auth=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -X) method="$2"; shift 2 ;;
    -H) case "$2" in Authorization:*) auth=1 ;; *) ;; esac; shift 2 ;;
    -w | --max-time | --data) shift 2 ;;
    --) url="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[ "${STUB_DOWN:-0}" = 1 ] && { printf '000'; exit 7; }
path="/${url#*://*/}"
while read -r m p code body; do
  [ -n "$m" ] || continue
  if [ "$m" = "$method" ] && [ "$p" = "$path" ]; then
    if [ "${STUB_GUARD:-0}" = 1 ] && [ "${path#/fhir}" != "$path" ] && [ -z "$auth" ]; then
      code=401 body='{}'
    fi
    printf '%s' "$body" > "$out"
    printf '%s' "$code"
    exit 0
  fi
done <<< "$STUB_ROUTES"
printf '{}' > "$out"
printf '404'
STUB
  chmod +x "$stub/curl"

  local health='GET /health/liveness 200 {}
GET /health/readiness 200 {"status":"UP"}
GET /health/info 200 {}'
  local facade_on='GET /fhir/metadata 200 {"resourceType":"CapabilityStatement"}'
  local ops_on="POST /fhir/\$tofhir 400 {\"resourceType\":\"OperationOutcome\"}"
  local ops_off="POST /fhir/\$tofhir 503 {\"resourceType\":\"OperationOutcome\",\"issue\":[{\"code\":\"not-supported\"}]}"

  # check <name> <wanted exit> <needle> <routes> [env assignments --] [args]
  check() {
    local name="$1" code="$2" needle="$3" routes="$4" rc=0
    shift 4
    local envs=()
    while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
      envs+=("$1")
      shift
    done
    [ "$#" -gt 0 ] && shift
    env PATH="$stub:$PATH" STUB_ROUTES="$routes" "${envs[@]}" \
      bash "$0" "$@" > "$work/out" 2> "$work/err" || rc=$?
    if [ "$rc" -ne "$code" ]; then
      echo "deploy-probe: self-test failed: $name exited $rc, wanted $code." >&2
      cat "$work/out" "$work/err" >&2
      exit 1
    fi
    if [ -n "$needle" ] && ! grep -qE -- "$needle" "$work/out"; then
      echo "deploy-probe: self-test failed: $name did not say '$needle'." >&2
      cat "$work/out" "$work/err" >&2
      exit 1
    fi
  }

  check "every lane on" 0 "facade +on" "$health
$facade_on
$ops_on" STUB_X=0 -- http://bridge.test
  check "operations on through 422" 0 "operations +on" "$health
$facade_on
POST /fhir/\$tofhir 422 {\"resourceType\":\"OperationOutcome\"}" STUB_X=0 -- http://bridge.test
  check "facade off is a 404" 0 "facade +off" "$health
$ops_off" STUB_X=0 -- http://bridge.test
  check "operations off is a 503 not-supported" 0 "operations +off" "$health
$ops_off" STUB_X=0 -- http://bridge.test
  check "a 503 without not-supported is failing" 1 "operations +failing" "$health
POST /fhir/\$tofhir 503 {}" STUB_X=0 -- http://bridge.test
  check "a readiness 503 fails health" 1 "health +failing" "GET /health/liveness 200 {}
GET /health/readiness 503 {\"status\":\"DOWN\"}
GET /health/info 200 {}
$facade_on
$ops_on" STUB_X=0 -- http://bridge.test
  check "a facade 500 is failing" 1 "facade +failing" "$health
GET /fhir/metadata 500 {}
$ops_on" STUB_X=0 -- http://bridge.test
  check "a facade 200 that is no CapabilityStatement is failing" 1 "facade +failing" "$health
GET /fhir/metadata 200 <html></html>
$ops_on" STUB_X=0 -- http://bridge.test
  check "a half-mounted health lane is failing" 1 "health +failing" "GET /health/liveness 200 {}
GET /health/readiness 200 {}
$facade_on
$ops_on" STUB_X=0 -- http://bridge.test
  check "no answer is uncovered, never passed" 3 "health +uncovered" "" STUB_DOWN=1 -- http://bridge.test
  check "a guarded facade without a token is uncovered" 3 "facade +uncovered" "$health
$facade_on
$ops_on" STUB_GUARD=1 -- http://bridge.test
  printf 'synthetic-token\n' > "$work/token"
  check "a guarded facade with a token is on" 0 "operations +on" "$health
$facade_on
$ops_on" STUB_GUARD=1 -- --token-file "$work/token" http://bridge.test
  check "an operations 418 is uncovered" 3 "operations +uncovered" "$health
$facade_on
POST /fhir/\$tofhir 418 {}" STUB_X=0 -- http://bridge.test
  check "a met expectation passes" 0 "operations +off" "$health
$facade_on
$ops_off" STUB_X=0 -- --expect operations=off --expect facade=on http://bridge.test
  check "a missed expectation fails" 1 "expected on" "$health
$facade_on
$ops_off" STUB_X=0 -- --expect operations=on http://bridge.test
  check "the unobserved findings are named" 0 "facade +off, failing, uncovered" "$health
$facade_on
$ops_on" STUB_X=0 -- http://bridge.test
  check "the JSON record is written" 0 "every lane observed" "$health
$facade_on
$ops_on" STUB_X=0 -- --json "$work/record.json" http://bridge.test
  if command -v jq > /dev/null 2>&1; then
    jq -e '.exit == 0 and (.lanes | length) == 3 and .lanes[1].finding == "on"' \
      "$work/record.json" > /dev/null ||
      { echo "deploy-probe: self-test failed: the JSON record is not as written." >&2; exit 1; }
  fi
  check "an unknown lane is a usage error" 2 "" "" STUB_X=0 -- --expect hl7v2=on http://bridge.test
  check "a missing base URL is a usage error" 2 "" "" STUB_X=0 --
  check "a second argument after --self-test" 2 "" "" STUB_X=0 -- --self-test --bogus

  rm -r "$work"
  echo "deploy-probe: self-test OK."
}

if [ "$#" -eq 1 ] && [ "$1" = "--self-test" ]; then
  self_test
  exit 0
fi

while [ "$#" -gt 0 ]; do
  case "$1" in
    --expect)
      [ "$#" -ge 2 ] || usage
      case "$2" in
        health=on | health=off | facade=on | facade=off | operations=on | operations=off) ;;
        *) die "--expect takes <health|facade|operations>=<on|off>, not '$2'" ;;
      esac
      EXPECT="$EXPECT$2"$'\n'
      shift 2
      ;;
    --json)
      [ "$#" -ge 2 ] || usage
      JSON_OUT="$2"
      shift 2
      ;;
    --token-file)
      [ "$#" -ge 2 ] || usage
      [ -r "$2" ] || die "cannot read the token file '$2'"
      TOKEN_FILE="$2"
      shift 2
      ;;
    --timeout)
      [ "$#" -ge 2 ] || usage
      case "$2" in '' | *[!0-9]*) die "--timeout takes whole seconds, not '$2'" ;; *) ;; esac
      TIMEOUT="$2"
      shift 2
      ;;
    http://* | https://*)
      [ -z "$BASE" ] || usage
      BASE="${1%/}"
      shift
      ;;
    *) usage ;;
  esac
done
[ -n "$BASE" ] || usage
command -v curl > /dev/null 2>&1 || die "curl is not installed"

run
