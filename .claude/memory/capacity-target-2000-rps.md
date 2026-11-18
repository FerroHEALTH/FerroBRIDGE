---
name: capacity-target-2000-rps
description: The owner wants FerroEHR to sustain 3000 requests per second at very low latency (raised from 2000 on 2026-11-16) with the full access path and access log on; measure capacity (the knee), not only head-against-base (owner, 2026-11-15)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner's throughput goal for FerroEHR is about 2000 requests per second,
with the access check and the signed access log on, as production runs.
The perf gate's 20 rps only compares head against base. A capacity test
(open-loop, stepped arrival rate, finds the knee where errors stay at 0 and
p99 stays under the SLO) is the measure.

**Why:** on 2026-11-15 the owner said 20 rps "is not what we are looking
for", and asked for numbers that would impress.

**How to apply:** measure capacity after each performance change. Report the
knee per request group, on one and on two instances, with the hardware
stated, and name the bottleneck at the knee. Never disable the access path or
the log to look faster. Related: [[findings-drive-redesign]].

**Raised 2026-11-16:** the target is now 3000 rps on the realistic mix, one
instance, 8 vCPU, with the access path and log on, and "very very low ms" for
reads, writes and AQL. The working latency targets (p99 at 3000 rps; our own
reading of the owner's ask, adjustable): reads 15 ms (p50 5 ms), writes 30 ms,
single-EHR AQL 20 ms, population AQL 100 ms.
