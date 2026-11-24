---
name: quiet-host-perf-runs
description: A baseline capacity run of FerroEHR runs on a quiet host, with every worker stopped and Podman empty, and perf fixes come only after it
metadata:
  type: feedback
---

A baseline ("true run") capacity measurement of FerroEHR runs on a quiet host: every worker is stopped first, nothing else runs on Podman (other containers removed with `rm -f -v`), and only the one measuring worker runs. After that run, work goes only to fixing the performance it shows until the owner says otherwise (owner, 2026-11-17).

**Why:** runs measured while other workers built and tested (host load 10 to 17 on 18 cores) gave numbers the owner could not trust, and old perf results went stale behind a long PR queue.

**How to apply:** before a baseline run, stop every agent (TaskStop), check `podman ps -a`, remove leftover worker containers, then start the single measuring worker and record host load per step. Related: [[capacity-target-2000-rps]].
