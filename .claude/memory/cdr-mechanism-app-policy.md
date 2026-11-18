---
name: cdr-mechanism-app-policy
description: The CDR provides the generic mechanism; national rules and controller policy come from the deployment or the application on top as content, never from code (owner, 2026-11-15)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A FerroHEALTH server (FerroEHR first) supplies the generic mechanism: the
evaluator, the refusal codes, the record of the decision. Which national law
applies, which retention period holds, and which role or organisation type may
reach data are the controller's decisions. The deployment or the application
on top supplies them as content the generic mechanism reads. The one exception
is EU law that binds the product everywhere (EHDS).

**Why:** the owner pointed out on 2026-11-15 that the orchestrator was asking
them to decide questions that belong to the application layer, for example the
Dutch Wabvpz checks in `authn/wabvpz.rs` and the national retention floors.

**How to apply:** when a design question is about policy rather than
mechanism, do not hard-code an answer and do not ask the owner to pick one.
Design the content shape the controller fills, and ship no national
catalogue. Related: [[findings-drive-redesign]], [[new-design-no-legacy]].
