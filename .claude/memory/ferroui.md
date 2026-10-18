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
- **Every component is shared, never single use** (owner, 2026-10-18: "not
  single use compoanats they should be used propperly shared"). A product
  that needs something FerroUI lacks files a FerroUI issue (label
  `component`); the piece is built in FerroUI generically, documented, shown
  in the gallery for every brand in light and dark, tested and
  contrast-checked, released as a `0.1.x`, and the product pins it. One
  worker owns changes to FerroUI at a time, so releases never collide. The
  crate is `ferrohealth-ui` (`ferroui` on crates.io is an unrelated project).
- **Look at the result.** After the switch, run every console, capture every
  screen in light and dark, and look at each screenshot before calling it
  done ([[autonomous-loop]]).
- **The best of each product, in one harmonized style** (owner,
  2026-10-18: "we just need to take the best of each product … create an
  harmonized UI style"). Each component compares the four products' versions
  and takes the best on its merits; FerroBRIDGE's console is the newest and
  usually loses. The chosen piece is restyled to the one type scale, spacing,
  radius, hover and focus treatment, so no component looks foreign.
- **Quality control before every FerroUI release.** The gallery's stat cards,
  tabs and hover states shipped worse than FerroEHR's and the owner caught
  it. Every release PR compares each component with the reference product's
  screenshot, captures hover and focus states in light and dark, and says per
  screen what was checked; the orchestrator looks at every screenshot before
  the tag.
- **The orchestrator signs off the screenshots** (owner, 2026-10-18: "you
  can decide okay because you can read and validate all the screenshots
  yourself"). The orchestrator reads every image, has the defects fixed and
  recaptured, and approves the tag or merge itself without waiting for the
  owner, who still gets the final set shown.
- **All four consoles move in lockstep** (owner, 2026-10-18: "if you update
  FerroUI from here then you also need to keep updating the others …
  otherwise it will never be the same"). Every FerroUI release made from
  here gets a pin-bump PR in FerroBRIDGE, FerroFED, FerroEHR and FerroTERM,
  each with the visual loop and screenshots the orchestrator reviews, and
  the four merge together. Only the FerroUI pin, its API changes and the
  visual fixes it brings are changed in a sibling; anything else found there
  is filed on its tracker.
