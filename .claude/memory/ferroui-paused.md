<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: ferroui-paused
description: FerroUI work is paused by the owner; no worker, PR or merge there until the owner lifts it
metadata:
  type: project
---

FerroUI (FerroHEALTH/FerroUI) is paused by owner decision on 2026-11-10: no
work, no new pull request, no auto-merge, no release there. Exception (owner,
2026-11-16): whatever FerroSMART's screens need (components, patterns,
locales) is built in FerroUI as general components and released; all other
FerroUI work stays paused, except where the owner asks for it explicitly (2026-11-17: issues #645, #649, #659, #672, #684 and #685 in one PR, then the v0.6.11 release).

**Why:** the owner stopped it explicitly ("stop working on FerroUI that is
paused"); the focus is the products' releases on the newest family crates.

**How to apply:** never brief a worker on FerroUI; an open FerroUI pull
request stays a draft with auto-merge off (#671 was parked that way); a gap
a product console hits in FerroUI is recorded for the owner, never fixed
there and never worked around locally ([[family-architecture]]).
