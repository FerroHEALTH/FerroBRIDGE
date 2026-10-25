<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md

**FerroBRIDGE** is a pure-Rust, standalone bridge between openEHR and two
interoperability targets: HL7 FHIR, driven by the FHIRconnect specification
(model-mapping and contextual-mapping YAML validated against its published JSON
schemas), and the OMOP Common Data Model, driven by the OMOCL specification.
Mappings are specification-conformant YAML, never hand-coded per resource or
table. It runs as its own server beside any openEHR CDR reached over the
openEHR ITS-REST API (FerroEHR is the reference CDR, never a compile-time
dependency) and uses a FHIR terminology server (FerroTERM is the reference) for
code systems and value sets.

The name follows the Ferro family (FerroEHR, FerroTERM). FerroBRIDGE in prose,
`ferrobridge` in identifiers.

## The design is recorded in `docs/architecture.md`

Read it first. It is the output of the 2026-09-03, 2026-09-05 and 2026-09-12
research passes over the primary sources (issue #1) and records the decisions
with their ground: FHIRconnect and OMOCL are two languages sharing one header, so the bridge is one shared
foundation, two interpreters, two sinks; the pins are FHIRconnect v1.0.0, FHIR
R4, OMOCL v1.0.0, OMOP CDM v5.4 and openEHR ITS-REST 1.1.0; the FHIR side is a
facade over the CDR, the OMOP side a batch ETL over AQL into a CDM database; OMOP
concept resolution is SQL over the loaded OHDSI vocabulary, never a FHIR
terminology operation. Where a specification is silent (FHIR search, identity,
failure policy, extension ordering) the document names the decision as
FerroBRIDGE's own; keep that labelling in code and docs.

## The two layers

- **Generated where a machine-readable source exists:** the FHIR model
  (`fhir-types`, emitted by `tools/fhir-codegen` from the vendored HL7 FHIR
  packages; by owner decision on 2026-09-05 the crate and its generator move
  here from the sibling terminology server, which then consumes the crate from
  crates.io; the move landed with #72's first increment) and the OMOP CDM v5.4
  row types from the OHDSI
  `CommonDataModel` field definitions, with the OHDSI PostgreSQL DDL vendored
  verbatim beside them (`docs/architecture.md` §10). Every file marked
  `// @generated` is off-limits: change the generator and regenerate, never
  hand-edit generated code. The openEHR model comes from the published
  `openehr-*` crates.
- **Hand-written and the product:** the shared mapping foundation, the
  FHIRconnect and OMOCL parsers, validators and interpreters, the ITS-REST
  client, the terminology client, the FHIR facade and the ETL runner. Modern
  idiomatic Rust of our own design, with the FHIRconnect, FHIR, OMOCL, OMOP CDM
  and openEHR specifications as the authority.

The Cargo workspace lands with v0.0.2, the foundation release
(`docs/architecture.md` §14); the round trips follow in v0.0.3 (FHIR) and
v0.0.4 (OMOP). Nothing is scaffolded before its issues are filed.

## Repo map

The Cargo workspace (#107) holds the root manifests with the full lint set,
eleven library crates (three at 0.0.0, the version that holds their crates.io
names until a first publish, and `fhir-types`, `hl7v2-types`,
`openehr-mapping-core`, `fhirconnect`, `ferrobridge-term`,
`ferrobridge-hl7v2`, `ferrobridge-fhirpath` and `ferrobridge-validator` on the
published crate line), and
the testkit tool crate. Beside it:

- `app/ferrobridge-server`: the bridge's one binary, `ferrobridge` (#21). A thin
  `main.rs` over the library run path, the five subcommands, the TOML and
  environment configuration tree with `_file` secrets and `deny_unknown_fields`
  throughout, the `tracing` console, the request log that carries the matched
  route and never a body, the health family over an indicator registry, the
  `tower-http` stack (request id, panic catch, timeout, body ceiling), and the
  bounded drain on `SIGTERM`. `serve`, `cdm init`, `etl run` and
  `mapping check` run (the last over `check`, the checker the operator API
  shares, #595); `vocab load` parses and names the issue that lands it.
  `src/cdr/` reaches the CDR through the generated ITS-REST 1.1.0 client of
  `openehr-its` (`rest-client`, #285, #293): the handle built from `[cdr]`
  with its `SecretString` credentials or, under `[cdr.credentials.oauth]`, the
  OAuth 2.0 client-credentials token renewed once on a CDR `401` (#421), the
  commit headers filled into the
  generated parameters, the `X-Request-Id` echo, the error body each outcome
  carries read into a refusal's diagnostics, the typed ids an `ETag` names,
  the two-route template fetch and the AQL paging, with the `wiremock`
  contract tests under `tests/it/cdr/`.
  `src/etl/` is the OMOP ETL runner (#91): the `[etl]` queries checked at load,
  the paged read, the `Mapper` the OMOCL engine fills (#90, `etl::mapper`
  compiles one program per template on first sight), the visits, the
  deletion check (`etl::deletions`, #527) and the run report. `src/facade/`
  is the FHIR R4 facade (#85), mounted under `/fhir` behind `[facade]`: the
  six interactions over the CDR and search over the AQL projections a
  `*.search.yml` file declares beside a context (#98),
  the `redb` identity store with the derivation
  `docs/architecture.md` §9 fixes, the one status table
  from a CDR answer to a FHIR answer, and the `CapabilityStatement` built from
  the loaded programs. `src/hl7v2/` is the HL7 v2 face (#255), behind
  `[hl7v2]` and beside the facade: the MLLP listener under the same drain,
  each message mapped by `ferrobridge-hl7v2` and written through the facade's
  `ingest` service as one transaction, and the acknowledgment chosen from the
  commit outcome (`hl7v2::answer`). `src/smart.rs` makes the facade and the
  operator API a SMART App Launch 2.x resource server behind `[auth]` (#400,
  #981) over `ferrohealth-smart`, the family's resource-server crate: the
  issuers' keys read and read again past their max age, every access token
  and `[[auth.basic.users]]` credential checked, and one audit line per
  request; `facade::guard` is the route table with the SMART v2 permission
  each interaction needs and the `OperationOutcome` refusals, and
  `facade::compartment` holds a `patient/` token to its EHR; every facade
  handler takes the caller's `Access`. `src/credentials.rs` hands the
  client-credentials token of `ferrohealth-smart` (cached, renewed once on a
  `401`) to the CDR and terminology clients, the token both lanes take under
  `[<lane>.credentials.oauth]`. `src/usage_report/`
  sends the installation report to FerroPULSE on its envelope v2 behind
  `[usage_report]` (#581): one `start` and one `daily` report from `serve`,
  the latency window the request log fills by route group, and the instance
  id under `state_dir`, validated against the vendored schema in the tests.
  `src/operator/` is the
  read-only operator API behind `[operator]` and `[auth]` (#596): the guard
  that admits only a token carrying `auth.product_scope`, the mapping roots
  with every wire path confined to its directory, and the running load as
  the `check` module reports it; with `[operator] mapping_dir` it checks a
  draft, saves it beside the provisioned tree and reloads (#597), swapping the
  facade's and the operations lane's programs, which every request reads once
  at entry. `src/mappings/compile.rs` is the one FHIRconnect load path the
  lanes, `mapping check` and the reload share (#725).
- `app/ferrobridge-console`: the operator console (#599, `docs/architecture.md`
  §4.10), its own binary `ferrobridge-console`: a Leptos app in islands mode
  on FerroUI 0.3.6 (the server draws each page, the browser hydrates its
  islands, every action a plain form post), halves chosen by target, with `serve`, `config check`
  and `healthcheck`, the TOML configuration with `FERROBRIDGE_CONSOLE__`
  overrides and `_file` secrets, the nonce CSP and the other browser security
  headers, and the typed bridge client (`src/bridge/`). Its server half
  links only `openehr-mapping-core`, `fhir-types` and
  `ferrobridge-operator-api` of the workspace and its browser half none, held by `tests/it/boundary.rs`; the CI `console` lane
  lints the wasm32 half and builds the site bundle against its budget
  (`.claude/rules/leptos-ui.md`). FerroUI (`ferrohealth-ui`, pinned exactly)
  leads its UI: its components, design and component rules come from the
  FerroUI repository at the tag of the pin, and a missing part is a FerroUI
  issue, never a local copy.

- `crates/fhir-types`: the generated FHIR model, Apache-2.0, emitted whole by
  the generator below and never hand-edited: one module per version, STU3,
  R4, R4B, R5 and R6, each behind its feature.
- `crates/hl7v2-types`: the generated HL7 v2 tables (#253), Apache-2.0,
  emitted whole by the `v2` root set of the generator below from the fetched
  v2 definitions: every message structure's segment-group tree, the field
  table of every segment definition (batch envelopes included), the component
  table of every data type, and every message definition indexed by code and
  trigger event with the structure variant it names (#289), plus `legacy`,
  the 1713 per-version structures of 2.3 to 2.8.2 (218 of them of the 57
  codes v2.9.1 withdrew, `ORM_O01`) from the build-time IGAMT export of
  HL7's v2 database (#303, #333), which the parser selects by MSH-12.
  Never hand-edited.
- `crates/ferrobridge-operator-api`: the bridge's health and operator
  documents as serde types (#777), which the server writes and the operator
  console reads, so the two sides compile against one definition.
- `crates/openehr-mapping-core`: the half of the mapping foundation both
  languages share (#74): the header they standardize between them, the YAML
  loader with anchors and source positions, the registry by mapping name and
  archetype id, the diagnostic model, and the openEHR path model with the
  `../` step resolved against an anchor.
- `crates/fhirconnect`: the FHIRconnect language. `model` is the file model and
  its three validation layers (#82). `tree` is the bidirectional path model
  behind `with.fhir` (#81): the expression parser with the `$resource`,
  `$fhirRoot` and `^` heads, the resolver over the `fhir-types` element table,
  and read and write over that crate's lexical `Value` tree, with every
  expression classified writable or read-only when it is parsed. `resolve`
  compiles one context into one immutable program (#83): the extensions
  applied in declaration order, the four version selectors checked, and every
  path on both sides resolved once so the interpreter parses none.
- `crates/ferrobridge-term`: the FHIR terminology client (#77) for
  `CodeSystem/$lookup`, `ConceptMap/$translate` and
  `ValueSet/$validate-code`, R4 and R4B over the generated `fhir-types`
  operation contracts, with the batch form and a `tx-issue-type` coding
  surfaced in every typed error.
- `crates/ferrobridge-hl7v2`: the HL7 v2 face (#254). `mllp` is the MLLP
  Release 1 frame codec and listener, `decode` the MSH-18 character sets,
  `parse` the positional split grouped by the `hl7v2-types` structure tree,
  `ack` the original-mode acknowledgment, `inbound` one message through
  them and `batch` the FHS and BHS batch file with its response batch
  (#642). `map` runs the v2-to-FHIR guide's ConceptMaps, read from a
  directory at run time, writing FHIR R4 through `fhirconnect::tree` and translating
  every table value through `ferrobridge-term`, with every condition, target
  or value it cannot carry a counted outcome.
- `crates/ferrobridge-fhirpath`: the FHIRPath evaluator (#436), FHIRPath
  Normative Release 2.0.0 with the FHIR R4 additions over the `fhir-types`
  `Value` tree and element table (`model::r4::R4`): a `logos` and `chumsky`
  parser, exact decimals, dates and times that keep their precision, a static
  check of the paths against the context type, and `resolve()`, `memberOf()`,
  `conformsTo()` and the other outward functions over caller-supplied seams.
  `ucum` converts quantities by UCUM, reading the UCUM essence embedded whole
  from `vendor/ucum/` (#459).
  The HL7 FHIRPath R4 test suite under `docs/specs/fhirpath-tests/` is its
  `fhirpath-r4` corpus (FHIRPath N1) and its `fhirpath-r4-stu3` corpus (the
  cases that use the FHIRPath 3.0.0 STU functions, run behind their opt-in).
- `crates/ferrobridge-validator`: the FHIR R4 profile validator (#432,
  foundation #437). `package` reads a package from a `.tgz` (`flate2`, `tar`)
  or a directory, `set` refuses a missing dependency by name and resolves
  StructureDefinitions by canonical into the generated `fhir_types::r4` model,
  and `validate` walks a `Value` over the R4 element table and the profile
  snapshot: unknown members, JSON shapes, cardinality per parent, choice
  types, `contentReference`, primitive lexical forms from the package's
  `regex` extensions, ranges and lengths, `fixed[x]`, `pattern[x]` and
  extensions by `url`. `report` holds typed issues and every slicing,
  invariant, binding and profile it met and did not evaluate, and converts to
  an R4 `OperationOutcome`.
- `tools/fhir-codegen`: the generator, with the seven vendored HL7 FHIR
  packages under `vendor/` (411 MB, a `PROVENANCE.md` each; one is the
  v2-to-FHIR guide, #252, and one the STU3 core package behind the `stu3`
  module, #453), the HL7 v2 definitions fetched at build time into the
  ignored `vendor/hl7-v2ig/`, and the IGAMT export of HL7's v2 database
  fetched the same way into `vendor/hl7-v2-legacy/` (#303; of each only the
  `PROVENANCE.md` is committed) as its inputs.
- `crates/omop-cdm`: the OMOP CDM v5.4 layer. `src/generated/` and `ddl/` are
  emitted by `tools/omop-cdm-codegen` from the vendored OHDSI definitions; the
  column metadata, the three column types and the DDL schema substitution are
  hand-written beside them, with `graph` (the record graph the OMOCL engine
  emits) and, behind the default `database` feature, the concept resolver,
  the CDM writer with its natural-key side table, and the derived tables.
- `tools/omop-cdm-codegen`: that generator, reading
  `docs/specs/omop-cdm/inst/` and emitting the 39 table modules, the metadata
  index and the embedded DDL.
- `tools/ferrobridge-testkit`: the test-support crate, a path-only
  dev-dependency (#78). `containers` starts PostgreSQL and the reference CDR
  behind the `FERROBRIDGE_E2E` gate, with every image pinned by digest in
  `docs/VERSIONS.md`; `stubs` carries the documented ITS-REST and FHIR R4
  terminology response shapes for `wiremock`; `fixtures/` holds the synthetic
  template, composition and FHIR resources; `matrix_pin` reads the pin matrix;
  `conformance` is the helper the corpus tests report their verdicts through;
  `vendor/` is the ignored home of the five HL7 FHIR examples packages the
  per-version model corpora read, fetched at build time (#373, #454).
  `vocabulary` holds the synthetic OHDSI vocabulary for the ten vocabulary
  tables and its `COPY` loader (the observed Athena shape is #88). `oidc` is
  the test OpenID Provider, which can also sign a real browser in.
  `tests/it/journeys/` holds the operator console's browser journeys (#602),
  `thirtyfour` over the Chrome for Testing release
  `scripts/release/journey-tools.sh` installs, against a running bridge and
  console behind the `FERROBRIDGE_JOURNEYS` gate, and the book's console
  screenshots behind `FERROBRIDGE_SCREENSHOTS`, with the guard that every
  console route has both.
- `docs/specs/`: the vendored specification corpora (#71), one directory per
  corpus with a `PROVENANCE.md`, fetched by `scripts/vendor/*.sh` from the pins
  in `docs/VERSIONS.md`: FHIRconnect with its schemas and the REST API
  chapter, the FHIRconnect mapping library, OMOCL, the OMOP CDM v5.4
  definitions and PostgreSQL DDL, and the ITS-REST OpenAPI documents. Beside
  them, `fhir-packages/` holds the R4 profile validator's package set (#435),
  one `<package>#<version>` tree per pin, fetched at build time by
  `scripts/vendor/profile-packages.sh` into the ignored `package/` and never
  committed (only each `PROVENANCE.md` is); its `--check` asserts the
  dependency closure is complete.
- `docker/Dockerfile`: the container recipe (#22), one stage on distroless
  static pinned by index digest, copying the binary the release lane stages
  under `dist/<os>_<arch>/`; it compiles nothing. `docker/console/Dockerfile`
  is the operator console's recipe (#603) on the same base, copying the
  console binary and site bundle staged under `dist/console/`.
- `compose.yaml`: the quickstart that runs the published image with the
  hardening set, file secrets, the `cdm`, `cdm-init` and `vocab-load`
  profiles over the pinned PostgreSQL, and the `console` profile that runs
  the operator console beside the bridge.
- `.claude/`: the working discipline. `rules/` (the path-scoped and standing
  rules), `hooks/`, `skills/`, `agents/`, `memory/`.
- `scripts/gh/`: the tracker helpers (`rel.sh`, `project.sh`, `fields.sh`, `watch-prs.sh` with
  `changelog-merge.sh`, which keeps a branch's entries under `[Unreleased]`,
  `labels.sh`, `migrate-fields.sh`).
- `scripts/checks/`: the committed guards. `comment-style.sh`; `versions.sh`,
  which fails when a file disagrees with the `docs/VERSIONS.md` pin matrix or
  claims a licence other than `BUSL-1.1`; `crate-version-guard.sh`, the crate
  bump rule; `favicon-sync.sh`, the book theme favicon against the brand mark;
  `usage-report-off.sh`, every launcher of the server in a test lane with the
  usage report switched off; `todo-issues.sh`, which fails a `TODO(#N)` or a pending subcommand's issue
  that the tracker reports closed;
  `changelog-released.sh`, every released changelog section against its tag;
  `changelog-structure.sh` and `changelog-security.sh`, the Keep a Changelog
  types and the CRA Security rules (a reason line, a `GHSA-` id or VEX path
  per entry) over `[Unreleased]` and, through `scripts/release/plan.sh`, over
  the section a release cuts;
  `pin-freshness.sh`, every pin no ecosystem watches against its newest
  upstream release; and `conformance.sh`, the pass-list gate over the six
  corpora under `conformance/` (fifteen: the two mapping libraries, the
  round-trip laws, the REST API draft, the two HL7 v2 message sets, the five
  FHIR examples packages per version, STU3 to R6, the R4 facade, the HL7
  FHIRPath test suite for R4 as N1 and with the 3.0.0 STU functions, and
  the R4 profile validator's agreement with the HL7 FHIR Validator) (a regression fails, a new pass asks for the
  list, `--update` rewrites the lists and the shields.io badge JSON).
  `affected.sh` computes the packages a change reaches, which decides the
  CI lanes a pull request runs, and `fast.sh` is the default local gate over
  the same set (`docs/ci-cd.md`).
- `.github/`: issue and pull-request templates, CODEOWNERS, Dependabot, and
  fifteen workflows (CI, PR guards, Docs, Scorecard, CodeQL, SonarQube Cloud,
  Release, Release build, Release image, Release console, Scan and tag images,
  Published image scan, Publish crates, Pin freshness, Fuzz). `ci.yml` runs
  its workflow, shell, container and guard tier beside the Rust tier; its
  `conclusion` job is the single required check on `main` (`docs/ci-cd.md`).
  `pr-guards.yml` re-runs the guard a changed label or description affects
  in the head commit's CI run.
  `release.yml` is dormant until a `v*` tag is pushed and cuts a release from
  the changelog section, and its `crates` leg publishes the library crates
  through Trusted Publishing. `release-build.yml`, `release-image.yml` and
  `release-console.yml` are the three reusable workflows it calls, which is
  what puts the signing identity out of reach of caller-defined steps (SLSA
  Build Level 3); they produce the attested tarballs, the SBOMs per target,
  and the multi-platform GHCR images of the bridge and of the operator
  console, pushed by digest. Its `vulnerability-record` job scans both
  images and the server tarballs at every severity and refuses the release
  while a finding lacks an OpenVEX judgement under `security/vex/`
  (`scripts/checks/vex-coverage.sh`); `scan-and-tag.yml` then gates each
  digest on fixable HIGH and CRITICAL findings before it applies the tags,
  and `image-scan.yml` scans the published images weekly with the VEX
  applied. `security/vex/rust-advisories.openvex.json` is generated from
  `deny.toml` and checked in the `cargo-deny` job. `pin-freshness.yml` reads
  every pin no Dependabot ecosystem covers against its newest upstream
  release each week.
- Root markdown: this file, `README.md`, and the community and governance set.

When the workspace lands, each crate carries its own `CLAUDE.md` with
crate-local discipline, and this section becomes the crate map.

## Issue workflow (the loop)

The tracker is GitHub Issues; the open issue list is the worklist
(`.claude/rules/issue-workflow.md`). The type (Bug, Feature, Task), the
priority (Urgent, High, Medium, Low) and the effort (High, Medium, Low) are
GitHub's native issue type and the organisation's `Priority` and `Effort`
issue fields, never labels, set with `scripts/gh/fields.sh`; an issue is
filed with `scripts/gh/fields.sh new` so it carries all three from the start.
A Task carries exactly one work-kind label (documentation/chore/refactor/
perf/test/ci), and domain labels are added as needed (`spec:FHIR`,
`spec:FHIRconnect`, `spec:openEHR`, `spec:OMOP`, `spec:OMOCL`). Read an issue
with `gh issue view <n> --json title,body,comments`, never `--comments`, which
prints nothing for an issue without comments. Milestones are releases. Record
progress on the issue (tick criteria, comment); a PR declares `Closes #N`. New
work found while working an issue is filed and fixed before the next unit
starts (the fix-first cadence).
Native sub-issue and dependency edges are set only with `scripts/gh/rel.sh`;
the roadmap board is a view over the tracker, written only with
`scripts/gh/project.sh` (`.claude/rules/issue-relationships.md`,
`.claude/rules/project-board.md`). The SessionStart hook prints the open issue
list with `<Type/Priority>` after each title, and `/next-task` takes the
highest priority first, the oldest first within a priority.

## Model orchestration (workflows and subagents)

**When the session runs on Fable 5 (effort `high`), Fable is the orchestrator,
not the implementer.** Fable plans, coordinates, reviews, and does the taste-
and intelligence-heavy work itself; it fans implementation out to subagents via
the `Agent` tool (`model: 'opus'`, which resolves to the newest Opus, currently
Claude Opus 5: 1M-token context, 128K output; the `.claude/agents/*` defs with
`model: opus` pick it up automatically) or a `Workflow` (per-agent `model`). The
main loop does not auto-delegate, so this section is the standing instruction
that it should.

Why this split: the win is context isolation, parallelism, and sparing the
orchestrator's capacity. Keep Fable's context clean and let Opus workers grind
through file-heavy implementation in parallel. It is an intelligence downgrade
per worker (see the table: Fable outranks Opus on both intelligence and cost),
so delegate by the nature of the work, never reflexively.

Rankings, higher = better. `cost` is relative spend, `intelligence` is how hard
a problem the model can be handed unsupervised, `taste` covers code quality,
API design, and clarity. Models are the ones the `Agent`/`Workflow` `model`
parameter accepts.

| model     | cost | intelligence | taste |
|-----------|------|--------------|-------|
| fable-5   | 2    | 9            | 9     |
| opus-5    | 4    | 8            | 8     |

Only these two models are used: Fable 5 orchestrates (and takes the rare
top-intelligence delegation), Opus 5 is the worker tier for everything else.
Sonnet and Haiku are not used in this project; never pass them to
`Agent`/`Workflow`, even for a mechanical pass.

How to apply (defaults, not limits; override when the output misses the bar.
Intelligence beats taste beats cost when the axes conflict for anything that
ships):

- **Orchestrator (Fable 5, high):** owns the issue loop, architecture and
  design decisions, spec-conformance judgement, and the hard bespoke logic (the
  mapping semantics of each specification, the target-side document
  construction, the terminology binding). Keeps these in context rather than delegating; they
  need top intelligence and taste and are the project's critical path.
- **Delegate to Opus 5 subagents:** bulk or parallelizable implementation on a
  clear spec (wiring handlers, DTO impls, client plumbing, test scaffolding),
  file-heavy investigation, and codebase analysis, which would otherwise burn
  the orchestrator's context. Fan out several concurrently in one message (max
  5 implementation workers, the owner cap since 2026-10-19). Opus 5's 1M-token window means one
  worker can hold a whole subsystem, so prefer one worker with the full spec up
  front over splitting a coherent task. Prompt-tune for Opus 5: it self-verifies,
  so drop "double-check your work" scaffolding from worker prompts; it also
  delegates onward readily, so tell workers NOT to spawn their own subagents.
- **Fable 5 subagents:** use when a delegated task still needs top intelligence
  or taste (a tricky algorithm, an API-shape decision) but you want it off the
  main context.
- **Reviews:** an independent read before committing a subsystem, especially
  spec and wire conformance. Spec questions and requirements extraction go to
  `spec-researcher`; bounded implementation to `implementer`. Both are defined
  in `.claude/agents/` and both are handed the governing spec sections in the
  prompt.
- Effort: keep Fable on `high`. Use `effort: 'low'` for cheap mechanical worker
  stages, higher tiers only for the hardest verify or judge stages.

Discipline is unchanged for subagents: they obey the hard rules below (never
hand-edit a generated file, no test-weakening, conventional-type branches, no
AI attribution). Delegate with a tight spec and verify the result. Every
worker brief names the worker's own scratch subfolder (`<scratchpad>/<task>/`)
and forbids the scratchpad root, so one worker never publishes another's file
(`.claude/memory/worker-scratch-subfolders.md`).

## IMPORTANT hard rules

- **The specification text is the oracle.** The conformance authority is the
  HL7 FHIR specification for the version in play, the FHIRconnect
  specification, the OMOP Common Data Model, the OMOCL specification, and the
  openEHR ITS-REST specification, never memory and never another
  implementation's behaviour. Read the governing section before implementing or
  reviewing any spec-facing behaviour, and cite it (spec, page or section) for
  conformance-relevant decisions. Full policy:
  `.claude/rules/spec-adherence.md`.
- **Cite only durable references:** the FHIRconnect and OMOCL specifications
  (including published JSON schemas), the HL7 FHIR specification, the OMOP
  Common Data Model documentation, the openEHR specifications, or official
  external documentation (the Rust book and
  reference, the docs.rs page of a pinned crate). Never cite an internal
  markdown file as a design authority; internal plan documents are deleted in
  the PR that implements them. Where no specification governs a decision
  (storage mechanics, the process model, infrastructure), flag it: "no
  specification governs this: our own design".
- **Never hand-edit a `// @generated` file.** Change the generator and
  regenerate (`.claude/rules/codegen.md`).
- **Comments follow RFC 505 and RFC 1574 with hard budgets:** line comments
  only, pending work is `// TODO(#NNNN):` naming its issue, a settled decision
  is `// NOTE:` as a citation and one sentence. No essays in code; the record
  lives on the issue or PR (`.claude/rules/comments.md`, enforced by
  `scripts/checks/comment-style.sh`).
- **Prose follows `.claude/rules/writing-style.md`:** no em dashes, no
  "not X but Y", no decorative triads, no filler buzzwords.
- **Branches use conventional types** (`feat/`, `fix/`, `chore/`, `docs/`,
  `refactor/`, `perf/`, `test/`, `ci/`, `build/`, `release/`) as
  `<type>/<kebab-case-slug>`. Never force-push `main`.
- **NEVER add AI or Claude attribution** to a commit, PR, issue, comment, or
  code comment: no `Co-Authored-By`, no "Generated with", no bot trailer or
  footer, no emoji marker, ever. This is an absolute rule with no exceptions.
  A `PreToolUse` hook blocks a commit or PR command carrying one.
- **Keep the changelog.** `CHANGELOG.md` follows Keep a Changelog 1.1.0: every
  change with user-visible effect adds an entry under `[Unreleased]` in the
  same PR. Releases are cut from the changelog.
- **Never weaken, skip, or delete a test** to make a build pass, and never edit
  a test to route around a bug it exposes (`.claude/rules/testing.md`).
- **Record progress on the tracker and commit before ending a session.** Issues
  and git survive `/clear` and `/compact`; the session todo list does not.
- **Build compiling, tested increments** once code exists. Do not defer
  compilation; keep every crate you touch green.

## Licence

The project's own code and text are under the **Business Source License 1.1**
(`LICENSE`, `NOTICE`): free to read, build, modify, and redistribute, free for
every non-production use and for non-commercial production use, a commercial
licence from the Licensor for any other production use (always for a hosted,
managed, or embedded service and for for-fee distribution), and Apache License
2.0 four years after each version. The Licensor, Cadasto B.V., handles the
business side, the commercial licence included (info@cadasto.com,
<https://www.cadasto.com/contact/>); the maintainer handles the technical
side. Every first-party file carries
`SPDX-FileCopyrightText: Cadasto B.V.` and `SPDX-License-Identifier: BUSL-1.1`
in its header. A generated file keeps its `// @generated … DO NOT EDIT.` banner
on the first line and carries the two SPDX lines under it, written by its
emitter, which names Apache-2.0 for `crates/fhir-types` and `crates/hl7v2-types`. A contribution
is licensed under the same licence and grants the Licensor the relicensing
right in CONTRIBUTING.md § Licensing of contributions; the pull request
checkbox records it and `contribution-licence-guard` enforces it.
Vendored specifications and third-party material keep their upstream terms,
recorded in a `PROVENANCE.md` beside each vendored tree
(`.claude/rules/vendored-inputs.md`).

The decision and its history are recorded in
`.claude/memory/license-busl.md`. Three places name Apache 2.0 as a licence of
its own: `LICENSE`, where it is the Change License, and `crates/fhir-types`
and `crates/hl7v2-types`, the two crates published under it (the second by
the owner ruling on #251: generated Rust is the project's own code).

## Working discipline (`.claude/`)

Path-scoped rules load on demand when files in their scope are read; the rest
apply always. Read the relevant one before working in that area.

- `.claude/rules/writing-style.md`: no AI tells in any prose. The top priority
  for docs and comments.
- `.claude/rules/rust-style.md`, `reliability.md`, `comments.md`, `testing.md`:
  the Rust engineering discipline (idiomatic style, safety posture, comment
  budgets, test discipline).
- `.claude/rules/spec-adherence.md`: FHIR, FHIRconnect, OMOP CDM, OMOCL, and
  openEHR ITS-REST as the oracles; strictness; cite the spec.
- `.claude/rules/codegen.md`: the generated-versus-hand-written rule, pending
  the research that fixes the boundary.
- `.claude/rules/vendored-inputs.md`: every external corpus is fetched by a
  committed `scripts/vendor/*.sh`, vendored verbatim, provenance-stamped.
- `.claude/rules/leptos-ui.md`: the operator console's Leptos discipline,
  scoped to `app/ferrobridge-console/**`. FerroUI leads the UI: the rule opens
  with FerroUI's consumer guide and component rules and keeps only what is
  particular to this console.
- `.claude/rules/ci-cd.md`, `ai-code-review.md`, `crates-publishing.md`: the
  workflow-security discipline, the advisory-analyzer policy (SonarQube Cloud,
  CodeQL), and the crates.io bump and publish rules.
- `.claude/rules/changelog.md`: the changelog structure and the Security
  rules a release cut is refused on (`docs/release.md`, `docs/post-market.md`).
- `.claude/rules/issue-workflow.md`, `issue-relationships.md`,
  `project-board.md`: the tracker work style.
- Skills: `/spec-lookup` (find the authoritative answer in oracle order),
  `/next-task`, `/phase-done`, `/phase-status` (the issue loop),
  `/regen-codegen` (regenerate `crates/fhir-types` and check for drift).
- Agents: `spec-researcher`, `implementer` (both on Opus 5).

## Sibling projects

`FerroEHR` (`../ferroehr`) is the reference openEHR CDR and `FerroTERM`
(`../FerroTERM`) the reference FHIR terminology server. Both are read-only
prior art from here: read their code, rules, and history freely, and never edit
either from this repository (`.claude/memory/sibling-projects.md`).

## References

- The FHIRconnect specification v1.0.0 (the FHIR mapping specification):
  <https://sevkohler.github.io/FHIRconnect-spec/build/site/FHIRconnect/v1.0.0/index.html>
- OMOCL (the OMOP mapping specification): <https://github.com/SevKohler/OMOCL>
- The HL7 FHIR specification: <https://hl7.org/fhir/>
- The OMOP Common Data Model: <https://ohdsi.github.io/CommonDataModel/>
- The openEHR ITS-REST specification:
  <https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>
- The tracker: `gh issue list --state open`. Issue #1 carries the research
  program that produces `docs/architecture.md`.
