<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: ferroui-paused
description: FerroUI work is paused by the owner; no worker, PR or merge there until the owner lifts it
metadata:
  type: project
---

FerroUI (FerroHEALTH/FerroUI) is paused by owner decision on 2026-11-10: no
work, no new pull request, no auto-merge, no release there.

**Why:** the owner stopped it explicitly ("stop working on FerroUI that is
paused"); the focus is the products' releases on the newest family crates.

**How to apply:** never brief a worker on FerroUI; an open FerroUI pull
request stays a draft with auto-merge off (#671 was parked that way); a gap
a product console hits in FerroUI is recorded for the owner, never fixed
there and never worked around locally ([[family-architecture]]).
