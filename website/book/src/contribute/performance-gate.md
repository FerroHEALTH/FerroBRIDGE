<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The performance gate

Every pull request that changes what the bridge is built from runs the
`perf` job, and so does every push to `main` and every release. The job fails
when the bridge does work while idle, or when it is slower, heavier or busier
under load than the baseline the repository commits. FerroPIX shipped an
outbox sender that busy-looped at about 8,000 PostgreSQL transactions a
second while idle, and nothing caught it before release; this gate catches
that shape. No specification governs it: it is FerroBRIDGE's own design over
the family's measuring tool, `ferrohealth-perf`.

## What runs

The `perf` workflow builds the release binary and installs
`ferrohealth-perf` at its pin in `docs/VERSIONS.md`. Then
`scripts/perf/gate.sh run`:

1. starts the FerroEHR CDR, its PostgreSQL and RabbitMQ on loopback, each at
   its pin in `docs/VERSIONS.md`, and loads the testkit's synthetic
   `ferrobridge.diagnoses.v1` template into the CDR;
2. starts `target/release/ferrobridge` with `perf/ferrobridge.toml`: the
   FHIR facade over the facade tests' mapping set, the HL7 v2 listener over
   TLS, and the change feed with its events lane following the CDR's outbox;
3. creates one Condition through the facade, the record the read and the
   search keep to;
4. runs `ferrohealth-perf` with `perf/plan.toml` against
   `perf/baseline.toml`, counting the transactions of the CDR's database;
5. plants a poller of the CDR's database that never waits, and runs the idle
   phase of `perf/busy-loop.toml` beside it, which must fail on idle
   transactions.

The idle phase holds the bridge to 2 % of one core, and the CDR's database
to five transactions a second, for 60 seconds, while the HL7 v2 listener and
the events lane run with nothing to do. The load phase sends 20 requests a
second for 60 seconds over the facade's hot paths: the
`CapabilityStatement`, a create, a read and a search. The HL7 v2 face is
measured idle only, because `ferrohealth-perf` sends HTTP and the face
speaks MLLP. The bridge runs with `[auth] enabled = false` on the runner, so
the requests carry no token.

## Reading the report

The job's summary names each breach by its metric, such as
`idle.transactions_per_second` or `read.p95_ms`. A load metric that breaches
is judged again on up to two fresh load phases, and the gate fails only when
it breaches in most of the runs; the check says in how many. The artifact
`perf-report-attempt-<n>` holds the JSON reports, every phase included, and
the bridge's log. Exit 1 is a breach. Exit 2 means the run cannot be judged:
a generator that lagged, a window that was not steady, or a runner that
stalled (every route's p99 over ten times its p50 at once).

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). An
attempt that ends with exit 1 or 2 on a CPU model the perf workflow lists in
`STALL_PRONE` runs again on a fresh runner, at most twice, and the gate
passes when the last attempt passed. A breach on any other model fails at
once. The job summary names each attempt's CPU model, its verdict and why a
retry ran.

## Updating the baseline

`perf/baseline.toml` holds one entry per host shape, the CPU model and the
memory size to the GiB together, because GitHub's hosted runners mix CPU
generations and sizes and a latency is comparable only on a like host.

- The `perf` job in `ci.yml` runs with `update: true`. A run on a shape the
  file holds is judged against that entry, and a run that passes may only
  lower it. A run on a shape the file lacks measures five load phases, each
  after its own warm-up, and writes the entry with each metric as their
  median and its noise floor as how far the worst phase strayed from it.
- Take `perf/baseline.toml` from the run's `perf-report-attempt-<n>`
  artifact. Machines of one shape still differ, so fold several runs of one
  shape into the committed file with
  `ferrohealth-perf merge --baseline perf/baseline.toml job-1.toml job-2.toml`.
  Each entry then takes the median over the jobs, and its floor the spread
  between them.
- Every tolerance stays at the tool's default: latency and CPU 25 %,
  throughput 20 %, memory 20 % and transactions 10 %. A raise is a hand edit
  in a pull request that says why.
- A change of the plan's rate, window or request set writes each entry
  afresh.

## Running it yourself

The gate reads the bridge's CPU and memory from `/proc`, so it judges them on
Linux. It needs Docker, `curl`, `jq` and `openssl`, or Podman with
`PERF_ENGINE=podman`:

```sh
cargo install ferrohealth-perf --version =0.4.2 --locked
bash scripts/perf/gate.sh build
bash scripts/perf/gate.sh run
```

`bash scripts/perf/gate.sh --self-test` checks the pin reader, the plan
rendering, the seeded Condition and the two plans' idle limits on any
machine.
