<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The performance gate

Every pull request that changes what the bridge is built from runs the
`perf` job, and so does every push to `main` and every release. The merge
queue does not run it: there the job passes at once and its summary says so,
since the pull request was judged and the push to `main` is judged again. The job fails
when the bridge does work while idle, or when the change makes it slower,
heavier or busier under load than its base, measured beside it on the same
runner. FerroPIX shipped an outbox sender that busy-looped at about 8,000
PostgreSQL transactions a second while idle, and nothing caught it before
release; this gate catches that shape. No specification governs it: it is
FerroBRIDGE's own design over the family's measuring tool,
`ferrohealth-perf`, in its A/B mode.

## Why the base runs beside the head

GitHub's hosted runners that report the same CPU model and memory differ by
25 to 45 % in latency and CPU per request, so a baseline stored on one
machine fails unchanged code on another. The gate builds the change, the
head, and its base, and measures both on one runner in one job, so the
difference between machines cancels. The base is the merge base of the
change with the pull request's base (with the merge group's, or with the
commit before a push), and on a release the previous release.

## What runs

The `perf` workflow builds the release binary of the base and then of the
head, both in one tree under `target/perf-build` with its own toolchain pin
and one target directory. The tree starts as `git archive` of the base's
commit; after the base is built, only the files the head changes are
rewritten, so cargo rebuilds for the head only the crates the change reaches
and the generated crates the two arms share build once. It installs
`ferrohealth-perf` at its pin in `docs/VERSIONS.md` and packs both binaries,
the base's commit and the tool into the artifact `perf-arms`. Each attempt
job downloads it on its own runner, extracts the base's tree again from its
commit, and runs `scripts/perf/gate.sh run`, so a retry measures the same
binaries and builds nothing:

1. starts two FerroEHR CDRs, each with its own PostgreSQL, one RabbitMQ and
   one FerroTERM over the v2-to-FHIR guide's table ConceptMaps on loopback,
   each at its pin in `docs/VERSIONS.md`, and loads the
   testkit's synthetic `ferrobridge.diagnoses.v1` and
   `ferrobridge.diagnose.v1` templates into each CDR;
2. starts each arm's `ferrobridge` in its own tree under that tree's
   configuration contract: its `perf/ferrobridge.toml` and the
   `FERROBRIDGE__` overrides of its `perf/arm.env`, or, in a tree from
   before that file, the overrides its own copy of the gate set. A pull
   request that renames a configuration key changes both files for the head
   alone, and the base boots on the keys its binary reads; a base that still
   cannot start ends the run with exit 2, quoting its log. Each arm runs the
   FHIR facade over the facade tests' mapping set, the HL7 v2 face over TLS
   through the face tests' Observation context, and the change feed with its
   events lane following its CDR's outbox;
3. creates one Condition through each facade, the record the read and the
   search keep to; each CDR assigns its own composition uid, so each arm's
   Condition has its own id, which the gate hands to that arm as `READ_ID`;
4. runs `ferrohealth-perf ab` with `perf/plan.toml`, counting the
   transactions of each arm's CDR database;
5. plants a poller of the head's CDR database that never waits, and runs
   the idle phase of `perf/busy-loop.toml` beside it, which must fail on
   idle transactions.

The idle phase holds the head bridge to 2 % of one core, and its CDR's
database to five transactions a second, for 60 seconds, while the HL7 v2
face and the events lane run with nothing to do; these are absolute limits.
Then four pairs of load phases run, each arm sending 20 arrivals a second for
60 seconds over the facade's hot paths, the `CapabilityStatement`, a create,
a read and a search, and an ORU^R01 laboratory result over MLLP with TLS as
the `PERFLAB` sender, which the face must acknowledge `AA`. The two phases
of a pair run at once, the arrivals of one arm half an interval after the
other's, so both arms meet the same runner at every moment. The bridges run
with `[auth] enabled = false` on the runner, so the requests carry no token.

## Reading the report

The job's summary names each breach by its metric, such as
`idle.transactions_per_second` or `read.p95_ms`, with the base, the head,
their ratio, the tolerance, the noise floor and in how many pairs the head
breached. A load metric fails when the head's median over its phases is
worse than the base's by more than the larger of the tolerance and the noise
floor, and the head breaches in most of the pairs. The tolerances are the
tool's defaults: latency and CPU 25 %, throughput 20 %, memory 20 % and
transactions 10 %; a raise is a hand edit of `perf/plan.toml` in a pull
request that says why. The artifact `perf-report-attempt-<n>` holds the JSON
reports, every phase and each arm's plan values included, and both bridges'
logs. Exit 1 is a breach. Exit 2 means the run cannot be judged: a generator
that lagged, a window that was not steady, a base that fell behind its
schedule, or a runner that stalled (every route's p99 over ten times its p50
at once).

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). An
attempt that ends with exit 2 runs again on a fresh runner on any CPU model,
since a run that cannot be judged says nothing about the code. An attempt
that ends with exit 1 runs again only on a CPU model the perf workflow lists
in `STALL_PRONE`. An attempt whose job ends cancelled while the run goes on
lost its runner (a shutdown, or the job's time limit) and left no exit code,
so it runs again on a fresh runner too, on any CPU model. A run cancelled by
a newer push or by a person retries nothing, and an attempt job that failed
fails the gate. At most two retries run, and the gate passes when the last
attempt passed. A breach on any other model fails at once.

A stall-prone runner is almost never judgeable: from 7 to 9 November 2026,
24 of 29 attempts on an AMD EPYC 9V74 could not be judged, against 2 of 42 on
every other model. Their stalls struck pair 1 as often as pair 4, in either
half of a window, and in both arms of a pair at once, so the host stalled,
and neither warm-up nor the order of the pairs explains them
([#1778](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1778)). An attempt
that lands on a model `STALL_PRONE` lists therefore measures nothing and ends
with exit 2 at once, unless it is the last attempt, which measures wherever
it lands. The job summary
names each attempt's CPU model, its verdict and why a retry ran, and the
verdict's summary names each attempt, a cancelled one with why the next ran.

The build job builds four crates at a time (`CARGO_BUILD_JOBS=4`), one per
core, and the build step logs the runner's used memory, swap and free disk
each minute. The hosted runner has 16 GB. A release build of `fhir-types` with
every resource of all five FHIR releases peaks above it on its own: on #1737
the log rose from 3.4 to 14.7 GB in the five minutes after that crate started
and the runner shut down. The bridge therefore compiles only the releases it
speaks, through the release features of `ferrobridge-term`. A base from
before that change still compiles all five, so the build job backs both
builds with a 20 GB swap file. Nothing is measured on that runner: each
attempt runs on a fresh one with the swap it booted with, and logs the swap
and memory before the gate measures anything. Swap changes how fast the
build runs, never the binary it makes. The swap file goes once a perf run on
`main` shows both builds well under 16 GB without it
([#1743](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1743)).

## Running it yourself

The gate reads the bridges' CPU and memory from `/proc`, so it judges them
on Linux. It needs Docker, `curl`, `git`, `jq` and `openssl`, or Podman with
`PERF_ENGINE=podman`:

```sh
cargo install ferrohealth-perf --version =0.5.2 --locked
bash scripts/perf/gate.sh build origin/main
bash scripts/perf/gate.sh run
```

`bash scripts/perf/gate.sh --self-test` checks the pin reader, the plan's
placeholders, the seeded Condition, the base resolution and the two plans'
idle limits on any machine.
