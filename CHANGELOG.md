<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintenance rule: every pull request that changes user-visible behaviour adds
an entry under **[Unreleased]** in the same PR. Cutting a release renames
[Unreleased] to the version and date, and adds a fresh link reference.

The architecture is recorded in `docs/architecture.md`, the output of the
research program on
[issue #1](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1). Releases on
the 0.0.x line carry the repository, its gates, its documentation, a Linux
binary per architecture and a container image, each with provenance and an
SBOM you can verify (`SECURITY.md`); from 0.0.3 they also carry the library
crates on crates.io.

## [Unreleased]

### Added

- The FHIRPath evaluator admits the functions FHIRPath 3.0.0 adds as
  Standard for Trial Use behind the opt-in
  `Environment::with_fhirpath_stu3_functions` (#633), so the default stays
  FHIRPath N1 2.0.0, the release FHIR R4 cites. This change implements
  `matchesFull()`, `trim()`, `split()`, `join()`, `encode()`, `decode()`,
  `escape()` and `unescape()`. HL7's FHIRPath R4 suite now counts as two
  corpora with a badge each: `fhirpath-r4` (874 of 886, N1) and
  `fhirpath-r4-stu3` (33 of 104, the tests that call one of the 13 3.0.0
  functions, run with the opt-in).

- The FHIRPath evaluator has a strict ordering mode,
  `Environment::with_strict_ordering`, off by default (#636). It refuses
  `first()`, `last()`, `tail()`, `skip()`, `take()` and the indexer over a
  collection derived from `children()` or `descendants()` with
  `EvalError::Unordered`, the error FHIRPath N1 §3.1 lets an implementation
  return for an order-dependent operation on a list whose order is
  undefined.

- The profile validator checks a resource against a profile on the
  abstract `Resource` or `DomainResource` (#620): a reference target, a
  `meta.profile` claim or a `profile` discriminator walks the resource over
  the profile's elements and its own core definition for the rest, so it
  conforms or fails instead of being listed. `Reason::AbstractProfile` is
  gone, as nothing is listed for it any more.

- The profile validator holds every resolved extension to where its
  definition admits it (#481): each `StructureDefinition.context` of the
  types `element`, `extension` and `fhirpath`, read as R4
  defining-extensions.html §2.5.1.2 and the `extension-context-type` code
  system define them, and each `contextInvariant` evaluated on the element
  holding the extension with `%extension` bound to it. A misplaced
  extension is an `extension-context` error and a broken context invariant
  a `context-invariant` error; a context the walk cannot decide, and a
  context invariant that does not parse or evaluate, stay listed as
  unevaluated with the reason. `Reason::ExtensionContext` now carries that
  reason as a `String`, and `Constraint::ContextInvariant` is new: a
  consumer matching the unit variant changes its pattern.

- A profile slice that holds too few items names why the closest item
  failed it (#490): when a `profile` discriminator, or slicing with no
  discriminator, keeps an item out of a slice that then falls short of its
  minimum, the errors of the rejected item with the fewest are reported as
  `slice-rejection` information beside the cardinality error (an EPS
  Bundle whose Patient lacks `birthDate` now names `birthDate`).

- The profile validator holds a `Reference` or a `canonical` to its
  element's `type.versioning` (#514, R4 elementdefinition.html
  §2.30.0.4.4): under `specific` a REST reference without `_history/[vid]`
  or a canonical without `|[version]` is a `versioning-mismatch` error,
  under `independent` one with a version is, and under any rule a
  `Reference` without a literal `reference` is. A reference that names no
  FHIR REST server counts as version specific (references.html,
  `Reference.reference`), so `independent` lists it as unevaluated.
  `Constraint::Versioning` and the reasons `NotRestfulReference` and
  `VersioningRule` are new.

- The operator console ships as its own image,
  `ghcr.io/ferrohealth/ferrobridge-viewer` (#603). `docker/viewer/Dockerfile`
  builds it on the bridge image's distroless base, runs as `65532:65532`,
  and probes itself with the binary's `healthcheck` subcommand. The
  reusable `release-viewer.yml`, called from the release lane, builds the
  console's musl binaries with `cargo auditable` and its site bundle, attaches
  them to the release with their checksums, provenance and SBOMs, and pushes
  the multi-platform image with the same attestations and self-verification
  as the bridge image; a cut refuses to publish without those assets.
  `compose.yaml` gains a `viewer` profile with a read-only root, every
  capability dropped, `no-new-privileges`, and the OIDC client secret as a
  file secret, and CI resolves the quickstart under every profile and
  dry-runs the console image build.
- `fhir-types` 0.1.118 resolves a JSON member name against the element
  table (#483): `TypeSchema::member` answers the element an object key
  names, the type code a choice form such as `valueQuantity` selects, and
  whether the key is the `_` sibling of a primitive (R4 json.html §2.6.2),
  and `FieldSchema::choices` lists a choice element's alternatives with
  their type codes. The profile validator uses both and drops its own
  copy of the choice matching. The change adds API and breaks none;
  `fhirconnect` moves to 0.1.132 with the requirement.
- `fhir-types` 0.1.119 lists every primitive type with the type its
  `StructureDefinition.baseDefinition` names, in each version's
  `schema::SCHEMAS.primitives`, read by `Schemas::primitive_base_of`
  (`code` specializes `string`) (#635). The new `primitives` field breaks a
  consumer that builds a `Schemas` value by hand: it adds the field.
  `fhirconnect` 0.1.133 follows the requirement.

### Changed

- The profile validator indexes each Bundle once per validation, its
  entries by `fullUrl` and by their resource, so resolving a reference
  inside a Bundle no longer scans every entry twice (#515). A synthetic
  document of 12002 entries validates in 3.6 s instead of 9.9 s in a
  release build; the matching rules of R4 bundle.html §2.36.4.1 are
  unchanged.

- The profile validator checks an invariant a profile declares on a choice
  element it narrows against the types the profile keeps (#573, R4
  elementdefinition.html §2.30.0.4.2): an expression naming a member only
  a removed type defines is listed as refused at the check stage. An
  invariant the profile inherits is still checked against every type the
  base choice admits.

### Fixed

- `/health/readiness` no longer answers `200` while the server is still
  handing the listener from the boot probe routes to the lanes (#796). When
  the CDR was not ready at boot, a successful retry marked the `startup`
  indicator up while the probe routes, which answer `503` for every other
  route, were still serving, so readiness reported ready a moment before
  the lanes did. The indicator now stays down, naming the handover, until
  the lanes serve their own readiness.
- The FHIRPath `is` operator and `is()` function test a FHIR primitive
  against the types it specializes, so `Patient.gender is string` is true,
  as FHIRPath N1 §6.3.1 and the R4 `code` definition ask; `ofType()` and
  `as` keep every primitive an independent type (FHIR R4 fhirpath.html
  §2.9.1.5) (#635).

- The profile validator holds a child that a profile's snapshot leaves out
  to the core definition of the type declaring it (#484), so the core
  bindings, invariants and fixed values of that child apply. Before, a
  partial snapshot left such a child with no element definition at all.
- `fhir-types` 0.1.119 holds the logical id of every resource to the `id`
  form, `[A-Za-z0-9\-\.]{1,64}`, in every version (#482). The published
  definitions type `Resource.id` as `id`
  (R4 resource-definitions.html#Resource.id), while the R4 4.0.1 package
  and 123 resource snapshots of the R6 ballot5 package state `string`
  (upstream report #798). The R4 and R6 element tables now list `id` for
  every resource's `id`, the generated decoders refuse an R4 or R6 resource
  whose id has a space, another character outside the form or more than
  64 characters, and the profile validator reports it as `lexical-form`.
  A consumer that decoded such a resource before gets a `BadValue` error
  at `<Type>.id`. One R4 example leaves the `fhir-r4` pass list, a
  `SearchParameter` whose 67-character id breaks the form.
  `fhirconnect` moves to 0.1.133 with the requirement.
- `fhir-types` 0.1.120 refuses a resource whose required repeating element
  is absent (#312): a `1..*` element such as `Provenance.target` or
  `SearchParameter.base` fails to decode with `MissingProperty` at its own
  path (R4 elementdefinition-definitions.html#ElementDefinition.min), and an
  empty array or a too-short one fails there too (json.html §2.6.2), where
  an empty array was reported at its parent. A `Path::lenient` reader keeps
  the items it finds. Each string primitive's lexical form is public as
  `<Primitive>::check(text)` with the new `codec::LexicalError`, and each
  version's `primitives::check_value(code, value)` decodes a JSON value as
  the primitive a type code names; the HL7 v2 face checks a value with it
  in place of decoding a probe `Extension`. A consumer that decoded a
  resource missing a required list before gets an error now. The
  `fhir-stu3`, `fhir-r4` and `fhir-r4b` pass lists lose the
  `SearchParameter` examples that carry no `base`. `fhirconnect` moves to
  0.1.134 with the requirement.

## [0.0.7] - 2026-10-17

### Changed

- The bridge's health and operator documents have one definition (#777):
  the new library crate `ferrobridge-operator-api` holds them as serde
  types, the server writes `/health/info`, `/health/readiness`, the engine
  state, the file listing and every load report through them, and the
  operator console reads them back into the same types, so a renamed field
  fails the build of both. The wire JSON is unchanged. The console's server
  half now links three workspace crates, and `tests/it/boundary.rs` holds it
  to them.
- A facade search translates a `conceptmap` code once per request (#763):
  the page and its `Bundle.total` share the answers of one terminology
  session, so a code repeated across their entries costs one
  `ConceptMap/$translate` call. Nothing is kept past the request.
- The facade, the FHIRconnect operations lane, `mapping check` and the
  operator API load a FHIRconnect set through one path,
  `mappings::compile` (#725): one read, one template resolution from the
  template directory or the CDR, one compile loop. The boot refusals are
  unchanged.
- `fhirconnect` 0.1.131: the four seam traits, `ReferenceSource`,
  `IdentitySink`, `Terminology` and `MappingFunctions`, require
  `Send + Sync`, and a compile-time assertion keeps `Seams` `Send`, so an
  async caller holds the seams of a run across an `.await` (#764). This
  breaks the crate's public API: an implementation that holds a `Cell` or a
  `RefCell` moves to a `Mutex` or an atomic. The facade's transaction path
  now looks a reference up in the Bundle's own entries through
  `BundleReferences` instead of copying them into its resolved set.

- A facade search lists every resource a `hierarchy.split` with
  `create: resource` creates beside the entry's own resource, each once
  (#731). A page holds at most `_count` resources and may end between two
  occurrences of one entry, `Bundle.total` counts every resource (the count
  reads the rows for such a context), and an `_id` naming a created resource
  answers that resource alone instead of matching nothing.
- `fhirconnect` 0.1.130: the four `operations::run` entry points
  (`to_fhir`, `to_fhir_with_warnings`, `to_openehr`,
  `to_openehr_with_warnings`) take the caller's `&Seams` in place of a
  `&dyn MappingFunctions` (#722), so a caller supplies the terminology server
  beside the mapping functions. This breaks the crate's public API: a
  consumer passes `&Seams::default().with_functions(functions)` where it
  passed `functions`. `$toopenehr` resolves a reference outside the request
  Bundle through the reference source the seams carry.
- The facade resolves a reference a request names and does not carry, for a
  `reference` mapping that reads it (#224). A relative reference, or one under
  `[facade] base_url`, is read through the identity map and the CDR; an
  absolute one is fetched only from a base listed in the new
  `[facade.references] fetch_bases` (empty by default, so nothing is fetched
  until you list a base), with redirects refused, `timeout_ms` (default
  10000) per fetch and `max_depth` levels (default 2), each reference once. A
  reference that is not resolved is a declared skip the write proceeds
  without; the log names the reference, the base and the upstream status, and
  `$validate` answers it as a `warning` issue. In a transaction Bundle an
  entry answers a reference to its `fullUrl` or its `<Type>/<id>` first
  (#753).
- `ferrobridge-term` translates by concept map with an optional target
  system and an optional `reverse` (#721). This breaks the crate's public
  API: `Client::translate` takes a `Translation` (`Translation::new(system,
  code)` with `with_version`, `with_concept_map`, `with_target_system` and
  `with_reverse`), and `Request::Translate` wraps one. Each of `version`,
  `url`, `targetsystem` and `reverse` is sent only when it is set, as R4
  `ConceptMap/$translate` declares each `0..1`, so a FHIRconnect
  `conceptmap`, which names only the map, can be translated in both
  directions.
- A resource the facade's outbound mapping creates, a further
  `hierarchy.split` occurrence with `create: resource` or the target of a
  `reference` mapping, is no longer carried as a contained resource with a
  `#id` reference (#225). It gets an identity-store binding of its own under
  an id derived from its parent's entry, the creating mapping and the openEHR
  occurrence, stable across re-runs and composition versions, and the parent
  references it as `<Type>/<id>`. `GET /fhir/<Type>/<id>` and the vread answer
  it, a `PUT` of it answers `405`, and a type only a `reference` creates is
  declared with `read` and `vread` in the `CapabilityStatement`. The identity
  store writes schema 3 and reads schemas 1 to 3; a file of schema 1 or 2
  opens unchanged and is stamped 3, and a build before this one refuses a
  schema 3 file by name.
- The FHIRconnect operations lane reads its mapping directory through the
  walk the facade uses (#524), so a `.YML` or `.YAML` file is now read by both
  lanes; the operations lane read only the lower-case spellings before.
- The README, the book's Licensing page and the landing page answer "do you
  need a commercial licence?" at a glance (#111): a table of what you are
  doing, what you need and the part of `LICENSE` that decides it, the same
  shape as the FerroHEALTH family table, with the change date and the
  commercial-licence contact beside it. The README's top navigation links the
  table, and the `ferrobridge-fhirpath` and `ferrobridge-validator` READMEs
  state the terms like the other BUSL-1.1 crates.
- `SECURITY.md` and `CONTRIBUTING.md` keep what is particular to FerroBRIDGE
  and link the FerroHEALTH family book at <https://ferrohealth.eu/docs/> for
  the rest (#402). `SECURITY.md` keeps the supported versions, the reporting
  route, the response times, the scope and release verification, and links
  the family policy for disclosure, advisories and the support period.
  `CONTRIBUTING.md` describes the current build and test gate,
  `scripts/checks/fast.sh`. The README and the book's introduction link the
  family book for the manufacturer, the Cyber Resilience Act, how the
  products fit together, and licensing and trademarks.
- The identity store records its schema (#298): a file with no schema row,
  which 0.0.6 or earlier wrote, reads as schema 1 and is stamped on open, and
  a file in a schema this build does not read stops the start with an error
  naming the file, the schema found and the schemas this build reads, leaving
  the file unchanged. The schema this build writes is in the entry for #225.

### Added

- The console edits mapping files (#608). A text area opens a file with
  the digest the operator API's `ETag` names. Check sends the draft to
  `POST /operator/mappings/check` and shows its diagnostics inline. Save
  sends the draft's exact bytes to `PUT /operator/mappings/{path}` under
  `If-Match` (or `If-None-Match: *` for a new file), with a one-line reason
  in `Ferrobridge-Reason`, behind a confirmation the server checks again.
  The result takes the focus: the reload's report after a save, the
  diagnostics of a refused draft (nothing written, the running set
  unchanged), or the `412` conflict when the file changed meanwhile.
  Reload calls `POST /operator/reload`. A provisioned file opens for a check
  alone, and no draft reaches a log line. The WebAssembly budget rises to
  314 KiB with the measurement in `.claude/rules/leptos-ui.md`.
- The console has a transform playground and a validation dry run (#605).
  The playground runs `$tofhir` (a canonical or FLAT composition, a
  template id and a `context.patient` reference) and `$toopenehr` (a
  `Bundle`, a template id and the composition format) against the bridge's
  running programs. The dry run runs `$validate` on a resource type the
  `CapabilityStatement` declares it on. What the operator enters travels in
  the request body alone, with autocomplete off. Every answer is `no-store`
  and drawn on the server: the `Bundle`'s resources, the composition, and
  the `OperationOutcome` issues ordered by severity, with the raw JSON
  behind a disclosure. A refusal shows its status and diagnostics, and
  nothing is stored or logged. A form waits for its page to hydrate, and a
  plain post is refused before the bridge is asked.
- The console shows the mapping files (#607): the list from `GET
  /operator/mappings` with each file's language, kind, owner, whether it is
  writable and its diagnostics count, and each file at `/files/{path}` from
  `GET /operator/mappings/{path}`, verbatim, one numbered line per line, its
  YAML highlighted on the server and its SHA-256 from the `ETag`. Beside
  each file are the running load's diagnostics about it, read into
  `openehr_mapping_core::diagnostic::Diagnostic` and each linked to its
  line. A file that is not UTF-8 is refused, never shown altered, and a path
  that is not plain names below a root is refused before the bridge is
  asked.
- The console shows the engine state (#604). The dashboard has the
  bridge's build facts, pins, lanes and listeners from `GET /health/info`,
  its readiness indicators from `GET /health/readiness`, and the counts of
  its running load from `GET /operator/state`. The capability screen shows
  the R4 `CapabilityStatement` as cards for its resources with their
  interactions, search parameters and operations, the system level and the
  security, with the JSON behind a disclosure and the SMART discovery
  document beside it. The mappings overview shows each context's program
  with its template, profile, resource type, models, extensions and search
  declaration, the templates with their source, the OMOCL files, and every
  diagnostic, read into `openehr_mapping_core::diagnostic::Diagnostic`. Each
  screen is drawn on the server. A refused or unreachable call draws its
  status and diagnostics, or the fault, in its section.
- The console has its shell and visual system (#601): a role layer
  (`surface`, `ink`, `muted`, `accent` and the status tints) derived from the
  "Indigo & Iron" brand tokens, the light, dark and system themes drawn by
  the server from a cookie and switched by a plain form post, the top bar
  with the bridge's status and version as a word and a tint, the theme
  switch and the operator menu, the navigation grouped Overview, Mappings,
  Tools and Bridge with a drawer on a narrow screen, a skip link, an About
  page naming Cadasto B.V., and the page for a path the console does not
  serve. A test computes WCAG 2.2 contrast for every text and ground pair in
  both themes (4.5:1 for text, 3:1 for a control edge and the focus ring),
  and the scroll padding keeps a focused control clear of the top bar (SC
  2.4.11). The WebAssembly budget rises to 244 KiB with the measurement in
  `.claude/rules/leptos-ui.md`.
- The operator API saves search declarations (#766): `PUT
  /operator/mappings/fhirconnect/{name}.search.yml` checks the declaration
  against the context it names and writes it under
  `mapping_dir/fhirconnect/`, where it was refused with `400` before. The
  reload binds every declaration, provisioned and console, and swaps them in
  with the facade's programs. A declaration that does not bind answers `422`
  with an `fb-search-declaration` diagnostic and changes nothing.
- The operator API saves OMOCL files (#757): `PUT /operator/mappings/omocl/{path}`
  checks the draft with the whole set and writes it under
  `mapping_dir/omocl/`, where it was refused with `409` before. `etl run`
  reads that directory beside `[mappings] omocl` whenever the operator API is
  on, through the same OMOCL load the reload checks with: a provisioned file
  owns a path both directories hold, and two files that define one mapping
  refuse the run.
- The operator API checks, saves and reloads mapping files (#597), with
  `[operator] mapping_dir` set: `POST /operator/mappings/check?path=` answers
  a draft's diagnostics over the running set; `PUT /operator/mappings/{path}`
  with `If-Match` (or `If-None-Match: *`) and a `Ferrobridge-Reason` header
  saves a FHIRconnect file under `mapping_dir/fhirconnect/` when the whole
  set compiles with it, written to a synced hidden file and renamed into
  place, and answers `422` with every diagnostic, leaving disk and programs
  unchanged, when it does not, `412` on a stale `ETag`, `428` without a
  precondition and `409` over a provisioned file; `POST /operator/reload`
  compiles the files on disk. A clean set replaces the facade's programs and
  search declarations, the FHIRconnect operations and the operator state in
  one swap each request reads at entry, so the `CapabilityStatement` follows
  a save at once. Console files join the set at start-up. Each save leaves an
  audit line with the caller's `sub`, the file, the old and new SHA-256 and
  the reason, never content. `compose.yaml` mounts `./operator-mappings` as
  the one writable path beside the read-only root file system.
- Operators sign in to the console with OpenID Connect (#600): the
  authorization code grant with PKCE (`S256`) and a `nonce`, the ID Token
  checked against the provider's JWK Set, issuer, audience, `azp` and
  `nonce`, and a session held on the console's server in two bounded pools,
  carried by one `__Host-` `HttpOnly` `SameSite=Lax` cookie. The console
  refreshes the access token on its server, once per session at a time,
  keeps a rotated refresh token, and ends the session when the provider
  refuses a refresh. `POST /logout` ends the session and sends the operator
  to the provider's end-session endpoint with the ID Token hint. Every
  request other than a safe method is taken only from the console's own
  pages, and the console calls the bridge with the operator's own access
  token. `[session]` and `[oidc]` configure it, and the book's Operate
  section documents it under "The operator console". The test OpenID
  Provider is `ferrobridge_testkit::oidc`.
- `ferrobridge serve` sends the FerroHEALTH installation report to FerroPULSE
  on its report envelope v2 (#581): one `start` report per process start and
  one `daily` report a day to `https://report.ferropulse.eu/v1/report`,
  carrying the version, the licence type, the deployment kind, coarse host
  size, the enabled lanes, the mapping counts as ranges, the upstream health
  and latency histograms per route group, and never clinical content, an
  identifier other than the random instance id, a mapping name, a URL or a
  request path. The new `[usage_report]` section (`enabled`, `endpoint`,
  `state_dir`, `deployment`) configures it; it is on by default, sends
  nothing without a writable `state_dir`, stays off in a CI environment,
  and logs one boot line saying which. The compose file keeps the instance
  id on a new `ferrobridge-state` volume. The book's new Operate page, The
  usage report, lists every field and carries the privacy notice. Every
  report the server builds is validated in the tests against FerroPULSE's
  schema, vendored by `scripts/vendor/ferropulse-schemas.sh`, and
  `scripts/checks/usage-report-off.sh` (a new CI job) fails a test launcher
  that leaves the report on.
- The daily installation report counts the HL7 v2 face's messages and the
  mapping failures (#744): a new `hl7v2_message` latency group times each
  message from its frame to its acknowledgment, the counters `hl7v2_aa`,
  `hl7v2_ae` and `hl7v2_ar` count the acknowledgment codes the face sent,
  and `mapping_failures` counts each resource or composition a FHIRconnect
  program refused to map, on a facade or HL7 v2 ingest, on a facade read or
  search (#773) and on an operation call. The book's usage report page lists
  the new keys.
- The server translates the codes of every `conceptmap` mapping through the
  configured `[terminology]` server (#722), on the FHIR facade, the
  `$tofhir` and `$toopenehr` operations and the HL7 v2 face. A composition
  going into FHIR calls `ConceptMap/$translate` with the map `url` and the
  source code, and FHIR going into openEHR adds `reverse=true`. Every code is
  resolved before the run that builds the result, so a terminology fault
  never leaves a half-built document. With no `[terminology]` section a
  `conceptmap` refuses with "no terminology server is configured to
  translate through".
- `fhirconnect` 0.1.128 translates the codes of a mapping that names a
  `conceptmap` through `ConceptMap/$translate` (#219), over a new
  `engine::terminology::Terminology` seam that `Seams::with_terminology`
  sets. With no server configured the run refuses
  (`EngineError::Terminology`), so a code is never passed through under a
  system it was not translated into. Each distinct code is translated once,
  with the resource's own system and code, before the composition is built;
  only an `equivalent` or `equal` match counts, and the translated concept's
  display wins over the source text. An untranslatable code at an optional
  element writes nothing and is declared as `SkipReason::Untranslated`; at a
  required element it refuses the unit (`EngineError::Untranslated`).
  Breaking for a consumer that matches `SkipReason` exhaustively: add an arm
  for `Untranslated`.
- The read-only operator API (#596), behind `[operator] enabled` (off by
  default, refused at boot without `[auth]`): `GET /operator/state` answers
  the compiled programs with their files, templates and search declarations,
  the OMOCL validation result and every diagnostic of the running load;
  `GET /operator/mappings` lists every mapping file with its language, kind,
  SHA-256, owner and diagnostics; `GET /operator/mappings/{path}` serves a
  file verbatim with its SHA-256 as the `ETag`, confined to its directory.
  Every route answers `401` without a token and `403` without
  `auth.operator_scope`, and logs one audit line naming the caller and the
  route, never content. The book's Operate section has a page for it.
  Every diagnostic it and `ferrobridge mapping check --format json` answer
  is the serde form of `openehr-mapping-core`'s `Diagnostic` (#726), so
  `model_path` is now an array of steps.
- `$validate` admits a token that carries the operator scope beside its `c`
  or `u` permission, and `$tofhir` and `$toopenehr` admit it as they admit
  any valid token, so an operator console dry-runs a mapping without a
  clinical write scope (#598).
- `ferrobridge mapping check` runs (#595). It loads the FHIRconnect tree of
  `[mappings] directory`, compiles every context against its template from
  `[mappings] templates` or the `[cdr]`, loads and validates the OMOCL files
  of `[mappings] omocl`, and prints every diagnostic with its file, line,
  column, severity, code and message, as text or with `--format json`. It
  exits 0 when no diagnostic is an error, 1 when one is, 69 when the check
  cannot run and 78 when the configuration names nothing to check. The
  checker is the library module `check`, which the operator API reuses.
- `openehr-mapping-core` 0.1.113: `Diagnostic` has a serde form (#726), one
  flat object with `file`, `line`, `column`, `severity`, `code`, `message`,
  `mapping` and `model_path`, and reading it back refuses an invalid code or
  mapping name as the constructors do. `DiagnosticCode::parse` reads a code
  from its stable spelling. `fhirconnect` moves to 0.1.129 with it, since it
  pins `openehr-mapping-core` exactly.

- `scripts/deploy-probe.sh` probes a running deployment at a base URL (#220)
  and reports each lane (health, the facade under `/fhir`, the FHIRconnect
  operations) as `on`, `off`, `failing` or `uncovered`. An `uncovered` lane
  fails the run with its own exit status, `--expect <lane>=<on|off>` fails a
  lane in the wrong state, and the report names the findings each lane was
  not observed in. The Operate section of the book documents it under
  "Checking a deployment".
- The operator console has its crate, `app/ferrobridge-viewer` (#599): the
  `ferrobridge-viewer` binary, a Leptos app rendered on the server and
  hydrated in the browser, with `serve`, `config check` and `healthcheck`.
  It reads a TOML configuration with `FERROBRIDGE_VIEWER__<SECTION>__<KEY>`
  overrides and `_file` secrets, answers every response with a per-response
  nonce Content-Security-Policy, `nosniff`, `DENY` framing and
  `no-referrer`, marks every document `no-store`, and compresses only its
  site bundle. Its bridge client reads the bridge's R4 `CapabilityStatement`
  and keeps the status and body of a refusal. A test holds its server half
  to `openehr-mapping-core` and `fhir-types` of the workspace and its browser
  half to none, and the CI `viewer` lane lints the WebAssembly half, builds
  the site bundle with pinned tools and fails it above a 112 KiB
  brotli-compressed budget. Sign-in and the pages follow in #600 and #601.
- `fhir-types` 0.1.117 carries a search parameter registry per FHIR version
  behind `resources` (#522): `<version>::search_parameters::SEARCH_PARAMETERS`
  holds every `SearchParameter` of the core package, apart from its examples,
  with canonical, code, base types, type and expression, and
  `SearchParameters::of(type, code)` answers the standard parameter a resource
  type answers, its own or a base type's. `fhirconnect` moves to 0.1.127 with
  it.
- The facade refuses at boot a search declaration that gives the code of a
  standard R4 search parameter of its resource type another type, and the
  `CapabilityStatement` names the standard parameter's canonical as
  `searchParam.definition` for every declared standard code (#522).
- `/health/readiness` carries a `cdm` indicator when a `[cdm]` section is
  configured (#615). It opens one connection to the CDM database over the
  configured TLS and asks `SELECT 1`, and reports down with the reason on a
  refused connection or a failed query, so an unreachable CDM shows before the
  next `etl run` fails. Without `[cdm]`, readiness is unchanged.
- The operate book says where the FHIR facade's identity store lives, which
  volume it needs on a read-only root filesystem, and what happens across
  replicas and restarts, with a Kubernetes pod spec (#419). The container
  page no longer says the server writes nothing.
- A facade read and vread answer `Last-Modified`, the commit time the CDR's
  revision history records for the version (#348). Each read makes one more
  CDR call, `GET /ehr/{ehr_id}/versioned_composition/{uid}/revision_history`.
  The `CapabilityStatement` declares `versioning: versioned-update` and
  `readHistory: true` on every resource type.

### Fixed

- The operator API checks and compiles the console's FHIRconnect files on
  every save and reload when no `[mappings] directory` is provisioned
  (#774). Before, those files were listed and never checked, so a broken
  draft was written. In that deployment the templates come from the CDR.
- The request log writes `<unmatched>` as the route of a request no route
  matched (#746), instead of the raw path the client sent, so an identifier
  in that path no longer reaches the log.

- A percent-encoded `_count`, such as `_count=%32%30`, reads as its decoded
  value on every facade route (#523). The request guard read `_count` from
  the raw query and refused it with a `400`, while the search read the
  decoded value; both now read the one decoded value.
- A vread of an earlier version after the latest version was deleted answers
  that version (#348). The facade names the version from the revision
  history, where it took the creating system from the latest version and so
  answered `410`.
- A CDR call whose token the CDR refused, and whose renewal the token
  endpoint then refused, is no longer retried (#663). The client asked the
  refusing token endpoint again on every retry of a `GET`; it now ends the
  call after the one renewal, with the `401` standing and the token error as
  its cause, and the facade still answers it with the credentials `502`.
- A CDR that does not answer the boot template fetch yet no longer fails the
  start (#422). `serve` binds its listen address, answers liveness `200` and
  readiness `503` with a `startup` indicator naming the attempt and its
  reason, and tries the start again after a wait that doubles from 1 to 30
  seconds, so a Kubernetes pod that starts before its CDR stays up instead of
  crash-looping. A documented refusal, such as a `404` for a template the CDR
  does not hold, still stops the start.
- The facade keys a second or later entry of a composition by the openEHR
  positional path `/content[<n>]`, `n` its 1-based place in `content`, and
  the first by `/content[<archetype>]`, both built with the `openehr-rm` path
  types (#242). Earlier builds wrote `/content[<archetype>, <n>]`, which the
  openEHR BASE path grammar reads as a name predicate and refuses. Each new
  binding records identity derivation 2. An id an earlier build recorded
  under the old form keeps naming its entry: the facade looks the old key up
  when the new one is unbound and still reads a stored binding of the old
  form, so no resource a client holds is renamed. A resource first bound by
  this build gets an id derived from the new path.
- A binding a build up to 0.0.5 recorded for a composition holding several
  entries of one archetype named its entry by its place in `content`, and
  0.0.6 read the same `/content[<archetype>, <n>]` text as the `n`th entry
  of the archetype, so such a resource could answer another entry (#659).
  The facade now reads both meanings: the path resolves when one names an
  entry or both name the same one, and when they name two entries, the read,
  search or bind that meets it answers `500` with an `exception` issue naming
  the path, and no id moves to another entry.
- A transaction entry, and a retry that binds a contribution an earlier
  delivery committed, now derive the entry's identity from the composition
  the CDR stored at its version (#284). Before, they derived it from the
  composition the current run built. Nothing the derivation reads differed
  between the two, so no id changes. A CDR that stores an entry
  `LOCATABLE.uid` the sender did not send now gives that entry the same id on
  the transaction path as on the single create.
- The `409` a create meets when an identity store written before 0.0.4 binds
  its `id` to two compositions now names the repair (#298): its `conflict`
  issue lists the resource id of each composition, and an update
  (`PUT [base]/[type]/[id]`) of the one the sender means revises it.
- A stored version container that the openEHR BASE `HIER_OBJECT_ID` grammar
  refuses, or a stored `ehr_id` carrying a character that would change the
  request path, now answers a `500` whose `exception` issue names the
  resource the record belongs to, the field and the value (#280). Before, the issue named
  neither the resource nor the field. The Operate page on failure and
  identity states the rule.

### Security

- An inline secret in the configuration file (`bearer_token`, `password`,
  `client_secret`, the `[cdm] url`) is a `SecretString` from the moment the
  file is parsed, so a `Debug` rendering of the configuration tree redacts it
  and its memory is zeroed when it is dropped (#309). A `_file` sibling's
  text is wrapped the same way as soon as it is read.

## [0.0.6] - 2026-10-17

### Added

- A FerroBRIDGE OMOCL `Person` mapping for the KDS person template (#568),
  `crates/omocl/tests/fixtures/projects/ferrobridge/kds_person/kds_person_data.yml`.
  The corpus file `person_data/person_data_v0.yml` reads gender at
  `items[at0019]`, which `KDS_Person` does not carry, so every KDS person
  took `gender_concept_id` 0. This file reads the administrative gender at
  `items[at0022]`: its code resolves through the loaded OHDSI vocabulary
  into `gender_concept_id` and is kept in `gender_source_value`, and a person
  with no gender, or a code with no standard concept, takes concept 0. Point
  `[mappings] omocl` at a directory holding it in place of the corpus file.
- `[etl.vocabulary_aliases]` maps a terminology id, such as a URI or an OID,
  to the OHDSI `vocabulary_id` the ETL looks a coded value up in (#625).
  The table is added over the defaults (`LOINC` to `LOINC`, `SNOMED-CT` to
  `SNOMED`) and checked at load: an empty id or vocabulary, or a vocabulary
  longer than the 20 characters of OMOP CDM v5.4 `VOCABULARY.vocabulary_id`,
  refuses the start. A KDS administrative gender coded under
  `http://terminology.hl7.org/CodeSystem/v2-0001` now reaches
  `person.gender_concept_id` once that id is aliased to the vocabulary that
  holds its codes; before, any terminology id longer than 20 characters
  resolved to concept 0.
- The FHIR facade answers `batch` Bundles on `POST [base]` (#531). Each entry
  runs as the read, vread, search, create or update its `request` names,
  through the same handler as the single interaction, with `ifMatch` and
  `ifNoneExist` as its conditional headers and the request's
  `Prefer: return` applied to it, and the answer is a `batch-response`
  Bundle with one entry per request entry in the same order, each carrying
  its status, `location`, `etag` and resource, or its `OperationOutcome` in
  `response.outcome` (R4 http.html §3.1.0.11). A CDR refusal of one entry is
  that entry's status through the status table and the others still run.
  Under `[auth]` each entry is authorised on its own, and an entry the token
  does not cover, or one outside a `patient/` token's compartment, answers
  `403` inside the response. An entry referencing another entry's `fullUrl`
  answers `400`, as R4 calls it non-conformant, and a Bundle with an entry
  that has no `request`, or a `POST` or `PUT` with no resource, is refused
  whole with `400`. Each entry leaves its own audit line, with its position
  as `entry`, beside the request's line, whose interaction reads `batch`. The
  `CapabilityStatement` declares `batch` beside `transaction`.
- The FHIR facade answers search (#98): `GET [base]/{type}?…` and
  `POST [base]/{type}/_search` over an AQL projection declared beside the
  context mapping, in a `<name>.search.yml` file that names the context, the
  subject parameters, the `token`, `string` and `date` parameters with the
  repeated node each is read under and its path relative to that node, and
  the total order pages are read in. The query binds that node in its `FROM`
  once per parameter occurrence, so a token's code and system always come
  from one coded value (#526), a path crossing a node the template lets repeat
  below the bound one is refused at load, and a composition matching on
  several nodes is one entry, counted once. FHIRconnect leaves
  search out, so the format is FerroBRIDGE's own. Every declaration is checked
  and every query it can build is parsed at boot, and a declaration that does
  not parse refuses the start. A search answers a `searchset` Bundle with an
  `id`, `meta.lastUpdated`, one entry per composition with a unique `fullUrl`
  and `search.mode = match`, `total` counting matches, and `self` and `next`
  links over the CDR's `offset` and `fetch` paging. `_id`, `_count` (capped by
  the new `[facade] search_count_max`, default page `search_count`), `_count=0`
  and `_summary=count`, `_sort` and `_total` are supported. A parameter the
  type does not declare is a `400` naming it unless the client sends
  `Prefer: handling=lenient`; an unsupported modifier is always a `400`. An
  unknown subject or id is an empty searchset with no CDR call, and a CDR
  refusal maps through the facade's status table, a query `400` to a `500` and
  a query `408` to a `502`. The `CapabilityStatement` names `search-type` and
  exactly the declared parameters with `_id`. Under `[auth]`, a search needs
  the `s` permission of its type, and a `patient/` token's search is held to
  its patient's EHR.
- FHIR `Patient` from the KDS person template through FHIRconnect (#468).
  The published KDS person chain of the FHIRconnect mapping library compiles
  against the published `KDS_Person` operational template, which
  `scripts/vendor/kds-person-opt.sh` vendors from the openFHIR test resources,
  under a FerroBRIDGE project context, and a synthetic MII Person `Patient`
  holds both lens laws through it. The published files that do not load,
  compile or convert are replaced by stand-ins, each refusal pinned by file
  and line; the defects are recorded on #475 and #468. Seven library files
  join the mapping-library pass list and the chain joins the round-trip one.
  `fhirconnect` 0.1.114 adds the `DV_DATE` against `date` and `DV_BOOLEAN`
  against `boolean` cells, reads `value` on a primitive as the primitive
  itself, the form FHIRconnect writes in a condition, and writes the
  context's `health_care_facility` as the `_health_care_facility` family. The
  facade keys a `Patient` by its first identifier with a value, so a
  `Patient` written through it is stored as the `KDS_Person` composition of
  its own EHR and reads back.
- The terminology client speaks FHIR STU3 (3.0.2) beside R4 and R4B (#455).
  `[terminology] wire_version = "stu3"` selects it, and every operation and
  the batch go through the generated STU3 operation contracts. The client
  sends a code system version as STU3's `version` on `$validate-code`, reads
  STU3's `code`-typed `$lookup` property values, and reads the same
  `$translate` match parts and equivalence codes as R4. A request naming a
  parameter STU3 does not declare (a concept map URL on `$translate`, a
  value set version on `$validate-code`) is refused with
  `Error::UnsupportedParameter` before anything is sent.
- The FHIR facade authenticates its callers as a SMART App Launch 2.x
  resource server (#400). A new `[auth]` section, off by default, names the
  issuer and the audience; the issuer's metadata and key set are read at
  boot, and every request under `/fhir` needs a bearer JWT access token
  (RFC 9068 profile by default) except `GET /fhir/metadata` and the new
  `/.well-known/smart-configuration` document. A missing or invalid token
  answers `401` and a token whose SMART v2 scopes (`patient/`, `user/`,
  `system/` with `cruds`, v1 `.read` and `.write` included) do not permit
  the request answers `403`, neither naming a claim or a scope. A
  `patient/` token reaches only its patient's compartment, a transaction is
  authorised entry by entry and fails whole, a create that revises a
  resource the facade holds needs `u` beside `c`, a scopeless token gets system
  rights only for a listed subject, and every FHIR request leaves one audit
  line on the `ferrobridge::audit` target without the token or the body.
  The `CapabilityStatement` carries `rest.security` with the SMART service
  and the `oauth-uris` extension. With `[auth]` off the facade stays open,
  and the server warns at start-up.
- The R4 profile validator evaluates every invariant on every element in
  play, the core definitions' and each profile's, with `ferrobridge-fhirpath`
  on each occurrence of its element (#439). The focus is the element,
  `%resource` the resource holding it and `%rootResource` its container when
  it is contained. A failed invariant is an issue of type `invariant` under
  its key and declaring canonical with the constraint's own severity; a
  best-practice one (`elementdefinition-bestpractice`) is a warning unless
  `Validator::with_best_practice(BestPractice::Error)` asks for errors. An
  empty result neither meets nor breaks the rule, which asks for "true"
  (conformance-rules.html §2.1.0.6), and is listed (`Reason::InvariantEmpty`). `resolve()` reaches contained resources
  and Bundle entries, `memberOf()` reads the value sets of the package set
  and `conformsTo()` validates against a profile of it. An invariant whose
  expression does not parse, does not fit its element's type or does not
  evaluate, including a call the run cannot answer, is listed as unevaluated
  with the error (`Reason::InvariantRefused` and `Reason::NoExpression`
  replace `Reason::Invariants`), and the published defects of
  `cid-0`, `inv-1` and `dom-3` name their upstream report. A constraint the
  snapshot repeats from the core is evaluated once per occurrence.
  `ferrobridge-fhirpath` adds `Focus::within` to name the resources
  `%resource` and `%rootResource` stand for.
  and the server warns at start-up. The guard classifies every route under
  `/fhir` by name, and a route it does not classify answers `403` without
  reaching a handler, so a route added without a classification fails closed
  (#547).
- `fhir-types` 0.1.111 names each type's base type in the element table
  (`TypeSchema::base`, from `StructureDefinition.baseDefinition`, and for a
  backbone element the `BackboneElement` or `Element` its element names), in
  every version, and carries the abstract types a base chain passes through
  (`Resource`, `DomainResource`, `BackboneElement`, and from R5 `Base`,
  `DataType` and `BackboneType`) as table-only entries.
  `Schemas::base_of` and `Schemas::specializes` walk the chain.
  `ferrobridge-fhirpath` reads it, so `is`, `as`, `ofType` and a leading type
  name see `Age` as a `Quantity` and `Patient` as a `DomainResource` and a
  `Resource`; the R4 profile validator takes a resource under an element typed
  `DomainResource` only when its type specializes `DomainResource`, so a
  `Binary` or a `Bundle` there is refused, and the same base chain decides the
  profiles such an element applies and a `type` slicing discriminator (#458).
- The R4 profile validator resolves references inside a Bundle by the method
  of the R4 Bundle page (absolute, relative against the entry's `fullUrl`,
  `urn:uuid:`, version-specific through `meta.versionId`), with a reference
  that matches two entries an error, so a `resolve()` discriminator reaches
  across entries (#441). It holds the resolved target to the element's
  `type.targetProfile`, any of them, and the reference to its
  `type.aggregation`; validates every resource against each profile it
  claims in `meta.profile`, reporting a claim the package set lacks; and
  holds a document Bundle to the content rules of R4 documents §3.3.1: every
  resource the Composition references is in the Bundle, and the Bundle holds
  only the Composition, what it references directly or indirectly, a CSS
  stylesheet `Binary` and the `Provenance` of what it includes. Target and
  claimed profiles are no longer listed as unevaluated when the validator
  can check them.
- `scripts/vendor/profile-packages.sh` fetches the package set of the R4
  profile validator at build time into the ignored
  `docs/specs/fhir-packages/<package>#<version>/package/` (#435), about
  570 MB that is never committed; only each `PROVENANCE.md` is. The set is the HL7 Europe Patient Summary `hl7.fhir.eu.eps` 1.0.0-ballot, HL7
  Europe Base `hl7.fhir.eu.base` 2.0.1 and the International Patient Summary
  `hl7.fhir.uv.ips` 2.0.0, with every package their `package.json` files name,
  transitively, at the version each dependent names: 17 package versions,
  among them EU Base 2.0.0 (the version EPS names) and five versions of
  `hl7.terminology.r4`. `hl7.fhir.r4.core` 4.0.1 is read from the generator's
  tree. Each tarball is checked against the registry shasum, and each tree
  against the file count and digest `docs/VERSIONS.md` pins, a cache-restored
  tree included. The CI job `profile-packages` fetches the set over a cache
  keyed on the pins and runs `--check`, which fails when a dependency of a
  pinned package is not pinned too; `scripts/checks/conformance.sh` fetches
  the set beside the other build-time inputs.
- `ferrobridge-fhirpath`, a FHIRPath evaluator for FHIRPath Normative
  Release 2.0.0 with the FHIR R4 additions (#436): parse an expression once,
  check its paths against the context type, and evaluate it on a resource or
  an element with exact decimals and dates that keep their precision.
  `resolve()`, `memberOf()`, `subsumes()`, `conformsTo()`,
  `elementDefinition()` and `slice()` call seams you supply, and a call
  without its seam is an error naming the function. The HL7 FHIRPath test
  suite for R4 runs as the `fhirpath-r4` conformance corpus, and every
  invariant of the vendored FHIR packages parses.
- `ferrobridge-fhirpath` converts quantities by UCUM (#459): `=`, `~`, the
  comparisons, `+` and `-` work across commensurable units in the more
  granular of the two, so `4 'g' = 4000 'mg'` is true and the R4 invariant
  `rng-2` compares a low and a high in different units, and
  `toQuantity(unit)` and `convertsToQuantity(unit)` convert to the unit
  asked for. The new `ucum` module parses UCUM expressions (prefixes,
  exponents, annotations, the `10*` and `10^` forms, parentheses) against
  the UCUM 2.2 essence, which `scripts/vendor/ucum.sh` vendors and the crate
  embeds whole with its licence. Degree Celsius, Fahrenheit and Réaumur
  convert by their functions; other special units, arbitrary units and units
  of different dimensions answer empty. Six more cases of the HL7 FHIRPath
  suite pass.
- `ferrobridge-validator`, the foundation of a FHIR R4 profile validator
  (#437). It reads FHIR packages offline from `.tgz` files and unpacked
  directories, refuses a set that lacks a dependency a manifest names, and
  resolves StructureDefinitions by canonical into the generated R4 model. It
  checks a resource against the core definition of its type or a profile:
  unknown elements, empty objects, arrays and strings, cardinality per parent,
  choice types, `contentReference`, the lexical form, range and length of
  every primitive, `maxLength`, `fixed[x]`, `pattern[x]`, and extensions
  resolved by `url`, with an unknown modifier extension an error. The report
  carries typed issues with their FHIRPath location and converts into an R4
  `OperationOutcome`; every slicing, invariant, binding and profile the
  validator meets and does not yet evaluate is named in it as `not-supported`.
- The R4 profile validator evaluates slicing (#438): the discriminator types
  `value`, `exists`, `pattern`, `type` and `profile`, combined; discriminator
  paths in the simple FHIRPath subset, with `resolve()` through contained
  resources, `extension(url)`, `ofType()` and `as()`; `value` and `pattern`
  matched by a required binding over a value set the package set enumerates;
  slicing without a discriminator; the rules `closed`, `open` and
  `openAtEnd`, ordered slicing, the `@default` slice and re-slicing; extension
  slices by `url`, including the relative urls of a complex extension's
  children; and the cardinality of each slice and of the slicing entry. An
  item matched to a slice is held to the slice's constraints, and content is
  validated against the profiles its type names. A discriminator the
  validator cannot decide (a path outside the subset, a reference outside the
  resource, a value set it cannot enumerate) is named as `not-supported`, and
  a `#id` reference that resolves to nothing is a warning.
- `fhir-types` 0.1.109 carries FHIR STU3 (3.0.2) as the `stu3` module, behind
  a feature of that name in the default set, emitted from `hl7.fhir.r3.core`
  3.0.2 (#453). The generator reads the shapes STU3 states differently from
  R4: it strips a UTF-8 byte order mark on read, decodes the single and the
  list form of `profile` and `targetProfile` and merges type entries of one
  code into one, takes a primitive value's type from its `_code` JSON type and
  refuses a type entry with neither, reads `structuredefinition-regex` and
  both STU3 binding forms, and gives `Element.id` and `Extension.url` the
  plain-string shape they have in R4. STU3 has no `TerminologyCapabilities`,
  so its terminology root set is the other seven resources, and its
  operation contracts carry exactly what 3.0.2 declares, without the
  terminology ecosystem overlay. `fhirconnect` steps to 0.1.109 with the
  requirement.
- The STU3 examples package, `hl7.fhir.r3.examples` 3.0.2, runs as the
  conformance corpus `fhir-stu3` with its README badge "FHIR STU3" (#454):
  every one of its 8287 resources decodes through the `stu3` model,
  re-encodes, and goes through the XML codec and back. It is fetched at build
  time like the other examples packages, and the suite strips the byte order
  mark 19 of its files begin with.
- The R4 profile validator checks terminology bindings (#440) on `code`,
  `string`, `uri`, `Coding`, `CodeableConcept` and quantity units: a code
  outside the value set is an error under `required` and under the
  `elementdefinition-maxValueSet` extension, a warning under `extensible`
  and information under `preferred`; `example` is not checked. Membership is
  read from the package set: listed concepts, whole code systems with their
  nested concepts, excludes, imported value sets (all of them, as R4
  valueset.html §4.9.5 composes them), pinned code system versions, the
  `is-a`, `descendent-of`, `is-not-a` and `generalizes` filters over an `is-a`
  hierarchy, the `=`, `in`, `not-in` and `exists` property filters,
  `compose.inactive`, and a complete expansion. A coding whose display is
  none its code system gives the code is a warning. A binding the package set
  cannot decide is named with its reason and kept as a pending check that
  `Report::resolve_bindings` sends to a terminology server through
  `ferrobridge-term`, one call or one batch; a refusal or failure stays named
  with its status and is never a pass. The slicing value-set discriminator
  uses the same membership.
- `ferrobridge-term` validates a `Coding` and a `CodeableConcept`, and a code
  with its `systemVersion` and `display`, with an optional `valueSetVersion`
  (#440). `Client::validate_all` answers several validations in one call or
  one batch, and `Error::upstream_status` names the status of a refusal.
- `omop_cdm::quality` runs the nine conformance checks of the OHDSI Data
  Quality Dashboard v2.9.0 (`cdmTable`, `cdmField`, `isRequired`,
  `cdmDatatype`, `isPrimaryKey`, `isForeignKey`, `fkDomain`, `fkClass`,
  `isStandardValidConcept`) against a CDM v5.4 PostgreSQL database, with the
  Dashboard's violated-row count, denominator, threshold verdict, check id and
  result fields (#404). The checks follow the CDM v5.4.3 definitions where
  the Dashboard's field settings disagree with them, so
  `episode.episode_object_concept_id` is checked against both of its domains,
  and `note_nlp.offset` is checked too. A query that fails or returns no
  counts is an error result and never a pass, and the run summary counts a
  pass only from a passed check. The generator emits the Dashboard's check
  catalogue and its table and field settings as `omop_cdm::generated::dqd`,
  refusing any threshold that is not a whole percentage, and the column
  metadata gains `fk_class`.
- `scripts/vendor/dqd.sh` vendors the OHDSI Data Quality Dashboard v2.9.0
  CDM v5.4 check catalogue, thresholds, the SQL templates of its fifteen
  conformance and completeness checks, and the R sources that define their
  verdicts under `docs/specs/dqd/` (#403), as the oracle of the Rust port of
  those checks (#97). A test pins the ten places where the Dashboard's field
  thresholds disagree with the CDM v5.4.3 definitions (#411).
- `fhirconnect` and `openehr-mapping-core` are published on crates.io at
  0.1.108, joining the crate line `fhir-types` carries (#397). Both build on
  the `openehr-*` 0.0.84 crates, so a project on that release, such as the
  family's federation gateway, runs FHIRconnect mappings in process with one
  copy of the openEHR types.
- The bridge obtains its CDR token with the OAuth 2.0 client credentials
  grant (RFC 6749 §4.4) and renews it before it expires (#421).
  `[cdr.credentials.oauth]` takes a `token_endpoint`, or an `issuer` whose
  RFC 8414 metadata (or OpenID Connect discovery document) names it, a
  `client_id`, a `client_secret` or `client_secret_file`, `client_auth`
  (`client_secret_basic` or `client_secret_post`), and an optional `scope`,
  `audience` and RFC 8707 `resource`. The token is fetched on the first CDR
  call, shared by every call, and fetched again `refresh_margin_ms` (30
  seconds by default) before `expires_in` runs out; concurrent calls share
  one fetch. A CDR `401` drops the token and the call is sent once more
  under a new one, and a second `401` reaches the caller as before. A
  refused grant is an error carrying the token endpoint's status and its
  OAuth `error` and `error_description`, and a token that cannot be
  obtained takes the `cdr` readiness indicator down with that reason.
- The `ferrobridge-fhirpath` static check narrows the focus under a type
  guard (#571). A test `$this is T`, `$this.is(T)` or `is(T)`, its `not()`,
  and an `and` or `or` of such tests, narrows what the focus reaches: the
  right operand of `and` and `implies` sees the test true, the right operand
  of `or` sees it false, `iif()` checks its second argument with the
  criterion true and its third with it false, and `where()` answers the
  types its criterion keeps. A member or call in a branch no type reaches is
  not refused, and a member only a type the guard excludes defines is.
  `toString()` on an input none of whose types is a primitive or
  a `Quantity` is refused as `SemanticError::Input` (N1 §5.5.8, §11). The
  `fhirpath-r4` corpus checks a case against the types its `contextType`
  names.

### Changed

- A facade search total costs one AQL count query that reads no composition
  (#661). A search that binds no node and no `_id`, over a template that
  places the context's entry only among the composition's own `content`
  items, counts the entries with
  `SELECT COUNT(*) … CONTAINS COMPOSITION c CONTAINS <class> x[<archetype>]`,
  which equals the entries the search answers. `_total=estimate` always
  answers from that count, or from the row count of a search that binds a
  node. `_total=accurate`, `_count=0` and `_summary=count` read the rows only
  for a search that binds a node or an `_id`, or a template that nests the
  entry in a `SECTION`, where AQL `CONTAINS` also finds nested entries.
- `fhir-types` documents `UnknownResource`, in every FHIR version, as the
  carrier of any resource's JSON body, a root-set type such as
  `MedicationRequest` included, so a relay can write another server's resource
  beside typed entries in one Bundle (#569). The encoder writes the body with
  its values as given, decimals in their lexical form, and its members in
  sorted key order; a decode still yields the typed variant for every resource
  type the enabled features select. A test pins an R4 Bundle carrying both.
  `fhir-types` 0.1.115 and `fhirconnect` 0.1.119.
- The `openehr-*` crates move to 0.0.87, the line the federation gateway pins; `fhirconnect` 0.1.117 and `openehr-mapping-core` 0.1.111 build on it.
- `[etl] aql` and `[etl.visits] aql` must read every `COMPOSITION` below
  `VERSION v[LATEST_VERSION]`, and a configuration whose query reads one
  without that scope (no `VERSION`, a bare `VERSION v`, or `ALL_VERSIONS`)
  now refuses to load with an `etl.aql` or `etl.visits.aql` error (#494).
  AQL 1.1.0 leaves the scope of a query without `LATEST_VERSION` open, and a
  deleted composition's newest version holds no data (openEHR RM Common
  1.1.0 §6.2.5), so the scope keeps deleted and superseded versions out of
  the CDM. Add `CONTAINS VERSION v[LATEST_VERSION]` above `COMPOSITION c` in
  an existing query.
- The bridge builds on the `openehr-*` 0.0.86 crates, the release the
  federation gateway pins, so a project on 0.0.86 that takes `fhirconnect`
  0.1.111 or `openehr-mapping-core` 0.1.109 carries one copy of the openEHR
  types.
- The CI `test` lane and the SonarQube Cloud lane restore and fetch the
  profile packages before the tests, and the profile validator's
  package-gated tests fail under CI when the packages are absent, where they
  returned early and passed (#503).
- `fhirconnect` 0.1.110 locates every loss a run declares (#498). A
  `fhirconnect::engine::outcome::Warning` is now a `Loss` (the former
  variants: `Skipped`, `Defaulted`, `LastOfMany`, `OneWayFallback`,
  `DeferredReference`) with a `Location`, read through `Warning::loss()` and
  `Warning::location()`. `Location::fhir()` is the FHIRPath of the FHIR
  element the loss concerns (the input going into openEHR, the output coming
  out to FHIR), with an index on each repeating element and no function, so it
  fits `OperationOutcome.issue.expression`; `Location::openehr()` is the
  archetype path of the openEHR node. Neither carries a value from the input.
  `operations::run::to_fhir_with_warnings` and `to_openehr_with_warnings`
  return a `Transformed` that holds the answer and the typed warnings in the
  order the engine raised them, so a consumer that logs each transformation no
  longer parses issue text; `to_fhir` and `to_openehr` keep their signatures.
  Each issue of a run's `OperationOutcome` sets `expression` to the FHIRPath
  and carries the openEHR path as a `details.coding` in
  `https://ferrobridge.eu/fhir/CodeSystem/openehr-path`
  (`operations::issues::OPENEHR_PATH_SYSTEM`); `details.text` is unchanged.
  `fhirconnect::tree::locate::locate` names one element of a document as such
  a FHIRPath. A consumer that matched `Warning::Skipped { .. }` matches
  `Loss::Skipped { .. }` on `warning.loss()`.

- SonarQube Cloud analyses FerroBRIDGE under the FerroHEALTH organisation:
  organization `ferrohealth`, project key `FerroHEALTH_FerroBRIDGE`, in
  `sonar-project.properties`, the MCP server configuration and the README
  badges.

- The `openehr-*` crates move from 0.0.72 to 0.0.84 as one pin group (#397).
  The generated ITS-REST client now reads a `2xx` body that depends on
  `Prefer` as the resource or its `Identifier` itself, so a CONTRIBUTION
  `201` body that is neither is refused by the client and the bridge still
  answers with the committed contribution's uid. Four new client refusals (a
  passed deadline, a failed DPoP proof, a credential provider that produced
  nothing, and credentials that cannot form an `Authorization` value) each
  name their own class in a diagnostic.
- A commercial licence, and any other business or licensing question, goes to
  Cadasto B.V., the Licensor, at info@cadasto.com or
  <https://www.cadasto.com/contact/> (#395). Every BUSL `LICENSE` copy names
  that contact in place of the maintainer, and the README, the book's licensing
  page, `MAINTAINERS.md`, `GOVERNANCE.md` and `SUPPORT.md` say the same. The
  maintainer handles the technical side. The licence terms are unchanged.
- The Licensor and copyright holder of the project's own work is Cadasto B.V.,
  replacing Vernum Projecten B.V. (#392). Every `Licensor:`, copyright and
  `SPDX-FileCopyrightText` line names the new company, as do the governance
  text and the site footer. The licence terms are unchanged, and maintainer
  credit stays a person. `fhir-types` steps to 0.1.108 for its new headers.
- The tracker records the kind, the urgency and the size of an issue in
  GitHub's native issue type (Bug, Feature, Task) and the organisation's
  Priority and Effort fields (#416). The `bug`, `enhancement` and `P0` to
  `P3` labels are retired, the bug and feature forms set the issue type, the
  release notes drop their two label-keyed categories, and
  `scripts/gh/fields.sh` and `scripts/gh/migrate-fields.sh` carry the model
  and the migration. `labels.sh` gains the `no-crate-bump` label the
  crate-version guard already reads.

### Fixed

- `fhir-types` 0.1.116 holds an element id to the `string` form in R4B and
  R5 (#378). The published definitions type `Element.id` as `string` in every
  version, while the 4.3.0 and 5.0.0 core packages state `id` on the copies
  `ElementDefinition.id`, `Extension.id` and the other datatypes inherit, so
  an element id such as `Patient.extension:birthPlace` or one longer than 64
  characters was refused. A resource id keeps the form its package states.
  The R4B and R5 examples corpora pass 544 and 191 more instances.
- A create with no `meta.versionId` of a source the facade already holds as
  one entry of a multi-entry composition is recognised as a replay when its
  mapping is that entry as it stands, and commits nothing (#667). The replay
  check compares what a revision would commit, that entry spliced into the
  standing composition, with the composition the CDR holds, and the replay
  answers that entry.
- The facade `CapabilityStatement` declares `conditionalUpdate: false`
  (#666). R4 `CapabilityStatement.rest.resource.conditionalUpdate` names the
  `PUT [type]?[search parameters]` interaction (http.html#cond-update), which
  the facade does not serve; `If-Match` on an update is the version-aware
  update, which the facade still honours.
- A facade update whose body names no `id`, or an `id` other than the one
  its URL names, answers `400` with an `invalid` `OperationOutcome` at
  `<type>.id` and commits nothing, as R4 http.html#update requires (#665).
  The facade used to accept it and record the body `id` as a consumed source
  of the resource the URL named.
- A CDR credential the token endpoint will not issue answers the FHIR caller
  `502` (#518). When the OAuth 2.0 client credentials fetch fails, because the
  token endpoint is unreachable or refuses the bridge's own credentials, the
  facade answers `502 Bad Gateway` with the token endpoint's status and OAuth
  error in the `OperationOutcome` diagnostics, never the client secret. It
  used to answer `500`.
- A request under `/fhir` over the body ceiling or past the request timeout
  answers an `OperationOutcome` (#548). The ceiling answers `413` with the
  `too-long` issue and the timeout answers `504 Gateway Timeout` with the
  `timeout` issue, both as `application/fhir+json` with the limit named in
  the diagnostics. A timed-out FHIR request used to answer a plain `408`,
  which tells a client it may resend a write the CDR may already hold. Every
  other route keeps its plain `408` and `413`.
- The KDS diagnosis project context keeps a diagnosis code system outside
  ICD-10-GM when it reads a Condition back (#379). The `coding` method of the
  `icd10ProblemDiagnose` append was gated by a `fhirCondition` only, which
  FHIRconnect applies to FHIR input alone, so out of openEHR it wrote the
  ICD-10-GM system over a SNOMED CT or any other coding. It now carries an
  `openehrCondition` on the stored terminology as well.
- A KDS diagnosis read no longer carries an
  `icd-10-gm-diagnosesicherheit` extension with a url and no value, which
  FHIR R4 ext-1 forbids (#629). The extension method of the same append had
  the same one-sided gate, so its url manual ran for every read; it now runs
  only when the composition holds a diagnostic certainty.
- A facade search that matches a composition's second entry answers that
  entry (#530). Each entry of the archetype a context maps is a resource with
  an id of its own, derived from the composition and the entry's place among
  the entries of its archetype; the first entry keeps the id it had alone. A
  search reads which entry a row matched from the node it selects, so a
  composition with two matching diagnoses answers two `Condition`s, each
  carrying its own code, and `Bundle.total` and the paging count entries. A
  read, a vread and an `_id` search of an entry's id answer that entry.
- A `CodeableConcept` with codings and no `text` fills a `DV_TEXT` (#626).
  The facade used to refuse such a resource with a `422`, because
  `DV_TEXT.value` had no source. It now takes the first coding's `display`,
  then its `code`, the fallback FHIRconnect writes for `DV_CODED_TEXT`, and
  records the one-way row in the outcome. Reading the resource back adds
  `code.text` holding that rubric and leaves the codings as they were.
- A `recorder` that carries only a reference no longer leaves
  `COMPOSITION.composer` empty (#627). The FHIRconnect engine used to count
  the empty party as a written composer, so the default composer was
  skipped and the CDR refused the composition with "missing field
  `composer`". A party holding no name, identifier or external reference now
  leaves the composer to its default, and a composer with an external
  reference travels as `ctx/composer_id`, `ctx/id_namespace` and
  `ctx/id_scheme` (Simplified Formats, §Composer).
- A facade update of one entry's resource keeps the composition's other
  entries (#657). A `PUT` of an entry's id, and a create that revises a known
  source, commit the composition version they follow with that entry replaced
  and every other item as it stood; before, the composition was rebuilt from
  the one resource and lost its other entries. `If-Match` names the
  composition version every entry shares, and an `If-Match` naming a version
  the CDR does not hold is `412`.
- A CDR credential the token endpoint will not issue answers the FHIR caller
  `502` (#518). When the OAuth 2.0 client credentials fetch fails, because the
  token endpoint is unreachable or refuses the bridge's own credentials, the
  facade answers `502 Bad Gateway` with the token endpoint's status and OAuth
  error in the `OperationOutcome` diagnostics, never the client secret. It
  used to answer `500`.
- A request under `/fhir` over the body ceiling or past the request timeout
  answers an `OperationOutcome` (#548). The ceiling answers `413` with the
  `too-long` issue and the timeout answers `504 Gateway Timeout` with the
  `timeout` issue, both as `application/fhir+json` with the limit named in
  the diagnostics. A timed-out FHIR request used to answer a plain `408`,
  which tells a client it may resend a write the CDR may already hold. Every
  other route keeps its plain `408` and `413`.
- A handler panic under `/fhir` answers an `OperationOutcome` (#655): `500`
  as `application/fhir+json` with the `exception` issue and the request id in
  the diagnostics, never the panic message. It used to answer the plain JSON
  body every other route keeps.
- A CDR token refusal the bridge cannot renew answers the FHIR caller with the
  token endpoint's refusal (#654). When the CDR answers `401` and the one
  renewal of the OAuth token fails, the facade answers the credentials `502`
  with the token endpoint's status and OAuth error (RFC 6749 §5.2) in the
  diagnostics, never the client secret. It used to answer "the openEHR
  service could not be reached".
- A facade search that binds no node, such as `Condition?patient=…`, answers
  every entry of the context's archetype in each matching composition, each
  with its own id, where it answered only the first (#658). `Bundle.total`
  counts those entries for every search, so `_count=0`, `_summary=count` and
  `_total=accurate` read the rows instead of running an AQL `COUNT`.
- Removing a person's newest `PERSON` writer hands the row to the writer
  that remains (#561). When several compositions of one EHR write the
  person's `PERSON` row and the newest of them is deleted in the CDR, or a
  new version of it no longer writes the row, the row now takes the columns
  of the newest writer that remains instead of keeping the removed writer's
  demographics. The bridge schema gains a `person_row` table holding each
  writer's columns; run `ferrobridge cdm init` once after upgrading to
  create it. A writer committed before the upgrade has no columns recorded,
  so a person left with only such writers keeps the row it has until one of
  them commits again.
- A visit query that names `$since` refuses the start (#542). The ETL never
  binds `--since` to the `[etl.visits]` query, so such a query passed the
  load check and the CDR refused it at run time for the unbound parameter;
  `etl.visits.aql` is now refused when the configuration loads, with the
  reason.
- A `TERM_MAPPING` whose `match` is `>`, `<` or `?` no longer fails the
  whole openEHR to FHIR run (#588). FHIRconnect calls the other operators
  "not mappable to FHIR", so the mapping still writes no `Coding`, and the
  run now declares it as a located `Loss::Skipped` whose new
  `SkipReason::TermMappingMatch` carries the match code, the target
  terminology and the code as fields; the value's text and every `=` mapping
  convert. Going back into openEHR, such a mapping the target already holds
  keeps its place. A consumer that matches `SkipReason` exhaustively needs an
  arm for the new variant (`fhirconnect` 0.1.120).
- The container harness starts the reference CDR with
  `FERROEHR__USAGE_REPORT__ENABLED=false` (#582). A container does not
  inherit the CI runner's environment, so FerroEHR's own CI detection never
  fired, and each "e2e (containers)" run reported itself to FerroPULSE as an
  installation.
- An invariant on a choice element reads every type the choice admits
  (#563). The FHIRPath evaluator checks an expression whose context is a
  choice element against all of its types, and on an occurrence of a choice
  a member only another of its types defines is empty, as
  `Observation.value.unit` is on a string value (FHIRPath N1 §3.2). R4
  `ras-1` now holds on a decimal `RiskAssessment.prediction.probability`
  where the R4 profile validator listed it as unevaluable, and the
  core-constraint sweep checks the 191 invariants on choice elements it
  skipped. A member no type of the choice defines is still refused.
- The R4 profile validator reports an extension whose `url` the package set
  cannot resolve to a definition (a `#` fragment, or two definitions under
  one canonical) as an unknown extension at that extension, a warning or an
  error under `modifierExtension`, and goes on, where it ended the whole
  validation with a resolution error (#550).
- `fhir_types::xml::from_xml` resolves the namespace of every element and
  attribute, so the namespace decides and the prefix never does: "FHIR
  elements are always in the namespace http://hl7.org/fhir" (R4 xml.html
  §2.6.1, the same rule in STU3 and R5). A root or nested resource outside
  it is refused as `WrongRoot`, any other element as `UnknownElement`, and a
  prefixed attribute in another namespace or under an unbound prefix as
  `UnknownProperty`; `<f:Patient xmlns:f="http://hl7.org/fhir">` is read. It
  took the local name and ignored the prefix, so
  `<x:Parameters xmlns:x="http://example.org/other">` read as a FHIR
  `Parameters` (#555).
- `fhir-types` 0.1.113 holds the narrative `div` to the XHTML namespace
  (R4 xml.html §2.6.1: "the only other namespace that occurs in FHIR
  resources is the XHTML namespace"; R5 §2.1.6.3.3: "a root `<div>` element
  in the xhtml namespace") and refuses it as `UnknownElement` at
  `Resource.text.div` otherwise. A declaration of the XML Schema instance
  namespace on the `div` or on any element inside it is refused as in the
  rest of the resource (R4 xml.html §2.6.1.1). The reader copied the
  narrative's attributes raw with no check. The content is otherwise carried
  verbatim (#556).
- `fhir-types` 0.1.114 writes every attribute of the narrative `div` it reads
  from XML in double quotes with a `"` the value holds escaped as `&quot;`
  (XML 1.0 §2.3 `AttValue`, §4.6), so `title='a "b"'` yields a well-formed
  `div` string that reads back equal. The reader wrapped the raw value in
  double quotes whatever quote the source used (#575).
- `fhir-types` 0.1.114 reads a narrative `div` whose XHTML prefix is
  declared on an ancestor (`xmlns:h` on the root, `<h:div>`) to a
  self-contained string in the default-namespace form R4 json.html §2.6.2.4
  shows, `<div xmlns="http://www.w3.org/1999/xhtml">`, with the prefix
  removed from the `div` and every element inside it, so the string is the
  `innerHTML` R4 narrative.html §2.4.0 describes. An element inside the
  narrative outside the XHTML namespace is refused as `UnknownElement`, and a
  prefixed attribute outside the `xml` namespace as `UnknownProperty` (R4
  xml.html §2.6.1). The reader copied `<h:div>` verbatim, leaving a prefix
  nothing in the string bound (#576).
- `fhir_types::xml::from_xml` refuses a resource that declares the XML Schema
  instance namespace on any of its elements, naming the declaring attribute
  (`Resource.xmlns:xsi`), because exchanged content "SHALL NOT specify the
  schema or even contain the schema instance namespace in the resource
  itself" (R4 xml.html §2.6.1.1, the same rule in STU3 and R5). It ignored
  the declaration and refused only the `xsi:schemaLocation` attribute, as an
  unknown property. The writer declares the FHIR namespace alone. The five
  FHIRPath suite cases on `parameters-example-types.xml`, which declares
  both, fail with that rule as their reason (#457).
- `fhir-codegen terminology` fails with the file's path when a package file
  cannot be read or is not JSON, where it skipped the file and bundled
  without it. The manifest and the index a package carries are told apart by
  having no `resourceType`. The generator's corpus tests fail the same way on
  a file that does not parse, and skip only a refusal they list with its
  citation (#487).
- The OMOCL engine writes a `PERSON` row from the corpus's `person_data`
  mapping (#560). CDM v5.4 requires `person.race_concept_id`, and OMOCL has
  no key that writes it, so every `Person` record was refused and no person
  got a row. The engine now writes concept `0` in a required standard concept
  column that no OMOCL key writes. The CDM conventions use `0` "if there is
  no information available for that field", and the column's own definition
  says "If a clear race or ethnic background cannot be established, use
  Concept_Id 0". `person.race_concept_id` is the only such column across the
  ten tables OMOCL writes. The run report counts each one as a concept `0`
  assignment. A mapping that leaves out a key it could have written, such as
  `ethnicity_concept_id`, is still refused.
- `openehr-mapping-core` 0.1.110 compares a name predicate the `aqlPath` of a
  Web Template node carries on an ancestor segment, so a mapping path below
  one of two sibling clusters that share a node id (the `Straßenanschrift` and
  `Postfach` addresses of `KDS_Person`) names the node below that cluster
  alone (#468).
- Every `person_id` an `etl run` writes names a `PERSON` row, where the
  rows of a person no mapping wrote a `PERSON` row for named a `person_id`
  that `PERSON` did not hold, against the CDM v5.4 foreign key (#541). A
  `PERSON` row an OMOCL mapping writes is the person's source; otherwise
  `[cdm] person_policy = "create_on_first_sight"` writes a placeholder row
  (every concept 0, `year_of_birth` 0, the `ehr_id` as
  `person_source_value`) that such a mapping later replaces, and `existing`
  refuses the row, also when the person map knows the EHR but its `PERSON`
  row is gone. The run report counts the `PERSON` rows and the placeholders
  (`persons`).
- A second composition of an EHR writing the `PERSON` row another
  composition of the EHR already writes commits, where the bridge's side
  table refused it and the commit failed (#540). Each writer is recorded,
  the one committed last rewrites the columns, and the row stays while any
  writer or any row of the person remains; every other table keeps its
  one-row-per-id guarantee. Run `ferrobridge cdm init` once after upgrading
  to bring an existing bridge schema to this shape.
- FHIRPath quantity products and quotients carry the UCUM unit with
  identical units merged, so `12 'cm' * 3 'cm'` answers `36 'cm2'` and
  `12 'cm2' / 3 'cm'` answers `4 'cm'` as FHIRPath N1 §6.6.1 and §6.6.2
  show, where the unit was the two expressions joined (`'cm.cm'`). Units
  that cancel give `'1'`; a prefix stays part of its unit, and an annotated
  unit is still joined whole (#499).
- The facade's loss log records each declared loss as a value-free kind
  (`skipped-unresolved-reference`, `deferred-reference`, `defaulted` and the
  rest) with its FHIRPath and its openEHR path, where it logged the loss's
  display text, which could carry a reference literal such as `Patient/123`.
  The `$validate` dry run renders each loss as the operations lane does, with
  its FHIRPath in `issue.expression` and its openEHR path as a
  `details.coding`, severity `warning` and code `incomplete`, or
  `information` and `informational` for a defaulted composition field (#509).
- `etl run` removes the CDM rows of a composition deleted in the CDR after an
  earlier run loaded it, where they stayed in the CDM (#527). After the
  compositions, each composition an earlier run committed and this run's
  `LATEST_VERSION` query did not answer is read from the CDR over ITS-REST
  1.1.0 `composition_get`; a `204`, the answer for a deleted composition,
  removes its rows, links and watermark in one transaction, and the derived
  tables are rebuilt after it. A `200` or a `404` keeps the rows, and a call
  that reaches no documented answer ends the run with a typed error.
  `[etl] deletions = "report"` lists a confirmed deletion and keeps its rows;
  the default is `remove`. The run report counts the checks and lists the
  removed, kept and unknown compositions with the rows removed per table.
- Updating or removing the composition that wrote a `PERSON` row keeps the
  row while any row of the person remains, where the update or removal
  deleted it and left clinical rows naming a missing person (#536). The
  `PERSON` row goes only when no composition writes it and no row of the
  person is left, and the newest version that writes it rewrites its columns.
- `etl run` removes a visit the visit query no longer answers, such as one
  whose every composition was deleted, where its `VISIT_OCCURRENCE` row stayed
  and the observation period kept spanning it (#537). A visit some CDM row
  still names is kept and counted, and a run whose visit query refused a row
  removes none.
- `fhir-types` 0.1.110 writes a repeating primitive whose every position
  carries only an `id` or an `extension` as the `_name` array alone, in every
  version, where it wrote a value array of nulls beside it (`"event": [null]`);
  FHIR JSON renders only the `_` property for a primitive with no value
  (<https://hl7.org/fhir/R4/json.html#primitive>). `primitive_arrays` returns
  the value array as an `Option`. The examples that carry the shape now
  round-trip: eight in STU3 and nine in each of R4, R4B, R5 and R6, among them
  the ActivityDefinitions with a `timingTiming` that holds only `_event`
  (#486).
- The R4 profile validator resolves a canonical that two versions of one
  package carry at one resource version to the higher package version, where
  it refused it as ambiguous; two different packages carrying one canonical
  at one version with different content stay ambiguous (#502).
- The R4 profile validator refuses a resource of a type its element's types
  do not name, where an element typed `Resource` or `DomainResource` still
  takes any resource (#488).
- The R4 profile validator resolves a version-less canonical a definition
  names within the dependency closure of the package that carries the
  definition, so a profile built against one version of a package no longer
  gets another loaded beside it, and falls back to the whole set only when
  that closure does not carry the canonical (#489).
- Readiness no longer reports a CDR that refuses the configured credentials
  as up. The CDR check lists the ADL 1.4 templates through the ITS-REST
  Definition API with the configured credentials, filtered so the answer is
  empty, instead of a `GET` on the REST root, and a `401` or `403` from the
  CDR or the terminology server marks that indicator down with the reason in
  its detail (#423).
- The mapping directory walk of the facade, the FHIRconnect operations and
  `etl run` reads a Kubernetes ConfigMap volume once. It skips every entry
  whose name starts with `..` and follows a symlink only when it names a
  file, so the `..data` link and the `..<timestamp>` directory behind it no
  longer make each mapping load three times and refuse the start with
  `duplicate-mapping-name` (#420).
- A `FERROBRIDGE__…` environment value takes the type of the key it sets: a
  string key takes the text verbatim, so
  `FERROBRIDGE__CDR__CREDENTIALS__USER=12345` is the user name `12345`
  instead of a number the boot refuses, and `true` or `1.5` stay text too. A
  number, boolean or array key still reads the text as TOML syntax. Quotes
  are kept as part of a string value, so write a value without the quotes
  that used to keep it text (`12345`, not `"12345"`). `[cdr.credentials]`
  and `[terminology.credentials]` gain `user_file` beside `password_file`
  (#424).
- The generated CDM metadata carries the vocabulary domains of a concept
  column as a list (`ColumnMeta::fk_domain`), so
  `episode.episode_object_concept_id`, which the CDM v5.4.3 definitions give
  the two domains `Procedure, Regimen`, accepts a concept of either. The
  `omocl` domain checks test membership instead of comparing the whole cell
  as one domain name, and the quality checks read the list instead of
  splitting the cell (#414).

### Security

- The TLS stack moves to `rustls` 0.23.46, which fixes RUSTSEC-2026-0346,
  RUSTSEC-2026-0347, RUSTSEC-2026-0348 and RUSTSEC-2026-0349 in the CDR,
  terminology and CDM database connections (#447).

## [0.0.5] - 2026-10-01

### Changed

- **The repository moved to the FerroHEALTH organization**
  (<https://github.com/FerroHEALTH/FerroBRIDGE>, #385); the old
  `rubentalstra/FerroBRIDGE` URLs redirect. The image now publishes to
  `ghcr.io/ferrohealth/ferrobridge`, and the quickstart, the book and the
  landing page name that reference. Everything published up to v0.0.4 stays
  available under `ghcr.io/rubentalstra/ferrobridge`, and its signatures name
  `rubentalstra/FerroBRIDGE` as the repository that built it (`SECURITY.md`).
  The roadmap board is now <https://github.com/orgs/FerroHEALTH/projects/3>.
  `fhir-types` steps to 0.1.107 for its new `repository` field and README
  links.

## [0.0.4] - 2026-09-26

### Added

- FHIR conformance per version (#373). HL7's examples package for R4, R4B,
  R5 and the R6 ballot, at the versions of the pinned core packages, is
  fetched at build time by `scripts/vendor/fhir-packages.sh --build-time`
  into `tools/ferrobridge-testkit/vendor/` (about 600 MB unpacked, so only
  each `PROVENANCE.md` is committed; the pins carry a file count and tree
  digest, `versions.sh` reads them back and `pin-freshness.sh` reads each
  version against the registry). Four corpora, `fhir-r4` to `fhir-r6`, send
  every example through the `fhir-types` JSON codec and back and then through
  the XML codec and back; a fifth, `fhir-r4-facade`, creates and reads every
  R4 example of a type the suite and KDS contexts map through the facade over
  a `wiremock` CDR, with the other examples counted by type, and behind
  `FERROBRIDGE_E2E` a sample of its passing cases runs against the reference
  CDR. The README's FHIR row carries the five badges beside the round-trip
  laws, and the CI `test`, `conformance` and coverage jobs restore the
  packages from a cache keyed on their pins.
- Conformance badges per standard, per HL7 v2 message family and per HL7 v2
  version (#366). The HL7 v2 corpus test records each case's family (MSH-9.1)
  and MSH-12 version in its result, and `scripts/checks/conformance.sh
  --update` writes one badge per family with a case in either corpus
  (`conformance/badges/hl7v2-family-<family>.json`) and one per version seen
  (`hl7v2-version-<version>.json`), each the pass fraction across both HL7 v2
  corpora. The script generates the README badge block: the build badges, then
  one conformance row per standard, each badge linking to its pass list;
  `--check` fails when a badge or the block is stale.
- The file-length guard (#351). `scripts/checks/file-length.sh` fails the CI
  guard tier when a hand-written Rust file exceeds 1000 lines or a listed
  breach in `scripts/checks/file-length-allow.txt` grows; the rule (1000
  hard, split into a module folder at 750, generated files excluded) is in
  `.claude/rules/rust-style.md`, and the forty files that breach it today are
  listed with the sub-issue that splits each.
- The FHIR facade answers the R4 vread, `GET /fhir/{type}/{id}/_history/{vid}`
  (#305), so the `Location` every create, update and transaction entry
  answers now resolves. The facade reads the composition version whose
  version tree id is `{vid}` and answers it with its `ETag`; a version the CDR
  does not hold is `404`, one it reports deleted is `410`, and the
  `CapabilityStatement` lists `vread` on every supported type.
- FerroBRIDGE's supplements to the HL7 v2-to-FHIR guide (#256):
  ConceptMaps in the guide's own shape, shipped inside `ferrobridge-hl7v2`
  under `supplements/` and loaded over the guide's package by
  `Corpus::with_shipped_supplements`, which the face calls before the
  `[hl7v2] supplements` directories; a supplement with the id or url of a
  loaded map replaces it, any other is added, and the run counts each one it
  uses as `supplemented` (`Outcome::Supplemented`, `corpus::Origin`). The
  shipped set runs CWE.1, CE.1 and CF.1 into `Coding.code` without the
  guide's narrative gate, names the CF components and the `Quantity`
  elements OBX-6 fills, keeps HD.2 out of `MessageHeader.source.software`,
  gives the destination one name, lets MSH-24 give the source endpoint beside
  an MSH-3 application name, writes ORC-2, SCH-26, SCH-27 and PID-2 to PID-4
  as `Reference.identifier` where the guide names a resource no map fills,
  fixes the `ADT_A01.PD1` row of the ADT_A05 and ADT_A09 message maps, and
  adds message maps for ADT_A03, BAR_P01, ORL_O22 and OUL_R22. The vendored
  HL7 corpora move from 142 to 153 of 511 and the NIST and AIRA smoke sets
  from 189 to 199 of 379. The HL7 v2 face page lists every supplement with
  its reason.
- The HL7 v2 face in the server (#255). `[hl7v2] enabled = true` starts an
  MLLP listener beside the HTTP server, under the same `SIGTERM` drain, with an
  `hl7v2` line in the boot banner and an `hl7v2-listener` readiness indicator.
  Each message runs through `ferrobridge_hl7v2::inbound` and the v2-to-FHIR
  ConceptMaps into an R4 message Bundle, whose entries go through the facade's
  ingest service as one transaction: the program is selected by `meta.profile`
  as the facade selects it, a configured profile per resource type is claimed
  where the guide wrote none, an entry naming no subject references the
  message's one `Patient`, and every composition's `FEEDER_AUDIT` names the
  message by MSH-10 and message type. The acknowledgment follows the commit:
  `AA` when the CDR holds the compositions or held them from an earlier
  delivery, `AE` with one `ERR` per refused entry for a refusal of the
  content, `AR` when the message cannot be read or the CDR or the terminology
  server failed (HL7 v2.5.1 chapter 2 §2.9.2.2). The section carries the
  listen address, the default MSH-18 character set, the ConceptMap directory
  and its supplements, the unmapped-entry rule, an EHR policy of its own, the
  idle and frame timeouts, the frame ceiling and whether an `AE` or `AR` logs
  the counted outcomes. No message byte reaches a log line; each connection
  and each message has its own span naming the peer, MSH-10 and the message
  type. The Operate book gains the HL7 v2 face page. Until a FHIRconnect
  context maps the guide's resources (#258), a message is answered `AE` for
  lack of a program. `[hl7v2] senders` lists the MSH-4 sending facilities
  accepted, by namespace id (HD.1) or by universal id and type (HD.2, HD.3);
  a message from any other is `AR` with an `ERR` at MSH^1^4 and is never
  mapped, counted as `refused` on the connection's span. `/health/info` gains
  `lanes`, the `hl7v2` lane with its listen address and whether its listener
  is up, and the facade's `CapabilityStatement` names the HL7 v2 face in
  `implementation.description` when it is configured.
- `ferrobridge_hl7v2::mllp::serve_with` takes an idle and a frame timeout: a
  connection idle past the first is closed, and a frame that does not complete
  within the second is answered `AR` as `Malformed::Stalled`.
  `mllp::Handler::handle` takes the connection's `mllp::Connection`, on which
  a handler counts the frames it refused by its own policy; the
  `mllp_connection` span records `messages` and `refused`.
- The ingest service reads a subject reference to another entry of the Bundle
  through that entry, taking the person from its first `identifier`, and a
  Bundle none of whose entries maps is refused naming every skipped entry and
  why. A Bundle whose EHR lookup the CDR failed or refused answers with the
  status the status table gives that answer, as a single create does, instead
  of `422`.

- `hl7v2-types` carries the message structures HL7 withdrew before v2.9.1
  (#303), so `ferrobridge-hl7v2` parses the `ORM^O01` orders legacy senders
  still send. `scripts/vendor/v2-legacy.sh` fetches the NIST IGAMT export of
  HL7's v2 database (versions 2.1 to 2.8.2) at build time, never committed,
  pinned in `docs/VERSIONS.md` by commit, file count and tree digest. The
  generator emits every structure whose code no v2.9.1 structure carries
  under `legacy::<version>`: 218 trees of 57 codes over 11 versions, each
  with its `version` and `withdrawn_as_of` (`ORM_O01`: 2.7), each code
  checked against table 0354 of `hl7.terminology`. The segments those trees
  name come from the same version's tables (495 per-version segments emitted;
  98 references linked to the v2.9.1 segment they agree with), and
  `message::LEGACY` with `message::find_legacy` indexes them by code, event
  and version. `parse::structure_for` falls back to it by MSH-12, taking the
  nearest earlier version the export carries, refuses a version before the
  first tree with `StructureError::NoLegacyVersion`, and a parse against a
  legacy tree is the counted outcome `withdrawn-structure`. A structure's and
  a segment's `url` is now `Option`, a field's data type can be
  `DataTypeRef::Legacy`, and `Optionality` gains `Na`.
- The v2-to-FHIR interpreter reads the condition and assignment forms the
  guide's own rows write (#326). A condition compares a component spelled
  `HD-3` (a name before `-` that is a data type and no segment) as well as
  `HD.3`, so the `hd-endpoint` maps' `IF HD-3 = "ISO"` and `NOT IN` rows
  run. An `assignment` of quoted literals and operands joined by `+`
  (`"urn:oid:"+HD.2`, `MSG.1+"^"+MSG.2+"^"+MSG.3`) is written as the joined
  text. A valued MSH-24 or MSH-25 of type ISO, UUID, DNS or URI now gives the
  guide's `urn:` endpoint. A check that names no operand (`IF NOT VALUED`)
  is counted as `defective-condition`, two parts with no `+` between them
  (`RP.3"/"RP.4`) as `defective-assignment`, and an operand the scope cannot
  read as `unevaluable-assignment`. An `IN` or `NOT IN` with no operand reads
  the row's own source, as `mapping_guidelines.md` §Conditions lists them,
  and so do `VALUED` and `NOT VALUED` (the bridge's own reading), so the
  `hd-endpoint` HD.3 rows write the guide's data-absent-reason endpoint.
  Across the 263 ConceptMaps, 415 conditions parse and 44 are refused (2 for
  a missing operand). Two qualifying data type maps that write one child with
  the same value agree, and only differing values count as
  `datatype-conflict`. The typed `urn:` endpoints come from the guide's rows
  alone: `convert::endpoint` builds only the derived
  `urn:ferrobridge:hl7v2-hd:` form, as a fallback a row of the guide at the
  same element replaces, so an HD in MSH-5 or a facility field (no guide row
  writes a url from it) now gets the derived form.
  `crates/ferrobridge-hl7v2/scripts/guide-forms.sh` lists every form with its
  count. The HL7 v2 corpus passes 134 of 511 messages, up from 111: the 23
  that value MSH-24. The smoke corpora stay at 189 of 379.
- `hl7v2-types` carries the data type and message definitions (#289).
  `data_type` holds the 83 data types of the HL7 v2 definitions (12
  primitive, 71 complex with 448 components), each component with its
  position, name, data type, cardinality, optionality, lengths and table, and
  a segment field now points at its data type's static (`Varies`, which has
  no definition, stays a code). `message` holds the 696 message definitions,
  indexed by message code and trigger event, each with the structure variant
  it names, so `ADT^A04` selects `ADT_A01-B`. Every tree and table type
  derives `PartialEq` and `Eq`. `ferrobridge-hl7v2`'s `parse::structure_for`
  selects the variant of a structure from that index, so `ORU^R01^ORU_R01`
  parses as `ORU_R01-A` with no caller-side table. The generator tolerates
  four more defects of the definitions in the files that carry them: the
  editorial group name in `MDM_T02-A` to `-E`, `CSU_C09` and `SRM_S01`, the
  misspelled `defintion` member of the complex data types, the
  `Message.structure` profile URLs, and the 34 message definitions that name
  no structure; a conformance length without `length` is tolerated in `CP`,
  `ERL`, `MO` and `MOP` beside `MSH`.
- HL7 v2 message corpora for the v2 face, and their corpus test (#292).
  `scripts/vendor/hl7v2-samples.sh` vendors three sets verbatim under
  `crates/ferrobridge-hl7v2/vendor/`, each with its `PROVENANCE.md` and
  upstream `LICENSE`: the Microsoft FHIR-Converter samples with their expected
  R4 Bundles (MIT, 139 messages), the CDC ReportStream data tests (CC0-1.0, 364
  `.hl7` files), and the HL7 v2-to-FHIR benchmark page, from which the script
  extracts the seven benchmark messages into derived files (Apache-2.0). With
  `--build-time` it fetches the NIST LRI, LOI and syndromic surveillance test
  bundles and the AIRA MQE examples into ignored directories, as
  `scripts/vendor/v2ig.sh` does, since neither repository carries a licence.
  The crate's `include` keeps every vendored file out of the package. The
  corpus test runs every message from an MLLP frame through decoding, parsing,
  the v2-to-FHIR interpreter (with the table maps answered from the guide's own
  maps) to an R4 Bundle, records every counted outcome, and counts each
  difference from an expected Bundle as a corpus gap, a supplement (#256), a
  candidate defect or a value beyond the oracle. Two pass lists and badges
  join the conformance gate: `hl7v2` (115 of 511 after #302) and `hl7v2-smoke` (189 of
  379, the fetched sets). The testkit's `Case` carries per-case outcome counts
  into the result the gate reads, `pin-freshness.sh` reads the seven new
  commit pins against the head of their branches, and the `test` and
  `conformance` jobs restore the build-time sets from a cache keyed on their
  pins.
- The ITS-REST client reads a contribution back, and a transaction survives a
  retry after its binding failed (#270). `ferrobridge-openehr` gains
  `Client::contribution` for `GET /ehr/{ehr_id}/contribution/{contribution_uid}`
  (`contribution_get`), with one outcome per documented status (`200` the
  CONTRIBUTION, `404` not found) over the generated parameters and the RM
  `Contribution`. `create_contribution` reads an empty `201` as
  `Returned::Minimal` whatever the preference, reads the `Identifier` the
  `201` schema also admits, and reports any other `201` body as
  `Error::CommittedBody` naming the committed contribution. The facade's
  transaction records the contribution uid against every keyed entry before
  it binds any of them (a fifth identity table, `source_contribution`), binds
  from the contribution read back when the commit answer lists no versions,
  and answers a re-sent Bundle whose first binding failed by reading that
  contribution back, committing nothing. The testkit carries the empty `201`,
  the `200` read and the CONTRIBUTION body as ITS-REST stub shapes. Every
  `2xx` with an empty body now reads as `Returned::Minimal`, so a composition
  create or update answered without a body is bound from its `ETag` instead of
  failing after the CDR stored it.
- The `ferrobridge-hl7v2` crate, the HL7 v2 face behind the server (#254),
  reserved on crates.io at 0.0.0. `mllp` frames `<SB> message <EB><CR>`
  (MLLP Release 1) on the `tokio-util` codec traits and runs a listener that
  answers each frame and drains on a shutdown signal; a frame with no start
  block, a stray start block, a missing trailer or a size past the ceiling is
  refused, with an `AR` when a header can still be read. `decode` reads the
  character set MSH-18 names (ASCII, ISO 8859 parts 1 to 9 and 15, UTF-8,
  with a per-connection default for an empty MSH-18) and refuses a byte
  outside it, never replacing it. `parse` splits by the MSH-1 and MSH-2
  delimiters, decodes the escape sequences, and places every segment in the
  structure's group tree from `hl7v2-types`, counting a Z-segment, a segment
  out of place and a field past its table, and refusing a missing required
  segment or field. `ack` answers `AA`, `AE` with one `ERR` per refusal, or
  `AR`, echoing MSH-10 in MSA-2, and `inbound` settles those answers for one
  message. `map` runs the 263 ConceptMaps of the v2-to-FHIR guide
  (`hl7.fhir.uv.v2mappings` 1.0.0, read from a directory, with a supplement
  directory that replaces a map by its url): the message map, then the
  segment, data type and table maps, the `Computable-ANTLR` conditions
  (398 of the corpus's rows parse) and the `[n]` and `(Type)` target
  notation, writing FHIR through `fhirconnect::tree` into an R4 message
  `Bundle` with the MessageHeader first and `urn:uuid` full urls. Every table
  value goes through `ConceptMap/$translate`. A narrative or unsupported
  condition, an unresolved target, an unmapped segment, field or component,
  a value a FHIR primitive cannot hold, and a resource that does not decode
  as R4 are each a typed, counted outcome. `tokio-util`, `bytes`,
  `encoding_rs`, `logos` and `chumsky` join the workspace dependencies; all
  five were already in the lock.
- The `hl7v2-types` crate, the generated HL7 v2 tables (#253), under
  Apache-2.0 beside `fhir-types` and reserved on crates.io at 0.0.0.
  `structure` holds every message structure of the HL7 v2 definitions (305,
  for example `ORU_R01-A`) as a static segment-group tree, with each
  segment's and group's position, cardinality and segment status, choice
  groups and the open `Hxx` slots. `segment` holds every segment definition
  (190), the batch envelopes `BHS`, `BTS`, `FHS` and `FTS` included, with
  its 2912 fields: position, name, data type
  code, cardinality, optionality, length, conformance length, standards status
  and the table binding by number and value set URL. A tree node points at the
  segment's own static, and `find` looks either up by definition id. The
  crate is emitted by a new `v2` root set in `tools/fhir-codegen`. It reads
  the fetched v2ig definitions, which have no `package.json`. The differential
  is read as the snapshot, and a definition carrying a snapshot is refused.
  Each defect the definitions carry is tolerated only in the files where it
  was found, so the same defect anywhere else fails the emit. `emit --check`
  covers the new tree, and the CI test and coverage lanes fetch the
  definitions the generator's tests read.
- The HL7 v2 inputs, vendored with provenance (#252). The v2-to-FHIR
  implementation guide package `hl7.fhir.uv.v2mappings` 1.0.0 (FHIR 4.0.1,
  263 ConceptMaps) is the sixth package `scripts/vendor/fhir-packages.sh`
  vendors under `tools/fhir-codegen/vendor/`. It carries the `LICENSE` and the
  mapping guidelines page of its source repository `HL7/v2-to-fhir` at the
  `1.0.0` tag, and its `PROVENANCE.md` records both licences, the package's
  CC0-1.0 and the repository's Apache-2.0. The HL7 v2 definitions
  (`HL7/v2ig` `input/sourceOfTruth`, 1,694 files, pinned by commit) have
  terms that do not permit redistribution, so the new `scripts/vendor/v2ig.sh`
  fetches them at build time into the ignored
  `tools/fhir-codegen/vendor/hl7-v2ig/` and checks the file count and tree
  digest on every run. The one committed file there is the `PROVENANCE.md`
  that quotes the licence text HL7 attaches and records the owner's decision
  to use the material as a generator input. The `codegen-drift` CI job
  restores the tree from a cache keyed on the commit and runs the fetch
  before the drift check. `scripts/checks/versions.sh` reads every FHIR
  package provenance and the new corpus rows, digests included, back against
  `docs/VERSIONS.md`.
- The conformance pass lists and their badges (#24). The corpus tests give
  every case of four corpora a verdict: each file of the FHIRconnect mapping
  library (2 of 107 pass: parse, published schemas or the pinned rejection
  set, strict schemas, a clean load, and a compiled program reaching it),
  each file of the OMOCL library (199 of 208), the two FHIR round-trip chains
  (both lens laws hold on both) and the two FSH operation definitions of the
  draft REST API chapter (both). The verdicts are ratcheted against committed
  lists under `conformance/`, and a listed case that stops passing fails the
  test. `scripts/checks/conformance.sh` compares, `--update` rewrites the
  lists and the shields.io endpoint badges under `conformance/badges/`, and
  `--check` runs as the new `conformance` CI job. The README shows the four
  badges.
- The boot banner and the console (#245). `ferrobridge serve` under the
  `pretty` format prints the FerroBRIDGE wordmark, the version, the pins it
  serves (FHIRconnect, FHIR, OMOCL, OMOP CDM, openEHR ITS-REST, the
  `openehr-*` crates and `fhir-types`, each read from its crate or from
  `Cargo.lock`) and the four lanes with the hosts they reach; under `json` it
  prints none of it. The boot logs one `console` event (the format, the filter
  and whether colour is on), one `build` event (the git commit, the build
  instant, honouring `SOURCE_DATE_EPOCH`, and the `rustc` version) and one
  `lane` event per lane with its hosts and what its mapping set loaded, before
  `listening`. `GET /health/info` answers the same build facts and pins as
  JSON. A carriage return or line feed inside a `pretty` record is written as
  `\r` or `\n`, so a value cannot forge a second line. `FERROBRIDGE_LOG_FORMAT`
  and `RUST_LOG` override `[telemetry] format` and `filter`.
- `etl run` ties each composition to a derived visit (#244): the visit of its
  EHR whose window contains the composition's `context/start_time` (for a
  composition without a context, the first date its mapping resolved), with
  the `context/health_care_facility` name deciding between several windows
  against each visit's source. Every clinical row of a tied composition
  carries `visit_occurrence_id`; a composition inside no window, or inside
  several the facility does not decide, carries none, and the run report
  counts both. The composition query may leave out `versioned_object_uid`,
  which the runner then reads from the `version_uid`, so a CDR that answers
  no `VERSIONED_OBJECT` in `FROM` can be read. The rule is FerroBRIDGE's own
  and is on the OMOP ETL page.
- The OMOP round trip, the v0.0.4 acceptance test (#92): three synthetic
  laboratory compositions in two EHRs, committed to the reference CDR over
  ITS-REST as canonical JSON, read by `etl run` with the published
  `Laboratory_test_result_v1`, `Laboratory_test_analyte_v1` and `Specimen_v1`
  files copied verbatim, and written into a CDM v5.4 PostgreSQL with the
  synthetic vocabulary. The `MEASUREMENT`, `SPECIMEN`, `FACT_RELATIONSHIP`,
  `VISIT_OCCURRENCE` and `OBSERVATION_PERIOD` rows are a reviewed snapshot; an
  unmapped analyte lands as concept 0 and is counted; a second run leaves the
  database identical, and a composition changed in the CDR replaces exactly
  its own row on the next run. The synthetic vocabulary gains the `LOINC`,
  `UCUM` and `SNOMED` stand-in codes the compositions use, the
  `Meas Value Operator` concepts, and synthetic visit and type concepts. The
  OMOP ETL page gives the Data Quality Dashboard steps for a populated
  database. `etl::job::run` is the job the binary calls, so a test drives the
  same path.

- The OMOCL engine (#90). `omocl::resolve::compile` binds a loaded mapping
  set to one template once: every file at every archetype root of the
  template it maps, an `Include` below its including root at its `base_path`
  (one archetype may be included at two base paths, a cycle is refused), a
  `CustomMapping` to its converter, and every path checked against the Web
  Template. A part of a mapping the template carries no node for is listed as
  unbound; a required column none of whose alternatives binds refuses the
  compile. `omocl::engine::run` walks one composition with that program into
  an `omop_cdm::graph::RecordGraph`, reading every value through the
  generated `openehr-rm` types. `alternatives` are tried in order and the
  first present value wins; a required column with no present value, a path
  that matches several nodes where the record does not iterate, an at-code a
  `conceptMap` does not list, a `multiplication` that overflows its exact
  decimal product, a resolved concept outside the domain of its column or
  its `type`, and an ambiguous code each refuse one record with a typed
  refusal naming the record, the column and the instance path, while the
  rest of the composition runs. A `DV_QUANTITY` projects its magnitude, its
  units (resolved in `UCUM`), its normal range and its magnitude status; a
  code with no standard concept writes concept 0 and keeps its source value.
  The graph's report carries every refusal and every element no mapping
  read. `FactRelationshipCustomConverter` links each laboratory analyte's
  `MEASUREMENT` row to the result's `SPECIMEN` row. The vocabulary is a
  `ConceptSource` trait asked once per distinct question in a run, and an
  openEHR terminology id is read as an OHDSI `vocabulary_id` through a
  configurable alias table (`SNOMED-CT` as `SNOMED` by default). A source
  code mapped to several standard concepts writes one row per concept, as the
  CDM conventions ask. `ferrobridge etl run` now runs: it reads the OMOCL
  files of the new `[mappings] omocl` directory at start, compiles them
  against each template the first time a composition of it arrives, maps every
  composition through the engine with the `[etl] type_concept_id`, resolves
  concepts, operators and domain concepts through the `omop-cdm` resolver,
  and prints the run report as text and as JSON. The record key gains a
  discriminator (the mapping, the entry and the branch), which the writer's
  side table keys on too; `graph::Refusal` names its table and column; the
  vocabulary's key, resolution and error types no longer need the `database`
  feature, and the error boxes its large fields. The `rust_decimal` 1.43.0
  crate joins the workspace for the exact product.
- The OMOP concept resolver (#89). `omop_cdm::vocabulary::ConceptResolver`
  looks a source code up in `CONCEPT` by `(vocabulary_id, concept_code)` and
  returns it when it is standard, or every standard concept its `Maps to`
  relationships reach, ordered by `concept_id`. Every row it touches must have
  no `invalid_reason` and validity dates that contain the record date. Two
  valid concepts under one key are a typed error naming the key and the date,
  and a code with no standard concept comes back unmapped with its key, for
  the caller to write as concept 0 and count. Each concept carries its
  `domain_id`. The queries go through `sqlx` 0.9.0 and are checked at compile
  time against the vendored DDL from the committed `.sqlx/` metadata, which
  `scripts/checks/sqlx-offline.sh` regenerates or checks.
  `omop_cdm::database` binds a pool to one CDM schema and applies OHDSI's
  tables, primary keys and indices to it in one transaction; the rendered
  constraints file stays out, because PostgreSQL refuses it (#232). The
  testkit carries a synthetic vocabulary for the ten vocabulary tables, in a
  CSV shape of FerroBRIDGE's own that `COPY` reads. The loader for an Athena
  export is #233, which waits on #88 recording that format from an observed
  export. The `sqlx-offline` CI job fails when the committed query metadata
  no longer matches the queries.
- `omocl::model`, the OMOCL file model (#87): the AST with positions for the
  header, the twelve entry `type` values, column entries with `optional` and
  ordered `alternatives` of exactly one of `path`, `code`, `conceptMap` and
  `multiplication` (whose factors are `path` or `code`), `base_path`, `Include`
  and `CustomMapping`; the JSON
  Schema FerroBRIDGE authors for OMOCL (`crates/omocl/schemas/`); the
  key-to-CDM-column projection table for all ten targets, checked against the
  CDM v5.4 column metadata; and the load-time rules (an unknown key, two keys
  writing one column, an unknown `CustomMapping` converter and an unresolved
  `Include` are refusals). A literal concept id is checked against the domain
  its CDM column takes through a hook the vocabulary resolver plugs into. 199
  of the 208 library files load; the nine refusals are pinned with their
  defect.
- `openehr_mapping_core::schema`, the one projection of a positioned tree into
  JSON and the one JSON Pointer locator that `fhirconnect` and `omocl` both
  validate their files through.
- `omop_cdm::meta::ColumnMeta::fk_domain`, the vocabulary domain the CDM v5.4
  field definitions name for a concept column, emitted by
  `tools/omop-cdm-codegen` from the `fkDomain` cell.
- The OMOP sink (#91). `omop_cdm::graph` is the record graph one composition
  becomes: rows checked against the column metadata as they are built, keyed
  by EHR, versioned composition, archetype root and occurrence, with
  references to a person, a visit or another row for the writer to resolve,
  `FACT_RELATIONSHIP` links, and the per-composition report. A row missing a
  required column, a date among them, is refused naming the column.
  `omop_cdm::writer::CdmWriter` commits one graph per transaction over its own
  `tokio-postgres` connection: ids come from one `integer` sequence per table
  in a bridge schema (`ferrobridge` by default) beside the CDM, a natural-key
  side table keeps each key on its id across runs, a later version deletes
  the rows the earlier one wrote, the rows cross through binary `COPY`, links
  are written in both directions with `relationship_concept_id` 0, and a
  watermark names the version committed. An exhausted sequence is a typed
  error and rolls the composition back. `omop_cdm::derived` rebuilds
  `OBSERVATION_PERIOD` from the first to the last clinical event per person,
  and `CONDITION_ERA` and `DRUG_ERA` with the PostgreSQL form of the scripts
  the CDM publishes (`crates/omop-cdm/sql/`, checked against the digest of
  the vendored page). The PostgreSQL side of `omop-cdm` sits behind a
  `database` feature, on by default; `omocl` takes the crate without it.
- The ETL runner and the `[etl]` configuration (#91): the composition query
  and the visit query are parsed with `openehr-query` 0.0.69 at load and
  refused without their four aliased projections, an `ORDER BY`, or with a
  `LIMIT`; the run pages them through `POST /query/aql`, reads each template
  once, validates each composition, commits it whole, reports a refused one
  and goes on, skips on `--resume` every composition whose watermark names the
  same version, binds `--since` to `$since`, derives visits grouped by EHR and
  source, and prints a run report with rows per table, concept 0
  assignments, zero-relationship links, unmapped fields and refusals.
  `[cdm]` gains `schema`, `bridge_schema` and `person_policy`.
- `ferrobridge cdm init` runs: it applies the CDM DDL and the bridge schema to
  the configured database. `ferrobridge etl run [--resume] [--since TIME]`
  parses and names #90, the OMOCL engine it waits on, and
  `ferrobridge vocab load DIR --schema NAME` parses and names #233.
- `docs/specs/omop-cdm/site/sqlScripts.qmd`, the source of the CDM's SQL
  scripts page, joins the vendored OMOP CDM corpus at the same tag.
- The CDM database connection carries TLS (#237). The concept resolver's pool
  and the CDM writer both honour the `sslmode` of `[cdm] url` as libpq
  documents it, `disable`, `require`, `verify-ca` and `verify-full`, over
  rustls with the aws-lc-rs provider, and check the certificate against the
  PEM CA in the new `[cdm] tls_ca` or `tls_ca_file`, or the webpki root set
  when none is set. A URL without `sslmode` (libpq's default is `prefer`),
  `prefer`, `allow`, an unknown mode and any other `ssl` URL parameter are
  refused when the configuration is read, naming the mode or the parameter,
  and so is a `PGSSLROOTCERT`, `PGSSLCERT` or `PGSSLKEY` in the environment,
  which the pool's client would read and the writer's would not. The
  quickstart's `cdm_url` now ends in `?sslmode=disable`. The `etl` lane's
  startup event carries the `sslmode` in effect.
  `omop_cdm::connection::CdmConnection` settles the TLS once for both clients,
  and `CdmWriter::connect_with` takes it.
- `ferrobridge cdm init` can run again (#238). It reads the schema's tables
  first: a schema holding every CDM v5.4 table is reported as initialised,
  with the `cdm_version` of its `CDM_SOURCE` rows, and nothing is applied; a
  schema holding some of them is refused naming the missing tables; the
  vendored OHDSI DDL is applied unchanged. `omop_cdm::database::init` returns
  `Init::Created` or `Init::AlreadyInitialised`.

### Changed

- `ferrobridge-hl7v2` splits its files over 750 lines into module folders
  (#355), with no change in behaviour: the interpreter run in `map/run/` by
  phase, the parser in `parse/` by phase, and the interpreter tests in
  `tests/it/map/`; the condition grammar tests move to `tests/it/condition.rs`,
  and a caller names the parse phases by their module (`parse::lex::lex`,
  `parse::structure::structure_for`, `parse::grouping::group`).
- `openehr-mapping-core`, `omocl` and `omop-cdm` split their six files over
  750 lines into module folders (#356), with no change in behaviour; the
  public types keep their names and a caller imports each from its new child
  module (`header::metadata::MappingName`, `graph::row::Row`,
  `writer::error::WriteError`).
- The five generator tests that emit a whole crate run only behind
  `FERROBRIDGE_CODEGEN_EMIT=1`, set by the `codegen-drift` job over a release
  build; the `test` job and the SonarQube Cloud coverage run skip them, since
  a full emit of `hl7v2-types` took up to fourteen minutes per test under
  coverage instrumentation and pushed both past their 45-minute timeouts.
  `codegen-drift` runs `emit --check` in release.
- `fhirconnect` splits its twelve files over 750 lines into module folders
  (#352), with no change in behaviour; a caller imports a moved public type
  from its child module (`engine::traverse::error::EngineError`,
  `resolve::program::mapping::Mapping`, `model::ast::keyword::Direction`).
- `ferrobridge-server` splits its seven files over 750 lines into module
  folders (#354), with no change in behaviour; the configuration file tree
  moves to `config::section` (`config::section::Telemetry`), and every other
- `fhir-codegen` splits nine of its files over 750 lines into module folders
  (#353), with no change in behaviour or in the generated crates; every
  public item keeps its path.
- `ferrobridge-hl7v2` parses every message against the tree and segment
  tables of the version its MSH-12 declares, not only the structures v2.9.1
  withdrew (#333). The required fields follow that version's optionality: a
  2.5.1 `DG1` is held to DG1-1, DG1-2 and DG1-6, and a 2.9.1 one to DG1-1,
  DG1-3 and DG1-6. An empty required field no longer refuses the message: it
  is counted as `missing-required-field` naming the field, the segment and
  the version, and the message is mapped and answered `AA`. Only MSH-9,
  MSH-10 and MSH-12, which the answer needs, still refuse; required segments
  and groups refuse as before. A guide row whose innermost group the
  version's tree omits (`ORDER_OBSERVATION.COMMON_ORDER.ORC` against the
  2.5.1 ORU_R01) reaches the segment placed in the parent group, counted as
  `group-path`. The IGAMT 2.7.1 and 2.8 tables' OBX-4 `R`, which every
  other version and v2.9.1 write `C`, is tolerated as an export defect and
  emitted `C`. A message declaring 2.9.1, 2.9, a version
  after 2.8.2, none, or a version whose tables lack the structure is parsed
  against v2.9.1. Each selection is counted as `version-selected` with the
  version used. `hl7v2-types::legacy` now holds every structure of versions
  2.3 to 2.8.2 (1713 of 254 codes). A segment or tree identical to an
  earlier one links to that static, so the crate packages at 1.2 MiB
  compressed. Siblings in a legacy tree are now ordered by row id, which
  fixes groups placed after their segments in the 2.5 to 2.8.2 trees.
- The nine `openehr-*` crates step from 0.0.71 to 0.0.72, whose generated
  ITS-REST client carries what the bridge's CDR client added around it
  (#293, FerroEHR #3487). The composition commit headers travel through the
  generated parameters as `openehr-version`, `openehr-audit-details` and
  `openehr-template-id`; a refusal's `issue.diagnostics` reads the error body
  the generated outcome or `ClientError` carries, so a `400` in the prose
  error shape is the documented `400`; a version uid travels with its `:`
  literal in the path segment; and `[cdr]` credentials are the
  `openehr-its` `Credentials`, built with `Credentials::basic` and
  `Credentials::bearer` over a `SecretString`, in place of the bridge's own
  copy of that type.
- The nine `openehr-*` crates step from 0.0.69 to 0.0.71, and `openehr-its`
  is taken with the new `rest` feature in place of `rest-server`, so the
  ITS-REST DTOs come without axum. 0.0.71 also ships the `rest-client`
  feature (FerroEHR #3485), which #285 adopts in place of the hand-written
  client.
- The `omop-cdm` record graph keys its rows by the `openehr-base` BASE 1.3
  identifiers (#279): the EHR and the versioned composition are a
  `HierObjectId` and the version an `ObjectVersionId`, so the ETL runner
  hands the ids it parsed from the AQL row straight to the CDM writer. The
  graph's own `EhrId`, `VersionedObjectUid` and `VersionUid` are gone.
  `Source::new` takes the EHR and the version and reads the versioned
  composition from the version's `object_id` as written, `RecordKey::new`
  takes the `Source` it belongs to, and an `ehr_id` a composition or visit
  row carries must now parse as a BASE 1.3 `HIER_OBJECT_ID`. A watermark
  whose stored version is no `OBJECT_VERSION_ID` is the new
  `WriteError::Version`. The side tables store the same strings as before.
- `ferrobridge-openehr` is the ITS-REST client transport only, and takes the
  openEHR model from the published crates (#276). The version, version
  container, template and `uid_based_id` identifiers are the `openehr-base`
  BASE 1.3 `ObjectVersionId`, `HierObjectId`, `TemplateId` and `UidBasedId`
  the `openehr-its` DTOs use, so an `ETag` or a stored id now parses against
  the BASE 1.3 `uid` grammar. `CommitContext` carries the lifecycle state as a
  `DvCodedText` and the audit as the ITS-REST `UpdateAuditData`, and renders
  the `openehr-version`, `openehr-audit-details` and `openehr-template-id`
  headers byte for byte as before; the facade builds that audit once for a
  single write and a transaction entry. The ADL 2 template fetch answers the
  `openehr-am` `ArchetypeHrid`. The bridge's own `ObjectVersionId`,
  `VersionedObjectUid`, `TemplateId`, `ArchetypeHrid`, `LifecycleState`,
  `ChangeType`, `Committer` and `CommitterRef` are gone; `EhrId`,
  `ContributionUid`, `SubjectId`, `SubjectNamespace` and `RequestId` stay.
- The facade's write pipeline is one transport-neutral service,
  `facade::ingest` (#261). It maps a resource through its FHIRconnect program
  with the origin on the engine defaults, resolves or creates the EHR by
  subject, strict-reads the built composition, commits over ITS-REST and
  records the identity binding and the consumed source version, taking plain
  values and answering a typed result. `ingest_bundle` answers one outcome per
  entry (committed, or skipped with a typed reason) under an explicit
  `UnmappedEntries` rule: `Refuse`, which the transaction route passes, or
  `SkipAndCount`, which skips an entry no program maps and commits the rest.
  `Provenance::Item` names one originating item for every composition, and
  `SourceItem::message` builds one from a message control id and type. The
  create, update and transaction handlers keep the HTTP half and answer
  exactly as before.
- `ferrobridge cdm init --with-constraints` applies OHDSI's
  `OMOPCDM_postgresql_5.4_constraints.sql` after the tables, one statement at
  a time in a transaction of its own, and names the file line and the text of
  a statement PostgreSQL refuses (#232). Without the flag the foreign keys are
  left out, as before. At the pinned OHDSI tag `v5.4.3` PostgreSQL refuses
  line 157, a foreign key onto `vocabulary (vocabulary_id)` that the primary
  keys file gives no key (SQLSTATE `42830`), so the flag fails the run and
  leaves the tables in place.
- The vendored OMOCL corpus moves to `SevKohler/OMOCL` commit `c082db8e`
  (#228): six new mapping files and the README acknowledgements, 208 mapping
  files in all.
- The server calls the CDR through the generated ITS-REST 1.1.0 client of
  `openehr-its` (`rest-client`, #285). The `[cdr]` keys keep their names and
  their meaning: `timeout_ms` is the per-request timeout of the `reqwest`
  engine, `retry.max_attempts`, `retry.initial_backoff_ms` and
  `retry.max_backoff_ms` are the client's retry budget over idempotent calls,
  and `credentials` (with its `_file` forms) are the Basic or Bearer
  credentials sent on every call. Two answers read differently: a `501` is no
  longer retried, since a repeat cannot change it, and a `204` to a create
  under `return=minimal` is a committed write instead of an undocumented
  status. The facade answers every CDR status with the same FHIR status and
  `OperationOutcome` as before.

### Removed

- `crates/ferrobridge-openehr`, the hand-written ITS-REST client (#285). Its
  identifiers, the commit-header rendering, the template fetch and the AQL
  paging move into the server's `cdr` module; its transport, outcome enums and
  retry are the generated client's. The 0.0.0 name was never published.

### Fixed

- The HL7 v2 parser no longer opens a segment group the following segments
  leave with a required member empty when a later node takes the segment
  (#343). A 2.9.1 OUL^R22 of `SPM, OBR, ORC, OBX` placed its OBX in
  `ORDER_DOCUMENT`, whose required TXA never came, and reported nothing; the
  OBX now opens the `RESULT` group, and an OBX followed by its TXA still
  opens `ORDER_DOCUMENT`. A group left with a required member missing is a
  `100` refusal naming the member and the group, located at the group's
  first segment.
- A facade read, vread or `return=representation` write answers a valid R4
  instance (#350). A `Condition` mapped through a context with no outbound
  subject row read back without `subject`, which R4 makes `1..1`, so the read
  body could not be sent back as an update. The facade now writes `subject`
  (or `patient`, where the type names it so) from the person the identity map
  recorded for the composition's EHR whenever the rendered resource lacks it,
  in the form a create reads back into the same EHR, and logs the element
  paths it filled. It then checks every `min 1` element of the `fhir-types`
  element table, nested and contained ones included, and answers
  `500 exception` naming the element when one stays absent. The identity map
  gains a seventh table from each `ehr_id` to its person, and a store an
  earlier version wrote gains its rows when it opens.
- A facade update that sends back the `ETag` a read answered, `If-Match:
  W/"1"`, commits the next version and answers `200` with `_history/2` and the
  new `ETag` (#347), as R4 concurrency prescribes. Before, every such `PUT`
  answered `412`, and only the CDR's own `uid::system::N` form succeeded. The
  facade completes `W/"N"`, `"N"` and a bare `N` to the current version of the
  bound composition, at the cost of one extra CDR read per versioned update,
  answers `412` with the current `ETag` when `N` is stale,
  and `400 invalid` when the value names no version. The CDR's own form still
  passes through, and one naming another composition is `412`.
- A transaction Bundle that carries one resource `id` at two `meta.versionId`s
  is refused with `400 invalid` naming both entries and commits nothing
  (#306), as R4 allows a resource in a transaction once by identity. Before,
  the two entries passed the duplicate check and an unknown `id` produced two
  compositions. An entry that repeats another's version as well keeps its
  `422 duplicate`.
- A FHIR path that names a choice alternative by its concrete key, such as
  `Observation.effectiveDateTime`, `valueQuantity` or an extension's
  `valueCodeableConcept`, resolves to the one type that alternative carries,
  so `Resolved::type_code` answers `dateTime` where it answered nothing
  (#290). The v2-to-FHIR interpreter drops its own reading of the suffix and
  asks the resolver, and a row whose target has no single type (a choice
  element no alternative names) is counted as `untyped-target` naming the
  element, where it went on under an empty type name.
- `scripts/checks/conformance.sh` stops with "cargo not found on PATH" and
  exit 1 before it runs anything, where a missing cargo read as every corpus
  test failing.
- A patient location (PL) gives one Location per level it values, linked by
  `partOf`, and the Encounter references the finest level: the bed, else the
  room, else the point of care (#342). The guide's `datatype-pl-to-location`
  writes the levels through `[1].` to `[6].` rows, which the interpreter
  counted as `unplaced-instance` (1538 outcomes over the vendored corpus), so
  only the bed was written. A `[k].` row of a data type map whose rows
  reference a labelled instance of the resource type they fill now writes the
  `k`-th sibling of the resource the `(Type)` row created, on first sight; a
  reference between siblings to a level no value reaches climbs to the level
  that one's own row names, and one that reaches no valued level is counted
  as `sibling-unresolved`; the references to the family take the finest
  level, counted as `finest-sibling`, or stay put as `sibling-ambiguous`
  when the chain leaves two. A supplement to the PL map (#332) links the
  levels in the order its PL.10 rows give (bed, room, point of care, floor,
  building, facility), where the guide's `partOf` rows disagree and the
  building names itself; writes the point of care's `mode` and
  `physicalType` at elements Location has; fixes the PL.10 labels and
  conditions; and writes PL.9 into the finest valued level. The guide's
  MDM^T02 sample now maps to the four Locations of its expected Bundle.
  The pass counts do not move (153 of 511 vendored, 199 of 379 smoke); over
  the vendored passes the expected-Bundle comparison counts 63 fewer
  corpus gaps (571 to 508: every `Location.partOf`, `identifier`, `mode` and
  `physicalType` difference) and 15 fewer count differences (166 to 151).
- A segment whose group path no row of the guide's message map names takes
  the rows of the one source whose groups differ from its own by a single
  group, other than the group holding the segment (#334). The guide writes
  `ORM_O01.ORDER_DETAIL.CHOICE.OBR` where the 2.3 tree places OBR at
  `ORM_O01.ORDER.ORDER_DETAIL.CHOICE.OBR`, so the OBR, RXO, NTE, DG1 and OBX
  of a legacy order now reach their rows, each counted as `group-path`
  naming the row path and the tree path. A row path that reaches two tree
  paths, or a tree path two row paths reach, stays `unmapped-segment`. Over
  the corpora 39 segments take this path; the pass counts do not move (142
  of 511 vendored, 189 of 379 smoke).
- A group of a row's path that names no tree group at its position pairs
  with the one tree group there whose segments hold the row's segment and
  every segment the guide's rows place in the row's group (#339). The guide
  writes `ORM_O01.PATIENT.VISIT.PV1` where the 2.3 tree has
  `ORM_O01.PATIENT.PATIENT_VISIT.PV1`, so the PV1 and PV2 of a legacy order
  now reach their rows, counted as `group-renamed` naming both group names.
  A pairing composes with the one-group tolerance of #334, and among the
  pairings the one with the fewest renamed groups counts; two tree groups
  that qualify leave the segment `unmapped-segment`. The v2.9.1 MDM_T02
  definitions name the observation group `FIXME`, so the OBX of a document
  notification now reaches the guide's `MDM_T02.OBSERVATION.OBX` rows. Over
  the corpora 60 segments take this path; the pass counts do not move (142
  of 511 vendored, 189 of 379 smoke).
- A legacy field whose version names its type by a version-specific code
  the guide has no data type map for resolves to the base type before the
  map lookup (#335): `CM_MSG` to `MSG` and `CE_0051` to `CE`, counted as
  `base-typed` naming both codes. A code the guide maps keeps the guide's
  map: `TS` stays on `datatype-ts-to-datetime`. `hl7v2-types` carries the
  link: each legacy version module gains a `data_type` module with one
  `LegacyDataType` per code its segments' fields name, whose base is `MSG`
  for `CM_MSG`, `CE` bound to table 0051 for `CE_0051` and `DTM` for `TS`,
  and `DataTypeRef::Legacy` points at it. A code that stands for no defined
  type keeps its code and stays `no-datatype-map` when no map names it. The
  pass counts do not move.
- The two `datatype-eip-<half>-to-identifier` maps each fill an `Identifier`
  of their own (#336). The guide maps SPM-2 into `Specimen.identifier[1]`
  and `identifier[2]` and ORC-4 into `DiagnosticReport.identifier[3]` and
  `identifier[4]`, and the placer map reads EIP.1 where the filler map reads
  EIP.2, so the rows in map order take the halves in component order: the
  first row the placer `Identifier` (`PGN`), the second the filler's
  (`FGN`), each counted as `datatype-half` naming the map chosen. A value
  valuing a component neither half reads (an EI written into ORC-4) stays a
  `datatype-conflict`. A `(Type)` row whose one step inside the referenced
  resource is a `Reference` no data type map fills runs the data type map
  from its source into that resource when every row of that map writes
  through that element, counted as `reference-root`: OBX-25 into
  `performer(PractitionerRole.practitioner)` and ORC-12 into
  `requester(PractitionerRole.practitioner)` run
  `datatype-xcn-to-practitionerrole`, which gives the `PractitionerRole` its
  `Practitioner`. A primitive a data type map writes at `$value` of a complex
  element with a primitive `value` child lands there, counted as
  `value-child`: TXA-16 through `datatype-st-to-identifier` gives
  `DocumentReference.identifier.value`. Over the corpora, `datatype-conflict`
  falls from 1226 to 155 and `no-datatype-map` from 1525 to 389; the pass
  counts do not move (142 of 511 vendored, 189 of 379 smoke), since no
  outcome decides a verdict.
- A field of a legacy v2 message takes its data type from the segment
  definition of the version the parser selected (#329). Where that type
  differs from the type the guide's row names, the version's type chooses
  the data type map and the run counts a `version-typed` outcome naming the
  field, both types and the version: an `ORM^O01` at 2.3 maps OBR-4 by `CE`
  (`datatype-ce-to-codeableconcept`) where the row names `CWE`. A code
  v2.9.1 dropped resolves by code through the same map lookup, and a message
  of a structure v2.9.1 still carries keeps the row's type. The corpus pass
  counts are unchanged: 112 of 511 vendored messages and 189 of 379 fetched
  ones.
- A v2-to-FHIR row whose data type maps are alternatives for one value runs
  one of them (#327). Two maps are alternatives when both map one component,
  with no condition, into the same child or into the target itself, as the
  four `datatype-ei-<qualifier>-to-identifier` maps do with EI.1. The map
  whose rows write from the most components of the value is chosen, and
  among equally specific maps that write the same, the one with the fewest
  rows. ORC-2, ORC-3 and TXA-12 each give one `Identifier`: a bare value
  from `datatype-ei-defaultassigner-to-identifier`, a value with an
  assigning authority from `datatype-ei-organization-to-identifier`, with
  its assigner. The `[System]` variant, whose EI.1 row writes `$value` and
  whose system rows are narrative, is no longer run, so the
  `no-datatype-map` it counted through `datatype-st-to-identifier` is gone.
  Equally specific maps that write differently count `datatype-ambiguous`
  naming them, and none writes. A row whose `mappedVia` names a data type
  map runs that map alone (`StructureDefinition-TypeInfo`: "Url of the
  mapping artifact for the item"). The pass counts do not move (134 of 511
  vendored, 189 of 379 smoke): no EI outcome decides a verdict there.
- A v2-to-FHIR row into a complex element runs every data type map the
  guide names for it (#324). The maps are found by the element's type, else
  by its element path, the form the guide's map titles use for a backbone
  element (`HD[endpoint]` and `HD[name]` into `MessageHeader.source`), so
  MSH-3 and MSH-24 into `source` and MSH-25 into `destination` run both HD
  maps where they counted `no-datatype-map`. A child that another row of the
  same field targets directly stays that row's, so MSH-3's own
  `source[1].endpoint` row keeps the endpoint. When two maps write the same
  child for one value, as the four EI maps into `Identifier` do with EI.1,
  the row writes nothing and counts `datatype-conflict` naming the child and
  the maps. The pass counts do not move (111 of 511 vendored, 189 of 379
  smoke): the guide's `datatype-hd-endpoint-to-messageheader-source` gates
  every endpoint row on a condition outside the guide's grammar (`HD-3`, `=`,
  a test with no operand) and assigns a concatenation, so its endpoint is
  never written and the 23 messages valuing MSH-24 still lack
  `MessageHeader.source.endpoint`.
- A required FHIR primitive carried only by its `_name` sibling with an
  extension counts as present, as FHIR R4 JSON represents a primitive with
  extensions and no value (<https://hl7.org/fhir/R4/json.html#primitive>)
  (#321). The `data-absent-reason` the guide's MSH-24 row writes into
  `_endpoint` now completes `MessageHeader.source`, so a v2 message valuing
  none of MSH-3, MSH-24 and MSH-4 is mapped with that endpoint instead of
  answered `AR`; a `_name` with an empty extension list is still missing.
  Where MSH-4 is valued its endpoint still replaces the extension. A message
  whose `MessageHeader.source` no row completes, such as one valuing MSH-24,
  whose row into `source` has no single data type map, is still refused as
  `MapError::NoMessageHeader`. The smoke corpus passes 189 of 379 messages
  (187 before, the 2 AIRA messages that name no sender); the vendored corpora
  stay at 111 of 511.
- The v2-to-FHIR interpreter emits only Bundles that decode as FHIR R4
  (#302). A value outside the lexical form of its target primitive, such as
  an application name with spaces written into the `url`
  `MessageHeader.source.endpoint`, is never written and counts as
  `invalid-value` naming the element and the type
  (<https://hl7.org/fhir/R4/datatypes.html#primitive>). An element lacking an
  element its definition requires (`MessageHeader.destination` without
  `endpoint`, an `extension` without `url`, `Provenance.agent` without `who`)
  is left out, and so is a resource lacking one (`MessageHeader` without
  `source`), each counted as `missing-required` with the definition path it
  lacks. A write taking a second alternative of a choice such as
  `Extension.value[x]` counts as `superseded`. A resource that still does not
  decode is left out of the Bundle as `undecodable`. The vendored corpus now
  passes 115 of 511 messages (8 before) and the build-time smoke corpus 189
  of 379 (26 before), and the corpus test asserts that no Bundle fails to
  decode and no resource is left out as `undecodable`.
- Every `message` Bundle the v2-to-FHIR interpreter emits opens with its
  `MessageHeader`, as FHIR R4 invariant `bdl-12` requires
  (<https://hl7.org/fhir/R4/bundle.html#invs>) (#311). A v2 `HD` written into
  a `url` element (MSH-3 into `MessageHeader.source.endpoint`, MSH-5 into
  `MessageHeader.destination.endpoint`) becomes the guide's `urn:oid:`,
  `urn:uuid:`, `urn:dns:` or `urn:uri:` form of its universal ID when HD.3
  names one of those types, and otherwise the derived
  `urn:ferrobridge:hl7v2-hd:` endpoint with each component percent-encoded;
  the namespace ID is kept in the `name` beside it. A message that names its
  sender in neither MSH-3 nor MSH-24 is answered `AR` with an `ERR` at MSH-3,
  and a run that still completes no `MessageHeader` is refused as
  `MapError::NoMessageHeader` naming the element it lacks. The corpus test
  asserts `bdl-12` over every Bundle it maps. The vendored corpus passes 107
  of 511 messages (115 before) and the smoke corpus 162 of 379 (189 before):
  the cases that left are header-less Bundles now refused, 8 that value
  MSH-24 (whose row into `MessageHeader.source` has no single data type map)
  and 27 that value neither MSH-3 nor MSH-24.
- A v2 message that values neither MSH-3 nor MSH-24 but names its sending
  facility in MSH-4 is mapped instead of answered `AR` (#315). The
  `MessageHeader.source.endpoint` and `source.name` come from the MSH-4 `HD`
  by the endpoint rule above, the guide's `sender` Organization from MSH-4
  stays, and the fallback is counted as a `facility-endpoint` outcome naming
  the field and the element. MSH-6 fills `destination.endpoint` the same way
  when MSH-5 and MSH-25 are empty. Only a message valuing none of MSH-3,
  MSH-24 and MSH-4 is answered `AR`. The smoke corpus passes 187 of 379
  messages (162 before); the 2 AIRA messages that name no sender at all stay
  refused.
- A v2-to-FHIR row whose own source field is empty runs when it maps that
  absence: it assigns a literal and its condition requires the field
  `NOT VALUED` (#318). The MSH map's MSH-24 and MSH-25 rows
  (`IF MSH-24 NOT VALUED AND MSH-3 NOT VALUED` and the destination pair)
  now write their data-absent-reason extension on the endpoint, and the
  data type rows of the same form (`XTN.3` when `XTN.4` is valued) run too.
  A row on an empty field whose condition only tests another field
  (`PID-13` with `IF PID-13.2 IS NOT VALUED`) still writes nothing. Where
  MSH-4 or MSH-6 gives the endpoint, the facility's value replaces the
  data-absent-reason, since the row's comment leaves the implementer the
  choice between a known value and the extension. The vendored corpora pass
  111 of 511 messages (107 before, the four MDM T06 and T10 samples); the
  smoke corpus stays at 187 of 379.
- A v2 message whose MSH-9.3 names a structure the definitions lack
  (`ADT^A08^ADT_A08`, `SIU^S13^SIU_S13`) is grouped by the structure the
  message definition of MSH-9.1 and MSH-9.2 names, as HL7 v2.5.1 chapter 2
  §2.15.9.9 derives it through table 0354, and counted as `other-structure`
  naming both (#319). An MSH-9.3 naming a known structure the definition
  contradicts stays refused, and so does one with no definition to fall back
  on (`ORM^O01^ORM_O01`, 10 vendored messages).
- A v2 message whose header carries no MSH-9.3 (the v2.3 senders in the
  vendored ReportStream set) resolves its structure from MSH-9.1 and MSH-9.2
  through the generated message index instead of being refused as unnamed
  (HL7 v2.5.1 chapter 2 §2.15.9.9, table 0354).
- A transaction entry with an `id` the identity map knows and a new
  `meta.versionId` commits a later version of that resource's composition
  inside the Bundle's contribution, as a single create of it does (#297).
  Before this fix it created a second composition under a second container.
  The entry goes in as an `UpdateVersion` whose `preceding_version_uid` is the
  composition's latest version, with a `modification` audit, binds to the
  resource id it already has, and answers `200 OK` in the
  `transaction-response` with a `Location` naming the new version. The
  commit-before-bind record, the `FEEDER_AUDIT` pairing and the retry after a
  failed binding hold for such an entry, and a Bundle entry now also claims
  its `resourceType` and `id` alone, as a single create does.
- A conditional create's `If-None-Exist: identifier=…` finds a resource the
  facade committed (#299). The search read the external-id table under the
  identifier's value, a key the ingest never wrote, so a conditional create
  of an existing resource committed a duplicate. Every committed resource now
  records each `Resource.identifier` it carries against its logical id (a
  sixth identity table, `identifier_internal`), and the search reads it in the
  R4 token forms `[code]`, `|[code]` and `[system]|[code]`, percent-decoded;
  the `[system]|` form answers `400 not-supported`. One match answers
  `200 OK` and commits nothing, several answer `412`.
- A single create re-sent with the `id` and `meta.versionId` the identity map
  already consumed commits nothing and answers `200 OK` with the `Location`,
  `ETag` and body of the composition the first delivery produced, as a
  re-sent transaction entry is answered (#288). Before this fix it committed a
  new version of that composition on every delivery. A create of a known `id`
  at another `meta.versionId` commits a later version of the same composition;
  before, it committed a second composition. A create of a known `id` with no
  `meta.versionId` is a replay when it maps to the content the composition
  holds now (the `uid` and the clock-filled times masked, as the transaction
  path masks them) and a later version otherwise. An `id` the map binds to two
  compositions is refused with `409 Conflict`. A single create also claims its
  `resourceType` and `id`, so two creates of one `id` at different versions
  cannot run at once. `PUT` and the transaction path are unchanged.
- Two overlapping deliveries of one transaction Bundle, or two overlapping
  creates of one resource, commit once (#273). Each delivery claims the source
  key of every entry before it reads the identity map and releases the claims
  when it answers, on every exit path including a panic. A delivery that finds
  a key claimed commits nothing and answers `409 Conflict` with a `duplicate`
  issue per entry; retried after the first delivery answers, it is recognised
  as a re-sent Bundle or resource. Before this fix both deliveries could pass
  the lookup and commit, leaving a composition no identity record named.
- A re-sent Bundle whose entries share one `FEEDER_AUDIT` item, as the entries
  of one message do, binds on retry after its first binding failed (#274).
  The content comparison that tells such entries apart now masks every
  `DV_DATE_TIME` the engine filled from its clock (the `ctx/time` default that
  reaches `EVENT_CONTEXT.start_time`, `HISTORY.origin`, `EVENT.time` and
  `ACTION.time`), where a retry mapped at a later instant failed the
  comparison with a `500` on every attempt.
- A single create of a resource that a transaction committed and did not
  finish binding binds from the contribution read back and answers `200 OK`
  with the composition the first delivery produced (#275), where it committed
  a second composition. The single and the transaction paths share the
  read-back and the `FEEDER_AUDIT` match; the single path passes over the
  versions of the transaction's other entries.
- A value a FHIR resource maps into openEHR keeps every attribute the
  reference model gives it (#241). The engine writes each data value whole as
  its canonical JSON under the FLAT `|raw` suffix (ITS-REST 1.1.0 Simplified
  Formats, master04 §Raw canonical JSON) instead of through per-class suffix
  tables, which dropped `hyperlink`, `language` and `encoding` on a
  `DV_TEXT` or `DV_CODED_TEXT` and `normal_status`, `normal_range` and
  `accuracy` on a `DV_DATE_TIME` with no error. A tail below a node is now
  admitted when the `openehr-rm` attribute model defines it, and refused at
  load (`fc-uncarried-tail`) when it runs through a list or sits below a
  structural node other than `ENTRY.provider`; a `manual` path must end on a
  string attribute. The composition defaults travel as the `ctx/` keys of
  master06, and a default setting whose code and value name two concepts of
  the openEHR `setting` group is refused. A value at `COMPOSITION.composer`,
  `language` or `territory` that the `ctx/` keys cannot carry whole is
  refused instead of losing its identifiers or its terminology. The `LINK`
  and `PARTICIPATION` lists a composition carries are read as typed values,
  and a defective one is a refusal instead of an empty list. An EHR the
  facade creates stamps `EHR_STATUS.archetype_details.rm_version` `1.2.0`, the
  release its types come from, instead of `1.1.0`.
- A transaction Bundle sent twice commits its compositions once (#264). The
  transaction path records each entry's identity binding and consumed source
  the way a single create does, asking the CDR for the committed CONTRIBUTION
  (`Prefer: return=representation`) to learn each entry's version and reading
  each version back to match it to its entry by the `FEEDER_AUDIT` the engine
  wrote, since ITS-REST states no order for `CONTRIBUTION.versions`; a
  version that matches no entry, or more than one, is a `500` naming the
  contribution and binds nothing. A committed
  transaction answers a `transaction-response` Bundle whose entries carry
  `201 Created`, the `location` `[base]/[type]/[id]/_history/[vid]` and the
  `ETag` a create answers (R4 §3.1.0.11.3), where it answered an
  `OperationOutcome`. A Bundle whose every entry an earlier delivery consumed
  commits nothing and answers `200 OK` per entry with the same locations; one
  only some of whose entries were consumed is refused with `409` naming them,
  and one carrying a resource twice is refused with `422`. The ingest service
  keys a message's entries on its control id and entry position, so a
  redelivered message commits once.

## [0.0.3] - 2026-09-25

The FHIR round-trip release. The `fhirconnect` crate runs every FHIRconnect
method over the generated FHIR model and the openEHR Web Template; the `$tofhir`
and `$toopenehr` operations and the FHIR R4 facade over a CDR are served by the
one binary; and the published KDS Diagnose chain round-trips under both lens
laws, through the operations and against a real CDR, modulo a declared set.
`v0.0.3-rc.1`, cut the same day, rehearsed the release lane with this content
and is superseded by this release.

### Added

- `scripts/vendor/its-rest.sh` also vendors the Simplified Formats and
  Simplified Data Template sources (`docs/simplified_formats/`,
  `docs/simplified_data_template/`) at the pinned ITS-REST commit, so the FLAT
  families the engine writes cite a vendored `master05-rm_mapping.adoc`.
- The KDS diagnosis round trip (#86). `scripts/vendor/kds-diagnose-opt.sh`
  vendors the published `KDS_Diagnose` operational template verbatim, pinned
  by commit, path and sha256 in `docs/VERSIONS.md`, with its provenance and
  the repository licence beside it. A FerroBRIDGE project context compiles
  the published `EVALUATION.problem_diagnosis.v1` chain of the FHIRconnect
  mapping library against it; every published file the chain reaches loads
  verbatim or has its refusal asserted by file and line, and the project
  directory carries a stand-in for each one that does not. The two lens laws
  are asserted as reviewed snapshots of the declared set, with a corrupted
  intermediate that breaks each, over the synthetic chain and over the KDS
  chain. Both laws hold on the KDS Diagnose chain in the engine, over the two
  FHIRconnect operations and through the facade against the reference CDR,
  each modulo its declared set. The end-to-end lane commits a synthetic KDS
  composition to the reference CDR and reads it back. The server logs every
  warning a compiled program carries once at load, naming the context, the
  file, the line and the code.
- Context resolution refuses two sibling mapping methods of one file that
  share a name (`fc-duplicate-method-name`, naming both positions), because an
  `overwrite` or an `appendTo` of that name has no single target, and warns
  when a listed extension extends a model the program never reaches
  (`fc-unreached-extension`). A compiled program carries its warnings.
- The carry-over wire cases on the facade (#115): `$validate` names the EHR
  the composition would be written into as well as its template, carries the
  validator's message verbatim, writes nothing either way, and refuses at the
  operation level as create does; a refused commit stores nothing; the three
  absences differ in `issue.code` and `diagnostics`; both JSON media types are
  read on every write and anything else is `415`; the feeder audit carries
  the source version and records an absent id as unknown; and an EHR created
  on first sight names its subject as a `PARTY_SELF` over a `PARTY_REF` whose
  `GENERIC_ID` scheme is the namespace, while the default policy creates none.

- The FHIRconnect engine records where a composition came from in its
  `FEEDER_AUDIT` (#187)
  (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
  `Defaults::with_origin` names the system and the source resource, which
  travel as `originating_system_audit` and `originating_system_item_ids`, and
  every composition field the engine defaulted travels as one
  `feeder_system_item_ids` entry, in the order of the `Warning::Defaulted`
  entries. The facade and `$toopenehr` both set the origin, so the facade no
  longer patches the audit onto the built composition.
- The FHIRconnect engine reads and writes a mapping's reference-model tail
  below the deepest template node, in both directions (#189). The resolver
  records the class of the tail's last attribute, the tail's class selects the
  data-type cell, and a tail no FLAT part of the node's class carries is
  refused with `EngineError::UnsupportedTail` instead of being dropped on the
  way to the wire.
- The FHIRconnect engine runs `reference`, `link`, `participationsFunction`
  and `hierarchy.split` (#186). What a run calls out to is `Seams`: the
  `mappingCode` registry, a `ReferenceSource` for referenced resources and an
  `IdentitySink` for the resources a run creates, whose default derives a
  stable id by digest. A reference that resolves to nothing is a declared
  skip and a reference cycle refuses. A `link` writes an openEHR `LINK` whose
  target must be an `ehr:` URI, a participation writes one
  `other_participations` entry, and a split creates one resource or one
  archetype instance per occurrence and per distinct `unique` tuple. Created
  resources travel on `Outcome::created`: `$tofhir` answers them as Bundle
  entries covered by the one `Provenance`, and the facade carries them as
  contained resources.
- The FHIRconnect operations compile against the templates the CDR serves when
  `[cdr]` is configured and `[mappings] templates` is not (#195): boot reads the
  context files, fetches each template they name at
  `GET /definition/template/adl1.4/{template_id}` (openEHR ITS-REST 1.1.0
  §Definition), and refuses the start with the template id and the upstream
  status when the CDR does not serve one. A `[mappings] templates` directory
  still wins when both are present. A `[mappings] directory` with neither
  source is refused when the configuration is read, before any upstream call,
  while `[operations] enabled` is true. A mapping set that does not load stops
  the start with "the mapping set could not be loaded" followed by the cause.
- `fhir-types` 0.1.106 carries the `isSummary` and `isModifier` flags of every
  element on `schema::FieldSchema` as `is_summary` and `is_modifier` (#213),
  read from the pinned `StructureDefinition` of each version
  (<https://hl7.org/fhir/R4/elementdefinition.html>). A server answering
  `_summary=true` (<https://hl7.org/fhir/R4/search.html#summary>) reads the
  summary set from the element table instead of keeping its own copy. The
  element-table test compares both flags with the package JSON for every
  emitted element and pins `Patient.active`, `Patient.name` and
  `Patient.photo` per version.
- `fhir-types` 0.1.105 refuses a primitive value outside its lexical form on
  decode (#204). Every primitive whose JSON form is a string carries the
  `regex` extension its own package puts on the `value` element
  (<https://hl7.org/fhir/R5/datatypes.html#primitive>), compiled once behind a
  `LazyLock` in the generated module and matched against the whole value as the
  XML Schema pattern it is (<https://www.w3.org/TR/xmlschema-2/#regexs>), so
  `"date": "yesterday"`, a `code` with a leading space, a `uri` holding a space
  and an id outside `[A-Za-z0-9\-\.]{1,64}` are a `DecodeError` with
  `BadValue` and the element path. The form is read per version and never
  copied between them: 4.0.1, 4.3.0 and 6.0.0-ballot5 require the offset with a
  time and 5.0.0 does not, and 4.0.1 types `CodeSystem.id` as `string` where
  4.3.0 and 5.0.0 type it as `id`. A primitive carried as a JSON number or a
  boolean keeps the number checks it already had.
- `fhirconnect::operations` and the `$tofhir` and `$toopenehr` HTTP surface
  (#114). The two operations the draft FHIRconnect REST API chapter defines
  (specification pull request #93, pinned by commit) are served by
  `ferrobridge serve`: `POST /fhir/$tofhir` takes a `Parameters` whose
  `composition` is the openEHR composition as a JSON string, canonical or FLAT
  with `templateId`, plus an optional `context` group of `ehr_id`, `patient`,
  `who` and `onBehalfOf`, and answers a `collection` Bundle carrying the mapped
  resources, exactly one `Provenance` and an `OperationOutcome` entry when the
  run declared a loss; `POST /fhir/$toopenehr` takes a Bundle and answers a
  `Parameters` with the composition string and an optional `outcome`. The
  direct form the chapter keeps outside the FHIR implementation guide is served
  too, as `POST /fhir/tofhir` with `Content-Type: application/openehr+json` and
  `POST /fhir/toopenehr` answering in that media type. `templateId`, `format`
  and `ehr_id` travel in the body or the query, with the body taking
  precedence; a FLAT composition with no template is `400 required`; any other
  media type is `415`; every refusal is an `OperationOutcome` with its R4 issue
  code. Strictness is the default, so a failed mapping answers an outcome and
  no Bundle or composition, a Bundle referencing more than one subject is
  refused naming the subjects, and `context.patient` takes precedence over the
  subject a mapping resolved without ever being required. The mapping set is
  compiled once at boot from `[mappings] directory` and the new
  `[mappings] templates` key, and `[operations]` carries the lane switch, the
  `Device` reference the `Provenance` defaults to and the composition defaults
  an inbound run applies.
- The FHIR R4 facade (#85), mounted under `/fhir` when `[facade] enabled` is
  set and answering nothing at all when it is not, so a disabled facade is a
  `404` rather than a `403`. It serves a `CapabilityStatement` built from the
  mapping set the server compiled at boot, creates a resource as a composition
  the CDR commits, reads one back out of its composition, updates one under
  `If-Match`, commits a `transaction` Bundle as one openEHR contribution, and
  answers `$validate` as a dry run that writes nothing. Every response is
  `application/fhir+json`, and everything the facade authors is an
  `OperationOutcome`: an openEHR error body travels verbatim inside
  `issue.diagnostics` rather than reaching the wire as its own document. One
  table maps each CDR answer to a FHIR answer with both sides cited, so a
  template refusal is a `422` carrying its `validationErrors`, a deleted
  composition is a `410`, a `401` keeps its `WWW-Authenticate`, and a CDR
  failure is a `502` naming the upstream status instead of an empty success.
- Identity for the facade (#85), in a `redb` file the lane opens at boot and
  readiness probes. A resource id comes from the entry's `LOCATABLE.uid` when
  it has one and otherwise from a digest over the version container, the entry
  path and the split occurrence, so the same composition reads back under the
  same id while `meta.versionId` moves with each new version. The map wins once
  written, which is what makes a re-sent resource update the composition it
  already produced instead of creating a second one. The store holds
  identifiers and no clinical content, and a test greps the file to prove it.
- Provenance on every inbound commit (#85): the composition carries a
  `FEEDER_AUDIT` naming the source resource's `id` and type, its
  `meta.versionId`, and the configured `system_id`, with an absent id recorded
  as unknown rather than invented.
- `[facade]` and `[mappings]` configuration sections (#85): `enabled`,
  `base_url`, `ehr_policy`, `identity_store`, `subject_namespace`, `system_id`,
  `composition_language` and `composition_territory`, plus the directory the
  mapping files are read from. An enabled facade needs `[cdr]`, a mapping
  directory, and both composition fields, and refuses to start without them.
- `fhirconnect::engine`, the bidirectional interpreter (#84).
  A data-type cell of the specification's chapter is one lens, written once:
  `get` reads an openEHR reference-model value into its FHIR element and `put`
  writes it back with the openEHR value the target already holds, and each pair
  carries the GetPut and PutGet laws as `proptest` properties over the subset
  its table declares lossless. The cells are `DV_CODED_TEXT` against
  `CodeableConcept` and against `Coding`, `CODE_PHRASE` and `TERM_MAPPING`
  against `Coding`, `DV_TEXT` against `string`, `Coding` and `CodeableConcept`,
  `DV_DATE_TIME` against `dateTime`, `DV_INTERVAL<DV_DATE_TIME>` against
  `Period`, `PARTY_IDENTIFIED` against `Reference` with `DV_IDENTIFIER` beside
  it, and `DV_PROPORTION` against `Quantity` for a percentage. A date and time
  value keeps the text it came with, so `Z`, `+00:00`, `+01:00` and fractional
  seconds survive both ways. An attribute the table marks as having no
  counterpart is carried from the value the target holds; a value the target
  would falsify refuses, so a `TERM_MAPPING` whose `match` is not `=`, a
  `DV_PROPORTION` that is not a percentage, and a `Period` collapsed into a
  `DV_DATE_TIME` where the page gives no rule are each a typed refusal naming
  the element. Beside the cells, the module carries the direction rule
  (`fhirCondition` runs when FHIR is the input, `openehrCondition` when openEHR
  is), the five condition operators with the connectives the specification
  fixes, the three recurrence rules as structured occurrences, and the closed
  set of losses a run may declare.
- One traversal runs a compiled program from either side (#84): `to_openehr`
  reads a FHIR resource and builds a canonical composition, `to_fhir` reads a
  composition and writes the resource the context names, and both walk the same
  mapping list top-down. Where a mapping writes follows one rule on whichever
  side is the output: the axes the parent bound keep their instance and every
  repeating element below them takes a fresh one per input occurrence, so a
  later mapping overwrites a single-valued output, two mappings append into a
  repeating one, and a `0..n` input into a `0..1` output leaves its last
  occurrence with the rest declared lost. A `manual` entry merges every path it
  names into one element, a `slotArchetype` recurses with the whole chain
  checked for a cycle, `type: NONE` only anchors what follows, and the composer
  and context start time are defaulted going into openEHR and recorded. Every
  refusal is typed and names the mapping: a value the element does not admit, a
  required `1..1` child the input does not carry, a slot chain that closes on
  itself, a `mappingCode` the registry does not hold, and the methods this
  milestone does not run (`reference`, `link`, `participationsFunction`).

- `fhirconnect::resolve`, context resolution into one immutable program per
  context mapping (#83). `compile` selects the start model mapping, applies the
  extensions the context declares in declaration order and file order, and
  resolves every path on both sides once: the FHIR side is bound to its anchor
  and resolved against the element table with its writability, and the openEHR
  side to a node of the Web Template with the repeating nodes on the way to it
  as its occurrence axes. The rules the specification leaves open are the
  bridge's own and each is refused rather than guessed: an `add` whose name
  collides, an `append` that carries mapping logic or names no target, an
  `overwrite` of a missing method, two extensions overwriting one name, an
  extension method on a nested mapping, a `slotArchetype` cycle, a template
  identifier or `sem_ver` the context and the template disagree on, an
  archetype revision the template's identifier contradicts, and a write side
  that names a filtering expression. A program carries the profile and template
  it was compiled against, each optional version recorded as pinned or
  unpinned, and `select` picks one by `meta.profile` membership on the FHIR
  side or by template identifier on the openEHR side, refusing an ambiguous
  selection by naming the candidates.
- The diagnosis chain of the vendored FHIRconnect mapping library compiles into
  one program, snapshot-tested, from the published model and extension files
  plus a FerroBRIDGE-authored context and extension, against a synthetic
  operational template in the testkit.
- `openehr_mapping_core::composition::NodeValue::under`, which writes a value
  under a value-internal family of its node (#84). Not every part of a data
  value is a datum suffix: Simplified Formats models an interval's `lower` and
  `upper` and a term mapping's `_mapping:0/target` as paths below the node.
- `openehr_mapping_core::index::node_id_matches` and
  `archetype_release_version`, the two archetype-identifier comparisons a
  mapping language needs outside a path: an identifier is matched in its
  interface form, and only an ADL 2 identifier states the release version below
  its major. `ResolvedNode::rm_path` hands back the `aqlPath` the index already
  parsed, so a consumer never re-parses it and the empty path of the root node
  reads as the composition root.
- A `fuzz/` crate with `cargo fuzz` targets over the YAML mapping loader and
  the openEHR mapping-path parser, seeded from the synthetic fixtures and a
  sample of the vendored mapping library, run weekly and on dispatch by
  `.github/workflows/fuzz.yml` on a nightly toolchain (#155). The lane is
  time-boxed and never a pull-request gate; a panic or a hang is the finding.
- `fhirconnect::model`, the FHIRconnect file model and its three validation
  layers (#82). `ast` carries one Rust type per construct a model, extension
  or context mapping file may hold, each node positioned at the YAML it was
  read from; `parse` lowers the positioned tree the shared loader returns and
  refuses an unknown key, a node of the wrong kind and a keyword value outside
  its documented set; `schema` compiles JSON Schema over both the schemas
  FHIRconnect publishes and the stricter pair this crate ships under
  `schemas/`; `semantic` applies the rules a schema cannot express (a
  condition's `criteria` against its operator, a `targetRoot` that names a
  child of the `with` path it filters, an extension method outside an extension
  file, a `reference` mapping without `$reference`, a `mappingCode` naming no
  registered function, and every cross-file reference against the loaded set);
  and `load` runs all of it over one file or a whole set, returning every
  diagnostic rather than the first.
- The vendored FHIRconnect mapping library is exercised by all three layers.
  One test pins the exact set the published schemas refuse (24 of the 107
  files, 34 errors, over `link`, `mappingCode` and a null `mappings`), so an
  upstream schema fix fails the test and forces a re-adjudication; another
  pins that this crate's schemas accept every file except the three that write
  `mappings` with no value; a third pins the library's own defects, including
  six duplicate `metadata.name` declarations and eleven cross-references that
  name no loaded mapping.

### Changed

- The `zizmor` pin moves from 1.29.0 to 1.30.1 in the CI lane and the pin matrix (#180). Its new `self-repository` audit is disabled in `.github/zizmor.yml` with the reason: actionlint 1.7.12 refuses the `$/` form it asks for, and the switch waits on actionlint (#223).
- The vendored draft REST API chapter of FHIRconnect gains
  `rest/sushi-config.yaml`, `rest/ig.ini` and `rest/input/pagecontent/index.md`
  at the pinned commit, fetched by `scripts/vendor/fhirconnect.sh`, so the
  implementation guide canonical behind the `$tofhir` and `$toopenehr`
  definitions is citable from the tree.
- The `openehr-*` crates move to 0.0.69, and `openehr-sdt` joins the workspace:
  the sibling split the Simplified Data Template engines (the Web Template
  builder, the FLAT and STRUCTURED codecs, the composition builder and the
  RM-instance validation) out of `openehr-its` at 0.0.68, so the mapping
  foundation and the tests reach them through `openehr_sdt` and `openehr-its`
  is taken with `opt14`, `json` and `rest-server` alone. `openehr-its` is
  Apache-2.0 again and `openehr-sdt` is BUSL-1.1, which the licence table
  records. `jsonschema` moves to 0.57.0, `thiserror` to 2.0.21, and both locks
  take every compatible update.
- The `openehr-*` crates move from 0.0.64 to 0.0.67 in the workspace, the
  fuzz manifest and every pin row, one hand-made bump for the five Dependabot
  ones (#207 to #211) that cannot build alone because the family is one
  lockstep line; 0.0.65 to 0.0.67 changed only the copyright holder in the
  source headers. `serde-saphyr` moves to 1.3.0 and `clap` to 4.6.7 with
  their pin rows (#206). Dependabot's cargo ecosystem gains an `openehr` group
  so the next family bump arrives as one pull request.
- The container base image `gcr.io/distroless/static-debian13:nonroot` moves
  to index digest `e2e927e`, in the `FROM` of `docker/Dockerfile` and its pin
  row (#199, landed by hand because the pin row lives outside the Dockerfile).
- The Licensor and copyright holder of the project's own work is Vernum
  Projecten B.V. (#197). Every `Licensor:`, copyright and
  `SPDX-FileCopyrightText` line names the company. The licence terms are
  unchanged, and maintainer credit stays a person.

- Keyword values compare case-insensitively inside their documented set, and a
  value outside the set is refused. The FHIRconnect text is case-inconsistent
  about its own keyword values, so `openEHR->fhir` and `$openEHRRoot` are
  admitted beside `openehr->fhir` and `$openehrRoot`. YAML keys stay exact,
  because the published schemas fix them with a JSON Schema `enum`.
- `fhirconnect::tree`, the bidirectional path model behind `with.fhir` (#81).
  It parses the expression FHIRconnect writes, including the two head forms the
  specification adds (`$resource` and `$fhirRoot`) and the `^` parent operator,
  resolves it against the R4 element table `fhir-types` emits, and reads and
  writes it over that crate's lexical `Value` tree. Navigation is by element
  name; a choice element resolves through `ofType()`, `as()` or the suffixed
  element name; a repeating element is an array addressed by a structured
  occurrence index; `extension(url)` selects or creates the entry carrying that
  url; and a primitive's `extension` lives in the sibling member named with a
  leading underscore, the FHIR JSON representation. `resolve()` returns a
  deferred outcome carrying the reference and the steps still to apply, and
  fetches nothing. Every expression is classified when it is parsed, so a
  mapping whose write side names a filtering expression (`where()`, `first()`,
  `last()`, an index) is refused with the offending step named, and every other
  refusal names the element path the table holds. A write applies to a copy and
  replaces the document only when every step succeeded.

### Fixed

- The FHIRconnect compiler derives the data-type pair of a mapping with no
  `type` key from its two sides, since the type "is derivable from the
  instances" (`data-mappings.adoc` §Deprecated), and the engine runs what it
  derived: an untyped `dateTime` element pairs with a `DV_DATE_TIME` node
  instead of converting as text; an untyped mapping onto a structural node
  anchors its children as `type: NONE` does, and one with no child is a
  `fc-anchor-without-children` warning; a choice element with no type filter,
  and an `Extension` against a data value through its `value[x]`, reads the
  alternative the document carries and writes the first pair of the node's
  class, refusing at load with `fc-underived-alternative` when the choice
  admits none; and a `type` key on a choice fixes the alternative both ways.
  The required-child check of `engine/Fail.adoc` counts a structural node as
  provided once a value below it is written, and skips a child its own
  condition closed. The same pass fixes four engine defects the KDS chain
  reached: one resource maps into one instance of a repeating start
  archetype, a repeating FHIR element is counted by its document path so
  `code.coding` and `verificationStatus.coding` no longer share instances, a
  `manual` entry's `openehrCondition` is evaluated going out of openEHR, and a
  node the composition does not hold is no input occurrence. `ENTRY.provider`
  travels as its `_provider` FLAT family, and the engine defaults
  `EVENT_CONTEXT.setting` to `238` "other care" as a declared default
  (`Defaults::with_setting` names another), so the declared set of a round
  trip closes.
- A mapping whose openEHR tail or `manual` path no FLAT part of the node's
  class carries is refused at load (`fc-uncarried-tail`, naming the mapping,
  the class and the tail) instead of on the request that first reaches it;
  the engine keeps its `UnsupportedTail` refusal as a backstop. The published
  `problem_qualifier.v2`, `KDS_problem_qualifier`, `problem_diagnosis.v1`,
  `multiple_coding_icd10gm.v1` and `report.v1.Condition` files name such
  tails, and the test contexts carry stand-ins for them.
- `participationsFunction` writes and reads `EVENT_CONTEXT.participations`
  (`$composition/context/participations`) as the `_participation:i` family
  under `context`, as it does `ENTRY.other_participations`.
- A mapping path now resolves to an element the operational template renames:
  the compiler matched a compacted node by the exact text of its `aqlPath`,
  so a template that constrains a name (`items[at0002,'Kodierte Diagnose']`)
  refused every mapping written by node id alone. It now applies the rule the
  index applies to an indexed node: a name the mapping does not write selects
  nothing, and one it does write must match.
- The FHIRconnect engine evaluates a program's own `openehrCondition` going out
  of openEHR and refuses a composition it does not admit with
  `EngineError::NotApplicable`, the rule it already applied to
  `fhirCondition` going in (`basics/Conditions.adoc`). A `manual` path no FLAT
  part carries refuses with `EngineError::UnsupportedTail` instead of being
  dropped, an openEHR position that does not fit is refused with its own
  `PositionError` instead of being dropped or reported as a zero, and a
  default whose template lookup fails for a reason other than a missing node
  refuses (#189, #187).
- `WebTemplateIndex::read` finds the n-th instance of a repeating node among
  that node's own occurrences (#186). It read the position as an RM
  positional predicate, which counts every element of the container (BASE
  Release 1.2.0 §Paths and Locators), so the first anatomical-location cluster
  behind a sibling element read as absent.
- `GET /fhir/metadata` declares `$tofhir` and `$toopenehr` in
  `CapabilityStatement.rest.operation` whenever the FHIRconnect operations lane
  is served beside the facade, with the canonical `OperationDefinition` URL of
  each (`http://fhirconnect.org/fhir/OperationDefinition/ToFhir` and
  `.../ToOpenEhr`), and leaves them out when the lane is off (#194). The two
  direct forms stay undeclared, because the draft REST API chapter keeps them
  outside the FHIRconnect implementation guide.
- The element table `fhir-types` emits now carries `Element` with its `id` and
  `extension`, so a table-driven consumer resolves the members of a primitive's
  underscore sibling (`_birthDate`) against the definition rather than against
  its own constants (#168). The entry is table-only: no `Element` Rust type is
  emitted, because an element typed `Element` becomes its own nested struct.
  The table types (`Schemas`, `TypeSchema`, `FieldSchema`, `Kind`, `ValueKind`)
  moved from `fhir_types::xml` to `fhir_types::schema` in the same change, and
  `Schemas::is_resource` is public.
- The conformance findings of the `fhirconnect` model and resolve review
  (#177). An openEHR path that names more than one template node is now
  `fc-ambiguous-template-node` naming the candidates instead of binding to
  their parent; the reference-model attributes a path walks below the deepest
  template node are checked against the `openehr-rm` attribute model; an
  `append` refuses every key but `followedBy`; a condition and a
  `hierarchy.split` path are read-only sites, so `where()`, `first()`,
  `last()`, an index filter and `resolve()` are accepted there; a slotted or
  extension file's `preprocessor` and a file-level `spec.conceptmap` and
  `spec.unidirectional` reach the program; `^` crosses a `reference` boundary
  into the enclosing resource; the model layer no longer refuses an `appendTo`
  that another extension's `add` supplies; the strict schema requires
  `targetAttribute` or `targetAttributes` on an `openehrCondition` again, as
  the published one does; a mapping-level data type beside a `with.type` is
  carried rather than dropped, and a disagreement between them is refused; and
  every refusal names the file it is about and the place it sits there, at any
  depth.
- The program carries what the engine would otherwise re-derive from path text
  (#177): a condition's attachment to the path it guards, a `manual` value as a
  literal or a named `$context` member, a `hierarchy.split` `create` as one of
  three elements, `FhirTarget::repeats()` beside the openEHR occurrence axes,
  and `Program::mapping_named` for a dotted method name. The profile-version
  refusal now names every program that claims the profile under another
  version instead of the last one.

## [0.0.2] - 2026-09-13

The foundation release. `v0.0.2-rc.1`, cut the same day, rehearsed the new
release lane with this content and is superseded by this release.

### Added

- `app/ferrobridge-server`, the `ferrobridge` binary shape (#21): a thin
  `main.rs` over a testable library run path, with `serve` plus the `etl run`,
  `cdm init`, `vocab load` and `mapping check` subcommands, which parse and
  exit 2 naming the issue that lands each. Configuration is an optional TOML
  file, named by `--config` or `FERROBRIDGE_CONFIG`, with
  `FERROBRIDGE__<SECTION>__<KEY>` environment variables over it; every struct
  refuses an unknown key, every default lives inline in its own `Default` impl,
  every credential is reachable through a `<key>_file` sibling read once at
  boot, and a refused configuration exits 78. A lane is off until its section
  is present, and a section that reaches identifiable data says so once at
  start-up, naming the section and never a value. The console is `tracing` with
  `auto`, `json` and `pretty` renderings, `auto` resolving by whether stdout is
  a terminal, and a filter that does not parse falling back to the default and
  logging the fallback. The HTTP surface is `GET /`, `GET /health/liveness` and
  `GET /health/readiness`, the last over a registry of per-upstream indicators
  that answers `503` while any of them is down. The middleware stack echoes an
  `X-Request-Id` only when it is printable ASCII of at most 128 characters and
  mints a version 4 UUID otherwise, renders a handler panic as a `500` with a
  JSON body carrying the request id, answers `408` past the request timeout and
  `413` past the body ceiling, and writes exactly one log line per request with
  the method, the matched route, the status, the latency and the request id.
  A body never reaches the log, and a query value only when the deployment
  named its parameter. `SIGTERM` and `SIGINT` start a bounded drain.
  The variable table is on the Operate page of the book.
- `ferrobridge-openehr` and `ferrobridge-term` each gain a `reachability`
  probe, one `GET` on the configured base and on `[base]/metadata`
  (<https://hl7.org/fhir/R4/http.html#capabilities>), so a readiness indicator
  asks whether the upstream answers at all without running an operation.
- The container image and the quickstart (#22). `docker/Dockerfile` is one
  stage on `gcr.io/distroless/static-debian13:nonroot`, pinned by the digest of
  its image index, copying the static musl binary the release lane stages at
  `dist/<os>_<arch>/ferrobridge`. It runs as uid 65532 written numerically,
  binds `0.0.0.0:8080`, exposes 8080/tcp, and carries an exec-form `ENTRYPOINT`
  with `serve` as the default command, so a Kubernetes `Job` sets `args:`
  alone. It declares no `HEALTHCHECK`, because the base has no shell: the probe
  is `GET /health/liveness` from outside, with `/health/readiness` answering
  `503` and a JSON body naming each down upstream. `compose.yaml` at the
  repository root is the quickstart: loopback port publishing with the Docker
  DNAT firewall hazard documented, a read-only root filesystem, every
  capability dropped, `no-new-privileges:true`, file secrets for the CDM URL
  and the CDR credential, and the `cdm`, `cdm-init` and `vocab-load` profiles
  over PostgreSQL 18.6. `scripts/checks/versions.sh` now checks the base-image
  digest, the quickstart image tag against the product and workspace versions,
  and the quickstart's PostgreSQL pin; the `docker` Dependabot entry points at
  `/docker`; and the book gains an Operate page for the image, the variables,
  the probe and the CDM profiles.
- The release lane at SLSA v1.2 Build Level 3 (#23). `release.yml` keeps the
  tag, the draft and the publish, and its build jobs become `uses:` calls to
  two new reusable workflows, which is what puts the signing identity out of
  reach of caller-defined steps. `release-build.yml` builds one native target
  per call on a runner of that architecture, with no cache and with
  `cargo auditable`, and produces the tarball carrying the binary, `LICENSE`,
  `NOTICE` and `README.md`, its SHA-256 checksum, a CycloneDX SBOM of the
  source graph at spec version 1.5, a syft SBOM read from the shipped binary,
  Sigstore bundles for all three attestations, and the provenance DSSE envelope
  as `.intoto.jsonl`, refused unless it carries the SLSA v1 predicate type.
  `release-image.yml` verifies this run's musl tarballs against the build
  lane's signer identity before staging them, pushes
  `ghcr.io/rubentalstra/ferrobridge` for `linux/amd64` and `linux/arm64`,
  attests the index and both platform manifests as OCI referrers, and verifies
  its own output the way a consumer would. `finalize-release` names the
  required asset set in full and refuses to publish a short release; a
  pre-release publishes with `--latest=false`. `SECURITY.md` carries the
  `gh attestation verify` commands for a tarball and for the image and says
  what each one proves, and the pinned tool versions are guarded by
  `scripts/checks/versions.sh`.

- `crates/ferrobridge-term`, the FHIR terminology client (#77):
  `CodeSystem/$lookup` resolves a code's display, `ConceptMap/$translate` maps
  a code into another system, `ValueSet/$validate-code` tests membership of a
  value set, and `batch` sends several of them as one `batch` `Bundle` that is
  answered by position. R4 and R4B are both read, over the generated
  `fhir-types` operation contracts rather than hand-written JSON; the two
  releases declare the same parameters for these operations, so the configured
  wire version selects the decoder. A negative answer an operation states in
  its own `out` parameters is an outcome, and every refusal is a typed error
  carrying the upstream status, the body, and any `tx-issue-type` coding the
  `OperationOutcome` holds. A missing display is a `NotFound` outcome, never an
  empty string; a `translate` refusal fails closed and is never a missing
  translation; every match is returned with its equivalence and `accepted()`
  yields the `equivalent` and `equal` ones alone, so a `wider` or `inexact`
  match never reaches a target system. Retry covers connect failures, timeouts
  and `5xx`, and the `POST` form is retried because the three operations
  declare `affectsState: false`. The testkit gains the `terminology()`
  container, the synthetic `CodeSystem`, `ValueSet` and `ConceptMap` the
  reference server loads, the `OperationOutcome` and `batch-response` stubs,
  and a pin row for the terminology server image.

- `openehr-mapping-core` gains the path module (#75): the Web Template built
  from either an OPT 1.4 or an ADL 2 OPT2 behind one `TemplateSource`, the
  `aqlPath` index with RM type, occurrences and node id, mapping-path
  resolution over it, the at-code versus id-code check that refuses an
  id-coded ADL 2 template, relative path derivation, composition build and
  read through `openehr-its` and `openehr-rm`, and the term-binding kind per
  generation. The ADL 2 fixtures are compiled from synthetic `.adls` sources
  with `openehr-adl` as a test-only dependency.
- The container harness, the upstream stubs and the synthetic fixtures in
  `tools/ferrobridge-testkit` (#78), with the two end-to-end tests they carry.
  `containers` starts PostgreSQL 18.6 and the reference openEHR CDR with its
  own database, each image pinned by tag and digest in `docs/VERSIONS.md`,
  which `scripts/checks/versions.sh` now compares with the harness constants,
  and torn down when the test's value drops; every container-backed test
  returns early unless `FERROBRIDGE_E2E=1`, so the ordinary suite stays
  offline.
  `stubs` carries the documented ITS-REST status shapes and the FHIR R4
  terminology `Parameters` for `wiremock`, and `fixtures` the synthetic
  operational template, composition and R4 resources. On that harness,
  `omop-cdm` applies its embedded OHDSI DDL to a real database and asserts the
  catalogue against the generated column metadata (#73), and
  `ferrobridge-openehr` commits, reads, updates, queries and deletes a
  composition against a real CDR over ITS-REST (#76). The `e2e` CI job runs
  both with the gate set. A second catalogue test pins what OHDSI's rendered
  `OMOPCDM_postgresql_5.4_constraints.sql` does at tag `v5.4.3`: PostgreSQL
  refuses it, because `vocabulary` is the one foreign-key target the rendered
  `primary_keys.sql` declares no key for.

- The generated OMOP CDM v5.4 layer and its generator (#73): `omop-cdm` carries
  a row type and a column-metadata static for every one of the 39 tables the
  OHDSI definitions declare across the `CDM`, `VOCAB` and `RESULTS` schemas,
  432 columns in definition order, with the CDM datatypes mapped to `i32`,
  `f64`, a bounded `Varchar<N>`, and the lexical `CdmDate` and `CdmDatetime`;
  OHDSI's four rendered PostgreSQL files are embedded verbatim and their
  `@cdmDatabaseSchema` placeholder is substituted through a checked PostgreSQL
  identifier. `tools/omop-cdm-codegen` emits all of it from the vendored
  definitions at tag `v5.4.3`, byte-deterministically, and the
  `omop-cdm-codegen-drift` CI job fails on any difference. Tests assert the
  generated column set of every table against OHDSI's DDL.

- `crates/openehr-mapping-core` carries the half of the mapping foundation both
  languages share (#74): the header FHIRconnect and OMOCL standardize between
  them, with `GrammarVersion`, `MappingName`, `MappingVersion`, `MappingType`
  and `ArchetypeId` as distinct types and everything a language adds under
  `spec` kept as an opaque positioned node; a YAML loader over `serde-saphyr`
  that resolves anchors, aliases and merge keys into a value tree where every
  node carries its line and column, and refuses a duplicate key, an
  unresolvable alias and a tab-indented file with a positioned diagnostic; a
  registry keyed by mapping name and by archetype id that refuses a repeated
  name naming both files; one `Diagnostic` type carrying file, position,
  mapping name, model path, severity and code; and a `MappingPath` over the
  `openehr-rm` BASE path parser that resolves the languages' own `../` step
  against an anchor and refuses a step above its root. Both vendored corpora
  load through the public seam in the test suite, which pins the exact set of
  files this crate refuses and why.
- The `fhir-types` element table (#121): every emitted type carries the path it
  is defined at, and every element carries its `ElementDefinition` path, `min`,
  `max`, the type codes the definition lists and the `contentReference` target,
  beside the XML kind the codec already read. `Schemas::element` resolves a
  dotted element path through complex types, backbone elements and content
  references, on a choice element's base path (`Observation.value[x]`) and on
  each expanded form (`Observation.valueQuantity`), and `Schemas::type_of`
  gives the type such a path resolves to. A test walks every vendored
  `StructureDefinition` of the emitted set and asserts the table agrees element
  by element.
- `fhir-types` converts between its own `codec::Value` and `serde_json::Value`
  (#122): `Value::into_serde_json` and `Value::to_serde_json` hand a document
  to `serde_json` only when every number comes back in the text the document
  carried, and otherwise return a typed error naming the element path, the
  lexical form and what `serde_json` writes, so a decimal never changes on the
  way out (FHIR regards `0.010` as different from `0.01`,
  <https://hl7.org/fhir/R4/datatypes.html#decimal>). `Value::from_serde_json`
  is the infallible other direction for the HTTP edge and test fixtures, and
  the strict codec's refusals still apply to a document that arrives that way.
- `ferrobridge-openehr` is the openEHR ITS-REST 1.1.0 client (#76): every call
  answers with an outcome enum whose variants are the statuses the governing
  operation documents, each carrying the upstream body, and a refused
  connection, a timeout, a `5xx` and a `401` are typed errors that keep the
  status, the body and the `WWW-Authenticate` challenge. It covers the EHR,
  COMPOSITION, CONTRIBUTION, AQL and template surfaces, sends `Prefer`
  explicitly on every request, sends `If-Match` as the bare quoted
  `version_uid`, sends the three 1.1.0 committal metadata headers on a commit,
  and fetches a template over both generations, `adl1.4` first and `adl2` on a
  `404` or a `406`. Timeout and bounded `backon` retry apply to idempotent
  calls only.
- The specification corpora under `docs/specs/` (#71), each fetched by a
  committed `scripts/vendor/*.sh` from the commit `docs/VERSIONS.md` pins and
  stamped with a `PROVENANCE.md`: the FHIRconnect specification source with its
  two published schemas and the unmerged REST API chapter, the FHIRconnect
  mapping library, the OMOCL corpus, the OMOP CDM v5.4 definitions with OHDSI's
  rendered PostgreSQL DDL, and the three `STABLE` openEHR ITS-REST OpenAPI
  documents. An integration test reads every tree, and
  `scripts/checks/versions.sh` fails when a provenance stamp stops naming the
  pin the matrix records.
- `fhir-types` carries a feature table (#120): `r4`, `r4b`, `r5` and `r6` gate
  the version modules, `terminology` carries the terminology root set the crate
  has always held, and `resources` widens the declared root set to every
  concrete resource each package defines with the complete closure of the
  datatypes it references. The default set is every version plus `terminology`,
  so a dependant that names no feature gets what it had. One emitted tree holds
  the union and each item carries the `cfg` of the narrowest feature that
  selects it, so the drift gate still covers the whole tree; a new
  `fhir-types-features` CI job checks each feature on its own.
- `docs/architecture.md` §3 supports both template generations (owner
  requirement, 2026-09-12): ADL 1.4 fetched as OPT 1.4 XML and ADL 2 fetched
  as AOM2 canonical JSON from the `adl2` route, decoded into the two OPT types
  and built into one Web Template by the two `openehr-its` builders behind one
  seam; the resolution order, the ADL 2 `Accept` rule, the HRID identifier
  handling, the at-code versus id-code refusal and the term-binding delta are
  recorded as FerroBRIDGE's own; `openehr-am` joins the pins and
  `openehr-adl` stays out until a CDR serves ADL 2 as text alone.
- `docs/architecture.md` carries ten mermaid diagrams: the system picture, the
  two languages over one foundation, the openEHR path pipeline, the FHIRconnect
  pipeline, the inbound create and the `$tofhir` sequences, the OMOP ETL flow,
  the crate graph, the FHIR identity derivation and the build order; the book's
  architecture tour gains the FHIRconnect pipeline and the ETL flow.
- The generated FHIR model and its generator (#72), moved in from the sibling
  terminology server by the owner decision of 2026-09-05: `crates/fhir-types`
  at 0.1.98, the one first-party crate under Apache-2.0, with the per-version
  R4, R4B, R5 and R6 modules, the strict JSON and XML codecs and the
  terminology operation contracts; `tools/fhir-codegen`, which emits the crate
  from the five vendored HL7 FHIR packages (380 MB, each with its
  `PROVENANCE.md`); `scripts/vendor/fhir-packages.sh`, which refetches those
  packages against the `docs/VERSIONS.md` pins; the `codegen-drift` CI job,
  which regenerates in check mode and fails on any difference; and the
  `regen-codegen` skill. Features, the public element table and the
  `serde_json::Value` conversion follow in the next increments of #72.
- The crates.io lane the published crate line needs (#72): the `crates` leg of
  `release.yml`, gated by the `crates-io` environment and authenticated with
  Trusted Publishing; `.github/workflows/publish-crates.yml`, the dispatch dry
  run and recovery path; `scripts/release/publish-crates.sh`, which uploads
  each member in dependency order and reads the registry back at each crate's
  own manifest version; the `publish-dry-run` CI job; and
  `scripts/checks/crate-version-guard.sh`, which fails a change that alters a
  crate's packaged content without moving its version.
- The workspace root discipline is complete (#20): a composite
  `.github/actions/setup-rust` action reads `rust-toolchain.toml` for every
  Rust-tier job, and every workspace member carries one integration binary at
  `tests/it/main.rs` (the foundation crate asserts the four `openehr-*` rows of
  the pin matrix name one line; the server test drives the library run path;
  the testkit tests its own pin-matrix reader).
- The Cargo workspace skeleton (#107): the root manifest with every lint the
  reliability rule names, the release profile, `rust-toolchain.toml`,
  `rustfmt.toml`, `clippy.toml`, `deny.toml` and a committed `Cargo.lock`;
  six placeholder library crates at version 0.0.0 (`openehr-mapping-core`,
  `fhirconnect`, `omocl`, `omop-cdm`, `ferrobridge-openehr`,
  `ferrobridge-term`), published to reserve their names, each carrying its
  pinned specification version as a constant that an integration test checks
  against the pin matrix; a thin `ferrobridge` binary over a library that does
  nothing yet; and the `ferrobridge-testkit` tool crate with the pin-matrix
  reader. CI tier 2 is active from this change on.

### Fixed

- `scripts/site/assemble.sh` renders the book into a temporary directory and
  copies from there (#58), so assembling the site no longer rebuilds
  `website/book/book/` in the source tree. The usage header says so.
- `cargo deny check` prints zero warnings (#145). The licence check now covers
  dev-dependencies (`include-dev = true`), which is what the `openehr-adl`
  exception needed to fire at all, and each of the ten `multiple-versions`
  duplicates carries a `skip` entry naming the older version and the crates
  that require it. `multiple-versions` stays at `warn`, so a new duplicate
  still shows.
- The server's request log line now covers every outcome (#21 follow-up): the
  log middleware sits outside the panic catcher, the request timeout and the
  body-size ceiling, so a caught panic, a `408` and a `413` each leave their
  one `request` line with the route, the status and the request id, where
  before they left none.
- The SonarQube Cloud coverage step compiles and instruments one workspace
  member at a time and merges the profiles into the single `lcov.info` Sonar
  imports (#146); the workspace-wide all-features form was killed on the
  runner with exit 143 and never produced a report. The step now fails when
  the report carries zero line records.

### Changed

- Two guards join the repository, and the book explains the version lines
  behind them. `scripts/checks/favicon-sync.sh` fails when a book theme favicon
  differs from the brand mark it copies, and runs in tier 1 of `ci.yml` (#52).
  `.github/workflows/pin-freshness.yml` reads every pin no Dependabot ecosystem
  covers, the analyzer versions in `ci.yml` and the documentation toolchain,
  against the newest upstream release each week, and opens one issue when a pin
  is behind rather than failing red (#35). `.claude/hooks/`
  `crate_version_bump_guard.sh` runs the crate bump guard before a `git commit`
  or a `git push`, so packaged content that moves without a version bump is
  refused where the fix is one edit (#79). The book gains an Operate page on
  the two version lines and Contribute pages on the bump rule and on cutting a
  release (#79, #66).
- `docs/ci-cd.md` records what watches each class of pin and that no Dependabot
  ecosystem is failing any more now that both manifests exist (#41), why the
  GitHub licence field reads `NOASSERTION` and why no layout change can fix it
  (#45), and a decision for each open Scorecard alert, including the
  branch-protection and code-review trade-offs a single maintainer accepts
  (#48).
- Every Rust job goes through the `./.github/actions/setup-rust` composite
  (#157), which gained `cache-shared-key` and `cache-save-if` inputs for the
  coverage lane; the `actions-rust-lang/setup-rust-toolchain` digest now lives
  in one file.

- Both generators now write the two SPDX tags directly under the
  `@generated … DO NOT EDIT.` banner, which keeps the first line (#129):
  Apache-2.0 for `crates/fhir-types`, the crate's own licence, and BUSL-1.1 for
  `crates/omop-cdm`'s generated tree and the provenance note beside the copied
  DDL. The four copied DDL files are vendored upstream material and carry no
  header of ours. `scripts/checks/versions.sh` anchors its SPDX pattern to the
  start of a line, so it still catches a header claim and no longer reads the
  emitter's own string literal as one. `fhir-types` moves to 0.1.102, its
  packaged bytes having changed. `fhir-codegen` also refuses a
  `contentReference` whose target is outside the emitted closure or behind a
  narrower feature than the element referencing it (#133), naming the element
  path and the reference; the vendored packages are consistent today, so the
  generated tree is unchanged. `deny.toml` records the rule its allow list
  follows (#134): a licence joins the list the day a dependency needs it, with
  the reason it is acceptable here, and leaves when the last crate carrying it
  does. All four licences the issue named are carried by crates in the current
  graph, so none is pruned and `cargo deny check` reports no unencountered
  licence.

- `docs/architecture.md` is rewritten as the output of the third research
  pass (2026-09-12): the `fhir-types` move is re-baselined on the crate as it
  is today (no cargo features, a lexical-precision `Value` instead of
  `serde_json::Value`, an XML-schema element table without cardinality, a
  crate line at 0.1.97 still published by the sibling); the draft FHIRconnect
  REST API (`$tofhir`, `$toopenehr`, specification pull request #93) becomes
  the engine surface beside the facade and ships ahead of it; FHIR identity
  takes the entry `uid` first per the upstream revision (pull request #94)
  with the hash as the fallback; the reference engine's 3.0.x behaviour
  changes are adjudicated (plural condition keys with OR semantics, lexical
  date and time preservation, no invented `DV_PROPORTION` extension URLs,
  refusal instead of warn-and-continue); a carry-over register records every
  decided behaviour, test invariant and known defect of the reference CDR's
  retiring FHIR connector with a disposition; the EHDS priority categories
  and the HL7 Europe guides become the planned profile targets in order (Base
  and Core, Laboratory, Medication, then Patient Summary), authored here and
  never a conformance claim; the build order gains v0.0.8 (the EU targets)
  and v0.0.9 (the change-feed adapter).
- `docs/VERSIONS.md` moves the `openehr-*` crates to 0.0.64 and the
  `fhir-types` floor to the sibling's latest release, pins the draft REST API
  chapter by pull-request commit, and gains a profile-package section that
  fills as contexts are authored. `CLAUDE.md` states the move as pending.
- `docs/architecture.md` §7 collapses the published crate set from twelve to
  seven (one crate per language, with modules), after a duplication check
  against both sibling projects; the tracker issues keep the module work units.

- `docs/architecture.md` is rewritten as the output of the second research
  pass (2026-09-05): the FHIR model is generated in this repository (the
  `fhir-types` crate and its generator move here from the sibling terminology
  server, which then consumes the crate from crates.io); the FHIRconnect
  mapping files are validated by a hand-written model, the vendored published
  schemas exercised as evidence, a strict schema of FerroBRIDGE's own, and
  semantic checks; a context mapping compiles once into an immutable program
  with pinned extension ordering; one interpreter serves both directions over
  data-type lenses; the openEHR wire is canonical JSON with the Web Template
  built locally from the OPT; FHIR ids derive from the stable composition uid
  and OMOP ids from sequences with a bridge-owned natural-key table; the OMOP
  commit unit is one composition's record graph; the library crates are
  published; the binary is one with subcommands; and the build order splits the
  round trips into v0.0.3 (FHIR) and v0.0.4 (OMOP). Every decision names its
  ground and its rejected alternatives.
- `docs/VERSIONS.md` pins the corpora by commit, the four `openehr-*` crates,
  the five HL7 FHIR packages the generator reads, and the crate line; the
  `fhir-terminology` row is removed, since the client contracts live in
  `fhir-types`. `scripts/checks/versions.sh` checks the new crate rows.
- The book's Evaluate, Operate and Integrate pages, `CLAUDE.md`,
  `docs/ci-cd.md` and the rules cite the pinned ITS-REST release and follow
  the rewritten design.

## [0.0.1] - 2026-09-05

### Added

- `.github/workflows/release.yml`, the release lane, dormant until a `v*` tag
  is pushed and running on nothing else. It validates the tag shape, refuses a
  `workflow_dispatch` that is not dispatched at the tag it names, checks the
  tag against every file that declares the product version (`CITATION.cff` and
  the `docs/VERSIONS.md` product row today, the root `Cargo.toml` as soon as it
  exists), and takes the release notes from the `## [X.Y.Z]` section of this
  changelog, failing when that section is missing or empty. The release is
  created as a draft and published only once its expected asset set is
  complete. The binary build sits behind the same root-`Cargo.toml` detection
  `ci.yml` tier 2 uses, so it is skipped on a tree with no workspace and
  activates by itself when one lands (#63).
- `docs/release.md`, the cut checklist in order, the no-retag rule, and what a
  published release is protected against: the immutable-releases setting
  freezes its notes and assets, and the `release-tags` ruleset blocks tag
  deletion and non-fast-forward updates and requires a signature (#63).
- The FerroBRIDGE brand assets under `assets/brand/`: the mark and its
  dark-tile, monochrome, and favicon variants, the three lockups
  (light, dark, and one that follows `prefers-color-scheme`), the 1200x630
  social card, the raster favicon set, the "Indigo & Iron" palette as
  `--ferrobridge-*` custom properties in `tokens.css`, and the brand README
  with the usage rules and the raster regeneration commands. The landing page
  now carries the favicon, the social card as its `og:image` and Twitter card
  image, and the mark in its header; the book gets the same favicon through
  `website/book/theme/`; and the repository README shows the lockup (#32).
- `scripts/site/assemble.sh` copies `assets/brand/` and the repository-root
  `llms.txt` into the assembled site, so the brand URLs the landing page names
  and <https://ferrobridge.eu/llms.txt> both resolve on the live host (#32).
- `.github/release.yml`, so GitHub's auto-generated release notes group pull
  requests by the tracker's own label taxonomy (Features, Fixes, Security,
  Documentation, Dependencies, Maintenance, Other changes), with
  `no-changelog` as the exclusion. The hand-curated changelog stays the primary
  record (#17).
- Two `cargo` and `docker` entries in `.github/dependabot.yml` beside the
  existing `github-actions` one, each grouping minor and patch bumps into one
  weekly pull request while majors stay individual, with the conventional
  commit prefixes `ci`, `deps` and `build` and `open-pull-requests-limit: 10`
  on cargo. Both are inert until their manifests exist (#17).
- The Rust half of `.github/workflows/sonar.yml`, gated on a root `Cargo.toml`
  at step level: the pinned toolchain with `llvm-tools-preview`, the
  instrumented `cargo llvm-cov nextest` run that writes `lcov.info`, and the
  `sonar.projectVersion` derivation that anchors the New Code window to the
  workspace version. `sonar-project.properties` names the lcov file and
  excludes build output, vendored trees, and the generated OMOP CDM subtree
  (#17).
- The documentation site at <https://ferrobridge.eu/>: a hand-written landing
  page at the root and an mdBook under `/docs/`, organised by reader intent
  (Evaluate, Operate, Integrate, Contribute). `.github/workflows/docs.yml`
  builds it on every pull request and deploys from `main` through GitHub Pages,
  `.github/actions/docs-toolchain/action.yml` installs the pinned mdBook
  toolchain, and `scripts/site/assemble.sh` places both halves and renders the
  roadmap block from the open milestones. `llms.txt` and `CITATION.cff` land
  with it, the README badge block covers CI, CodeQL, Scorecard, the Sonar
  quality gate and coverage, the licence and the latest release, and
  `scripts/checks/versions.sh` now guards the citation version and the three
  documentation-toolchain pins (#18).
- `.github/workflows/ci.yml`, the CI gate, in two tiers. Tier 1 runs on a
  repository with no code: zizmor at `--min-severity=low` with the online
  audits enabled, actionlint through its digest-pinned official image,
  shellcheck at `--severity=style` over every tracked shell program, hadolint
  over every tracked Dockerfile, and the comment-style and versions guards.
  Tier 2 is the Rust set (rustfmt, clippy, nextest and doctests, rustdoc,
  `cargo deny`, MSRV, dependency review), gated behind a `detect` job that
  looks for a root `Cargo.toml`, so it activates by itself when the workspace
  lands. The `conclusion` job reads every job's result and is the single
  required status check on `main`. Housekeeping the lanes read lands with it:
  `.hadolint.yaml`, `.dockerignore`, `.github/actionlint.yaml`, and the
  `.gitattributes` `linguist-generated` line reserved for the generated OMOP
  CDM subtree. `docs/ci-cd.md` records the design, why a mostly-skipped
  pipeline is still enforceable, and the owner actions that cannot be scripted
  (#16).
- The CI tool pins (zizmor, actionlint, shellcheck, hadolint) as a section of
  `docs/VERSIONS.md`, with `scripts/checks/versions.sh` failing when
  `ci.yml` and the matrix disagree (#16).
- `docs/VERSIONS.md`, the pin matrix: one place that records every version pin
  (the five specification pins, the published `fhir-types` and
  `fhir-terminology` crate pins, the Rust toolchain,
  edition, resolver and MSRV, the product version, and the licence) with the
  files that repeat each one. `scripts/checks/versions.sh` fails on any
  disagreement and on a first-party file claiming a licence other than
  `BUSL-1.1`, and skips with a printed reason for each file that does not
  exist yet. A `PostToolUse` hook runs the guard when a pinned file changes
  (#15).

### Changed

- `SECURITY.md` and `MAINTAINERS.md` name both protections a published release
  has: the immutable-releases setting freezes its notes and assets, and the
  `release-tags` ruleset stops its tag being moved or deleted (#68).

- The README's mark uses an absolute URL, so it renders wherever the README
  travels rather than only on the repository page (#60).

- The landing page footer shows the mark beside the wordmark, so the mark
  brackets the page instead of appearing only in the header (#56).

- The FerroBRIDGE mark is now the fork the product draws: a filled circle for
  the openEHR source, a two-way arrow to HL7 FHIR, and a plain line into a
  table for the OMOP Common Data Model. Only the FHIR branch carries
  arrowheads, because that side is an exchange and the OMOP side writes rows.
  Every file in `assets/brand/` is redrawn from the new masters, along with the
  book theme favicon, the social card, the lockups and the raster favicon set.
  The palette is unchanged. `assets/brand/README.md` describes the new
  geometry and its two drawing rules, and the landing page image alt text names
  the new shapes (#54).


- The landing page runs the approved "Indigo & Iron" palette. It carried a
  pre-brand blue that twenty-one rules read, so every link, button and
  gradient disagreed with the mark beside them. Every accent now clears
  WCAG AA against its background in both light and dark (#51).
- The social card's specification line is larger and lighter, so it stays
  legible at the size a link preview actually renders (#51).

- `SECURITY.md` links the private advisory form directly, so a reporter can
  act on the policy without navigating by hand. The document carried no
  hyperlink at all, which is what Scorecard's Security-Policy check scores
  (#47).

- `SUPPORT.md` sends a reader to the documentation site first. It still said
  there was no site, which went false the day the site went live and
  contradicted `README.md` on the same tree (#44).

- The contributor page at `/docs/contribute/checks-and-gates.html` names the
  zizmor audit path `.github/`, matching what CI runs. It still named
  `.github/workflows/`, so following it missed the Dependabot configuration
  and everything under `.github/actions/` (#40).

- The zizmor lane in `.github/workflows/ci.yml` audits `.github/` rather than
  `.github/workflows/`, so `dependabot.yml` and every composite action under
  `.github/actions/` are covered. A composite action runs with the calling
  workflow's permissions, so it is the same class of token-holding code.
  `CONTRIBUTING.md` and the pull-request template name the wider path too, so a
  local run still matches CI (#34).
- The Dependabot cooldown on the `github-actions` and `docker` ecosystems is
  7 days, up from 3, which is the floor zizmor's `dependabot-cooldown` audit
  enforces and matches the 7-day minor value already on `cargo`. Security
  updates are exempt from cooldown, so an advisory still arrives immediately.
  `docs/ci-cd.md` records why the earlier defence of the 3-day value did not
  hold and why no suppression was recorded (#34).
- `CONTRIBUTING.md` lists all six tier-1 gates `ci.yml` runs, in the same order
  and with the same flags. It named four, so a contributor who ran the listed
  commands could still fail CI on hadolint or the versions guard (#36).
- The licence of the project's own code and text is the Business Source
  License 1.1 (`LICENSE`, `NOTICE`): free for non-production use and for non-commercial production use, a commercial
  licence for any other production use, and Apache License 2.0 four years
  after each version. Every header, the README badge and licensing section,
  and the community and governance documents name it (#12).

[Unreleased]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.7...HEAD
[0.0.7]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.1
