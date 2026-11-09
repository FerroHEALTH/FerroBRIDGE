#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The performance gate of the bridge (#1656): ferrohealth-perf
# (FerroHEALTH/FerroSYS#249) in its A/B mode, the head of a change judged
# against its base measured beside it on the same runner. Each arm is the
# release build of ferrobridge over its own FerroEHR CDR and its own CDR
# database, at the "FerroEHR CDR image" and "FerroEHR CDR database image"
# pins of docs/VERSIONS.md, with one RabbitMQ and one FerroTERM terminology
# server at their pins for both.
#
#   scripts/perf/gate.sh build [BASE]
#       Builds target/release/ferrobridge of the checkout, the head, and of
#       its base: the merge base of HEAD with BASE, a commit or ref, or with
#       no BASE the newest v* tag before HEAD, else HEAD's first parent. Both
#       arms build in one tree, target/perf-build, with its own toolchain pin:
#       first the base from `git archive` of that commit, then the same tree
#       brought to the checkout by rewriting only the files that differ, so
#       cargo rebuilds for the head only the crates the change reaches.
#   scripts/perf/gate.sh pack FILE
#       Writes FILE, a tar of both built arms, the base's commit and the
#       ferrohealth-perf on PATH, so a later job on another runner measures
#       the same binaries without building them again.
#   scripts/perf/gate.sh unpack FILE
#       Restores what `pack` wrote into the checkout, puts ferrohealth-perf
#       under target/perf-tools, and extracts the base's tree from its commit
#       with `git archive`, which needs that commit in the checkout's history.
#   scripts/perf/gate.sh run
#       Starts both CDRs, their databases, RabbitMQ and FerroTERM over the
#       v2-to-FHIR guide's table ConceptMaps on loopback, loads the
#       two synthetic templates into each CDR, starts each arm's ferrobridge
#       in its own tree with the head's perf/ferrobridge.toml, the harness the
#       plan is written for, over its tree's facade and HL7 v2 mapping
#       fixtures, seeds a
#       Condition through each facade, and runs `ferrohealth-perf ab` over
#       perf/plan.toml with each arm's values: the head's idle phase held to
#       absolute limits, then pairs of load phases, the facade's requests and
#       an ORU^R01 over MLLP with TLS. Then it plants a poller of the head's
#       CDR database that never waits and runs the idle phase of
#       perf/busy-loop.toml beside it, which must fail on idle transactions.
#       Exit 0 when both hold, 1 when the gate fails or the planted poller
#       passes, 2 when the run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the pin reader, the plan's placeholders, the seeded Condition,
#       the agreement of the two plans' idle limits and the base resolution
#       offline.
#
# Needs Docker (or Podman through PERF_ENGINE=podman), curl, git, jq,
# openssl and ferrohealth-perf on PATH; the servers' CPU and memory are read
# from /proc, so the gate judges them on Linux alone. No specification
# governs this: our own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly PLAN=perf/plan.toml
readonly BUSY_PLAN=perf/busy-loop.toml
readonly CONFIG=perf/ferrobridge.toml
readonly WORK=target/perf-run
readonly BASE_TREE=target/perf-base
readonly BUILD_TREE=target/perf-build
readonly TOOLS=target/perf-tools
readonly NETWORK=ferrobridge-perf
readonly BROKER_NAME=ferrobridge-perf-broker
readonly TERMINOLOGY_NAME=ferrobridge-perf-terminology
readonly TERMINOLOGY=http://127.0.0.1:18100
# The role the CDR database image creates, in the gate's own throwaway
# containers, which the run removes when it ends.
readonly CDR_DB_PASSWORD="${PERF_CDR_DB_PASSWORD:-ferroehr}"
readonly TEMPLATES=(tools/ferrobridge-testkit/fixtures/opt/diagnoses.opt tools/ferrobridge-testkit/fixtures/opt/diagnose.opt)
# The facade's mapping set and the HL7 v2 face's Observation context, under
# each arm's own tree.
readonly MAPPINGS=(app/ferrobridge-server/tests/it/repeated app/ferrobridge-server/tests/it/hl7v2_fixtures)
# The v2-to-FHIR guide and FerroBRIDGE's value package, whose table
# ConceptMaps the terminology server answers the HL7 v2 face from.
readonly GUIDE_MAPS=tools/fhir-codegen/vendor/hl7.fhir.uv.v2mappings/package
readonly VALUE_MAPS=terminology/hl7v2-values
# HL7 Terminology, which holds the value sets the guide's table maps name as
# their source.
readonly THO=tools/fhir-codegen/vendor/hl7.terminology/package
# Where a tree from before the maps were generated keeps the guide, which its
# binary still reads through [hl7v2] concept_maps.
readonly DIRECTORY_MAPS=crates/ferrobridge-hl7v2/vendor/hl7.fhir.uv.v2mappings/package
readonly CONDITION=tools/ferrobridge-testkit/fixtures/fhir/r4/condition.json
readonly PROFILE=http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis
readonly SUBJECTS=http://example.org/fhir/sid/ferrobridge-subject
readonly READER=ferrobridge-perf-events
# The subject named in the HL7 v2 face's sender list and the PERFLAB
# client certificate.
readonly SENDER_SUBJECT=perflab.example.org
# A poll of a table of the CDR's database with no wait between two reads.
readonly POLL='SELECT count(*) FROM public.perf_busy_loop;'
engine="${PERF_ENGINE:-docker}"

die() {
  echo "perf-gate: $*" >&2
  exit 2
}

# The loopback ports of each arm: the facade, the MLLP listener, the CDR and
# its database.
port() {
  case "$1:$2" in
    head:facade) echo 18080 ;;
    base:facade) echo 18081 ;;
    head:mllp) echo 12575 ;;
    base:mllp) echo 12576 ;;
    head:cdr) echo 18090 ;;
    base:cdr) echo 18091 ;;
    head:db) echo 15432 ;;
    base:db) echo 15433 ;;
    *) die "no port $2 for $1" ;;
  esac
}

facade() { echo "http://127.0.0.1:$(port "$1" facade)"; }
cdr_origin() { echo "http://127.0.0.1:$(port "$1" cdr)"; }
admin_url() { echo "postgresql://postgres:postgres@127.0.0.1:$(port "$1" db)/ferroehr?sslmode=disable"; }

# pin ITEM [MATRIX]: the pin of the docs/VERSIONS.md row whose first cell is
# ITEM, backticks off.
pin() {
  local item="$1" matrix="${2:-$root/docs/VERSIONS.md}"
  awk -F'|' -v item="$item" '{ gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2) }
    $2 == item { v = $3; gsub(/[[:space:]`]/, "", v); print v; exit }' "$matrix"
}

# placeholders PLAN: the names of PLAN's placeholders, sorted, one per line.
placeholders() {
  grep -oE '@[A-Z][A-Z0-9_]*@' "$1" | tr -d '@' | sort -u
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

# base_rev [BASE] [AT]: the commit the base is built from, as `build` names
# it, resolved from AT (HEAD by default).
base_rev() {
  local base="${1:-}" at="${2:-HEAD}" tag
  # The commit before the push that created a branch is all zeros.
  [[ "$base" =~ ^0+$ ]] && base=""
  if [[ -n "$base" ]]; then
    git -C "$root" merge-base "$at" "$base"
  elif tag="$(git -C "$root" describe --tags --abbrev=0 --match 'v*' "$at^" 2> /dev/null)"; then
    git -C "$root" rev-parse "$tag^{commit}"
  else
    git -C "$root" rev-parse "$at^1"
  fi
}

# to_checkout REV TREE: brings TREE, an extract of REV, to the checkout's
# files: each file the checkout changes against REV (committed or not) or
# adds untracked is copied, and each it lacks is removed. A file left alone
# keeps its modification time, which is what tells cargo it is unchanged.
to_checkout() {
  local rev="$1" tree="$2" path
  while IFS= read -r -d '' path; do
    if [[ -e "$root/$path" || -L "$root/$path" ]]; then
      mkdir -p "$tree/$(dirname "$path")"
      rm -f "$tree/$path"
      cp -P "$root/$path" "$tree/$path"
    else
      rm -f "$tree/$path"
    fi
  done < <(git -C "$root" diff --name-only --no-renames -z "$rev" -- && git -C "$root" ls-files --others --exclude-standard -z)
}

# build [BASE]: the base's release binary into target/perf-base/ferrobridge,
# then the head's into target/release/ferrobridge, both from one tree and one
# target directory.
build() {
  local rev
  cd "$root"
  rev="$(base_rev "${1:-}")" || die "the base of ${1:-the previous release} cannot be resolved"
  echo "perf-gate: the base is $rev"
  rm -rf "$BASE_TREE" "$BUILD_TREE"
  mkdir -p "$BUILD_TREE"
  base_source "$rev"
  git archive "$rev" | tar -x -C "$BUILD_TREE"
  # From inside the build tree, so its own toolchain pin builds each arm.
  (cd "$BUILD_TREE" && cargo build --release --locked -p ferrobridge-server --bin ferrobridge --target-dir "$root/target")
  cp target/release/ferrobridge "$BASE_TREE/ferrobridge"
  to_checkout "$rev" "$BUILD_TREE"
  (cd "$BUILD_TREE" && cargo build --release --locked -p ferrobridge-server --bin ferrobridge --target-dir "$root/target")
}

# base_source REV: the base's tree of REV under target/perf-base/src, and
# REV in target/perf-base/rev.
base_source() {
  local rev="$1"
  rm -rf "$BASE_TREE/src"
  mkdir -p "$BASE_TREE/src"
  git -C "$root" archive "$rev" | tar -x -C "$root/$BASE_TREE/src"
  printf '%s\n' "$rev" > "$root/$BASE_TREE/rev"
}

# pack FILE: both arms, the base's commit and ferrohealth-perf, in one tar
# that keeps their modes.
pack() {
  [[ $# -eq 1 ]] || die "usage: gate.sh pack FILE"
  local file="$1" tool
  cd "$root"
  [[ -x target/release/ferrobridge && -x "$BASE_TREE/ferrobridge" && -s "$BASE_TREE/rev" ]] ||
    die "both arms are not built: run gate.sh build first"
  tool="$(command -v ferrohealth-perf)" || die "ferrohealth-perf is not on PATH"
  mkdir -p "$TOOLS"
  [[ "$tool" -ef "$TOOLS/ferrohealth-perf" ]] || cp "$tool" "$TOOLS/ferrohealth-perf"
  tar -cf "$file" target/release/ferrobridge "$BASE_TREE/ferrobridge" "$BASE_TREE/rev" "$TOOLS/ferrohealth-perf"
}

# unpack FILE: what pack wrote, back in the checkout, and the base's tree
# extracted again from its commit.
unpack() {
  [[ $# -eq 1 ]] || die "usage: gate.sh unpack FILE"
  local file rev
  file="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
  cd "$root"
  tar -xf "$file"
  rev="$(< "$BASE_TREE/rev")"
  git cat-file -e "$rev^{commit}" 2> /dev/null || die "the base $rev is not in this checkout's history"
  base_source "$rev"
}

head_pid=""
base_pid=""
poller_pid=""
cleanup() {
  [[ -n "$poller_pid" ]] && kill "$poller_pid" 2> /dev/null || true
  local pid arm
  for pid in "$head_pid" "$base_pid"; do
    [[ -n "$pid" ]] && kill "$pid" 2> /dev/null || true
    [[ -n "$pid" ]] && wait "$pid" 2> /dev/null || true
  done
  for arm in head base; do
    "$engine" rm --force "ferrobridge-perf-$arm-cdr" "ferrobridge-perf-$arm-db" > /dev/null 2>&1 || true
  done
  "$engine" rm --force "$BROKER_NAME" "$TERMINOLOGY_NAME" > /dev/null 2>&1 || true
  "$engine" network rm "$NETWORK" > /dev/null 2>&1 || true
}

# tls: a CA, the MLLP listeners' certificate for 127.0.0.1, and the PERFLAB
# sender's client identity (its certificate and key in one file), under
# $WORK/tls.
tls() {
  local dir="$WORK/tls"
  mkdir -p "$dir"
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=ferrobridge perf CA" \
    -keyout "$dir/ca.key" -out "$dir/ca.pem" 2> /dev/null
  openssl req -newkey rsa:2048 -nodes -subj "/CN=127.0.0.1" \
    -keyout "$dir/mllp-key.pem" -out "$dir/mllp.csr" 2> /dev/null
  printf 'subjectAltName = IP:127.0.0.1, DNS:localhost\n' > "$dir/server.cnf"
  openssl x509 -req -in "$dir/mllp.csr" -CA "$dir/ca.pem" -CAkey "$dir/ca.key" \
    -CAcreateserial -days 1 -extfile "$dir/server.cnf" -out "$dir/mllp-chain.pem" 2> /dev/null
  openssl req -newkey rsa:2048 -nodes -subj "/CN=$SENDER_SUBJECT" \
    -keyout "$dir/perflab-key.pem" -out "$dir/perflab.csr" 2> /dev/null
  printf 'subjectAltName = DNS:%s\nextendedKeyUsage = clientAuth\n' "$SENDER_SUBJECT" > "$dir/client.cnf"
  openssl x509 -req -in "$dir/perflab.csr" -CA "$dir/ca.pem" -CAkey "$dir/ca.key" \
    -CAcreateserial -days 1 -extfile "$dir/client.cnf" -out "$dir/perflab-cert.pem" 2> /dev/null
  cat "$dir/perflab-cert.pem" "$dir/perflab-key.pem" > "$dir/perflab.pem"
}

# cdr ARM: starts ARM's CDR database and CDR on the gate's network.
cdr() {
  local arm="$1" db_image="$2" cdr_image="$3" db="ferrobridge-perf-$1-db"
  "$engine" run --quiet --detach --name "$db" --network "$NETWORK" \
    --publish "127.0.0.1:$(port "$arm" db):5432" --env POSTGRES_PASSWORD=postgres \
    "$db_image" > /dev/null
  wait_for "the $arm CDR's PostgreSQL" 120 "$engine" exec "$db" \
    pg_isready --host 127.0.0.1 --username ferroehr --dbname ferroehr
  "$engine" run --quiet --detach --name "ferrobridge-perf-$arm-cdr" --network "$NETWORK" \
    --publish "127.0.0.1:$(port "$arm" cdr):8080" \
    --env "FERROEHR__DB__URL=postgres://ferroehr:${CDR_DB_PASSWORD}@${db}:5432/ferroehr" \
    --env FERROEHR__AUTH__ENABLED=false --env FERROEHR__EHR_INDEX__PROVIDER=local \
    --env FERROEHR__USAGE_REPORT__ENABLED=false --env "FERROEHR__OUTBOX__READERS=${READER}" \
    "$cdr_image" > /dev/null
}

# templates ARM TREE: loads TREE's synthetic templates into ARM's CDR.
templates() {
  local arm="$1" tree="$2" template status
  for template in "${TEMPLATES[@]}"; do
    status="$(curl --silent --output "$WORK/$arm/template.json" --write-out '%{http_code}' \
      --header 'content-type: application/xml' --data-binary "@$tree/$template" \
      "$(cdr_origin "$arm")/ferroehr/rest/openehr/v1/definition/template/adl1.4")"
    [[ "$status" = 201 ]] ||
      die "the $arm CDR answered $status to $template: $(head -c 400 "$WORK/$arm/template.json")"
  done
}

# serve ARM BINARY TREE: starts ARM's bridge in TREE with the head's
# perf/ferrobridge.toml over TREE's mapping fixtures, and prints its process
# id. Both arms take the head's harness, as they take the head's plan, so a
# change to the harness reaches both; a key the base binary does not know
# stops the base at boot, which the run reports with exit 2.
serve() {
  local arm="$1" binary="$2" tree="$3" dir="$root/$WORK/$1" config="$root/$CONFIG" fixtures
  mkdir -p "$dir/mappings"
  for fixtures in "${MAPPINGS[@]}"; do
    cp "$tree/$fixtures"/*.yml "$dir/mappings/"
  done
  (
    cd "$tree"
    if [[ -d "$tree/$DIRECTORY_MAPS" ]]; then
      export FERROBRIDGE__HL7V2__CONCEPT_MAPS="$tree/$DIRECTORY_MAPS"
    fi
    FERROBRIDGE__USAGE_REPORT__ENABLED=false \
      FERROBRIDGE__SERVER__LISTEN="127.0.0.1:$(port "$arm" facade)" \
      FERROBRIDGE__CDR__BASE_URL="$(cdr_origin "$arm")/ferroehr/rest/openehr/v1" \
      FERROBRIDGE__MAPPINGS__DIRECTORY="$dir/mappings" \
      FERROBRIDGE__FACADE__BASE_URL="$(facade "$arm")/fhir" \
      FERROBRIDGE__FACADE__IDENTITY_STORE="$dir/identity.redb" \
      FERROBRIDGE__HL7V2__LISTEN="127.0.0.1:$(port "$arm" mllp)" \
      FERROBRIDGE__HL7V2__TLS__CERTIFICATE_FILE="$root/$WORK/tls/mllp-chain.pem" \
      FERROBRIDGE__HL7V2__TLS__KEY_FILE="$root/$WORK/tls/mllp-key.pem" \
      FERROBRIDGE__HL7V2__TLS__CLIENT_CA_FILE="$root/$WORK/tls/ca.pem" \
      FERROBRIDGE__CHANGE_FEED__URL="$(cdr_origin "$arm")/ferroehr/rest/extensions/v1" \
      FERROBRIDGE__CHANGE_FEED__DEAD_LETTERS="$dir/dead-letters.redb" \
      FERROBRIDGE__EVENTS__EXCHANGE="ferrobridge.perf.$arm" \
      FERROBRIDGE__TERMINOLOGY__BASE_URL="$TERMINOLOGY/r4" \
      FERROBRIDGE__TERMINOLOGY__WIRE_VERSION=r4 \
      exec "$binary" --config "$config" serve > "$dir/server.log" 2>&1
  ) &
  echo "$!"
}

# terminology IMAGE: starts FerroTERM over HL7 Terminology, the guide and the
# value package, packed and unpacked as an operator loads it, whose table
# ConceptMaps the HL7 v2 face translates every table value through.
terminology() {
  local packed="$root/$WORK/hl7v2-values" tarball
  rm -rf "$packed"
  mkdir -p "$packed/unpacked"
  tarball="$(bash "$root/scripts/terminology/pack-hl7v2-values.sh" "$packed/packed")"
  tar -xzf "$tarball" -C "$packed/unpacked"
  "$engine" run --quiet --detach --name "$TERMINOLOGY_NAME" --publish 127.0.0.1:18100:8080 \
    --volume "$root/$THO:/data/hl7.terminology:ro" \
    --volume "$root/$GUIDE_MAPS:/data/hl7.fhir.uv.v2mappings:ro" \
    --volume "$packed/unpacked/package:/data/hl7v2-values:ro" \
    --env 'FERROTERM__TERMINOLOGY__CODE_SYSTEMS=["/data/hl7.terminology","/data/hl7.fhir.uv.v2mappings","/data/hl7v2-values"]' \
    --env FERROTERM__USAGE_REPORT__ENABLED=false "$1" > /dev/null
}

start() {
  local cdr_image db_image broker_image terminology_image arm
  cdr_image="$(pin "FerroEHR CDR image")"
  db_image="$(pin "FerroEHR CDR database image")"
  broker_image="$(pin "RabbitMQ image")"
  terminology_image="$(pin "FerroTERM terminology server image")"
  [[ -n "$cdr_image" && -n "$db_image" && -n "$broker_image" && -n "$terminology_image" ]] ||
    die "docs/VERSIONS.md pins no FerroEHR CDR, CDR database, RabbitMQ or FerroTERM image"
  [[ -x target/release/ferrobridge ]] || die "target/release/ferrobridge is not built"
  [[ -x "$BASE_TREE/ferrobridge" ]] || die "$BASE_TREE/ferrobridge is not built: run gate.sh build first"
  rm -rf "$WORK"
  mkdir -p "$WORK/head" "$WORK/base"
  chmod 0700 "$WORK"
  tls

  "$engine" network create "$NETWORK" > /dev/null
  "$engine" run --quiet --detach --name "$BROKER_NAME" --publish 127.0.0.1:15672:5672 \
    "$broker_image" > /dev/null
  terminology "$terminology_image"
  for arm in head base; do
    cdr "$arm" "$db_image" "$cdr_image"
  done
  for arm in head base; do
    wait_for "the $arm CDR" 120 curl --fail --silent "$(cdr_origin "$arm")/health/readiness"
  done
  wait_for "RabbitMQ" 120 broker_up
  wait_for "FerroTERM" 300 curl --fail --silent "$TERMINOLOGY/health"
  templates head "$root"
  templates base "$root/$BASE_TREE/src"

  head_pid="$(serve head "$root/target/release/ferrobridge" "$root")"
  base_pid="$(serve base "$root/$BASE_TREE/ferrobridge" "$root/$BASE_TREE/src")"
  for arm in head base; do
    wait_for "the $arm bridge" 120 curl --fail --silent "$(facade "$arm")/health/readiness" ||
      die "the $arm bridge did not come up: $(tail -n 20 "$WORK/$arm/server.log")"
  done
}

# seed ARM: creates the Condition of the subject the read and the search
# keep to through ARM's facade, and prints its logical id.
seed() {
  local arm="$1" status
  status="$(condition synthetic-subject-perf-read | curl --silent --output "$WORK/$arm/seed.json" \
    --write-out '%{http_code}' --header 'content-type: application/fhir+json' \
    --data-binary @- "$(facade "$arm")/fhir/Condition")"
  [[ "$status" = 201 ]] ||
    die "the $arm facade answered $status to the seed: $(head -c 400 "$WORK/$arm/seed.json")"
  jq --exit-status --raw-output '.id' "$WORK/$arm/seed.json"
}

# busy_loop: the planted poller must fail the head's idle phase on its
# transactions.
busy_loop() {
  local code=0 summary="$WORK/busy-loop.md" db=ferrobridge-perf-head-db
  "$engine" exec "$db" psql --quiet --username postgres --dbname ferroehr \
    --command 'CREATE TABLE IF NOT EXISTS public.perf_busy_loop (id integer)' > /dev/null
  yes "$POLL" | "$engine" exec --interactive --env PGAPPNAME=perf-busy-loop "$db" \
    psql --quiet --no-psqlrc --username postgres --dbname ferroehr > /dev/null 2>&1 &
  poller_pid=$!
  GITHUB_STEP_SUMMARY='' ferrohealth-perf run --plan "$BUSY_PLAN" --pid "$head_pid" \
    --base-url "$(facade head)" --database-url "$(admin_url head)" \
    --baseline "$WORK/no-baseline.toml" \
    --out-dir target/perf/busy-loop --summary "$summary" > /dev/null || code=$?
  kill "$poller_pid" 2> /dev/null || true
  poller_pid=""
  "$engine" exec "$db" psql --quiet --username postgres --dbname ferroehr --command \
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
    printf '## The planted idle busy-loop\n\nA poller of the head CDR database that never waits %s.\n\n' "$outcome"
    awk '/^### Breaches/ { on = 1 } /^### Checks/ { on = 0 } on' "$summary"
  } >> "$GITHUB_STEP_SUMMARY"
}

run() {
  [[ $# -eq 0 ]] || die "usage: gate.sh run"
  cd "$root"
  command -v ferrohealth-perf > /dev/null || die "ferrohealth-perf is not on PATH"
  trap cleanup EXIT
  start
  local head_id base_id body search code=0 arm
  head_id="$(seed head)" || exit 2
  base_id="$(seed base)" || exit 2
  body="$(condition synthetic-subject-perf-write)"
  search="subject:identifier=$(jq --raw-input --raw-output '@uri' <<< "$SUBJECTS|synthetic-subject-perf-read")"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf "The base is \`%s\`.\n\n" "$(< "$BASE_TREE/rev")" >> "$GITHUB_STEP_SUMMARY"
  fi
  ferrohealth-perf ab --plan "$PLAN" \
    --base-url "$(facade base)" --base-pid "$base_pid" \
    --base-mllp-address "127.0.0.1:$(port base mllp)" --base-database-url "$(admin_url base)" \
    --base-var "READ_ID=$base_id" --base-var "CREATE_BODY=$body" --base-var "SEARCH=$search" \
    --head-url "$(facade head)" --head-pid "$head_pid" \
    --head-mllp-address "127.0.0.1:$(port head mllp)" --head-database-url "$(admin_url head)" \
    --head-var "READ_ID=$head_id" --head-var "CREATE_BODY=$body" --head-var "SEARCH=$search" \
    --out-dir target/perf || code=$?
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    for arm in head base; do
      echo "perf-gate: the $arm bridge's log follows" >&2
      tail -n 50 "$WORK/$arm/server.log" >&2
    done
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
  for item in "FerroEHR CDR image" "FerroEHR CDR database image" "RabbitMQ image" "FerroTERM terminology server image"; do
    check "the $item is pinned by digest" 1 "$([[ "$(pin "$item")" == *@sha256:* ]] && echo 1 || echo 0)"
  done
  printf "path = \"/x/@READ_ID@/@A_1@\"\nbody = 'a@b.org @lower@ @READ_ID@'\n" > "$work/plan.toml"
  check "the placeholders are read once each" "$(printf 'A_1\nREAD_ID')" "$(placeholders "$work/plan.toml")"
  check "the committed plan holds the three placeholders the gate gives each arm" \
    "$(printf 'CREATE_BODY\nREAD_ID\nSEARCH')" "$(placeholders "$root/$PLAN")"
  check "the seeded Condition names its subject and the profile" \
    "$PROFILE s-1" "$(condition s-1 | jq --raw-output '"\(.meta.profile[0]) \(.subject.identifier.value)"')"
  check "the seeded Condition carries no id" null "$(condition s-1 | jq --raw-output '.id')"
  check "the arms listen apart" "18080 18081 12575 12576" \
    "$(port head facade) $(port base facade) $(port head mllp) $(port base mllp)"
  local file
  for file in "${TEMPLATES[@]}" "$CONDITION" perf/messages/oru-r01.hl7; do
    check "$file exists" 1 "$([[ -f "$root/$file" ]] && echo 1 || echo 0)"
  done
  check "the guide's table ConceptMaps are vendored" 1 \
    "$(compgen -G "$root/$GUIDE_MAPS/ConceptMap-table-*.json" > /dev/null && echo 1 || echo 0)"
  check "the value package holds table ConceptMaps" 1 \
    "$(compgen -G "$root/$VALUE_MAPS/ConceptMap-table-*.json" > /dev/null && echo 1 || echo 0)"
  for file in "${MAPPINGS[@]}"; do
    check "$file holds mapping files" 1 "$(compgen -G "$root/$file/*.yml" > /dev/null && echo 1 || echo 0)"
  done
  printf '[idle]\n# a note\nmax = 1.0\n\n[load]\nrate = 2.0\n' > "$work/plan.toml"
  check "the idle table is read alone" "$(printf '# a note\nmax = 1.0')" "$(idle_table "$work/plan.toml")"
  check "the gate and the busy-loop plan hold the same idle limits" \
    "$(idle_table "$root/$PLAN")" "$(idle_table "$root/$BUSY_PLAN")"
  [[ -n "$(idle_table "$root/$PLAN")" ]] || {
    echo "  FAIL: $PLAN holds no [idle] table" >&2
    failed=1
  }
  local repo="$work/repo" saved_root="$root"
  git init --quiet "$repo"
  commit() { git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false -c tag.gpgsign=false commit --quiet --allow-empty -m "$1"; }
  commit one
  commit two
  git -C "$repo" -c tag.gpgsign=false tag v0.0.1
  commit three
  git -C "$repo" branch side
  commit four
  git -C "$repo" checkout --quiet side
  commit side-only
  git -C "$repo" checkout --quiet -
  root="$repo"
  check "with no BASE the base is the newest tag before HEAD" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev "")"
  check "a BASE names its merge base with HEAD" \
    "$(git -C "$repo" rev-parse HEAD~1)" "$(base_rev side)"
  check "an all-zero BASE takes the previous release" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev 0000000000000000000000000000000000000000)"
  check "with no tag before it the base is the first parent" \
    "$(git -C "$repo" rev-parse v0.0.1~1)" "$(base_rev "" v0.0.1)"
  printf 'a\n' > "$repo/kept"
  printf 'b\n' > "$repo/changed"
  printf 'c\n' > "$repo/gone"
  git -C "$repo" add kept changed gone
  commit files
  local from tree="$work/tree"
  from="$(git -C "$repo" rev-parse HEAD)"
  mkdir -p "$tree"
  git -C "$repo" archive "$from" | tar -x -C "$tree"
  touch -t 200001010000 "$tree/kept" "$tree/changed"
  printf 'B\n' > "$repo/changed"
  git -C "$repo" rm --quiet gone
  commit edit
  mkdir -p "$repo/new"
  printf 'n\n' > "$repo/new/file"
  printf 'u\n' > "$repo/kept"
  git -C "$repo" checkout --quiet -- kept
  to_checkout "$from" "$tree"
  check "a changed file takes the checkout's content" B "$(cat "$tree/changed")"
  check "a changed file is newer than the extract" 1 "$([[ "$tree/changed" -nt "$tree/kept" ]] && echo 1 || echo 0)"
  check "an unchanged file keeps its time" 200001010000 "$(date -r "$tree/kept" +%Y%m%d%H%M 2> /dev/null || stat -c %y "$tree/kept")"
  check "a file the checkout removed is removed" 0 "$([[ -e "$tree/gone" ]] && echo 1 || echo 0)"
  check "an untracked file is copied" n "$(cat "$tree/new/file")"
  mkdir -p "$repo/target/release" "$repo/$BASE_TREE" "$work/bin"
  printf '#!/bin/sh\necho head\n' > "$repo/target/release/ferrobridge"
  printf '#!/bin/sh\necho base\n' > "$repo/$BASE_TREE/ferrobridge"
  printf '#!/bin/sh\necho perf\n' > "$work/bin/ferrohealth-perf"
  chmod 0755 "$repo/target/release/ferrobridge" "$repo/$BASE_TREE/ferrobridge" "$work/bin/ferrohealth-perf"
  printf '%s\n' "$from" > "$repo/$BASE_TREE/rev"
  check "both arms are packed" 0 "$( (PATH="$work/bin:$PATH" pack "$work/arms.tar") > /dev/null 2>&1; echo $?)"
  rm -rf "$repo/target"
  check "the packed arms unpack" 0 "$( (unpack "$work/arms.tar") > /dev/null 2>&1; echo $?)"
  check "the head unpacks runnable" head "$("$repo/target/release/ferrobridge")"
  check "the base unpacks runnable" base "$("$repo/$BASE_TREE/ferrobridge")"
  check "ferrohealth-perf unpacks runnable" perf "$("$repo/$TOOLS/ferrohealth-perf")"
  check "the base's tree is extracted from its commit" b "$(cat "$repo/$BASE_TREE/src/changed" 2> /dev/null)"
  check "the base's tree holds what its commit removed later" c "$(cat "$repo/$BASE_TREE/src/gone" 2> /dev/null)"
  printf '%s\n' 0123456789012345678901234567890123456789 > "$repo/$BASE_TREE/rev"
  (cd "$repo" && tar -cf "$work/stranger.tar" target/release/ferrobridge "$BASE_TREE/ferrobridge" "$BASE_TREE/rev" "$TOOLS/ferrohealth-perf")
  check "a base outside the history is refused" 2 "$( (unpack "$work/stranger.tar") > /dev/null 2>&1; echo $?)"
  root="$saved_root"
  [[ "$failed" -eq 0 ]] || exit 1
  echo "perf-gate: self-test OK ($n checks)."
}

case "${1:-}" in
  build)
    shift
    build "$@"
    ;;
  run)
    shift
    run "$@"
    ;;
  pack)
    shift
    pack "$@"
    ;;
  unpack)
    shift
    unpack "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh build [BASE] | pack FILE | unpack FILE | run | --self-test" ;;
esac
