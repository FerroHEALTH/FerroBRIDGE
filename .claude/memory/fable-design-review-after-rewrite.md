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

**How to apply:** start it after the rewrite PR is ready and the quiet-host comparison is done, so the baseline is post-rewrite. The worker profiles first, prototypes and measures every candidate (deterministic counts, release profiles, quiet-host capacity runs at 1000, 2000, 3000, 4000 and 5000 rps; 4000 and 5000 are the owner's stretch goals), then gives a ranked recommendation with sub-issues. It checks every idea against the spec (ITS-REST, RFC 8785, AQL, FHIR and IHE), the exact-reads rule (#5891), the pool rules (#5879) and audit completeness. Related: [[core-rewrite-one-pr]], [[quiet-host-perf-runs]], [[capacity-target-2000-rps]].
