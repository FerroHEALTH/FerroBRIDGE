<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Memory index

- [Family architecture](family-architecture.md): five separate servers that
  reach each other only over published protocols, shared crates for
  cross-cutting work, nothing hand-written that a shared crate covers,
  server-rendered consoles; FerroFORMS, the umbrella and Veredictum out of scope;
  owner 2026-10-25
- [Product scope](product-scope.md): the owner's product statement is the
  ceiling on what this repository may claim; standalone server and
  mapping-driven are decided, the rest is the architecture of record from
  issue #1; FHIR and OMOP are FerroBRIDGE's lane, FerroEHR carries none
- [OMOP target](omop-target.md): the owner confirmed OMOP as a first-class
  target beside FHIR on 2026-09-03; FHIRconnect covers FHIR (openFHIR is its
  reference implementation) and OMOCL covers OMOP (Eos is its reference
  implementation); origin FerroEHR #2652, now FerroBRIDGE #2; claim nothing
  about OMOP beyond the product statement
- [Owner work style](owner-work-style.md): research-first and evidence-based,
  from first principles; confirm foundational decisions before scaffolding; no
  code while the design is open
- [Licence: BUSL 1.1](license-busl.md): owner decision 2026-09-04 (#12), the
  Business Source License 1.1 on FerroEHR's and FerroTERM's terms, replacing
  the 2026-09-03 Apache 2.0 choice; non-commercial production free, commercial
  production needs a licence, Apache 2.0 four years after each version;
  inbound equals outbound, no contributor licence agreement
- [Sibling projects](sibling-projects.md): FerroEHR at `../ferroehr` is the
  reference CDR and FerroTERM at `../FerroTERM` the reference terminology
  server, FerroFED and FerroPIX the other products; all are read-only prior
  art reached over their protocols, never edited from this repository; a
  tracker issue in a sibling may be filed from here when the owner asks
  (2026-09-05, FerroTERM #300)
- [Domain ferrobridge.eu](domain-ferrobridge-eu.md): the public domain is a
  Pages setting mirroring ferroterm.eu, never a `CNAME` file; owner 2026-09-04
- [Milestones 0.0.x](milestones-0-0-x.md): milestones start at v0.0.1 and step
  by a patch number, never a v0.1.0 opener; owner 2026-09-04
- [PR auto-merge](pr-auto-merge.md): enable auto-merge on every pull request
  the moment it is opened (`gh pr merge <n> --auto`);
  owner 2026-09-04
- [Memory lives in the repo](memory-lives-in-repo.md): every learning is a
  tracked file in `.claude/memory/`, never a per-user note; owner 2026-09-04
- [Standalone product](standalone-product.md): public documents never name
  FerroEHR or FerroTERM; any CDR, any terminology server, crates by crate name;
  the licence framing stands on its own; owner 2026-09-04
- [Crates published](crates-published.md): owner decision 2026-09-05, the
  library crates are published to crates.io on the sibling model (`crates/*`
  published, `app/*` and `tools/*` never), so the release lane carries a
  crates.io leg and every `pub` surface is designed as API from the start
- [Subagent reports go to a file](subagent-reports-to-file.md): a long
  agent report is written to the scratchpad by the agent, because a truncated
  result is lost and a finished agent cannot be resumed; 2026-09-12
- [Mermaid diagrams](mermaid-diagrams.md): the architecture carries mermaid
  diagrams; render every fence with mermaid-cli against the installed Chrome
  before a pull request; a semicolon ends a sequence message; 2026-09-12
- [CI runner memory](ci-runner-memory.md): a workspace-wide check or clippy
  is killed with exit 143 when two generated giants compile together; lint
  per package with cargo hack, never the all-features union of fhir-types;
  2026-09-12
- [PostgreSQL 18](postgresql-18.md): every PostgreSQL the project tests
  against or documents is the latest release (18.6 on 2026-09-12), never 16;
  owner ruling 2026-09-12
- [End-to-end gate](e2e-gate.md): container tests run only with
  `FERROBRIDGE_E2E=1` through the testkit harness; images pinned by digest and
  checked by the versions guard; 2026-09-12
- [Merge queue and signed commits](merge-queue-signed.md) — main merges through the merge queue since 2026-10-17; arm with gh pr merge --auto, never update-branch --rebase
- [Release tag is mine](release-tag-is-mine.md): the session merges every release PR (auto-merge), pushes every tag without a local preflight, and approves every `crates-io` environment; nothing in a cut waits on the owner
- [Upstream reports carry no milestone](upstream-reports-no-milestone.md): an upstream-report issue is never in a milestone; the in-repo decision it forces is a separate, milestoned issue
- [Upstream reports stay here](upstream-reports-stay-here.md): the issue is the record; nothing is filed on external trackers and no owner-action issue for filing is created
- [Dependency sweep to latest](deps-latest-sweep.md): every session compares the workspace pins with crates.io and the sibling's crate list (openehr-sdt split off openehr-its on 2026-09-24); the family lands together by hand; owner 2026-09-24
- [openEHR crates are the model](openehr-crates-are-the-model.md): never restate RM, AM, BASE or FLAT knowledge; `|raw` writes and `openehr_rm::v1_2::model` lookups; owner flag 2026-09-25, #241
- [Legacy standards are first class](legacy-standards-first-class.md): the bridge spans the oldest wire formats (HL7 v2.x first) to the newest; built in from the start and scheduled into the current milestone; owner 2026-09-25, research on #248

- [openehr-its rest-client upstream](openehr-its-rest-client-upstream.md) — the generated client since 0.0.71; the #293 workarounds went at 0.0.72
- [HL7 v2 test corpora](hl7v2-test-corpora.md) — the vendored v2 message sets, their licences, and that no real traffic is public
- [File-length rule](file-length-rule.md) — hand-written .rs ≤ 1000 lines, split at 750 into module folders; guard with ratchet allow-list (#351)
- [Repo moved to FerroHEALTH](repo-moved-to-ferrohealth.md) — 2026-10-01 transfer; ghcr.io/ferrohealth image, org board #3, what keeps the old owner
- [Native issue types and fields](native-issue-types-and-priority.md): type, priority and effort are the native issue type and the FerroHEALTH Priority and Effort fields, set with scripts/gh/fields.sh; bug, enhancement and P0 to P3 retired; owner 2026-10-08 (#416)
- [Remove worktrees after merge](remove-worktrees-after-merge.md): remove each worktree, its branches and its target/ as soon as its PR merges, in every FerroHEALTH repository; cargo clean the locked ones; owner 2026-10-15 and 2026-10-19
- [Worktree target dir](worktree-target-dir.md) — each worktree builds into its own target/; a shared CARGO_TARGET_DIR mixes two checkouts' builds; 2026-10-19
- [PR merge watch](pr-merge-watch.md) — watch every PR until merged with scripts/gh/watch-prs.sh; fix red checks, conflicts and BEHIND at once; owner 2026-10-15
- [Worker wait loops](worker-wait-loops.md) — worker briefs forbid self-matching pgrep waits; find and kill stuck shells by working directory; owner 2026-10-16
- [PR backlog discipline](pr-backlog-discipline.md) — several open PRs are fine if each merges cleanly; no stacked PRs, overlapping work in sequence, every open PR tracked to its merge; owner 2026-10-17
- [Autonomous loop](autonomous-loop.md) — never stop, never wait idle, never ask the owner; keep working the issues while PRs drain; owner 2026-10-17
- [FerroUI](ferroui.md): one shared UI component library for every console, pinned Cargo dependency, server-rendered since 0.6 (#510), owner sign-off to migrate the siblings; owner 2026-10-18
- [One milestone at a time](one-milestone-at-a-time.md) — every worker on the current milestone until it is cut; an externally blocked straggler moves to the next
- [Release closes milestone](release-closes-milestone.md): release.yml refuses a tag whose milestone is missing or not empty and closes it after every leg; check it after a manual or recovery release; owner 2026-10-19 (#983)
- [Worker scratch subfolders](worker-scratch-subfolders.md): each worker writes scratch files under its own scratchpad subfolder, never the root; check a file is yours before `--body-file` and re-read the PR body after; 2026-10-20 (#1021)
- [Admin bypass stays](admin-bypass-stays.md) — the org-admin bypass on main stays in every family repo; never remove it, and no worker merges through it; owner 2026-10-21
- [Family toolchain](family-toolchain.md): newest stable Rust as toolchain, the toolchain minus one as MSRV (1.99.1 and 1.98 on 2026-10-21); check the Rust blog first; never lower an MSRV; owner 2026-10-21
- [Worktrees in the repo](worktrees-in-repo.md): every worktree at <repo>/.claude/worktrees/<name>, never a sibling folder; owner 2026-10-21
- [Console redesign program](console-redesign-program.md): design-first, PIX, FED, BRIDGE, TERM, then EHR, the orchestrator reviews and moves on, then FerroUI finished across 0.3.0 to 0.3.2 before one lockstep console rollout (FerroUI#161, v0.4.0); owner 2026-10-21 and 2026-10-22
- [Reactive console UI](reactive-console-ui.md): superseded by the owner on
  2026-10-29 (FerroUI #510): server-rendered consoles, plain script modules,
  no WebAssembly; a navigation is one document
