---
name: findings-drive-redesign
description: A real-world scenario or test that our design can't handle is a design finding (draft model or code), never something to work around in a fixture or probe (owner, 2026-11-14)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

When workers run realistic scenarios or write tests, a step the design can't express or gets wrong is a
finding about the draft model or the codebase. It is never worked around with direct SQL, another caller,
extra grants, a looser policy, or a weakened assertion. The worker files it as a linked issue and reports
a draft-model finding to the orchestrator at once. The orchestrator and the owner then rethink the draft
or the code properly.

**Why:** the owner, on 2026-11-14 during the hospital-scenario tests (FerroEHR #5610, #5611): "we should
then rethink as well our draft and also our codebase right!! because we should do it propperly!!"

**How to apply:** every brief for tests, probes or scenario runs carries this rule. Treat a draft-model
finding as a design decision for the owner, not a worker fix. Related: [[new-design-no-legacy]],
[[no-half-solutions]].
