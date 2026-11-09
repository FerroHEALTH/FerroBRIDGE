---
name: focus-ferroehr-refactor
description: The FerroEHR-only focus of 2026-11-01 is over; on 2026-11-02 the owner asked for every worker slot to be used across all products
metadata:
  node_type: memory
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

From 2026-11-01 every worker slot goes to the FerroEHR refactor: the service-layer reshape (#4212 and its sub-issues #4655 to #4662), the forwarder removals (#4663), and the bugs that work turns up (such as #4675). FerroTERM, FerroFED, FerroPIX and FerroBRIDGE wait until that is done. Their open PRs are still kept green, but no new issue there is started.

**Why:** the owner, 2026-11-01, after the orchestrator started FerroFED #1468 and FerroBRIDGE #239 while working "autonomically": "do we need to do FED and BRIDGE right now? I want to focus now first on the whole refactor and whole rewrite of FerroEHR, and then we keep going with those other ones".

**How to apply:** working the issue list autonomously ([[check-open-prs-proactively]]) means the FerroEHR refactor issues, in the bottom-up order of [[no-compat-shims]]. Ask before starting work in another product. Notes for later: FerroBRIDGE #239's batch half is already on main, and its last criterion waits on #117, the change-feed adapter. FerroFED #1468 (dispensation withdrawal) was stopped before any PR.

**Update 2026-11-02:** the owner lifted the pause for FerroBRIDGE when deciding that broker delivery moves from FerroEHR to FerroBRIDGE: "move it but also work on the BRIDGE then because the pause is lifted". FerroBRIDGE issues are worked again, starting with the change-feed adapter #117 and its AMQP consumer #1367 over FerroEHR's cursor feed (FerroEHR #4786). FerroTERM, FerroFED and FerroPIX still wait; ask before starting there.

**Update 2026-11-02, later:** the owner lifted the pause for every product: "why do we only have 1 worker running? we have more products right? ... there is no time to wait and go slow". Keep all five worker slots busy across FerroEHR, FerroBRIDGE, FerroTERM, FerroFED and FerroPIX, each worker working its product's open milestone under that repo's own rules. No need to ask before starting a product.

**Update 2026-11-03:** the slot count is ten, not five ([[worker-cap-ten]]).
