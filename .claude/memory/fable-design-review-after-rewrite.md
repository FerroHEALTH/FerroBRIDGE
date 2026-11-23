---
name: fable-design-review-after-rewrite
description: When the FerroEHR core rewrite (#6001) is finished, run ONE Fable worker on #6019, a whole-system performance review (open scope, research AND test)
metadata:
  type: project
---

Once the FerroEHR core-rewrite PR (#6001) is finished, the orchestrator starts exactly one worker on model Fable (owner request, 2026-11-18) for issue #6019. The review's scope is the whole system, not only JSON. It covers:
- the request path, authentication and the access decision (#6013), and the audit records per request;
- PostgreSQL: schema, indexes, plans, round trips, pools, WAL, autovacuum, partitioning, and SQL against Rust;
- the stored formats (JSON or binary);
- the AQL engine;
- Rust allocations, copies, caches and the runtime;
- the write path;
- anything the measurements point at.

**Why:** the owner expects much more to win beyond JSON, and wants the strongest model to research it and to TEST it.

**Scope widened (owner, 2026-11-18):** after the quiet-host comparison, the orchestrator collects everything still open and gives it ALL to the one Fable worker; no separate Opus workers for those items. That covers #6019, #6022 (the 8.5 s lab AQL), #6020 (the pool timeouts under load), the remaining metadata conversions on commits and raw reads, the callgrind ceilings, #6013 (authentication and access-decision CPU), and the rewrite PR #6021's CI failures, if any.

**How to apply:** owner's order (2026-11-18): when G and H are done, open the rewrite PR; while its CI runs on GitHub, run the quiet-host comparison locally; then start the Fable worker while the PR waits. It measures on the feat/core-rewrite head, so the baseline is post-rewrite. The worker profiles first, prototypes and measures every candidate (deterministic counts, release profiles, quiet-host capacity runs at 1000, 2000 and 3000 rps; the owner fixed 3000 rps as the cap on 2026-11-18 and dropped the 4000 and 5000 stretch goals), then gives a ranked recommendation with sub-issues. It checks every idea against the spec (ITS-REST, RFC 8785, AQL, FHIR and IHE), the exact-reads rule (#5891), the pool rules (#5879) and audit completeness. Related: [[core-rewrite-one-pr]], [[quiet-host-perf-runs]], [[capacity-target-2000-rps]].

**Fable for every worker in this programme (owner, 2026-11-19):** the owner rejected an Opus worker for caseload and the protected-person label and wants Fable to do it. For FerroEHR v6 work the worker runs on Fable (`model: fable`), not Opus, and there is exactly ONE worker at a time (the owner stopped a second, parallel Fable worker the same day); new work is added to its queue. This overrides CLAUDE.md's "never Fable as a worker" for this programme.

**Opus workers again (owner, 2026-11-19, afternoon):** after the Fable usage limit was reached (reset 2026-11-24 19:00 CET), the owner said to keep going with Opus workers only, since the hard design thinking was done with Fable. From then on the queue runs on Opus workers, several in parallel when their work is independent; the one-worker rule applied to the performance programme's quiet-host measurements.
