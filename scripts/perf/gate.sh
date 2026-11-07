#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The performance gate of the bridge (#1656): ferrohealth-perf
# (FerroHEALTH/FerroSYS#249) against the release build of ferrobridge, over
# the FerroEHR CDR and its database at the "FerroEHR CDR image" and "FerroEHR
# CDR database image" pins of docs/VERSIONS.md, and RabbitMQ at its pin.
#
#   scripts/perf/gate.sh build
#       Builds target/release/ferrobridge.
#   scripts/perf/gate.sh run [--update]
#       Starts the CDR, its database and RabbitMQ on loopback, loads the
#       synthetic template, starts target/release/ferrobridge with
#       perf/ferrobridge.toml, seeds a Condition through the facade, renders
#       perf/plan.toml with what the seed made, and runs the gate against
#       perf/baseline.toml, one entry per host shape. Then it plants a poller
#       of the CDR's database that never waits and runs the idle phase of
#       perf/busy-loop.toml beside it, which must fail on idle transactions.
#       --update writes the entry of the runner's host shape from a run that
#       passes. Exit 0 when both hold, 1 when the gate fails or the planted
#       poller passes, 2 when the run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the pin reader, the plan rendering and the agreement of the
#       two plans' idle limits offline.
#
# Needs Docker (or Podman through PERF_ENGINE=podman), curl, jq, openssl and
# ferrohealth-perf on PATH; the server's CPU and memory are read from /proc,
# so the gate judges them on Linux alone. No specification governs this: our
# own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly PLAN=perf/plan.toml
readonly BUSY_PLAN=perf/busy-loop.toml
readonly CONFIG=perf/ferrobridge.toml
readonly WORK=target/perf-run
readonly BASE=http://127.0.0.1:18080
readonly CDR=http://127.0.0.1:18090/ferroehr/rest/openehr/v1
readonly CDR_ORIGIN=http://127.0.0.1:18090
readonly NETWORK=ferrobridge-perf
readonly DB_NAME=ferrobridge-perf-cdr-db
readonly CDR_NAME=ferrobridge-perf-cdr
readonly BROKER_NAME=ferrobridge-perf-broker
readonly DB_PORT=15432
# The role the CDR database image creates, in the gate's own throwaway
# container, which the run removes when it ends.
readonly CDR_DB_PASSWORD="${PERF_CDR_DB_PASSWORD:-ferroehr}"
readonly ADMIN_URL="postgresql://postgres:postgres@127.0.0.1:${DB_PORT}/ferroehr?sslmode=disable"
readonly TEMPLATE=tools/ferrobridge-testkit/fixtures/opt/diagnoses.opt
readonly CONDITION=tools/ferrobridge-testkit/fixtures/fhir/r4/condition.json
readonly PROFILE=http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis
readonly SUBJECTS=http://example.org/fhir/sid/ferrobridge-subject
readonly READER=ferrobridge-perf-events
# A poll of a table of the CDR's database with no wait between two reads.
readonly POLL='SELECT count(*) FROM public.perf_busy_loop;'
engine="${PERF_ENGINE:-docker}"

die() {
  echo "perf-gate: $*" >&2
  exit 2
}

# pin ITEM [MATRIX]: the pin of the docs/VERSIONS.md row whose first cell is
# ITEM, backticks off.
pin() {
  local item="$1" matrix="${2:-$root/docs/VERSIONS.md}"
  awk -F'|' -v item="$item" '{ gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2) }
    $2 == item { v = $3; gsub(/[[:space:]`]/, "", v); print v; exit }' "$matrix"
}

# render PLAN OUT NAME=VALUE...: PLAN with each @NAME@ replaced by its VALUE,
# refused when a placeholder is left.
render() {
  local plan="$1" out="$2" text pair
  shift 2
  text="$(< "$plan")"
  for pair in "$@"; do
    text="${text//@${pair%%=*}@/${pair#*=}}"
  done
  if grep -qE '@[A-Z_]+@' <<< "$text"; then
    echo "perf-gate: $plan holds a placeholder the gate does not fill" >&2
    return 1
  fi
  printf '%s\n' "$text" > "$out"
}

# idle_table PLAN: the [idle] table of PLAN, blank lines off.
idle_table() {
  local plan="$1"
  awk '/^\[/ { inside = ($0 == "[idle]"); next }
       inside && !/^[[:space:]]*$/ { print }' "$plan"
}

# condition SUBJECT: the synthetic Condition of the testkit, claiming the
# facade fixtures' profile, for the subject named by identifier SUBJECT.
condition() {
  local subject="$1"
  jq --compact-output --arg profile "$PROFILE" --arg system "$SUBJECTS" --arg value "$subject" \
    'del(.id) | .meta = {profile: [$profile]}
     | .subject = {identifier: {system: $system, value: $value}}' "$root/$CONDITION"
}

wait_for() {
  local what="$1" tries="$2"
  shift 2
  for _ in $(seq 1 "$tries"); do
    "$@" > /dev/null 2>&1 && return 0
    sleep 1
  done
  die "$what did not come up"
}

# broker_up: RabbitMQ has logged that it accepts connections.
broker_up() {
  "$engine" logs "$BROKER_NAME" 2>&1 | grep -qF "Server startup complete"
}

server_pid=""
poller_pid=""
cleanup() {
  [[ -n "$poller_pid" ]] && kill "$poller_pid" 2> /dev/null || true
  [[ -n "$server_pid" ]] && kill "$server_pid" 2> /dev/null || true
  [[ -n "$server_pid" ]] && wait "$server_pid" 2> /dev/null || true
  "$engine" rm --force "$CDR_NAME" "$DB_NAME" "$BROKER_NAME" > /dev/null 2>&1 || true
  "$engine" network rm "$NETWORK" > /dev/null 2>&1 || true
}

# mllp_tls: a CA and the MLLP listener's certificate for 127.0.0.1, under
# $WORK/tls, as perf/ferrobridge.toml names them.
mllp_tls() {
  local tls="$WORK/tls"
  mkdir -p "$tls"
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=ferrobridge perf CA" \
    -keyout "$tls/ca.key" -out "$tls/ca.pem" 2> /dev/null
  openssl req -newkey rsa:2048 -nodes -subj "/CN=127.0.0.1" \
    -keyout "$tls/mllp-key.pem" -out "$tls/mllp.csr" 2> /dev/null
  printf 'subjectAltName = IP:127.0.0.1, DNS:localhost\n' > "$tls/ext.cnf"
  openssl x509 -req -in "$tls/mllp.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" \
    -CAcreateserial -days 1 -extfile "$tls/ext.cnf" -out "$tls/mllp-chain.pem" 2> /dev/null
}

start() {
  local cdr_image db_image broker_image
  cdr_image="$(pin "FerroEHR CDR image")"
  db_image="$(pin "FerroEHR CDR database image")"
  broker_image="$(pin "RabbitMQ image")"
  [[ -n "$cdr_image" && -n "$db_image" && -n "$broker_image" ]] ||
    die "docs/VERSIONS.md pins no FerroEHR CDR, CDR database or RabbitMQ image"
  [[ -x target/release/ferrobridge ]] || die "target/release/ferrobridge is not built"
  rm -rf "$WORK"
  mkdir -p "$WORK"
  chmod 0700 "$WORK"
  mllp_tls

  "$engine" network create "$NETWORK" > /dev/null
  "$engine" run --quiet --detach --name "$DB_NAME" --network "$NETWORK" \
    --publish "127.0.0.1:${DB_PORT}:5432" --env POSTGRES_PASSWORD=postgres \
    "$db_image" > /dev/null
  wait_for "the CDR's PostgreSQL" 120 "$engine" exec "$DB_NAME" \
    pg_isready --host 127.0.0.1 --username ferroehr --dbname ferroehr
  "$engine" run --quiet --detach --name "$CDR_NAME" --network "$NETWORK" \
    --publish 127.0.0.1:18090:8080 \
    --env "FERROEHR__DB__URL=postgres://ferroehr:${CDR_DB_PASSWORD}@${DB_NAME}:5432/ferroehr" \
    --env FERROEHR__AUTH__ENABLED=false --env FERROEHR__EHR_INDEX__PROVIDER=local \
    --env FERROEHR__USAGE_REPORT__ENABLED=false --env "FERROEHR__OUTBOX__READERS=${READER}" \
    "$cdr_image" > /dev/null
  "$engine" run --quiet --detach --name "$BROKER_NAME" --publish 127.0.0.1:15672:5672 \
    "$broker_image" > /dev/null
  wait_for "the CDR" 120 curl --fail --silent "$CDR_ORIGIN/health/readiness"
  wait_for "RabbitMQ" 120 broker_up

  local status
  status="$(curl --silent --output "$WORK/template.json" --write-out '%{http_code}' \
    --header 'content-type: application/xml' --data-binary "@$TEMPLATE" \
    "$CDR/definition/template/adl1.4")"
  [[ "$status" = 201 ]] || die "the CDR answered $status to the template upload: $(head -c 400 "$WORK/template.json")"

  FERROBRIDGE__USAGE_REPORT__ENABLED=false target/release/ferrobridge --config "$CONFIG" serve \
    > "$WORK/server.log" 2>&1 &
  server_pid=$!
  wait_for "the bridge" 120 curl --fail --silent "$BASE/health/readiness" ||
    die "the bridge did not come up: $(tail -n 20 "$WORK/server.log")"
}

# seed: creates the Condition of the subject the read and the search keep to,
# and prints its logical id.
seed() {
  local status
  status="$(condition synthetic-subject-perf-read | curl --silent --output "$WORK/seed.json" \
    --write-out '%{http_code}' --header 'content-type: application/fhir+json' \
    --data-binary @- "$BASE/fhir/Condition")"
  [[ "$status" = 201 ]] || die "the facade answered $status to the seed: $(head -c 400 "$WORK/seed.json")"
  jq --exit-status --raw-output '.id' "$WORK/seed.json"
}

# busy_loop: the planted poller must fail the idle phase on its
# transactions.
busy_loop() {
  local code=0 summary="$WORK/busy-loop.md"
  "$engine" exec "$DB_NAME" psql --quiet --username postgres --dbname ferroehr \
    --command 'CREATE TABLE IF NOT EXISTS public.perf_busy_loop (id integer)' > /dev/null
  yes "$POLL" | "$engine" exec --interactive --env PGAPPNAME=perf-busy-loop "$DB_NAME" \
    psql --quiet --no-psqlrc --username postgres --dbname ferroehr > /dev/null 2>&1 &
  poller_pid=$!
  GITHUB_STEP_SUMMARY='' ferrohealth-perf run --plan "$BUSY_PLAN" --pid "$server_pid" \
    --base-url "$BASE" --database-url "$ADMIN_URL" --baseline "$WORK/no-baseline.toml" \
    --out-dir target/perf/busy-loop --summary "$summary" > /dev/null || code=$?
  kill "$poller_pid" 2> /dev/null || true
  poller_pid=""
  "$engine" exec "$DB_NAME" psql --quiet --username postgres --dbname ferroehr --command \
    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = 'perf-busy-loop'" > /dev/null
  local breaches
  breaches="$(awk '/^### Breaches/ { on = 1; next } /^### / { on = 0 } on' "$summary")"
  if [[ "$code" -eq 1 ]] && grep -qF 'idle.transactions_per_second' <<< "$breaches"; then
    echo "perf-gate: the planted poller fails the idle phase on its transactions"
    report_busy_loop "failed the gate, as it must" "$summary"
    return 0
  fi
  echo "perf-gate: the planted poller was not failed on idle transactions (exit $code)" >&2
  cat "$summary" >&2
  report_busy_loop "was NOT failed on idle transactions (exit $code)" "$summary"
  return 1
}

report_busy_loop() {
  local outcome="$1" summary="$2"
  [[ -n "${GITHUB_STEP_SUMMARY:-}" ]] || return 0
  {
    printf '## The planted idle busy-loop\n\nA poller of the CDR database that never waits %s.\n\n' "$outcome"
    awk '/^### Breaches/ { on = 1 } /^### Checks/ { on = 0 } on' "$summary"
  } >> "$GITHUB_STEP_SUMMARY"
}

run() {
  local update=()
  case "${1:-}" in
    --update) update=(--update) ;;
    "") ;;
    *) die "usage: gate.sh run [--update]" ;;
  esac
  cd "$root"
  command -v ferrohealth-perf > /dev/null || die "ferrohealth-perf is not on PATH"
  trap cleanup EXIT
  start
  local read_id code=0
  read_id="$(seed)" || exit 2
  render "$PLAN" "$WORK/plan.toml" \
    "CREATE_BODY=$(condition synthetic-subject-perf-write)" \
    "READ_ID=$read_id" \
    "SEARCH=subject:identifier=$(jq --raw-input --raw-output '@uri' <<< "$SUBJECTS|synthetic-subject-perf-read")" ||
    exit 2
  ferrohealth-perf run --plan "$WORK/plan.toml" --pid "$server_pid" --base-url "$BASE" \
    --database-url "$ADMIN_URL" --baseline perf/baseline.toml --out-dir target/perf \
    ${update[@]+"${update[@]}"} || code=$?
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    echo "perf-gate: the bridge log follows" >&2
    tail -n 50 "$WORK/server.log" >&2
  fi
  exit "$code"
}

self_test() {
  local work n=0 failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now
  trap "rm -r -- '$work'" EXIT
  check() {
    local label="$1" want="$2" got="$3"
    n=$((n + 1))
    if [[ "$want" != "$got" ]]; then
      echo "  FAIL: $label: expected [$want], got [$got]" >&2
      failed=1
    fi
  }
  # shellcheck disable=SC2016 # the backticks are the row's, never expanded
  printf '| Item | Pin | Repeated in |\n|---|---|---|\n| RabbitMQ image | `rabbitmq:1@sha256:00` | x |\n' > "$work/matrix.md"
  check "a pin is read without its backticks" "rabbitmq:1@sha256:00" "$(pin "RabbitMQ image" "$work/matrix.md")"
  check "a missing row reads empty" "" "$(pin "Nothing" "$work/matrix.md")"
  local item
  for item in "FerroEHR CDR image" "FerroEHR CDR database image" "RabbitMQ image"; do
    check "the $item is pinned by digest" 1 "$([[ "$(pin "$item")" == *@sha256:* ]] && echo 1 || echo 0)"
  done
  printf "path = \"/fhir/Condition/@READ_ID@\"\nbody = '''@CREATE_BODY@'''\n" > "$work/plan.toml"
  render "$work/plan.toml" "$work/out.toml" "READ_ID=c-1" 'CREATE_BODY={"a":"b"}'
  check "the placeholders are filled" "$(printf "path = \"/fhir/Condition/c-1\"\nbody = '''{\"a\":\"b\"}'''")" "$(< "$work/out.toml")"
  check "a placeholder left is refused" 1 \
    "$(render "$work/plan.toml" "$work/out.toml" "READ_ID=c-1" 2> /dev/null && echo 0 || echo 1)"
  check "the committed plan is filled by the gate's three values" 0 \
    "$(render "$root/$PLAN" "$work/out.toml" CREATE_BODY=x READ_ID=x SEARCH=x 2> /dev/null && echo 0 || echo 1)"
  check "the seeded Condition names its subject and the profile" \
    "$PROFILE s-1" "$(condition s-1 | jq --raw-output '"\(.meta.profile[0]) \(.subject.identifier.value)"')"
  check "the seeded Condition carries no id" null "$(condition s-1 | jq --raw-output '.id')"
  printf '[idle]\n# a note\nmax = 1.0\n\n[load]\nrate = 2.0\n' > "$work/plan.toml"
  check "the idle table is read alone" "$(printf '# a note\nmax = 1.0')" "$(idle_table "$work/plan.toml")"
  check "the gate and the busy-loop plan hold the same idle limits" \
    "$(idle_table "$root/$PLAN")" "$(idle_table "$root/$BUSY_PLAN")"
  [[ -n "$(idle_table "$root/$PLAN")" ]] || {
    echo "  FAIL: $PLAN holds no [idle] table" >&2
    failed=1
  }
  [[ "$failed" -eq 0 ]] || exit 1
  echo "perf-gate: self-test OK ($n checks)."
}

case "${1:-}" in
  build)
    cd "$root"
    cargo build --release --locked -p ferrobridge-server --bin ferrobridge
    ;;
  run)
    shift
    run "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh build | run [--update] | --self-test" ;;
esac
