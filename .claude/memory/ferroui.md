---
name: ferroui
description: One shared UI component library, FerroHEALTH/FerroUI, for every FerroHEALTH console, linked by a pinned Cargo dependency; the owner signed off on migrating FerroEHR, FerroFED and FerroTERM to it from here
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The operator UIs of FerroBRIDGE, FerroFED, FerroEHR and FerroTERM move onto
one component library in its own repository, `FerroHEALTH/FerroUI` (owner,
2026-10-18: "i want to have an uniform ui compoanats libary for all the
different UI's so all the products look and feel the same", name chosen
"FerroUI").

**Why:** four consoles drifted apart, and every fix was made once per repo.
The owner found FerroBRIDGE's console missing the Lucide icons and editor
highlighting the others have, which a shared library prevents by
construction.

**How to apply:**

- **One design, every product.** FerroEHR's console is the starting
  reference; a better pattern from another product (FerroFED's tables, the
  editor highlighting of #850) wins and moves into FerroUI. Each product
  sets its own colours through the role tokens, never by forking a
  component.
- **Linked by a pinned Cargo dependency, never a git submodule** (owner,
  2026-10-18: "yes go with the pinned cargo dependency"). Each product pins
  FerroUI exactly (crates.io through Trusted Publishing, or a git dependency
  at a release tag), the lockfile records it, and Dependabot proposes the
  update. The crate ships its stylesheet and tokens.
- **All four switch in one coordinated change**, not one product at a time.
- **FerroTERM's console becomes server rendered** like the others (owner,
  2026-10-18: "yes do that then while you do the rewrite"): its own
  `ferroterm-viewer` binary and image, a backend-for-frontend holding the
  operator's session, the terminology server serving no UI files, so FerroUI
  targets one rendering model (server rendered and hydrated).
- **The owner signed off on editing the sibling repositories for this
  migration** (2026-10-18: "it's an sign off you are allwed to update there
  repo's"). Each change is made in that repository's own checkout, branch and
  pull request, under its own `CLAUDE.md`, rules and hooks, never mixed into
  a FerroBRIDGE branch ([[sibling-projects]]).
- **FerroUI is a fully set up repository**, not just a crate (owner, 2026-10-18:
  "so this repo is fully setup and everything!!"): `CLAUDE.md` and `.claude/`
  (rules with `leptos-ui.md` as its own component rule set, agents, skills,
  hooks, memory), the README and the community files, BUSL `LICENSE` and
  `NOTICE` with SPDX headers, CI with the same workflow security and a single
  required `conclusion`, the Rust gates, Dependabot, CodeQL, Scorecard, the
  release lane with signed tags and crates.io Trusted Publishing, templates,
  labels, and the ruleset with merge queue and signed commits.
- **Look at the result.** After the switch, run every console, capture every
  screen in light and dark, and look at each screenshot before calling it
  done ([[autonomous-loop]]).
