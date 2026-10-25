---
name: console-redesign-program
description: The family console redesign runs design-first, one console at a time, reviewed by the orchestrator, then FerroUI finished across 0.3.0, 0.3.1 and 0.3.2 before any console is rewritten, then one lockstep rollout
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner ordered a redesign of every console on a shared FerroUI design language, decided on 2026-10-21. The order:

1. FerroPIX's console (approved 2026-10-21).
2. FerroFED (FerroHEALTH/FerroFED#1218).
3. FerroBRIDGE.
4. FerroTERM.
5. FerroEHR, last.

Each step is high-fidelity mockups of every screen and every interactive state, as HTML and PNG at 1440 light, 1440 dark and 390 light, kept in `~/Downloads/<Product>-ontwerp/`. When every design is settled, FerroUI ships the shared components.

FerroUI is finished completely before any console is rewritten (owner decision, 2026-10-22): every component across the 0.3.0, 0.3.1 and 0.3.2 releases. Only then does the console rollout start (FerroHEALTH/FerroUI#161, milestone v0.4.0), and every console moves in one go with its breaking changes. In the owner's words: "i do not want it half done i want that FerroUI fully finished so we can propperly rewrite all those other REPO's".

**Why:** a component change found in a later console's design, or a component still missing from FerroUI, would otherwise force a second breaking rollout. Getting the UI and UX right comes before shipping it.

**How to apply:** do not start rewriting any console while a FerroUI 0.3.x component is still open; the rollout waits for FerroUI#161. The orchestrator owns the design review (owner delegation, 2026-10-21). When a design worker reports, review every screen, send the findings back, and repeat until satisfied. Then start the next console's design without waiting for the owner. The approved FerroPIX decisions are the baseline. The main ones:

- the joined search;
- the docked decision bar;
- binder tabs;
- no key hints on phones, and arrows for previous and next;
- one configuration mode per deployment;
- preferences behind the user's name;
- no top-bar theme toggle.

Related: [[ferroui]], [[reactive-console-ui]].
