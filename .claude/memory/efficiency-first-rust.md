---
name: efficiency-first-rust
description: The owner wants FerroEHR (and the family) stable, fast, low on storage, CPU and RAM; idiomatic Rust without needless clones, allocations or layered indirection; external suites (Cadasto aql-conformance, cdr-benchmark) measure it too (owner, 2026-11-16)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner's bar for the products is: stable, fast, small on disk, and low on CPU and RAM. Because the
products are written in Rust, the code has to use that, with no needless
copying (`clone`, `to_owned`, `collect` into temporaries), borrowing in place of
owning where lifetimes allow, no layers of functions that only call functions,
and no per-request allocations that a reused buffer or a borrowed view would
avoid. Two external suites run against each release candidate as an outside
measure beside our own gates: https://github.com/Cadasto/aql-conformance
(AQL correctness) and https://github.com/Cadasto/cdr-benchmark (comparative
performance).

**Why:** owner, 2026-11-16: "we need stable, speed, and also low storage
usages. but also low CPU and low RAM usages. because we use RUST so we need to
make sure that we do it as efficient as possible".

**How to apply:** every performance change reports CPU per request, RSS and
storage beside latency. Reviews look for copies and allocations on hot paths.
Run both external suites after each round of changes and compare with the last
run. Related: [[capacity-target-2000-rps]].
