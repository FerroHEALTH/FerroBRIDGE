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

- **The HL7 Europe Medication Prescription and Dispense 1.0.0 round trip**
  (#116). Two FerroBRIDGE contexts map FerroBRIDGE's medication template, an
  ADL 2 template over seven published CKM archetypes, to
  `MedicationRequest-eu-mpd` and `MedicationDispense-eu-mpd`, each with a
  referenced `Medication-eu-mpd` and the dosage as a `Dosage-eu-mpd`; the
  dispense is the medication ACTION at its careflow step "Prescription
  dispensed". `scripts/vendor/ckm-medication.sh` vendors the archetypes at
  the CKM mirror commit `docs/VERSIONS.md` pins, and the suites build the
  operational template at test time. `fhirconnect` 0.1.196 (with `ferrobridge-hl7v2` 0.1.150 and `openehr-mapping-core` 0.1.131) takes
  `openehr-adl` as a test dependency, and the bridge moves to the
  `openehr-*` crates at 0.0.100, whose ADL 2 Web Template builder reads the
  ADL 1.4 term constraints of the CKM sources.

- The run report of `etl fhir` carries an element ledger (#408): per input
  resource, every element with a value and one of four outcomes. `mapped`
  names the openEHR node a FHIRconnect mapping read the element into and the
  CDM `table.column` targets an OMOCL column reads there, `dropped` names the
  loss a mapping declared, `refused` the reason the resource or its
  composition was refused, and `unmapped` marks an element no mapping names
  or no column reads where it went. The text report adds a `ledger` line with
  the counts. `fhirconnect` 0.1.193 records every element a mapping reads
  going into openEHR (`Outcome::reads`, `Transformed::reads`), the OMOCL
  program names the composition node each CDM column reads
  (`Program::column_reads`), and `ferrobridge-hl7v2` moves to 0.1.147.

### Changed

- The operator console hydrates the whole page on FerroUI 0.5.0: the
  server draws the first page and the browser draws every page after it,
  so a link, a tab, the search or an action no longer reloads the document,
  and the browser's back and forward walk the pages you visited. The pages
  load in five groups, each the first time you open one of its pages or
  earlier while the browser is idle. Every page and action still works with
  no WebAssembly, as plain links and form posts. The facade's
  `CapabilityStatement` reaches the browser reduced to the new
  `ferrobridge-operator-api` `capability::Capability` document, which the
  console's server writes, and the entry WebAssembly's budget rises to
  646 KiB
  ([#1256](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1256)).
- **The bridge takes the `openehr-*` crates at 0.0.99 and drops three local
  openEHR spellings** (#618). The archetype HRID of an ADL 2 template ETag
  and of an archetype node id now reads through `openehr-am`'s `FromStr` for
  `ArchetypeHrid`, so the server no longer depends on `openehr-adl`. A
  `TERMINOLOGY_ID` is built with `openehr-base`'s `TerminologyId::new`. The
  `change_type` and `lifecycle_state` codes and rubrics of a commit come from
  `openehr-term`'s named concepts, now a direct dependency. What the bridge
  writes is unchanged. `fhirconnect` 0.1.195, `openehr-mapping-core` 0.1.130
  and `ferrobridge-hl7v2` 0.1.149 carry the new pins. `fhirconnect` removes
  the public `fhirconnect::engine::lens::code_phrase::terminology_id`; a
  caller uses `TerminologyId::new(system, version)`.

- `openehr-mapping-core` and `ferrobridge-operator-api` compile for
  `wasm32-unknown-unknown`, so the operator console's browser half can draw
  the bridge's documents. The workspace takes `openehr-its` without its
  ITS-REST client, which only the server adds, and the console CI lane
  checks the operator contract for wasm32
  ([#1256](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1256)).

### Fixed

- **A `manual` entry whose openEHR paths reach two data elements writes
  both.** The engine merged every path of one entry into the element the
  last path reached, so an entry that set the `current_state` and the
  `careflow_step` below one `ism_transition` wrote only the careflow step,
  with no diagnostic, and the build then defaulted the state. Each data
  element the paths reach is now created, as FHIRconnect's manual mappings
  page says ("Full FHIR or openEHR elements are created from all given
  paths").

## [0.0.13] - 2026-10-25

The security and OMOP release. A write the facade makes for an authenticated
caller states that caller as its committer, and `[auth] mode =
"introspection"` checks opaque access tokens (RFC 7662). On the OMOP side,
`etl fhir` loads FHIR R4 into the CDM with no CDR and every row names its
resource, `vocab load` loads an Athena download, the OMOCL targets write
`DEATH`, `provider_id` and `visit_detail_id`, `[cdm.pseudonymisation]`
pseudonymises source identifiers and shifts dates, and the Data Quality
Dashboard's conformance, completeness and temporal plausibility checks run in
Rust through `dq check`. The derived tables keep another ETL's rows, the CDM
pool trusts the configured CA alone, FHIRconnect converts `DV_ORDINAL` and
writes an `ACTION`'s time, and the first HL7 Europe profile contexts arrive.

The library crates publish at `fhirconnect` 0.1.192, `ferrobridge-term`
0.1.136, `ferrobridge-validator` 0.1.136 and `ferrobridge-hl7v2` 0.1.146, and
`fhir-types` 0.1.128, `openehr-mapping-core` 0.1.128, `ferrobridge-fhirpath`
0.1.128 and `hl7v2-types` 0.1.127 stay where they are. A crate version an
entry below names is the step at which that change came in.

### Added

- Every CDM row `etl fhir` writes names the FHIR resource it was mapped from
  (#409): the bridge schema's `resource_origin` table records each
  composition's resource type, id and `meta.versionId` in the commit's
  transaction, and `CdmWriter::origins_of_row` and `rows_of_resource` look a
  row's resources and a resource's rows up. `cdm init` adds the table.
- `ferrobridge_eu_laboratory.context` targets HL7 Europe Laboratory Report
  2.0.0 over the published `KDS_Laborbericht` template: the laboratory
  result becomes a `DiagnosticReport-eu-lab` that references one
  `Observation-resultslab-eu-lab` per analyte and one `Specimen-eu-lab` per
  specimen. The round trip asserts each profile's canonical and elements:
  the report's identifier, status, code, effective time, conclusion and
  references; the result's status, `laboratory` category, code, quantity,
  interpretation and method; the specimen's type, accession identifier,
  received time and collection. The mapping library's `eehrxf_lab` context
  is recorded as refused (#1235). The testkit exposes the template as
  `KDS_LABORBERICHT_OPT` (#116).
- `ferrobridge-term` 0.1.135 takes the client credentials token of
  `ferrohealth-smart` as its `TokenProvider` behind the new `smart` feature
  (#1227): `smart::SmartToken`, built from a `TokenSource`, sends the cached
  token and drops one the server refuses with `401`, so the repeated request
  carries a token fetched anew. The server's terminology lane uses it in
  place of its own adapter. `ferrobridge-hl7v2` 0.1.144 and
  `ferrobridge-validator` 0.1.135 move with it.

- The first HL7 Europe profile context: `ferrobridge_eu_condition.context`
  targets Base and Core 2.0.0 `condition-eu-core` over the published
  `KDS_Diagnose` template, with the published diagnosis, qualifier and
  composition model mappings and FerroBRIDGE's extensions for the profile's
  `assertedDate` extension and `onsetDateTime` slices and its canonical. The
  round trip asserts the code, both slices, `clinicalStatus` and
  `verificationStatus` (#116).
- `ferrobridge mapping check --vocabulary` checks every literal concept id of
  the OMOCL files against the vocabulary loaded in the `[cdm]` database: a
  literal the vocabulary does not hold, or whose domain is not one the CDM
  field definitions name for its column, is a diagnostic (#97).
- `ferrobridge_eu_patient.context` targets HL7 Europe Base and Core 2.0.0
  `patient-eu-core` over the published `KDS_Person` template, loading the
  person chain of the KDS person context with a gender extension that names
  the EU profile in place of the MII one and leaves out the German
  gender-amtlich extension. The round trip asserts the required `name` and
  `birthDate`, `gender` and the canonical (#116).
- The profile package set pins `hl7.fhir.eu.laboratory` 2.0.0 and
  `hl7.fhir.eu.mpd` 1.0.0, the HL7 Europe Laboratory Report and Medication
  Prescription and Dispense guides the EU profile contexts target, fetched
  with their provenance by `scripts/vendor/profile-packages.sh`; every
  package they depend on was already in the set (#116).
- The Data Quality Dashboard's seven temporal plausibility checks run in Rust
  beside its conformance and completeness checks (#1222):
  `measureObservationPeriodOverlap`, `plausibleTemporalAfter`,
  `plausibleDuringLife`, `withinVisitDates`, `plausibleAfterBirth`,
  `plausibleBeforeDeath` and `plausibleStartBeforeEnd`, each planned where the
  Dashboard's v2.9.0 settings enable it, with its threshold, and translated
  from its SQL template, which `scripts/vendor/dqd.sh` now vendors.
  `dq check` and the post-load step of `etl run` report them. A person whose
  year of birth is 0, as a placeholder `PERSON` row carries, has no birth date
  for these checks instead of failing them.
- `[cdm.pseudonymisation]` pseudonymises every row the CDM writer writes
  (#410): the columns the CDM field definitions describe as a link back to the
  source carry an HMAC-SHA-256 pseudonym under a `_file` secret key, stable
  across runs so incremental loads join, the provider's name, `npi` and `dea`
  and a location's address are NULL, and `date_shift_days` moves every date
  of a person by one per-person shift, so intervals within a person stay
  intact. The Integrate and Operate pages state what it does not de-identify.
- `cdm init` and every `etl run` and `etl fhir` count the rows of each CDM
  table whose ids the bridge assigns that no entry of its side table owns,
  rows another load wrote, and warn of each table; the run report carries the
  counts as `foreign_rows` (#240). The Operate page states that one bridge owns
  the tables it writes and that era ids are not stable across rebuilds.
- `ferrobridge-hl7v2` 0.1.143 serves a consumer of its listener and parser
  alone. `parse::structure::structure_declared` selects the structure
  MSH-9.3 names over the message definition where the two disagree, so an
  IHE ITI-8 `ADT^A05^ADT_A01` groups by the A01 structure (IHE ITI TF-2
  §3.8.4.1.2.1; HL7 v2.5.1 chapter 2 §2.15.9.9) (#1217).
  `parse::ErrorCode::UnknownKeyIdentifier` is table 0357's `204` (#1218).
  `mllp::Connection::peer` gives a handler the address of its connection's
  peer (#1220). `map` and its dependencies (`fhirconnect`,
  `ferrobridge-term`, `fhir-types` and the openEHR crates) sit behind the
  default `map` feature, so `default-features = false` drops them (#1221).
  A consumer that matches `ErrorCode` exhaustively adds the new variant.
- A write the facade makes for an authenticated caller states that caller as
  the version's committer, as ITS-REST 1.1.0 recommends ("provision these
  headers based on authentication and authorization runtime data"): a
  single write sends `openehr-audit-details` with `committer.name` set to the
  token's `sub` (or the Basic user-id) and a `committer.external_ref` in the
  issuer's namespace, and a transaction states the same committer in every
  `UPDATE_AUDIT` of its CONTRIBUTION. `system_id` keeps naming the bridge.
  A write no caller made (an HL7 v2 message, an open surface) still commits
  under the bridge's name. Reads still reach the CDR under the bridge's own
  credential (#532).
- `[auth] mode = "introspection"` with an `[auth.introspection]` table
  checks opaque access tokens by OAuth 2.0 Token Introspection (RFC 7662)
  through `ferrohealth-smart`'s introspector: each token is posted to the
  issuer's introspection endpoint under the bridge's client authentication
  (`[auth.introspection.client]`) or a client credentials token
  (`[auth.introspection.bearer]`), an active answer with `scope`,
  `client_id` and `exp` admits the caller with its scopes and `patient`
  context, the answer is held until its `exp` and at most
  `cache_max_age_seconds`, an inactive token answers `401` and an endpoint
  that fails `503` (#533). The Operate configuration page lists the keys.
- `ferrobridge etl fhir PATH` loads FHIR R4 resources, a Bundle or an NDJSON
  file, into the CDM with no CDR (#1209, decided on #407): each resource maps
  through the FHIRconnect set into an openEHR composition held in memory,
  which the OMOCL set maps to CDM rows the writer commits. The composition's
  identifiers derive from the resource's type, id, `meta.versionId` and
  subject, so a re-run writes the same rows, and the run report counts the
  resources read, mapped, unmapped and refused. `[etl] aql` becomes optional
  at load; `etl run` refuses to start without it.
- `ferrobridge vocab load DIR --schema NAME` loads an OHDSI Athena vocabulary
  download into the vocabulary tables of the CDM schema (#233). It takes the
  nine files every download carries and `SOURCE_TO_CONCEPT_MAP.csv` when
  present, replaces each table in a transaction of its own through `COPY`
  with the column list from the header row checked against the CDM
  definitions, names every other file and leaves it alone, and prints the
  rows per table and the vocabulary version. The format is the one the Athena
  application writes, read from its source at a pinned commit and vendored
  under `docs/specs/athena/` by `scripts/vendor/athena.sh` (#88). The Operate
  pages describe the load and the vocabulary licences you accept.
- FHIRconnect converts `DV_ORDINAL` against `CodeableConcept`
  (`DV_ORDINAL.adoc` §CodeableConcept, #93): the coding carries the symbol,
  and going into openEHR the ordinal value is the one the template's
  `C_DV_ORDINAL` lists for the symbol's code. A symbol the template does not
  list, or a node whose template lists no options, is refused.
  `fhirconnect` 0.1.191 adds `RmValue::Ordinal`, `lens::ordinal` and a
  fifth parameter to `cell::put`, the node's ordinal options; a caller of
  `cell::put` passes `&[]` where it has none.
- An OMOCL mapping's `provider_id` and `visit_detail_id` now write (#243):
  the record graph names a `PROVIDER` by the text the mapping reads and a
  `VISIT_DETAIL` by the composition's visit and that text, and the CDM
  writer resolves or creates each row inside the composition's transaction,
  the visit detail taking its person, concept, dates and type concept from
  the visit. A record that names a visit detail without a visit is refused.
  The bridge schema gains the `provider` and `visit_detail` key tables, which
  `cdm init` creates on an existing schema.
- The OMOCL `Death` target writes its `DEATH` row (#1208): the row takes the
  person's `person_id` as its id, the newest composition that writes one gives
  it its columns, and removing that composition restores the row of the
  writer that remains. `cdm init` adds the `death_row` side table and lets
  several compositions of a person record a `DEATH` row.
- The Data Quality Dashboard's six completeness checks run in Rust beside its
  conformance checks (#405): `measurePersonCompleteness`,
  `measureConditionEraCompleteness`, `measureValueCompleteness`,
  `standardConceptRecordCompleteness`, `sourceConceptRecordCompleteness` and
  `sourceValueCompleteness`, each planned where the Dashboard's v2.9.0
  settings enable it, with its threshold, and translated from its SQL
  template. A run now carries the Dashboard's not-applicable status: a check
  of a missing or empty table, or of a missing or unpopulated field, is
  neither failed nor passed and names why.
- `ferrobridge dq check` runs the ported Data Quality Dashboard checks against
  the database `[cdm]` names and prints the failing checks and the counts, or
  with `--format json` the run in the Dashboard's own result fields
  (`CheckResults`, `Overview`, `startTimestamp`), so the Dashboard's viewer
  reads it. It exits non-zero when a check fails or cannot be evaluated
  (#406). With `[etl] quality_checks = true`, `etl run` runs the same checks
  over the tables it wrote and adds the verdicts to its report.

### Fixed

- A FHIRconnect mapping can write an `ACTION`'s time (#1226): `$archetype/time`
  under an `ACTION` no longer refuses with `fc-uncarried-tail`, and the engine
  writes and reads it as the action's `time`, the way it carries an
  observation's history origin. A FHIR `Procedure`'s `performedDateTime` now
  reaches `ACTION.time`, and on the `etl fhir` path the
  `PROCEDURE_OCCURRENCE.procedure_date` OMOCL reads from it. `fhirconnect`
  moves to 0.1.192 and `ferrobridge-hl7v2` to 0.1.145. Breaking for a consumer of
  `fhirconnect::engine::rm`: `Carried::HistoryOrigin` is `Carried::EntryTime`,
  and `HISTORY_ORIGIN` and `HISTORY_ORIGIN_KEY` are `ENTRY_TIMES` and
  `entry_time_key`.
- A run rebuilds `OBSERVATION_PERIOD`, `CONDITION_ERA` and `DRUG_ERA` only for
  the persons the bridge writes, so the derived rows another ETL wrote into
  those tables survive it (#1223). `[etl] derived_tables = "whole_table"`
  keeps the whole-table rebuild for a deployment where the bridge is the only
  writer.
- With `[cdm] tls_ca` set, the concept resolver's pool now trusts that CA
  alone, as the CDM writer does (#266). The pool no longer runs its
  PostgreSQL client's own TLS, which added a built-in root set to the CA: it
  reaches the database through a relay on a private Unix socket that
  handshakes with the writer's TLS configuration. A URL that asks for TLS
  names one host and one port; a list of hosts is refused at start.

### Changed

- The bridge and `ferrobridge-term`'s `smart` feature take
  `ferrohealth-smart` 0.7.0 in place of 0.6.0, the line the family moves to
  together: `ferrobridge-term` 0.1.136, `ferrobridge-hl7v2` 0.1.146 and
  `ferrobridge-validator` 0.1.136. No configuration key or behaviour changes.

- The operator console's search answers under the field as you type: a panel
  anchored under the top bar's search asks the console's `/search/answer`
  route and shows the running load's contexts, mapping files and templates
  grouped by kind, five of each, with the words searched for emphasised;
  the arrow keys walk them and Enter opens one. Enter with none chosen, or
  "See all results", opens the new search page (`/search`), which lists
  every match in the page. The console moves to FerroUI 0.4.0, and a
  setting row says where its value came from through FerroUI's own marker
  (#1219, #1176).

## [0.0.12] - 2026-10-25

The FHIRconnect and facade outcomes release. A write through the facade now
shows the client the losses its mapping declared, a transaction names why it
refuses a linked resource, and the FHIRconnect operations answer `502` when
the terminology server fails. FHIRconnect looks up the display of a `DV_TEXT`
that fell back to a code, ranks a direct data-type pair above the text
fallback, and `openehr-mapping-core` exposes the `DV_ORDINAL` options a
template constrains. The `fhir-types` XML codec holds the narrative `div` to
well-formed XHTML both ways. The terminology client gains `$expand`,
`$subsumes`, the `date` input, a caller-built HTTP client for mutual TLS or a
private trust root, and context-aware `$validate-code`, and the operator
console draws the running configuration.

The library crates publish at `fhir-types` 0.1.128, `openehr-mapping-core`
0.1.128, `fhirconnect` 0.1.190, `ferrobridge-term` 0.1.134,
`ferrobridge-validator` 0.1.134, `ferrobridge-fhirpath` 0.1.128 and
`ferrobridge-hl7v2` 0.1.141, and `hl7v2-types` stays at 0.1.127. A crate
version an entry below names is the step at which that change came in.

### Added

- `WebTemplateIndex::ordinal_options` returns the options an OPT 1.4
  `C_DV_ORDINAL` constrains at a `DV_ORDINAL` node, in template order: each
  `OrdinalOption` carries the ordinal `value` and the code and terminology
  of its symbol, and a node with no ordinal constraint answers `None`.
  `openehr-mapping-core` moves to 0.1.127, `fhirconnect` to 0.1.186 and
  `ferrobridge-hl7v2` to 0.1.135 with it (#1090).
- The facade shows a write's declared losses to the client that wrote it: a
  committed transaction entry carries them in `response.outcome`, and a
  single create or update asked for `Prefer: return=OperationOutcome` lists
  them after the committed version, one issue per loss with the FHIRPath in
  `expression` and the openEHR path in `details` (#1115).
- `GET /operator/configuration` answers the configuration the bridge started
  on, behind `[operator]` and the operator scope like the other operator
  routes: every key with its value and its source (the file, the
  environment or the default), and each secret by reference alone (set,
  read from a file, or unset), never its value. The document is
  `ferrobridge_operator_api::configuration::Configuration`, which the
  operator console decodes (#1147).
- The operator console draws the running configuration: a Configuration
  page with the Logging, Mappings and Server and access tabs, a Settings tab
  on the FHIR facade, HL7 v2, the OMOP ETL and each connection, and the
  host and credentials of each upstream in the Connections table. Every
  value is read-only, with its TOML key and whether it came from the
  environment or is the default, and every secret is shown by reference
  alone; How configuration works opens from the line that names the file.
  An empty `[auth] product_scope` is answered as the scope the bridge runs
  on, `__ferrobridge.operator` (#1147).
- `hl7v2-types` (Apache-2.0) and `ferrobridge-hl7v2` (BUSL-1.1) join the
  crate line at 0.1.127, leaving their 0.0.0 name reservations, so the next
  publish ships the HL7 v2 tables and the HL7 v2 face (MLLP framing, MSH-18
  decoding, the positional parser, the acknowledgment and the v2-to-FHIR
  map) on crates.io for other projects to depend on. Each README carries a
  worked example, and the entry points of `ferrobridge-hl7v2` carry tested
  `# Examples`. Pin each exactly, as for every crate of the line (#1131).
- `ferrobridge-term` 0.1.130 sends `ValueSet/$expand` and
  `CodeSystem/$subsumes`, on STU3, R4 and R4B and in a `batch`.
  `Client::expand` takes an `Expansion` (`url`, `valueSetVersion`, `date`,
  `filter`, `offset` and `count`, `includeDesignations`) and answers the
  expanded value set with the nesting of `expansion.contains` kept, each
  entry with its system, version, code, display, `abstract`, `inactive` and
  designations; a value set the server does not know is
  `ExpandOutcome::NotFound` carrying the `OperationOutcome` and its
  `tx-issue-type`, and an expansion the server could not make is a refusal,
  never an empty expansion. `Client::subsumes` answers `SubsumesOutcome`:
  `Equivalent`, `Subsumes`, `SubsumedBy` or `NotSubsumed`.
  `Client::code_system_count` asks
  `GET [base]/CodeSystem?url=<system>&_summary=count`. `ferrobridge-validator`
  moves to 0.1.130 and `ferrobridge-hl7v2` to 0.1.128 with it (#1177).
- `ferrobridge-term` 0.1.131 reaches a terminology server through an HTTP
  client the caller builds: `Client::with_http(config, reqwest::Client)`
  sends every request through it, so a deployment can present a client
  certificate for mutual TLS, trust a private root beside the platform's or
  use a proxy. The configured timeout still bounds each request, and the
  credentials, the retry budget and the typed errors apply as for
  `Client::new` (#1180).
- `ferrobridge-term` 0.1.131 validates a code whose system its context
  implies: `Coded::InContext` sends `ValueSet/$validate-code` with
  `context`, `code` and `display` and no `system`, which R4 and R4B accept
  ("If a code is provided, a system or a context must be provided"), alone
  or in a `batch`. STU3 requires a system for a code, so an STU3 client
  refuses the form with the new `Error::SystemRequired` and sends nothing,
  and `Client::check` answers the same (#1181). `ferrobridge-validator`
  moves to 0.1.131 and `ferrobridge-hl7v2` to 0.1.129 with it.

### Changed

- `fhirconnect` 0.1.188 moves `engine::rm::NULL_FLAVOUR`, `NULL_REASON` and
  `null_flavour_of` to `engine::rm::null_flavour`, which breaks an import of
  them: name them through `engine::rm::null_flavour::` (#1194).
- A transaction entry whose mapping links a resource as a composition of its
  own is refused with `422` and `not-supported`, naming why and the ways that
  commit it: the resource on its own, or as an entry of a `batch`, whose
  entries commit one at a time and follow the link as a single create does
  (#1120).
- `ferrobridge-term` 0.1.130 carries the `date` input of `CodeSystem/$lookup`
  and `ValueSet/$validate-code`: `Client::lookup_at` sends it, and
  `Request::Lookup` and `Validation` gain a `date` field. This breaks a
  struct literal of either: add `date: None` where no date is meant (#1177).
- `SECURITY.md` states the fix-time targets every product of the Ferro family
  commits to, counted from the confirmed assessment: a fixed release within
  7 days for a Critical vulnerability, 30 days for High, 90 days for Medium
  and with the next release for Low, beside the acknowledgement within about
  5 business days and the initial assessment within about 10, both best
  effort as before.

### Fixed

- The `fhir-types` XML codec holds a narrative `div` to well-formed XHTML
  both ways: `xml::to_xml` refuses a `div` that is no well-formed XHTML
  `div` in the XHTML namespace with an `EncodeError` naming its element path,
  such as `Basic.text.div`, and `xml::from_xml` refuses a start tag whose
  attributes are not well-formed XML (no whitespace between two attributes,
  a name that is no XML name, a `<` in a value), in the narrative and in
  every other element, where it accepted them and wrote them back repaired.
  `fhir-types` moves to 0.1.128, `fhirconnect` to 0.1.189,
  `ferrobridge-term` and `ferrobridge-validator` to 0.1.134,
  `ferrobridge-fhirpath` to 0.1.128 and `ferrobridge-hl7v2` to 0.1.140 with
  it (#1178).
- A coding with no `display` written into a `DV_TEXT`, whose value would
  otherwise fall back to the coding's code, is looked up through
  `CodeSystem/$lookup` as one written into a `DV_CODED_TEXT` is: the display
  becomes the value, an unknown code refuses unless
  `[terminology] display_fallback` is set, a failed call refuses, and with no
  server the code stands. `fhirconnect` moves to 0.1.185 and
  `ferrobridge-hl7v2` to 0.1.134 with it (#1188).
- An untyped `dateTime` mapped onto an `ELEMENT` whose value admits
  `DV_TEXT` and `DV_DATE_TIME` converts into `DV_DATE_TIME` whatever the
  template order: a class that pairs with the FHIR type directly outranks
  the text fallback. `fhirconnect` moves to 0.1.181 and `ferrobridge-hl7v2`
  to 0.1.130 with it (#1114).
- `$toopenehr` and `$tofhir` answer `502` with the issue type `transient`
  when the terminology server fails during the run, with the upstream status
  in the diagnostics, as the facade's writes do. A translation the server
  answers with no match stays a `422` (#1119).

## [0.0.11] - 2026-10-24

The operator console release. The console is rebuilt page for page on
FerroUI 0.3.6 and renamed from `ferrobridge-viewer` to `ferrobridge-console`,
so its image, environment variables, compose profile, cookies and page
addresses change: the Changed section below lists each, with what an
operator changes. Release images are scanned before they are tagged, every
release carries a vulnerability record in which each finding is judged, and
the notes state the support period and link the CRA information for users.
FHIRconnect registers the last `mappingCode` functions of the published
library and gains display resolution, `NULL_FLAVOUR`, the `EVENT` choice and
the quantity, temporal, URI, string and interval data-type pairs.

### Added

- The book has a [CRA information for users](https://ferrobridge.eu/docs/evaluate/cra-user-information.html)
  page (#1152): the information and instructions Annex II of the Cyber
  Resilience Act (Regulation (EU) 2024/2847) has accompany each release,
  point by point, from the manufacturer and the vulnerability contact to the
  support period, secure use, updates, decommissioning and the SBOMs. The
  release notes `scripts/release/plan.sh` composes now end with a link to it,
  and the composition refuses when the book does not list the page.
  `SECURITY.md` names the operator console's release tarballs and site
  bundle among the supported artefacts.

- The eighth `mappingCode` function of the published FHIRconnect library,
  `ehrStatusAddExternalREF`, is registered (#1065), so
  `KDS_admin_entry_person.yml` loads in the published person context. It
  writes no template node: it offers each `Identifier` the mapping reads as
  the subject of the EHR, on the new `Outcome::subject` channel
  (`fhirconnect::engine::subject::SubjectIdentifier`), and coming out of
  openEHR it is declared skipped (`SkipReason::EhrStatusSubject`). The
  facade writes it into `EHR_STATUS.subject.external_ref` through ITS-REST
  1.1.0 `ehr_status_update` with `If-Match` on the version it read, before
  the composition is committed, for the identifier of the new
  `[facade] ehr_status_identifier_system` key. An `EHR_STATUS` that names
  another subject answers `409`, a CDR refusal answers through the status
  table with nothing committed, and with the key unset a write whose mapping
  offers identifiers answers `422` naming the key. New for a consumer of
  `fhirconnect`: `MappingFunctions::writes_subject` and `to_subject`, with
  defaults, and the `SkipReason` variant. `fhirconnect` 0.1.180.
- A FHIRconnect mapping that names an `EVENT` node itself, with a FHIR time
  on the other side, writes and reads the event's time (#1059), as
  `types-of-mappings/data-type/EVENT.adoc` §POINT_EVENT and INTERVAL_EVENT
  pairs them. A `dateTime`, `instant` or `date` is a `POINT_EVENT`'s `time`;
  a `Period` is an `INTERVAL_EVENT` whose `time` is `Period.end`, whose
  `width` is the span back to `Period.start` and whose `math_function` is
  the RM default `640|actual|`. A choice element such as `effective[x]`
  picks the class from the alternative going in and from the event's class
  coming out. A FHIR `time`, a period with a missing or partial bound, and a
  non-choice element of the other class are refused; an interval event whose
  `math_function` is not `actual` declares it as a loss going out.
  `WebTemplateIndex::amend` edits one node instance of a built composition
  and validates it again. New variants a consumer matching exhaustively adds
  arms for: `PathError::AbsentInstance`, `Derived::Event`,
  `EngineError::EventTime` and `SkipReason::EventFunctionUncarried`.
  `laboratory_test_result.v1.yml:120` and `:448` no longer warn
  `fc-anchor-without-children`. `openehr-mapping-core` 0.1.126,
  `fhirconnect` 0.1.179.
- FHIRconnect LINKED mappings (#94): a `link` whose FHIR side names a
  resource, such as a Condition's `encounter`, now commits that resource as a
  composition of its own through the context its `meta.profile` names, and
  writes `ehr:/<ehr_id>/compositions/<versioned_object_uid>` into the
  linking composition's `LINK.target`. A facade create or update runs in two
  passes, because ITS-REST 1.1.0 lets the CDR, not the client, assign a new
  composition's id: a probe run finds the linked resources, each is
  committed as a `POST` would commit it (a replay links the composition the
  first delivery produced, and a resource this server already serves links
  where it stands), and the final run writes their URIs. A resource no
  context maps is a declared loss, `skipped-unmapped-link`. A linking commit
  the CDR refuses answers its refusal with a warning naming each linked
  composition it left. A transaction or batch entry that links a resource is
  refused, since a transaction is one `CONTRIBUTION`, and `$validate` runs
  the link without committing. The engine gains the
  `engine::linked::LinkedCompositions` seam (`Seams::with_linked`),
  `SkipReason::UnmappedLink` and `ReferenceError::Link`; a run with no seam
  refuses as before. `fhirconnect` 0.1.174.
- The FHIRconnect engine converts `ELEMENT.null_flavour` against the code
  of the R4 `data-absent-reason` extension both ways (#93), through
  `engine::lens::null_flavour` with GetPut and PutGet property tests and a
  test per entry of its committed table. A tail on the element reaches
  `null_flavour` as the `_null_flavour` family of the node the element
  shares with its value, with `_null_reason` beside it. FHIRconnect
  `NULL_FLAVOUR.adoc`'s table is no function; the cell reads it with the
  openEHR terminology's null flavours (271 "no information", 253
  "unknown"), keeps a FHIR code with no flavour of its own in
  `null_reason`, and refuses a code outside either set. **Breaking:**
  `SkipReason` gains `NullReasonUncarried`, the declared loss of a
  `null_reason` that names no code, so a consumer that matches `SkipReason`
  exhaustively adds the arm. `fhirconnect` 0.1.176.
- A FHIRconnect mapping can now name the origin of an observation's
  history, `$archetype/data/origin`, which no Web Template node carries
  (#95). The engine writes it as the Simplified Formats `history_origin` of
  the observation and reads it back from `HISTORY.origin`. An `OBSERVATION`
  whose `HISTORY` carries events, each with an `ITEM_LIST`, now round-trips
  under both lens laws against a synthetic template: going out each event
  becomes its own resource (the FHIRconnect hierarchy split), and the origin
  and each event's time keep their offset in both directions. The round-trip
  pass list gains `ferrobridge_observation_events` (5 of 5). `fhirconnect`
  0.1.171.

- Going into openEHR, a coding that carries no display now has its display
  resolved through `CodeSystem/$lookup` on the `[terminology]` server
  (#95), as the FHIRconnect `DV_CODED_TEXT` page recommends. A found
  display becomes the `DV_CODED_TEXT` value. A code the server does not know
  refuses the resource unless `[terminology] display_fallback = true`
  (`FERROBRIDGE__TERMINOLOGY__DISPLAY_FALLBACK`) writes the code instead. A
  failed call refuses either way, carrying the upstream status. With no
  terminology server the code stands as before, the page's alternative "if
  a term server is not an option". Lookups join the translations in one
  call or one `batch` per round. The FHIRconnect terminology seam gains
  `lookup` and `display_fallback`, both with refusing defaults. The engine
  answers `EngineError::UnknownDisplay` and `EngineError::DisplayLookup`.
  `fhirconnect` 0.1.170.
- Five more `mappingCode` functions of the published FHIRconnect library
  are registered (#95). `timingToDaily` writes a frequency per one unit of
  time as the `1/<unit>` frequency of the daily timing cluster (or an
  interval of them with `frequencyMax`), one event per longer period as the
  interval between doses, and the one time of day. `dosageQuantityToRange`
  writes a dose `Range` as an included `DV_INTERVAL<DV_QUANTITY>`.
  `rangeToText` writes a rate `Range` as the text `<low> <code> - <high>
  <code>`, the only form the rate element admits beside a quantity.
  `ratio_to_dosage` and `ratio_to_dosage_action` write a rate `Ratio` (R4's
  "500ml over 2 hours") as the rate per unit of time and the administration
  duration. Every quantity and range converts through the engine's
  `DV_QUANTITY` and `DV_INTERVAL<DV_QUANTITY>` cells, a rate divides and
  multiplies exactly or is refused, and each function refuses what it
  cannot carry. A port onto a polymorphic `ELEMENT` now
  reads only the value node whose `_type` matches. The library's
  `dosage.v2.yml`, `dosage.v2.BackboneElement.yml` and
  `timing_daily.v1.yml` now load, so the medication statement context gets
  to compile. Its refusals there are recorded on #101. It landed in
  `fhirconnect` 0.1.169 beside #1076, with no version of its own.
- The FHIRconnect engine converts a scalar `Integer` or `Integer64` tail
  against FHIR `integer` and a scalar `Real` tail against `decimal` both
  ways (#1081), through `engine::lens::scalar` with GetPut and PutGet
  property tests; going out these refused with no cell. A value outside
  FHIR's 32-bit `integer` is refused going out, and a decimal whose text
  the `Real` cannot render back (`5.40`, `1e3`) is refused going in, where
  it was rounded to the nearest binary number with no loss declared.
  `fhirconnect` 0.1.173.
- The FHIRconnect engine converts `DV_QUANTITY` against `Quantity`, `decimal`
  and `integer`, and `DV_COUNT` against `integer`, `Quantity` and `Count`
  (#93), each a lens with GetPut and PutGet property tests over the tables of
  FHIRconnect `DV_QUANTITY.adoc` and `DV_COUNT.adoc`. Where a table is silent
  or contradicts R4, the cell decides and refuses rather than drops: the UCUM
  system is written beside a code (R4 `qty-3`), `units_display_name` is the
  counterpart of `unit`, and a `magnitude_status` no comparator states, a
  magnitude with more places than its `precision`, a count outside FHIR's
  32-bit `integer` and a comparator into a count are typed refusals.
  `FhirKind`, `FhirValue` and `RmValue` gain variants, so a consumer that
  matches them exhaustively adds the arms. `fhirconnect` 0.1.163.
- A FHIRconnect PROGRAMMED mapping now runs a registered function (#95).
  `fhirconnect::engine::library::Library` is the registry the crate ships,
  and the server hands it to the loader, the compiler and every run. A
  function declares the openEHR ports it reads and writes below the
  mapping's `with.openehr`, and the compiler resolves each port against the
  template at load: a port the template lacks is `fc-unknown-template-node`,
  one below a repeating node `fc-repeating-function-port`, and ports under a
  mapping with no openEHR side `fc-unanchored-function-port`. The first two
  functions of the published library are registered:
  `dosageDurationToAdministrationDuration` (`Timing.repeat.duration` and its
  unit against the administration duration) and `timingNonDaily` (a timing
  once per week, month or year against the interval of the non-daily
  timing), both converting through the engine's `DV_DURATION` cell and
  refusing what they cannot carry. The library's
  `timing_non_daily.yml` now loads in every published context. Breaking for
  consumers: `MappingFunctions` takes the FHIR element and the port values,
  `Method::Programmed` carries its ports, and `MappingCodeRegistry` gains
  `ports`. `fhirconnect` 0.1.166.
- Every published FHIRconnect context now has its operational template in
  the tree (#258). Nine KDS templates join the three already vendored, each
  fetched by its own `scripts/vendor/kds-*-opt.sh` from the openFHIR commit
  `docs/VERSIONS.md` pins, with a sha256 and a `PROVENANCE.md`: Fall einfach,
  Laborbericht, Medikationseintrag, Medikamentenverabreichungen, Person
  Pseudonymisiert, Prozedur, Studienteilnahme, Todesursache and Vitalstatus.
  The `fhirconnect` suite compiles all 13 contexts verbatim against their
  templates and pins each verdict. None compiles: ten stop at load on a
  reference the library cannot resolve, three are refused at compile, and the
  EHDS laboratory template the library ships does not parse. A context that
  compiles joins the mapping-library pass list on its own. The list stays at
  10 of 107.
- The FHIRconnect engine converts `DV_TIME` against `time` and `DV_DURATION`
  against `Duration` (#93). The duration cell runs the UCUM to ISO 8601
  bridge of FHIRconnect `DV_DURATION.adoc`, the eight
  `CommonUCUMCodesForDuration` rows as one committed table: a duration of
  several components reads in its smallest one by the factors UCUM fixes, a
  code outside the bridge, a comparator and a system other than UCUM are
  typed refusals, and `ms` is a one-way row. The time cell moves the text
  unchanged, gives the ISO 8601 basic form its separators, and refuses a time
  with an offset or with no seconds, which R4 `time` cannot state.
  `fhirconnect` 0.1.164.
- The FHIRconnect engine converts `DV_URI` against `url`, and `DV_MULTIMEDIA`
  and `DV_PARSABLE` against `Attachment` (#93), from FHIRconnect
  `DV_URI.adoc`, `DV_MULTIMEDIA.adoc` and `DV_PARSABLE.adoc`. A parsable
  value travels base64 encoded as `Attachment.data`, its formalism as the
  content type only when it is itself a media type (the page gives no table),
  and a multimedia `integrity_check` is `hash` only for SHA-1, the algorithm
  R4 names. A compressed multimedia value is refused going out, since an
  `Attachment` has no field for the algorithm, and an attachment with data and
  no size takes the decoded length, a one-way row. `fhirconnect` now depends
  on `base64`. `fhirconnect` 0.1.165.
- A FHIRconnect mapping writes a `Coding`, an `Identifier` or a `Reference`
  into an openEHR `String` attribute and reads it back (#93), the cells of
  FHIRconnect `String.adoc`: a coding travels as `system::code`, an
  identifier as `system::value`, and a reference as its identifier, or as its
  `reference` string coming in when it has none, a one-way row. Text holding
  `::` that would read back as another system and code is refused. Before, a
  tail on a `String` attribute against those elements refused with no cell.
  `fhirconnect` 0.1.166.
- The FHIRconnect engine converts `DV_INTERVAL<DV_QUANTITY>` against `Range`
  (#93), the `Range` table of FHIRconnect `DV_INTERVAL.adoc`, each bound
  through the `DV_QUANTITY` cell. An absent bound is the unbounded side, as
  the `Period` cell reads it, and an excluded bound or a bound with a
  comparator, which no `Range` states (R4 `sqty-1`), is refused. A node the
  Web Template types `DV_INTERVAL<DV_DATE_TIME>` or `DV_INTERVAL<DV_QUANTITY>`
  now reaches its cell and its pairs by the class `DV_INTERVAL`; before, the
  generic name matched no cell and a run over such a node refused.
  `fhirconnect` 0.1.167.
- The shipped `datatype-pl-to-location` supplement writes PL.11, the
  assigning authority, into the `identifier.assigner` of each location level
  the PL carries (#345). The guide's two PL.11 rows address all six levels
  with a `[1-6].` prefix the guide's notation does not define, so the
  interpreter still refuses them as `unsupported-target`; the supplement
  spells them as one row per level under that level's condition. The
  vendored `hl7v2` corpus loses 22 `unsupported-target` outcomes.
- `ferrobridge-hl7v2` ships message maps for 18 HL7 v2 structures the
  v2-to-FHIR guide has none for (#637): ADT_A15, ADT_A16, ADT_A21, ADT_A39,
  ADT_A44, ADT_A45, ADT_A60, BAR_P02, BAR_P12, DFT_P03, DFT_P11, MDM_T01,
  OMG_O19, OUL_R23, OUL_R24, PPR_PC1, RDE_O11 and REF_I12. Each takes the
  rows of a guide message map (or the shipped OUL_R22 map) for the segments
  it shares, at its own group paths, so the guide's segment maps apply, and
  its description names the map it composes. A supplement for ADT_A06 names
  the guide's `ADT_A01.PD1` row `ADT_A06.PD1`, so ADT_A06 messages map too.
  The `hl7v2` corpus gains the 56 messages, to 510 of 511.
- The HL7 v2 face reads a batch file (#642). A frame that opens with FHS or
  BHS is split by the HL7 v2 chapter 2 §2.10.3 structure into its batches
  and messages, each message is answered as if it had arrived alone, and
  the acknowledgments return in one response batch whose headers echo the
  file's FHS-11 and BHS-11 and whose BTS-1 counts them (§2.10.3.2). A
  trailer count the file contradicts is a logged warning, and a file whose
  segments stand outside the structure is not answered.
  `ferrobridge-hl7v2` gains the `batch` module. The ReportStream
  `sample-batch-pdi-20210608-0001.hl7` file now passes the `hl7v2` corpus.
- The HL7 v2 parser counts a valued repetition past a field's cardinality,
  a valued component past its data type's components and a valued
  subcomponent past its component's data type, as the new
  `extra-repetition`, `extra-component` and `extra-subcomponent` outcomes
  (#295). HL7 v2 chapter 2 §2.6 tells a receiver to ignore such
  constituents, so none refuses the message and each value stays in the
  parsed message. Components are checked against the v2.9.1 data types
  only, since the tables of earlier versions carry no component tables.
- A FHIRconnect mapping with no `type` onto an `ELEMENT` whose value admits
  several data types converts the `ELEMENT`'s own value, then runs its
  `followedBy` children (#544), as FHIRconnect `FollowedBy.adoc` orders them
  ("after the parent mapping method has been executed"). The compiler picks
  the value alternative whose class lists the FHIR type earliest in
  `resolve::derive::PAIRS`, the first in template order on a tie (our own
  design), and the program carries it as `Derived::Value`; such a mapping was
  an anchor that wrote nothing. The statutory insurance identifier of the
  library's `versicherungsinformationen.v0` now round-trips in the KDS person
  chain as a `DV_IDENTIFIER`, and the two `collected` mappings of
  `specimen.v1` no longer warn `fc-anchor-without-children`. `Derived` gains
  a variant, so a consumer that matches it exhaustively adds the arm.
  `fhirconnect` 0.1.158.
- `fhirconnect::operations::run::to_fhir` takes the id of the mapped
  resource from a caller-supplied identity seam when the seams carry one
  (#619): `Seams::with_resource_identity` over the new
  `engine::seam::ResourceIdentity` trait, which answers a
  `ResourceRequest` (the resource type, the template, the composition's
  `OBJECT_VERSION_ID` and the `id` the mapping wrote). A seam that refuses
  ends the run with `OperationError::Identity`, and an id outside the R4
  `id` grammar with `OperationError::InvalidId`. Without a seam the id still
  derives from the written `id` or the composition's `versioned_object_uid`.
  FHIRconnect `engine/id-management.adoc` has the engine maintain its own
  resource ids and keep an export's ids the same every time; the seam is
  where a caller's identity store does that. `OperationError` gains two
  variants, so a consumer that matches it exhaustively adds the arms.
  `fhirconnect` 0.1.156.
- `[auth] scopes_supported`: the scopes the bridge honours, listed as
  `scopes_supported` in the SMART discovery document, which "the server
  SHALL support" (SMART App Launch 2.2.0, Conformance, Metadata); each
  value is one scope token, checked at load (#1007).

### Changed

- The operator console moves to FerroUI 0.3.3 (`ferrohealth-ui = "=0.3.3"`,
  #1154) with the library's `split` feature on, since its site builds with
  `cargo leptos build --split`. The parts the 0.3.2 rewrite drew stand-ins
  for are now drawn as designed (#1146): the Bridge group of the navigation
  and the dashboard's flow and Upstreams card take the design's `flame`,
  `message-square`, `table-2` and `plug` glyphs; every state a table cell,
  a fact or a title shows is the F12 state mark; a context file's subtitle
  bolds its resource type and links its template. The Files tab's Language
  and Owner filters are the library's filter menu: a checkbox per choice
  with the number of files it keeps, sent with the find words, so
  `?owner=console&owner=provisioned` names several choices and an address
  with one value keeps working.
- The operator console moves to FerroUI 0.3.4 (`ferrohealth-ui = "=0.3.4"`,
  #1155), which carries the two parts the 0.3.3 move still drew stand-ins
  for. The dashboard's flow draws the design's `send` glyph for the HL7 v2
  senders and `book-open` for the terminology server, and its head offers
  How it is drawn, which opens the How the flow is drawn dialog: each state
  the flow draws, its line and what it means. The mapping editor and the
  playground's JSON input take keys from the first paint, before the page
  hydrates.
- The operator console moves to FerroUI 0.3.5 (`ferrohealth-ui = "=0.3.5"`,
  #1166). The console draws no part 0.3.5 changes, so its pages look and
  behave as before.
- The operator console moves to FerroUI 0.3.6 (`ferrohealth-ui = "=0.3.6"`,
  #1168). The console draws no activity chart, the one part whose interface
  0.3.6 changes, so its pages look and behave as before.

- Both container images name their manufacturer: the
  `org.opencontainers.image.vendor` label and index annotation of
  `ghcr.io/ferrohealth/ferrobridge` and `ghcr.io/ferrohealth/ferrobridge-console`
  read `Cadasto B.V.` where they read `FerroBRIDGE` (#1152).

- A release's notes end with its support period (#1149): the month and year
  until which Cadasto B.V. keeps publishing security fixes, five years from
  the release date (CRA Art 13(8) and (19)). The release lane refuses a cut
  whose `### Security` entries name no `GHSA-` advisory or `security/vex/`
  statement, or that mixes security and functional changes without saying
  why (CRA Annex I Part II(2)); `scripts/release/preflight.sh vX.Y.Z` runs
  the same checks before the tag. `SECURITY.md` adds `info@cadasto.com` as a
  reporting route and describes the advisory each fix is published with.
- The `ferrobridge` and `ferrobridge-console` images of a release are tagged
  only after a Trivy scan of each pushed digest finds no fixable HIGH or
  CRITICAL vulnerability (#1149), and a release carries
  `ferrobridge-vX.Y.Z-vulnerability-record.json`: every finding of a full
  scan of both images and the server tarballs, joined with the OpenVEX
  statement under `security/vex/` that judges it. The release is refused
  while a finding has none. `security/vex/rust-advisories.openvex.json`
  publishes the argument for each advisory `deny.toml` accepts, and the
  published images are scanned every week with those documents applied.
- **Breaking:** the operator console is renamed from `ferrobridge-viewer`
  to `ferrobridge-console` (#1133), as every FerroHEALTH product names its
  operator UI. The crate and its binary are `ferrobridge-console`
  (`app/ferrobridge-console`), the image is
  `ghcr.io/ferrohealth/ferrobridge-console`, built by the reusable
  `release-console.yml` from `docker/console/Dockerfile`, and the
  `ferrobridge-viewer` image is no longer built. Upgrade note: change the
  image reference to `ghcr.io/ferrohealth/ferrobridge-console`; rename the
  environment variables from `FERROBRIDGE_VIEWER__` to
  `FERROBRIDGE_CONSOLE__` and `FERROBRIDGE_VIEWER_CONFIG` to
  `FERROBRIDGE_CONSOLE_CONFIG`; in `compose.yaml` the profile and service
  `viewer` are `console` and the secret file
  `secrets/viewer_oidc_client_secret` is
  `secrets/console_oidc_client_secret`, mounted at
  `/run/secrets/console_oidc_client_secret`; the compose variables
  `FERROBRIDGE_VIEWER_PORT` and `FERROBRIDGE_VIEWER_OIDC_CLIENT_SECRET_FILE`
  are `FERROBRIDGE_CONSOLE_PORT` and
  `FERROBRIDGE_CONSOLE_OIDC_CLIENT_SECRET_FILE`. The session, sign-in and theme
  cookies are renamed from `ferrobridge_viewer_*` to
  `ferrobridge_console_*`, so every operator signs in again after the
  upgrade and the theme starts from the system's. The CI job and the build
  scripts follow (`console`, `scripts/release/console-site.sh`,
  `console-tools.sh`, `scripts/checks/console-bundle.sh`,
  `console-paths.sh`).

- **Breaking:** the operator console is drawn anew on FerroUI 0.3.2
  (`ferrohealth-ui = "=0.3.2"`, #1143, FerroHEALTH/FerroUI#161), page for
  page from the FerroBRIDGE design. It runs in Leptos islands mode: the
  server draws every page whole and the browser hydrates only the menus,
  the code editor and the JSON input, each island loaded lazily from its
  own chunk (the entry bundle measured 316603 bytes brotli-compressed). The
  pages are the dashboard with the flow, the Mapping set with its Contexts,
  OMOCL, Templates, Files and Diagnostics tabs, each mapping file with its
  Program tab, each template, the editor, the playground with its
  `$tofhir`, `$toopenehr` and `$validate` tabs, the FHIR facade, HL7 v2,
  OMOP ETL, Connections, Your preferences and About. Upgrade note: the
  addresses moved, and an old bookmark answers the page-not-found page:
  `/files` is `/mappings/files`, `/files/{path}` is
  `/mappings/files/{path}`, `/edit` is `/mappings/new`, `/edit/{path}` is
  `/mappings/edit/{path}`, `/validate` is `/playground/validate`, and
  `/capability` is `/facade/capability` and `/facade/security`.
- **Breaking:** the console registers no server function. Every action (a
  preference, a reload, a check, a save, a playground run) is a plain form
  post to its page's own address, which the server answers with the page it
  draws, so every action works before any WebAssembly arrives. Upgrade
  note: the console's `/api/…` server-function routes no longer exist;
  the console has no API, and a script reads the bridge's operator API.
- **Breaking:** the theme toggle is gone from the top bar. The theme is set
  in the user menu and in Your preferences alone. Your preferences has
  three tabs (Appearance, Language and region, Keyboard), and each choice is
  kept in a cookie of its own for a year: `ferrobridge_console_theme`,
  `_density`, `_motion`, `_language`, `_week-start` and `_hints` (under
  `__Host-` when the cookies are secure), with Reset to defaults clearing
  them. The sidebar folds to a rail, kept in `ferrobridge_console_sidebar`.
  Upgrade note: every operator chooses the theme again; nothing to
  configure.
- **Breaking:** a sign-out with no `end_session_endpoint` configured lands
  on the new signed-out page, `/signed-out`, instead of the sign-in. A
  sign-in the provider refused, could not complete, or a full session pool
  now draws the sign-in card with what happened and the way on, with the
  status `401`, `502` or `503`. Upgrade note: nothing to configure; a
  monitor that followed the sign-out to `/` follows it to `/signed-out`.
- The editor saves a draft with LF line ends: a text area posts its line
  breaks as CRLF (HTML Living Standard §4.10.11), and the console reads the
  draft back with the LF the editor shows, so the bytes saved are the bytes
  you edited. A `$toopenehr` answer in the playground shows the composition
  its `composition` parameter carries, with the issues of its `outcome`
  above it, and the template is a choice of the running set's templates.
  The Settings tabs and the Configuration page of the design wait for the
  operator API's configuration document (#1147); the design's navigation
  glyphs, its state words and a subtitle with markup wait for FerroUI
  (#1146).
- The workspace builds with Rust 1.99.1 in place of 1.98.1, and the MSRV
  stays at 1.98: the FerroHEALTH family builds with the latest stable
  toolchain and declares the release before it as its MSRV. Clippy 1.99's
  new `assert_is_empty` lint is answered in the tests it names, which moves
  `fhirconnect` to 0.1.179 with no API change, since its unit tests live
  under `src/`.

- Every word the operator console shows now comes from its catalog
  (#956): the faults and refusals it draws in a page, the severity of a
  diagnostic, and the plain-text answers of sign-in, sign-out and a
  request from outside its pages, which follow the request's
  `Accept-Language` like the pages do. English is the one catalog, so the
  words are unchanged, and a language joins as one catalog file. The
  stray-literal check in CI now holds the console to no string outside the
  catalog, with a reason recorded for each protocol token it allows.
- A reference the facade could not fetch now refuses the request with
  `502 Bad Gateway` when a mapping reads it (#94): a sending site that
  answers a status other than `404` or `410`, times out, or answers a body
  that is not the referenced resource, and a CDR read that fails, are
  upstream failures, which `engine/Fail.adoc` does not let a mapping drop.
  The single create and update, a transaction and `$validate` all answer
  it, and the `OperationOutcome` names the reference and the upstream
  status. A resource the sending site or this server does not hold stays a
  declared skip.
- The container lane runs the reference CDR at FerroEHR 5.1.0, which serves
  demographic AQL rooted at `PERSON`, `ORGANISATION` and `ROLE` (#965). The
  facade's party search runs `identifier`, `name`, `_id` and `_count` on
  `Patient`, `Practitioner`, `Organization` and `PractitionerRole` against it
  and finds the parties the test created, paged and with a deleted party
  left out. A search configured over `GROUP`, `AGENT` or
  `PARTY_RELATIONSHIP`, which the CDR refuses, answers `501 not-supported`
  naming the Cadasto experimental AQL demographic extension.
- The FHIRconnect engine's data-type cell dispatch is a module folder,
  `engine::cell` with the held-value readers in a child module (#1079),
  under the file-length threshold. Behaviour and public API are unchanged.
  It landed in `fhirconnect` 0.1.170 beside the display lookup (#95), with no
  version of its own.
- The operator console takes FerroUI 0.2.0 and reads every word it hands
  FerroUI from its catalog at run time (#956): table captions, column
  headers, pager nouns with their plurals, navigation, fields, dialogs and
  page titles come from `locales/en/ferrobridge-viewer.ftl`, and the
  stray-literal baseline drops from 384 to 78 literals. English remains the
  only locale.
- `openehr-mapping-core` 0.1.121 parses a mapping header's
  `spec.openEhrConfig.archetype` through the `openehr-base` BASE 1.2.0
  `ArchetypeId` and drops its own copy of the grammar (#1026). The accepted
  form is unchanged: FHIRconnect v1.0.0 writes the header's archetype as
  `openEHR-EHR-ACTION.informed_consent.v0` and never names a namespaced
  ADL 2 id, so the namespaced form and a full version stay refused (our own
  design). Breaking for a consumer that matches on `ArchetypeIdError`: its
  `Shape`, `QualifiedRmEntity`, `DomainConcept` and `VersionId` variants are
  gone, and a refusal is now `Lexical`, carrying the `openehr-base`
  `IdError` as its source, or `VersionRange` for a version past `u32`.
  `ArchetypeId::base` returns the `openehr-base` value. `fhirconnect`
  0.1.163 renders the new requirement.
- **Breaking:** the facade, the operator API and the CDR and terminology
  lanes authenticate through `ferrohealth-smart` 0.1.0, the FerroHEALTH
  family's SMART and OAuth 2.0 resource-server crate (#981). Its sections
  replace the bridge's own, so a deployment changes these keys:
  - `[auth]` is on by default (`enabled = true`). `serve` refuses to start a
    facade, an operator API or a FHIRconnect operations lane until `[auth]`
    names `issuer` and `audience`, or until `enabled = false`
    (`FERROBRIDGE__AUTH__ENABLED=false`) serves them open. A server that
    serves only `/` and the health routes, and the jobs, read no `[auth]`.
  - `auth.audience` is a list (`audience = ["https://…/fhir"]`, and
    `FERROBRIDGE__AUTH__AUDIENCE='["https://…/fhir"]'`); a token's `aud`
    must contain one of them.
  - `auth.jwks_refresh_interval_ms` becomes `auth.jwks_refresh_interval_seconds`
    (default `60`), and `auth.jwks_max_age_seconds` (default `600`) is new:
    a key set older than that is read again before a token is checked, so a
    key the issuer withdrew stops verifying (#972).
  - `auth.algorithms` defaults to `RS256`, `RS384`, `PS256`, `PS384`,
    `ES256` and `ES384`, where it was `RS256` and `ES256`.
  - `auth.issuer` may be plain `http` on a loopback host; elsewhere it stays
    `https`.
  - `auth.operator_scope` becomes `auth.product_scope`, and its default
    `ferrobridge.operator` becomes `__ferrobridge.operator`, the extension
    form SMART App Launch 2.2.0 gives a scope (a string starting with `__`
    or a full URI); any other value refuses the start. Grant the new scope
    in the authorization server before you upgrade.
  - New in `[auth]`: `[[auth.issuer]]` tables for several trusted issuers,
    `[[auth.basic.users]]` for HTTP Basic callers with an Argon2id
    `password_hash` (or `password_hash_file`) and their `scopes`,
    `jwks_file`, and `realm`.
  - `[cdr.credentials]` and `[terminology.credentials]` lose `user_file`;
    the user-id is read inline from `user`.
  - `[cdr.credentials.oauth]` and `[terminology.credentials.oauth]` take
    `client_auth = "private_key_jwt"` with `private_key_file`, `kid` and
    `assertion_alg`, and `timeout_ms` (default `10000`), which bounds a call
    to the token endpoint in place of the lane's own timeout; an empty
    `scope` sends no `scope` parameter.

  The facade's refusals keep their `OperationOutcome` bodies and challenges;
  two `Authorization` values, or a value that is no credential (a scheme
  with nothing after it), answer `400` with `invalid_request` (RFC 6750
  §3.1), and a route the guard does not classify answers its `403` without
  a challenge. The discovery
  document no longer claims `permission-v2`, which SMART App Launch 2.2.0
  defines as granular scope support the facade does not evaluate. The audit
  line moves to the `ferrohealth_smart::audit` target with the family's
  field set: `principal` replaces `subject`, `target` (`Condition/c-1`)
  replaces `resource_type` and `resource_id`, and `via`, `issuer` and
  `route` are new. The operator API's request line moves there too; a save
  or a reload still writes its change record on `ferrobridge::audit`.
  `src/auth/` and `src/cdr/oauth/` are gone from the server (#977), and
  `ferrobridge_server::credentials::UpstreamToken` hands the crate's token to
  both upstream clients.
- The HL7 v2 corpora count a message the face refuses correctly as a pass
  (#639). `crates/ferrobridge-hl7v2/tests/it/corpus/expected-refusals.txt`
  lists 167 messages with the acknowledgment code and the HL7 table 0357
  error HL7 v2 chapter 2 owes them: 152 ReportStream mapping-inventory
  fragments, 6 FHIR-Converter samples that break their declared structure,
  4 AIRA examples built to omit the RXA, and 5 messages whose bytes leave
  the declared character set or whose MSH-18 names no table 0211 code. A
  listed case fails when the face takes the message or answers it
  otherwise. The `hl7v2` badge moves from 290 to 453 of 511 and the
  `hl7v2-smoke` badge from 375 to 379 of 379.
- The release workflow refuses a tag whose milestone, titled exactly as the
  tag, is missing or still holds an open issue or pull request, and names
  each one; once every leg has succeeded, crates.io included, it closes that
  milestone (#983).
- The `openehr-*` crates move from 0.0.87 to 0.0.89 (`openehr-base`,
  `openehr-rm`, `openehr-its`, `openehr-sdt`, `openehr-am`, `openehr-query`
  and `openehr-adl`). 0.0.89 adds the `openehr-sdt` party builder, the
  builder option that names `ARCHETYPED.rm_version`, and the BASE 1.3.0
  `ARCHETYPE_ID` grammar in `openehr_base::v1_3`, which now admits the
  namespaced ADL 2 form and a full version. `openehr-mapping-core` 0.1.119
  and `fhirconnect` 0.1.160 render the new requirements.
- A composition the bridge builds names each structure the FLAT form leaves
  out (an `ITEM_TREE` under an entry, for example) by its term definition in
  the template, where it named it by its node id before (RM Common IM,
  `LOCATABLE.name`). The `openehr-sdt` 0.0.89 builder does this.
- `ferrohealth-smart` 0.4.0 (#1110). Every `[<lane>.credentials.oauth]`
  table takes the keys the crate adds: `client_auth = "tls_client_auth"` or
  `"self_signed_tls_client_auth"` with `client_certificate_file` and
  `client_key_file` for RFC 8705 §2 mutual TLS to the token endpoint,
  `ca_file` for trust roots added to the built-in ones, and
  `assertion_audience = "issuer"` for a `private_key_jwt` assertion that
  names the issuer (RFC 7523 §3) in place of the token endpoint.
- `ferrohealth-smart` 0.3.1 and the `openehr-*` crates 0.0.90 (#1062),
  which `ferrohealth-smart` 0.3.1 pins; `fhirconnect` 0.1.168 and
  `openehr-mapping-core` 0.1.124 build on them. The facade and the operator
  API refuse a token whose `sub` is empty or whitespace only (RFC 9068
  §2.2, RFC 7519 §4.1.2) and a bearer value outside the RFC 6750 §2.1 `b64token` syntax,
  each with the generic `401 invalid_token`. An enabled `[auth]` section
  with Basic users and no issuer is refused at load: the discovery document
  and the `CapabilityStatement` name the issuer's token endpoint.
- `ferrohealth-smart` 0.2.0 (#1018). The SMART discovery document writes
  `issuer` and `sso-openid-connect` only when `[auth] capabilities` names
  `sso-openid-connect`; SMART App Launch 2.2.0 (Conformance, Metadata) omits
  `issuer` otherwise. A client that read `issuer` from the document reads the
  authorization endpoints it names, or the bridge advertises OpenID Connect
  sign-in by configuration. The document declares `permission-v1`,
  `permission-patient` and `permission-user`, the scopes the route table
  and the patient compartment enforce.
- `ferrohealth-smart` 0.1.1 (#1007). The operator scope moves from
  `[operator] scope` to `[auth] product_scope` (environment
  `FERROBRIDGE__AUTH__PRODUCT_SCOPE`), default `__ferrobridge.operator`,
  checked by the crate in the same two SMART extension forms. The audit
  line's field for what a request addressed is `addressed`, where it was
  `target`, so a JSON log keeps it beside the event's own `target`; a log
  query that read `target` for the addressed resource reads `addressed`.
  `[auth] capabilities` refuses at load a value SMART App Launch 2.2.0
  does not define unless it is a full URI. The issuer and resource checks
  of `[<lane>.credentials.oauth]` and the Basic password read are the
  crate's now, with the same refusals at load.
- `serve` warns that `[auth]` is off only when a surface it guards runs,
  and names those surfaces (the facade, the operator API, the operations
  lane); a server with only `/` and the health routes logs nothing
  (#1005).
- `openehr-mapping-core` 0.1.120 passes the RM release it chooses to the
  `openehr-sdt` composition builder (#901), so the builder writes
  `ARCHETYPED.rm_version` and the walk that rewrote it afterwards is gone.
  The choice is unchanged: Release 1.1.0, unless the composition holds a
  `GENERIC_ENTRY` whose `data` is no `ITEM_TREE`.
- `openehr-mapping-core` 0.1.120 builds a party through the `openehr-sdt`
  template walk (#906) and drops its own party assembly with the OPT 1.4
  structure index behind it. `ROLE.performer`, which no template node holds,
  is still placed on the root after the walk (#1000). Breaking for a
  consumer that matches on `PartyError`: `NoStructure`, `Unconstrained` and
  `InstanceGap` are gone, and a value the builder refuses is the new
  `PartyError::Build`, which carries the `openehr-sdt` `FlatError` as its
  source. `fhirconnect` 0.1.161 renders the new requirement.

### Fixed

- A FHIRconnect untyped choice element that admits both `Period` and
  `Range`, mapped onto a `DV_INTERVAL<DV_QUANTITY>` node, is written as
  `Range` (#1080); it picked `Period` and the run refused with no cell.
  `resolve::derive::PAIRS` keys the interval rows by parameter
  (`DV_INTERVAL<DV_DATE_TIME>` with `Period`, `DV_INTERVAL<DV_QUANTITY>` with
  `Range`), an interval of any other parameter pairs with nothing and is
  refused at load, and the cells refuse a `Period` into a quantity interval
  and a `Range` into a date-time one. `PAIRS` gains two rows, so a consumer
  that reads the table sees them. `fhirconnect` 0.1.177.
- A facade write, transaction or `$validate` whose mapping run fails because
  the terminology server failed (no answer, a timeout, a `5xx`, a refused
  `$translate` or `$lookup`, an answer that does not decode) now answers
  `502` with `transient` and the upstream status in the diagnostics (#1094).
  It answered `422` with `processing`, as if the resource were wrong. A
  translation the server answers with no match stays the mapping's `422`.
- A FHIRconnect `mappingCode` mapping, a `manual` mapping and a
  `hierarchy.split` inside a `reference` now run when their FHIR path climbs
  out of the referenced resource with `^` (#1095). They read and wrote the
  referenced resource and refused with an unknown element; they now read
  and write the resource the path climbs to, as every other mapping has
  since the worked `^^.use.coding` example ran. `fhirconnect` 0.1.173.
- A value node of a polymorphic `ELEMENT` reads only a value of its own class
  (#1100). `WebTemplateIndex::read` let every alternative read the element's
  one value, so a `DV_INTERVAL` was also read at the `DV_QUANTITY` node. The
  `_type` of the value now picks the alternative, the most specific class it
  conforms to, and `WebTemplateIndex::alternatives` and
  `WebTemplateIndex::holds` are new. A value with no `_type` at a polymorphic
  `ELEMENT` is `PathError::UntypedAlternative`, a new variant. In
  `fhirconnect`, a mapping whose type the compiler matched to one alternative
  refuses an `ELEMENT` that holds another, and an instance that holds another
  alternative still counts when the engine probes occurrences.
  `openehr-mapping-core` 0.1.125, `fhirconnect` 0.1.178.
- A FHIRconnect `DV_INTERVAL<DV_DATE_TIME>` whose present bound is excluded
  is refused going out to `Period` (#1076), because both R4 `Period` bounds
  are inclusive; it was written as an inclusive bound with no loss declared.
  The refusal is a `LensError::Shape` naming `DV_INTERVAL.lower_included` or
  `DV_INTERVAL.upper_included`. `fhirconnect` 0.1.169.
- A `^` run that climbs out of a referenced resource now runs in both
  directions (#94). The compiler accepted the worked `^^.use.coding`
  example of `basics/path_operators.adoc`, and the engine then refused it,
  reading the Encounter's path in the referenced Condition. Each FHIR
  target now records the resource it is rooted in, the engine keeps the
  document a `reference` mapping left with the occurrence it was entered
  at, and a condition `targetRoot` and the children of such a mapping
  anchor in that resource too. `fhirconnect` 0.1.170.
- A FHIRconnect tail below a `DV_INTERVAL<T>` node, such as `/lower` below
  `DV_INTERVAL<DV_QUANTITY>`, compiles and runs (#1077): the tail is looked
  up on the interval class and a bound is read as the node's parameter, so
  `/lower` converts through the `DV_QUANTITY` cell. It was refused at load
  with `fc-uncarried-tail`, and a tail the parameter does not declare is
  still refused. `fhirconnect` 0.1.172.
- The console's file editor names its mode in its header (#982): the New
  file route reads "New mapping file" and an existing file reads "Edit
  <file>", with the matching page title.
- An untyped FHIR choice element mapped onto an `ELEMENT` whose value admits
  several data types converts (#1055), the shape the published
  `laboratory_test_analyte.v1.yml` and `vital_status.v1.yml` write for
  `Observation.value`. The compiler keeps every pair of a choice alternative
  with a value alternative the data-type chapter defines
  (`Derived::Polymorphic`); going into openEHR the alternative the document
  carries selects the value node, and coming out the class the composition
  holds selects the alternative written. An alternative or a class no pair
  names refuses the run. Before, the mapping derived nothing and drew an
  `fc-anchor-without-children` warning. `Derived` gains a variant, so a
  consumer that matches it exhaustively adds the arm. `fhirconnect` 0.1.168.
- An HL7 v2 message sent under a code and trigger event that no v2.9.1
  message definition carries, and whose MSH-9.3 names no known structure, is
  parsed against the structure the interactions of its version's tables send
  under that event (#1016): `REF^I14^REF_I14` at 2.8 takes `REF_I12`, counted
  as `other-structure`, where it was refused with AR. The HL7/v2ig
  definitions carry no REF or RRI message, so `fhir-codegen` reads the
  `interactions.json` table of the IGAMT export and `hl7v2-types` gains
  `message::LEGACY_EVENTS` and `message::legacy_events`, every other code and
  event a legacy version's interactions send one of its structures under.
  FHIR-Converter's `REF-I14-01` now maps.
- A FHIRconnect model mapping that names an archetype node the operational
  template excludes now compiles: the method is left out of the program with
  everything under it and an `fc-excluded-template-node` warning, where it
  was refused with `fc-unknown-template-node` (#1040). FHIRconnect
  `model-mappings.adoc` §Mapping 0..0 fields says such methods "are ignored
  and not executed". The node counts as excluded only when the archetype
  root of an OPT 1.4 defines its at-code and the template carries no node
  for it at that place; a code the archetype never defined, a name the
  template does not carry, and a method an extension contributes are still
  refused. `WebTemplateIndex::archetype_defines` is the new
  `openehr-mapping-core` call behind it.
- A FHIRconnect mapping onto `$composition/context/end_time` now compiles and
  round-trips (#1043). The engine writes `EVENT_CONTEXT.end_time` as the
  `_end_time` family of the `context` node, as Simplified Formats master05
  §EVENT_CONTEXT spells it, and reads it back from the context, where it was
  refused with `fc-uncarried-tail`.
- A FHIRconnect path that names an `ELEMENT` the Web Template compacts into
  its value now reaches the element's `links` (#1042), as
  `FHIRReference.adoc` §Dedicated Node writes `$openehrRoot/links` below an
  element. A `link` there writes the element's `_link:i` family and reads it
  back from the element, and a condition on `links` reads the element's list;
  the resolver used to walk `links` on the value's class and refuse it.
  `openehr-mapping-core` gains `WebTemplateIndex::read_element` and
  `PathError::NotAnElementValue`.
- Two sibling FHIRconnect mapping methods that share a name now compile with
  an `fc-duplicate-method-name` warning, and the refusal moves to the
  `overwrite` or `appendTo` that names the shared name (#1041). FHIRconnect
  `basics/body.adoc` gives a mapping name no role outside those references.
- The HL7 v2 interpreter removes every reference a kept resource holds to a
  resource it left out of the Bundle and counts each as a
  `dangling-reference` outcome naming the element and the resource left out
  (#313). FHIR R4 resolves a Bundle reference against the entries'
  `fullUrl`, so such a reference pointed at nothing. The referring resource
  stays unless the removal leaves it without a required element, and the
  corpus test asserts no Bundle keeps one. The vendored `hl7v2` corpus
  carries 29 of them in 27 messages.
- A Bundle entry member the HL7 v2 interpreter cannot write is a counted
  `entry-unwritable` outcome naming the resource and the path, where it was
  only a warning in the log (#316).
- The `defining_code` of a `DV_CODED_TEXT` value read out to a FHIR `string`
  is a declared loss (#1011). The chapter writes no `string` row for
  `DV_CODED_TEXT` (FHIRconnect `DV_CODED_TEXT.adoc` §STRING calls the pairing
  not possible), so the value converts as `DV_TEXT` and its code was dropped
  with no trace; the run now reports it as a `Loss::Skipped` with the new
  `SkipReason::DefiningCodeUncarried`, located at the element and the value's
  `defining_code`. A consumer that matches `SkipReason` exhaustively adds the
  arm. The facade's loss log labels it `skipped-defining-code-uncarried`.
  `fhirconnect` 0.1.160.
- `$toopenehr` resolves a `reference` mapping against the request Bundle by
  the R4 rules for a reference inside a Bundle (R4 bundle.html §2.36.4.1),
  the same resolver the subject check runs (#1010): a version-specific
  reference matches the entry's `meta.versionId`, and a relative `<type>/<id>`
  is read against the server base of the mapped entry's `fullUrl`. Before, the
  seam matched only an exact `fullUrl` or `<type>/<id>`, so a versioned
  reference fell through to the further source. A reference two entries
  answer refuses the run with the new `ReferenceError::Ambiguous`, and
  `BundleReferences::with_referrer` names the entry whose references it
  resolves. A consumer that matches `ReferenceError` exhaustively adds the
  arm. `fhirconnect` 0.1.161.
- The FHIRconnect operations answer a refusing identity seam and an id it
  answers outside the R4 `id` grammar as the server failures they are (#1012):
  `500` with the issue code `exception` for `OperationError::Identity` and
  `500` with `value` for `OperationError::InvalidId`, where both were a `400`
  with `processing`. `OperationError::issue_type` changes for the two
  variants, and the server's status table names every variant, so a new one
  is a compile error until it is placed. `fhirconnect` 0.1.162.
- A `TERM_MAPPING` that a `DV_TEXT` or `DV_CODED_TEXT` value carries out to a
  FHIR `string`, or a `DV_CODED_TEXT` value carries out to a single `Coding`,
  is a declared loss (#623). Those pairs give `mappings` no FHIR row
  (FHIRconnect `DV_TEXT.adoc` §String, `DV_CODED_TEXT.adoc` §Coding), so the
  engine writes none of them, as before, and now reports each one as a
  `Loss::Skipped` with the new `SkipReason::TermMappingUncarried`, located at
  the element and the value's `mappings`; `term_mapping::Withheld` gains
  `cause()`. A consumer that matches `SkipReason` exhaustively adds the arm.
  The facade's loss log labels it `skipped-term-mapping-uncarried`.
- `SkipReason::EmptyReference` documents what the engine does (#511): a
  `Reference` with no literal reference is skipped whatever else it carries,
  because a `display` or an `identifier` alone names no resource to fetch
  (R4 references.html §2.3.0.3 and §2.3.0.4). `fhirconnect` 0.1.159.
- The HL7 v2 parser judges a segment group by placing every following
  segment with the same choice rule it applies to the segment itself, so a
  repeated `ORDER`'s ORC in an OML^O21 opens a new `ORDER` instead of an
  optional-led `PRIOR_RESULT.ORDER_PRIOR` it cannot complete (#638). The six
  NIST LOI OML_O21 messages refused for it now parse and map. The nested
  trial walks share a budget of 4096 placements per segment, our own bound
  on the work an ambiguous message can cost.
- The HL7 v2 face decodes a message whose MSH-18 repeats with its first
  repetition, the default character set by HL7 v2.9.1 chapter 2 (#641). It
  still refuses one whose MSH-20 names a switching scheme from table 0356.
  In `ferrobridge-hl7v2`, `DecodeError::AlternateCharsets` is replaced by
  `DecodeError::SwitchingScheme { scheme }`; a caller matching the old variant
  matches the new one.
- `$toopenehr` compares the subjects of a Bundle as the entries their
  references resolve to (#399), by the R4 rules for resolving a reference in
  a Bundle (bundle.html §2.36.4.1): an absolute reference (`urn:uuid`,
  `urn:oid`, a URL) matches an entry's `fullUrl`, a relative `<type>/<id>`
  is made absolute against the root of the referring entry's RESTful
  `fullUrl`, and a version-specific reference also matches
  `meta.versionId`. A document that addresses its Patient entry by a
  `urn:uuid` `fullUrl`, as both HL7 Europe EPS example documents do, now
  maps; references that resolve to two Patient entries are still refused. A
  relative reference in an entry with no RESTful `fullUrl`, which R4 gives no
  meaning, is read against each entry's type and id (our own design).
  `fhirconnect` 0.1.153.
- `scripts/gh/watch-prs.sh` no longer writes a broken line when the default
  branch and a pull request both changed the same version line (#1022). The
  merge read its second capture after a nested match had overwritten it, so
  `fhirconnect = { … version = "0.1.157" }` became `0.1.157` glued to the
  next line. It now keeps the higher version at each position, leaves any
  hunk that differs beyond its versions to stop the merge, and refuses a
  manifest `cargo metadata` cannot read.

### Security

Shipped with functional changes because: only the newest release is supported (SECURITY.md), so a separate 0.0.10 patch published beside 0.0.11 would be out of support the moment 0.0.11 lands and give no one a release to stay on; each fix is a dependency or base-image bump with no behaviour change of its own.

- `ctutils` moves from 0.4.2 to 0.4.5 (RUSTSEC-2026-0385,
  GHSA-qq87-xvj3-pfgj): before 0.4.4 its constant-time mask generation could
  compile to a branch on a secret value. It reaches the build through
  `digest` 0.11.
- The TLS stack the bridge calls the CDR, the terminology server and the
  sending sites through no longer trusts the distrusted emSign
  intermediates: `rustls-webpki` 0.103.16 rejects their public keys
  (RUSTSEC-2026-0382), and `webpki-roots` and `webpki-root-certs` 1.0.10
  drop the root that allowed them (RUSTSEC-2026-0381, RUSTSEC-2026-0383,
  GHSA-6q27-c9m3-vj9m).
- The bridge and operator console images no longer trust the compromised
  eMudhra (emSign) intermediates in the system trust store (Debian
  DSA-6572-1, GHSA-6q27-c9m3-vj9m, #1140). The distroless base
  `gcr.io/distroless/static-debian13:nonroot` moves to index digest
  `d620678`, which ships `ca-certificates` 20250419+deb13u1, in the `FROM`
  of `docker/Dockerfile` and `docker/console/Dockerfile` and in the pin row.

## [0.0.10] - 2026-10-19

The openEHR demographics release. The FHIR facade serves `Patient`,
`Practitioner`, `PractitionerRole` and `Organization` from FHIRconnect party
contexts over the ITS-REST 1.1.0 Demographic API, searches them through the
Cadasto experimental AQL demographic extension, chains clinical searches
through the patient identifier, and gives a created `Patient` its EHR. The
terminology lane authenticates with OAuth 2.0 client credentials, and the
static bearer token is gone from both upstreams.

### Added

- The terminology lane authenticates with the OAuth 2.0 client credentials
  grant (#962): `[terminology.credentials.oauth]` takes the keys of
  `[cdr.credentials.oauth]`, and the same token cache obtains the token,
  shares it across calls, renews it before it expires, and renews it once
  when the terminology server answers `401`. A renewal that obtains no token
  ends the call with the terminology server's refusal and the token
  endpoint's error. `ferrobridge-term` 0.1.129 adds
  `config::Credentials::OAuth` over a caller-supplied `config::TokenProvider`,
  and the `error::Error::Token` and `error::Error::Renewal` variants.
  `ferrobridge-validator` moves to 0.1.129 with it.

- The party search takes `name`, `_id` and `_count=0` beside `identifier`
  (#931). `name` matches a name that starts with the value, case-insensitively
  as R4 string search asks: `[facade.party_search.types.<Type>] name_query`
  binds the value as typed, in lower case and capitalised, because the
  extension's `LIKE` is case-sensitive; other case forms and accents are a
  stated gap. `_id` reads the one party it names. `_count=0` answers the
  number of matches in `total` with no entries (R4 search §3.1.1.5.3), and
  `_count` is capped at `search_count_max`. A bare identifier value two
  parties carry answers both. A type's `query` is now optional, and the
  `CapabilityStatement` declares exactly the parameters each type takes.
- A `Patient` created in a party context gets its EHR under the subject link
  when `[facade] ehr_policy` is `create_on_first_write` (#932): the facade
  finds the EHR keyed by the new party's id, or creates it with that id in
  `EHR_STATUS.subject`, so the clinical resources that name the `Patient` find
  it. Under `existing` the create sends no EHR call. A failed EHR call after
  the party committed answers the EHR refusal with a warning naming the stored
  `Patient`. No specification governs this: our own design.
- `fhir-types` 0.1.127: every `schema::FieldSchema` carries
  `target_profiles`, the `ElementDefinition.type.targetProfile` canonicals of
  the element's types in definition order, in all five versions (#926), so a
  consumer reads which resource types a `Reference` admits from the element
  table. A consumer that builds a `FieldSchema` literal adds the field.
- The facade's subject search parameters take the `:identifier` modifier
  (#923): `Condition?patient:identifier=[system]|[value]` matches the
  `Reference.identifier` of a resource's subject (R4 search.html
  §3.1.1.4.12). That identifier is the subject the identity map keys by
  system and value, which the facade renders as an identifier. A subject
  rendered as a literal reference carries none and matches nothing, and so
  does `|[value]`. `[value]` alone and `[system]|` answer `400 not-supported`,
  because the map keys a subject by system and value. No specification
  governs the identity map: our own design.
- The facade searches `Patient`, `Practitioner`, `PractitionerRole` and
  `Organization` by `identifier` through the Cadasto experimental AQL
  demographic extension, behind `[facade.party_search]` with one configured
  query per type (#480). Each page runs the query with `offset` and `fetch`
  and reads every party it names over the Demographic API. A CDR that answers
  `400` to the query, as one without the extension does, answers `501` with
  `not-supported` naming the extension. The `CapabilityStatement` declares
  `search-type` and `identifier` on exactly the configured types, and a
  configured type no party context serves refuses to boot.
  `docs/architecture.md` §4.12 writes down the dialect's semantics as the
  extension's guide documents them and Cadasto observed them, with the
  findings Cadasto plans to change. No specification governs the extension:
  our own design.
- The FHIR facade serves `Patient`, `Practitioner`, `PractitionerRole` and
  `Organization` from party contexts over the ITS-REST 1.1.0 Demographic API,
  DEVELOPMENT, behind `[cdr] demographic` (#470). Read, vread,
  `GET [type]/[id]/_history`, create, update (over `If-Match` or the latest
  version) and `DELETE [type]/[id]`, which answer `not-supported` for a type no
  party context serves; a party context with the key off is a boot refusal. The
  id is the party's `versioned_object_uid` and `meta.versionId` its version
  tree id, with no identity-map entry; a Demographic API `400` answers `500`,
  and `404`, `409`, `412` and `422` answer as themselves. The
  `CapabilityStatement` declares the six interactions and no search per party
  type. No specification governs a party context: our own design.
- A party context maps `ROLE.performer`, the mandatory `PARTY_REF` no template
  node holds (RM Demographic IM, the `ROLE` class), as `$archetype/performer`,
  so the facade creates and updates a `PractitionerRole` (#911).
  `PractitionerRole.practitioner` must name a `Practitioner` the facade
  serves, relative or under its base, with nothing beside `reference`. It
  becomes `PARTY_REF {namespace "demographic", type "ACTOR", id HIER_OBJECT_ID
  <uid>}` and reads back as `Practitioner/<uid>`, the type taken from the
  element's `targetProfile`. A reference of another server
  (`EngineError::ForeignParty`), of another type, carrying a display, or local
  to the request answers `422`, and a `ROLE` with no `performer`, built or
  served, is refused (`PartyError::RoleWithoutPerformer`). An element whose
  reference admits no single type is refused at load
  (`fc-party-ref-target`). `fhirconnect` 0.1.151 adds `RmValue::PartyRef`,
  the `PARTY_REF`-against-`Reference` pair, `EngineError::ForeignParty` and
  `EngineError::PartyRefTarget`, which breaks an exhaustive match over
  either enum; `openehr-mapping-core` 0.1.118 adds
  `PartyError::RoleWithoutPerformer` the same way. No specification governs
  this correspondence: our own design.

- The facade's search chains through the patient identifier (#492):
  `Observation?patient.identifier=MRN-1`, `subject.identifier` and
  `subject:Patient.identifier`, on every type a search declaration answers.
  With `[facade.identifier_search] enabled`, a configured demographic AQL
  query in Cadasto's experimental extension resolves the identifier to its
  parties, each party's uid names a patient, and the EHR comes from the
  identity map or `GET /ehr?subject_id=`. An unknown identifier answers an
  empty searchset with no clinical query, and `_count=0` answers the total
  alone. A value naming a system needs `system_query`. The
  `CapabilityStatement` documents the chain on each subject parameter while
  it is on. No specification governs the resolution: our own design.
- A party program runs through the engine (#905). `WebTemplateIndex::
  build_party` builds a canonical `PERSON`, `ORGANISATION` or `ROLE` from the
  node values a party context writes, through the `openehr-rm` types: the
  objects between two template nodes are the ones the OPT 1.4 constrains
  there, named by their rubric, every list attribute is an array, no
  composition attribute is written, and the result decodes strictly as the
  root's class (`PartyError::Invalid` otherwise). `openehr-sdt` 0.0.87 builds
  no correct party (#897, FerroHEALTH/FerroEHR#3810). `fhirconnect`'s
  `engine::traverse::party::to_party` and `from_party` map a FHIR resource to a
  party and back; `WebTemplateIndex::read` reads any `CanonicalDocument`. No
  specification governs a party build: our own design.
- FHIR `Organization` round-trips through the KDS laboratory request chain
  (#543). The published `KDS_Laborauftrag` template is vendored by
  `scripts/vendor/kds-laborauftrag-opt.sh` at the openFHIR commit
  `docs/VERSIONS.md` pins, and a FerroBRIDGE project context compiles the
  library's `organisation.v1.yml` verbatim, reached through the service
  request's `reference` to its requester: a contained `Organization` maps into
  the `Einsender` cluster and comes back as a created `Organization` the
  request references, and both lens laws hold. `organisation.v1.yml` joins the
  mapping-library pass list and `kds_laborauftrag` the round-trip list; every
  published file of the chain that does not compile is pinned by file and
  line.
- FHIRconnect party contexts compile (#898). A context whose operational
  template is rooted at a `PERSON`, `ORGANISATION` or `ROLE` is a party
  context, read from the RM type of the template root, and `Program::party`
  names the class. A party context refuses at load, each with a code of its
  own: `$composition` (`fc-party-composition-variable`), a path naming a
  composition attribute the defaults or `ctx/` set
  (`fc-party-composition-field`), `participationsFunction`
  (`fc-party-participation`), `link` (`fc-party-link`) and entering a context
  whose `start` slots a composition (`fc-party-sub-context-composition`).
  `to_openehr` and `to_fhir` refuse a party program with
  `EngineError::PartyProgram` until the party build lands (#897). No
  specification governs a party context: our own design. Breaking for a
  consumer that matches `ResolveCode` or `EngineError` exhaustively: add the
  new arms.

### Changed

- Only the newest release is supported (#979). `SECURITY.md` says that every
  earlier release stops receiving fixes when a new one is published, with no
  maintenance branches, no long-term-support line and no backports, and that
  upgrading is free. Cadasto B.V. commits to keep publishing security fixes
  for at least five years after each release (Regulation (EU) 2024/2847
  Art 13(8)) and ships them only in the newest release (Art 13(10)).

- A CDR that refuses the chained `patient.identifier` query of the Cadasto
  experimental AQL demographic extension now answers `501` with
  `not-supported`, the answer the party search gives, where it answered
  `500` (#954). Both paths go through one status function, naming the
  extension and carrying the CDR's words.
- **Breaking for a library consumer** of `fhirconnect` 0.1.148:
  `fhirconnect::resolve::program::binding::PartyClass` is gone, and
  `Program::party` answers `openehr_mapping_core::party::PartyClass`; a
  consumer imports it from there. `EngineError` gains `CompositionProgram` and
  `PartyBuild`, and a consumer that matches it exhaustively adds the arms.
- The operator console draws with `ferrohealth-ui` 0.1.4 (#904). The
  dashboard's build instant reads on one line as the library draws every
  date-time, in UTC and cut to the second, with the value as the bridge
  sent it in its title. On a phone the dashboard's tiles are compact
  one-line tiles, and a disabled field keeps its edge in the dark theme.
- The operator console draws with `ferrohealth-ui` 0.1.5 (#924), the
  FerroUI release every FerroHEALTH console moves to together. The console
  looks the same: 0.1.5 adds a per-row form field and a header title it
  does not use yet, and its browser half no longer asks for a wasm-bindgen
  of its own.
- The operator console draws with `ferrohealth-ui` 0.1.6 (#948), the
  FerroUI release every FerroHEALTH console moves to together. The console
  looks the same: 0.1.6 adds the shared field shapes the other consoles
  needed, and the console's fields already draw through the library.
- The operator console draws with `ferrohealth-ui` 0.1.7 and is ready for
  more languages, English the one that ships (#956). The library's words and
  the console's come from Fluent catalogs, the console's under
  `app/ferrobridge-viewer/locales/`, so a language joins as a catalog file
  with no code change. The locale is chosen once per request, the best
  language of `Accept-Language` the console's catalog holds or else the new
  `[server] locale` (default `en`), and the page writes it as `<html lang>`.
  `scripts/checks/stray-literals.sh` fails CI on a console string outside
  the catalog beyond the per-file baseline the remaining strings hold while
  they move.
- The README, the landing page and the build-order page name v0.0.9 as the
  latest release, and `scripts/checks/versions.sh` compares the release each
  names with the workspace version and its changelog date, so the next cut
  fails the guard until they move with it (#908).

### Removed

- **Breaking:** the static bearer token is gone from `[cdr.credentials]` and
  `[terminology.credentials]` (#961). The bridge authenticates to an upstream
  with RFC 7617 basic authentication, or to the CDR with a token it obtains
  through the OAuth 2.0 client credentials grant (`[cdr.credentials.oauth]`).
  A configuration that still sets `bearer_token` or `bearer_token_file`, or
  the `FERROBRIDGE__CDR__CREDENTIALS__BEARER_TOKEN[_FILE]` and
  `FERROBRIDGE__TERMINOLOGY__CREDENTIALS__BEARER_TOKEN[_FILE]` variables,
  refuses to start on an unknown key. Replace it with `user` and
  `password_file`, or for the CDR with `[cdr.credentials.oauth]`. The compose
  quickstart's `cdr_bearer_token` secret is now `cdr_password`, read through
  `FERROBRIDGE__CDR__CREDENTIALS__PASSWORD_FILE` beside
  `FERROBRIDGE__CDR__CREDENTIALS__USER`, and the quickstart passes the
  `FERROBRIDGE__CDR__CREDENTIALS__OAUTH__` variables through. The Operate
  page recommends the OAuth path when the FHIR facade and the CDR are separate
  audiences of one authorization server (#952). `ferrobridge-term` 0.1.128
  drops `config::Credentials::Bearer`, and the server's
  `CdrConfig::with_credentials` becomes `with_basic`; a consumer passes a
  user and a password.

### Fixed

- The compose quickstart passes the terminology server's credential
  variables through to the bridge, `FERROBRIDGE__TERMINOLOGY__CREDENTIALS__USER`
  and `__PASSWORD_FILE` for basic authentication and the five `__OAUTH__`
  variables for the client credentials grant, as it does for the CDR, and
  mounts a new `terminology_password` secret (#976). Create
  `secrets/terminology_password` before the next `docker compose up`; it may
  be empty when the terminology server checks no credentials. The CI compose
  check asserts every CDR and terminology credential variable reaches the
  bridge.
- `scripts/checks/crate-version-guard.sh` diffs from the merge base of its base
  ref and the head, and the CI job passes the target branch as fetched now, so
  a crate change `main` gained after a pull request forked is no longer counted
  against that pull request (#969). `--self-test` reproduces that case over a
  scratch repository, and the CI `versions` job runs it.
- `scripts/gh/watch-prs.sh` judges only the newest run of each workflow on a
  pull request's head, so the failed jobs of a run a later push superseded or
  cancelled no longer stop it as red (#955). `--self-test` checks the
  verdict against fixture checks, and the CI `tracker-helpers` job runs it.
- A chained `patient.identifier`, `subject.identifier` or
  `subject:Patient.identifier` value that more than one patient carries now
  matches the clinical resources of every one of them (#944). The facade
  answered `400 multiple-matches` instead, which was non-conformant: FHIR R4
  search §3.1.1.4.10 matches a bare value "irrespective of the value of the
  system property", §3.1.1.4.15 answers every matching target, and R4's one
  refusal there is for a chain whose matches span resource types. The default
  `[facade.identifier_search] query` drops its `LIMIT 2`; a configured query
  with a `LIMIT` should drop it too.
- A FHIRconnect run into openEHR declares every `INSTRUCTION.narrative` it
  leaves to the FLAT builder (#919). The narrative is a mandatory `DV_TEXT`
  (RM EHR IM §INSTRUCTION), and the builder fills one no mapping writes with
  a placeholder text. The run now records it as one more `Loss::Defaulted`
  warning per instruction instance, and in the composition's `FEEDER_AUDIT`
  when the run carries an origin, so the stored composition names the value
  no mapping wrote.
- `scripts/gh/watch-prs.sh` no longer stops on a changelog conflict whose
  `[Unreleased]` keeps a blank line between two bullets or a heading with no
  blank line above it (#946). The entry merge keeps the blank line between
  bullets, renders a heading with one above it, and refuses only when the
  text lines would not read back in order, naming the first such line.
- A pull request merged after a release no longer files its changelog
  entries under that release (#913). `scripts/gh/watch-prs.sh` merges
  `CHANGELOG.md` by entry through the new `scripts/gh/changelog-merge.sh`,
  which keeps a branch's entries under `[Unreleased]`, also when a clean line
  merge would have placed them under a version cut meanwhile.
  `scripts/checks/changelog-released.sh` fails CI when a released section from
  0.0.9 on differs from its text at its tag.

## [0.0.9] - 2026-10-19

### Added

- The ITS-REST 1.1.0 Demographic API, DEVELOPMENT, behind `[cdr] demographic`
  (off by default; #894). The Demographic OpenAPI document is vendored beside
  the other ITS-REST documents, its `x-status: DEVELOPMENT` checked, and a test
  holds the generated `openehr-its` client to every operation it declares.
  `CdrClient::demographic` reads, creates, updates and deletes `PERSON`,
  `ORGANISATION` and `ROLE`; a `400`, `404`, `409`, `412` or `422` maps to a
  facade answer through `facade::status::demographic`. A Release 1.1.0
  `PARTY` that carries `reverse_relationships` reads: the member is dropped
  before the decode and the dropped entries are counted, because each one
  restates a relationship the repository holds (RM Demographic IM Release
  1.1.0 §2.2.1; #845). No FHIR resource is served from it yet.
- A FHIRconnect sub-context's `template.id` is checked against the template
  the parent's OPT 1.4 states at the slot node (`C_ARCHETYPE_ROOT.template_id`,
  read by the new `WebTemplateIndex::embedded_template`). A different template
  is refused as `fc-sub-context-template-mismatch`, and a slot node that states
  none leaves the warning `fc-sub-context-template-undecided`
  (`sub-contexts.adoc` §Validation errors; `openehr-mapping-core` 0.1.115,
  `fhirconnect` 0.1.145; #886).
- FHIRconnect sub-contexts (#864, `types-of-mapping-files/sub-contexts.adoc`).
  `fhirconnect` 0.1.143 reads `slotContext` on a mapping, and `scope` (`root`
  or `sub`) and `contexts` on a context. The strict schemas carry all three,
  with `slotArchetype` and `slotContext` mutually exclusive.
  - The semantic rules refuse:
    - a `slotContext` or a `contexts` entry naming no loaded context;
    - a `slotContext` at `openehr: $reference`;
    - a `scope: sub` context no context lists;
    - a cycle through `contexts`.
  - The resolver compiles a `slotContext` into the parent program as
    `Method::SlotContext`, from the entered context's `start` with that
    context's own extensions and against the parent's template, and skips the
    composition slot of a root context entered this way. It refuses:
    - a sub-context compiled as a root;
    - a `slotContext` the entering context does not list;
    - a sub-context whose `start` slots a composition;
    - a `start` that does not match the slot's archetype.
  - The engine maps a sub-context into the parent composition in both
    directions, and `Outcome::consumed` names the resource a `slotContext`
    pulled in through a `reference`.
  - The server loads only root contexts as programs and never fetches a
    sub-context's template. A transaction passes over, as a root, an entry
    another entry's `slotContext` consumed, and answers it `200 OK` with no
    location of its own. An entry nothing consumes is still processed as a
    root.

- `fhir-types` 0.1.126 reads `ValueSet` and `CodeSystem` release-neutrally
  (#876). `definition::ValueSetRead` covers the composition (includes,
  excludes, filters, imports, `inactive`) and the expansion, and
  `definition::CodeSystemRead` covers content, hierarchy meaning, case
  sensitivity, the declared properties and the concepts. A nested concept or
  expansion entry is read through the same trait as its parent
  (`ConceptRead`, `ContainsRead`). Every version implements them from its own
  package, and STU3's `uri` imports read as R4's canonicals. The profile
  validator now decides STU3 bindings from the STU3 `ValueSets` and
  `CodeSystems` of the package set as it does R4 ones. `Release` names the
  release's `ValueSet` and `CodeSystem`, `PackageSet::value_set` and
  `code_system` answer in the release's model, and
  `Reason::TerminologyNotRead` is gone.

- A `Patient` keys its own EHR through the configured subject link (#867).
  Under `[facade] subject_id_form = "logical_id"` or `"uuid"` a Patient create
  or update finds or provisions its EHR by its logical id in
  `subject_namespace`, the id a `Patient/<id>` reference of its resources
  names, so on a CDR whose owner links each EHR under its own namespace the
  Patient and its resources reach one EHR. A Patient with no `id`, or under
  `uuid` one whose `id` is no UUID, is refused naming
  `facade.subject_id_form`. In a Bundle, a subject reference to a Patient
  entry that carries an `id` keys by that id too, and one without an `id`,
  such as an HL7 v2 message's Patient, keeps the identifier key. Under
  `reference` nothing changes.

- The profile validator validates STU3 resources against STU3 profiles
  (#838). `PackageSet::<Stu3>::load_release` reads a package set as STU3, and
  `Validator` over it walks the STU3 element table with the checks, refusals
  and unevaluated listing of R4: cardinality, the types of a choice, the
  single STU3 `type.profile` and `type.targetProfile`, slicing, the lexical
  forms of the STU3 package, invariants as FHIRPath over the STU3 types, and
  extension contexts in STU3's `resource`, `datatype` and `extension` codes.
  The walk is generic over the sealed `release::Release` (`R4`, `Stu3`) and
  reads every definition through the release-neutral `fhir_types::definition`
  read. `PackageSet` and `Validator` take the release as a type parameter
  defaulting to R4, and `PackageSet::load` and `PackageSet::new` still read
  R4. An STU3 binding is never decided from the package set, since its
  `ValueSets` and `CodeSystems` have no release-neutral read yet (#876): it is
  listed with the new `Reason::TerminologyNotRead` and kept for a terminology
  server.

- The facade's patient-to-EHR link is configured to match the CDR owner's own
  link (#467). `[facade] subject_type` sets the `PARTY_REF.type` of the
  subject of an EHR the facade creates (default `PERSON`, until now fixed),
  and `[facade] subject_id_form` sets how the id of a literal subject
  reference is written: `reference` (the default, `Patient/<id>` as before),
  `logical_id` (the bare `<id>`) or `uuid` (the bare `<id>`, refused with a
  diagnostic naming the key when it is no UUID). A `subject_type` outside
  the BASE `PARTY_REF` `Type_validity` set, or a `subject_namespace` that
  breaks the `OBJECT_REF` namespace rule, refuses the boot naming the key;
  both are checked through `openehr-base`. The bridge never derives a
  subject id the CDR mints itself, such as a pseudonym.

- `fhir-types` 0.1.124 completes the release-neutral definition read (#859).
  `ElementDefinitionRead` gains `is_modifier`, and its reads carry a type's
  `regex`, a binding's `max_value_set` and a constraint's `best_practice`,
  read from the core extensions with those urls in every version (STU3
  states `maxValueSet` as a `Reference`, R4 as a `canonical`; the R5 and R6 core
  packages define neither of the two binding and constraint extensions). The new `StructureDefinitionRead` reads the url,
  version, type, base definition, kind, `abstract`, derivation, snapshot,
  contexts and context invariants of every version: an STU3 context (one
  `contextType` beside a list of strings) reads as the same `ContextRead`
  entries as an R4 context, each release keeping its own type codes.
  `fhirconnect` moves to 0.1.140 for the requirement. Breaking for a consumer
  that builds `TypeRead`, `BindingRead` or `ConstraintRead` literals, or
  implements `ElementDefinitionRead` itself.

- The ITS-REST 1.1.0 Overview and System OpenAPI documents are vendored
  beside EHR, Query and Definition (#478). All five are `x-status: STABLE`
  at tag `Release-1.1.0`. The Overview defines the common headers the CDR
  client sends (`openehr-version`, `openehr-audit-details`,
  `openehr-template-id`), and System defines the `OPTIONS /` capability call.
  `scripts/vendor/its-rest.sh` fetches and checks all five, and the corpus
  test reads each.
- The FHIRPath evaluator has an STU3 type model, `model::stu3::Stu3`, over
  the generated STU3 element table (#837), so paths, `ofType()`, `is` and
  `as` resolve against STU3 types (an STU3 `valueAttachment`,
  `Patient.animal`). Every STU3 core invariant with a typed element passes
  the static check, except the two the package publishes with a defect
  (`inv-1`, `sdf-15`).

### Changed

- **Breaking for a library consumer** of `fhirconnect` 0.1.146: a demographic
  `Reference.reference` resolves through the new `Demographics` seam
  (`engine::demographics`, #469), and the `Seams` default, `NoDemographics`,
  refuses it. A consumer installs `IssuerDemographics` (the ids of its own
  FHIR server, the fallback `engine/demographics.adoc` names) or its own
  `Demographics` with `Seams::with_demographics`. The identity travels in
  `PARTY_IDENTIFIED.identifiers` where the FLAT format carries it, and in
  `PARTY_PROXY.external_ref` on the composer and on a participation's
  performer; a read turns it back into the reference. The server installs
  the facade-base port, `[facade] base_url` is checked at boot as its
  issuer, and the operations lane uses it when the facade is configured.
- `ferrobridge-validator`, `ferrobridge-term` and `ferrobridge-fhirpath` join
  the crate line at 0.1.126, leaving their 0.0.0 name reservations, so the
  next publish ships the validator as it stands on `main`. Pin each exactly,
  as for every crate of the line (#432).

- The operator console draws with `ferrohealth-ui` 0.1.3, the FerroHEALTH
  component library the four consoles share (#850). It keeps only its brand
  theme and its screens: the shell, the cards, tables, pagers, code panels,
  forms, refusals and empty states are the library's, its icons are the
  library's Lucide glyphs, and the console's own Tailwind stylesheet, style
  module and highlighter are gone; the site build no longer installs the
  Tailwind CSS CLI (#890). The top bar carries the one theme toggle,
  a moon in the light theme and a sun in the dark one, which posts with no
  script and, once hydrated, swaps the theme in place while the server keeps
  the choice in its cookie. The playground (its navigation entry now
  reads "Playground") takes its composition, Bundle and resource in JSON
  editors with numbered lines, shows its answers as highlighted JSON in
  bounded panels, and an issue row shows its `details.text` and the
  openEHR path it names. The mapping editor highlights the draft as YAML
  with numbered lines as you type, marks the lines a check's diagnostics
  name, and links each diagnostic to its line. A mapping or a resource
  without a profile says "none". The dashboard's Bridge card carries the
  build facts across one row with the file counts the tiles leave out, a
  table column no row fills is left out, and every screen reads at 390 CSS
  pixels as well as 1440.
- The README's generated badge block keeps the two HL7 v2 corpus badges in
  view and folds the per-family and per-version badges into a collapsed
  block, which `scripts/checks/conformance.sh --update` renders (#860).
- The FHIRconnect specification source moves to upstream commit
  `e5871641`, which carries the merged REST API chapter (pull request #93) and
  the sub-contexts (pull request #100) (#477). The chapter is no longer a
  draft with a pin of its own: `scripts/vendor/fhirconnect.sh` vendors it and
  its FSH operation definitions (`rest/`) from the one commit, and
  `docs/specs/fhirconnect/draft-rest-api/` is gone. The conformance corpus,
  its badge and the worked-example fixtures are renamed `rest-api`. Against
  the head the bridge first implemented, the merged chapter changes no FSH
  definition and no worked example; it adds that the `context` fields are a
  fixed list an engine MUST refuse to extend, which `$tofhir` already
  refused and a test now asserts by name. Upstream re-rendered both
  published v1.0.0 schemas in place, adding `spec.unidirectional` and
  `operational`; the bridge adopts the new render, re-records both sha256
  values in `docs/architecture.md` §2, and reports the in-place change as
  #856. No refusal of the strict schemas changes. The sub-contexts are filed
  as #864. `fhirconnect` 0.1.139.
- The openEHR RM is pinned (#476): the conformance target is RM Release
  1.1.0, the newest published release, and the code compiles against the
  `openehr_rm::v1_2` development generation, which `openehr-sdt` and
  `openehr-its` build on. `docs/VERSIONS.md` and `docs/architecture.md` §2
  carry the sixth specification row and what `v1_2` changes against Release
  1.1.0 (`EHR.tags`, `GENERIC_ENTRY.data` widened to `ITEM`,
  `PARTY.reverse_relationships` removed); §3 records the `1.2.0`
  `ARCHETYPED.rm_version` stamp as a deviation (#844).
  `scripts/checks/versions.sh` compares six specification pins and fails a
  hand-written `openehr_rm::v1_N` path under `app/`, `crates/` or `tools/`
  that names another generation.
- `scripts/vendor/profile-packages.sh` hashes a profile package tree once
  (#840): the count and digest of a verified tree are stamped under the
  ignored `target/` with a fingerprint of its files' sizes and modification
  and change times, and a run that finds the fingerprint unchanged reads them
  from the stamp and says so. A changed or added file changes the
  fingerprint and the tree is hashed again; the pin is compared on every run.
  A second run over the 17 trees takes about 2 seconds instead of about 3
  minutes. `.gitignore` refuses the `*.pending-snap` and `*.snap.new` files
  insta writes (#839).

### Fixed

- A demographic `Reference.reference` (a recorder, a provider, a
  participant) was dropped without a trace on the way into openEHR; it now
  reaches the composition as the identity the demographics endpoint gives it,
  or the run refuses (#469). The unused `Loss::DeferredReference` is gone.
- Every composition the bridge builds, and every `EHR_STATUS` the facade
  creates, names RM Release 1.1.0 in `ARCHETYPED.rm_version` in place of
  1.2.0, a release openEHR has not published (RM Common IM Release 1.1.0
  §3.2.3). A composition holding a `GENERIC_ENTRY` whose `data` is no
  `ITEM_TREE`, which only the development line admits, keeps 1.2.0
  (`openehr-mapping-core` 0.1.114, `fhirconnect` 0.1.144; #844).
- `fhir-types` 0.1.125: the release-neutral `TypeRead::regex` reads a
  primitive's lexical form from the STU3 extension
  `structuredefinition-regex` as well as from R4's `regex`, so an STU3
  primitive such as `date.value` reads its form instead of none (#875).

## [0.0.8] - 2026-10-18

### Added

- `fhir-types` 0.1.123 carries a release-neutral read of `ElementDefinition`,
  `definition::ElementDefinitionRead`, implemented for every version's
  generated `ElementDefinition` from that version's package (#836). It reads
  the element id, path, slice name, cardinality, `maxLength`,
  `contentReference`, the types with their profiles, target profiles,
  aggregation and versioning, the binding strength and value set, the
  constraints, the slicing, and `fixed[x]` and `pattern[x]` in their JSON
  form. STU3's single `type.profile` and `type.targetProfile` read as lists,
  and its `binding.valueSetUri` or `valueSetReference` as the value set's
  URL (STU3 and R4 elementdefinition.html), so the profile validator can
  read STU3 definitions without a second model. `fhirconnect` moves to
  0.1.138 with the requirement.
- The R4 profile validator is held to the HL7 FHIR Validator 6.10.4 on a
  synthetic corpus, the `validator-agreement` conformance corpus (#442): ten
  cases (conforming resources, and resources that break one invariant, one
  required binding, one profile cardinality, one slice and one entry
  profile, the EU ones against `patient-eu-core` and `bundle-eu-eps`), the
  reference outcomes recorded beside them, and
  `scripts/vendor/hl7-validator-agreement.sh`, which fetches the jar and the
  packages it loads by sha256 from the new "HL7 FHIR Validator" table of
  `docs/VERSIONS.md` and regenerates the outcomes offline. Nine of ten agree;
  the tenth is adjudicated in the test (the reference indexes a Bundle entry
  off by one, FHIRPath N1 §5.3.1).

- `fhirconnect` 0.1.136 holds every FHIR resource a run out of openEHR
  produces or creates to R4 `ext-1` before it leaves the engine (#651). An
  extension with a url and no value, or with a value and nested extensions,
  refuses the run as the new `EngineError::Invariant`, naming the constraint
  and the instance path, so a facade read or a `$tofhir` call refuses it
  instead of answering a resource that breaks an error-severity constraint
  (R4 conformance-rules.html §2.1.0.6). `engine::invariant` holds the checks,
  one per constraint key; `docs/architecture.md` §4.4 records why they are
  targeted checks and not FHIRPath evaluation. A consumer matching
  `EngineError` exhaustively gains a variant.
- The operator console has browser journeys, screenshots and a book page
  (#602). The journeys drive headless Chrome through chromedriver with the
  `thirtyfour` WebDriver client against a running bridge (`ferrobridge
  serve`'s run path over a `wiremock` CDR and a synthetic mapping tree) and
  a running console serving its release site bundle, and fail on any error
  the browser logs. They cover the sign-in, the shell and its theme switch
  and sign-out, the engine state, every mapping file view, the transform
  playground (an answer, a refusal, and an operations lane that is off) and
  the mapping editor end to end: a refused draft leaves the disk and the
  running set unchanged, a save is byte-identical and reloads the programs,
  and the bridge's audit line names the actor and the file and nothing of
  the draft. They run behind `FERROBRIDGE_JOURNEYS=1` in the new
  `journeys (browser)` CI job at the Chrome for Testing release
  `docs/VERSIONS.md` pins, which `scripts/release/journey-tools.sh` installs
  locally. `FERROBRIDGE_SCREENSHOTS=1` captures every console page in light
  and dark at 1440 pixels for the book's operator console page, and a guard
  in the ordinary suite fails on a console route without both images. The
  test OpenID Provider of the testkit can now sign a real browser in.
- The FHIRPath evaluator admits the functions FHIRPath 3.0.0 adds as
  Standard for Trial Use behind the opt-in
  `Environment::with_fhirpath_stu3_functions` (#633), so the default stays
  FHIRPath N1 2.0.0, the release FHIR R4 cites. This change implements
  `matchesFull()`, `trim()`, `split()`, `join()`, `encode()`, `decode()`,
  `escape()` and `unescape()`. HL7's FHIRPath R4 suite now counts as two
  corpora with a badge each: `fhirpath-r4` (874 of 886, N1) and
  `fhirpath-r4-stu3` (33 of 104, the tests that call one of the 13 3.0.0
  functions, run with the opt-in).
- The FHIRPath 3.0.0 opt-in adds `lowBoundary()`, `highBoundary()`,
  `precision()` and `comparable()` (#633). A decimal boundary is half a unit
  of its last place away, rounded outward to the precision asked for. A date
  and time boundary fills the missing components and keeps the value's own
  offset, as the 3.0.0 examples show. `fhirpath-r4-stu3` moves to 85 of 104;
  the suite cases that contradict those examples are the upstream report
  #815.
- The FHIRPath 3.0.0 opt-in adds `sort()`, with the `asc` and `desc` key
  directions of the 3.0.0 grammar's `sortArgument` (#633), which completes
  the 13 functions. `fhirpath-r4-stu3` moves to 93 of 104.

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

- FHIRPath `distinct()` and `isDistinct()` are near-linear (#803). An item
  is compared with `=` only against the kept items that share its value's
  bucket, so the result is unchanged and 10000 distinct strings take 1.5 ms
  where they took 1.4 s. The R4 core Bundle invariant `bdl-7` no longer
  grows quadratically with a Bundle's entries.
- `ferrobridge_fhirpath::model::path_types` types a path that continues
  through a choice element (`Observation.value[x].unit`, the `unit` of
  `Quantity`) or below a primitive (`string.extension`, the `extension` of
  `Element`), so the R4 core constraint sweep now also checks those 54
  invariants (#572). Only the invariants of the five logical models remain
  untyped.
- The profile validator and the HL7 v2 face each state their support for
  an STU3 terminology server (#558). The validator supports one: a binding
  to a versioned value set, which STU3 `$validate-code` cannot carry, is set
  aside with the client's `UnsupportedParameter` and stays listed while the
  other bindings are still sent, where before one such binding failed the
  whole batch. The HL7 v2 face does not: its translations name each table
  map by canonical `url`, which STU3 `$translate` lacks, so the run fails
  before anything is sent. `ferrobridge-term` gains `Client::check`, which
  answers whether the configured release can carry a request without
  sending it, and the validator's `Terminology` trait gains `supports`,
  defaulted.

- `fhirconnect` 0.1.135, the HL7 v2 face and the profile validator name a
  choice element's JSON members through the element table's
  `FieldSchema::choices` and `TypeSchema::member` (#813), so the R4
  json.html §2.6.2 naming rule lives in `fhir-types` alone. Breaking for a
  consumer of `fhirconnect::tree::element::Move`: `Move::Choice` carries
  `schema: &'static FieldSchema` in place of `variants`, whose alternatives
  `schema.choices()` lists.

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

- A facade read, vread or search whose stored composition the engine refuses
  to render answers `500` with an `OperationOutcome` whose `issue.code` is
  `processing`, naming the refusal and never a clinical value (#831). It
  answered `422` before, the status of a write whose posted content is at
  fault; a read sends no content, so the failure is the server's (R4
  http.html §3.1.0.4). The row is `facade::status::OUTBOUND_REFUSED`, with
  a wire test per path; a write whose content the engine refuses keeps its
  `422`.
- `fhir-types` 0.1.122: the XML reader refuses an element or attribute under
  a prefix with no namespace declaration in scope as `MalformedXml` naming
  the prefix anywhere in a resource, not only inside the narrative, since
  such a document is not namespace-well-formed (Namespaces in XML 1.0 §5,
  Prefix Declared) (#830). Outside the narrative it answered
  `UnknownProperty` for an attribute and `WrongRoot` for a root before; the
  refusal stays a refusal and only its kind changes. `fhirconnect` moves to
  0.1.137 with the requirement.
- `/health/readiness` no longer answers `200` while the server is still
  handing the listener from the boot probe routes to the lanes (#796). When
  the CDR was not ready at boot, a successful retry marked the `startup`
  indicator up while the probe routes, which answer `503` for every other
  route, were still serving, so readiness reported ready a moment before
  the lanes did. The indicator now stays down, naming the handover, until
  the lanes serve their own readiness.
- `fhir-types` 0.1.121: the XML reader refuses a narrative element or
  attribute under a prefix with no namespace declaration in scope as
  `MalformedXml`, naming the prefix, since such a document is not
  namespace-well-formed (Namespaces in XML 1.0 §5, Prefix Declared) (#577).
  It refused these before as `UnknownElement` or `UnknownProperty`. The
  narrative content rules `txt-1` and `txt-2` stay invariants of
  `Narrative.div` that the profile validator evaluates through
  `htmlChecks()`, now with a test for each. `fhirconnect` moves to 0.1.135
  with the requirement.
- The FHIRPath `is` operator and `is()` function test a FHIR primitive
  against the types it specializes, so `Patient.gender is string` is true,
  as FHIRPath N1 §6.3.1 and the R4 `code` definition ask; `ofType()` and
  `as` keep every primitive an independent type (FHIR R4 fhirpath.html
  §2.9.1.5) (#635).
- A FHIRconnect operation the operations lane cannot serve answers with the
  true reason (#810). With `[operations] enabled = false` the `503` now says
  `[operations] enabled is false`; it said "no mapping set is configured"
  even when `[mappings] directory` was set, which is still the reason when
  it is not.
- The operator console's theme switch returns to the page it was used on
  (#602). It returned only to the dashboard and the About page, so a switch
  on any page added since landed elsewhere; it now returns to every route
  of the console, a file path below `/files` and `/edit` included, and still
  never off the console.
- The operator console serves the brand favicon at `/favicon.ico` (#602).
  Every first page a browser loaded asked for it and logged a `404`.
- A bridge refusal the console draws carries each issue's `details.text` as
  well as its `diagnostics` (#602). The FHIRconnect operations put their
  reason in `details.text`, so a refused `$tofhir` or `$toopenehr` showed
  its status and no reason.
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

[Unreleased]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.13...HEAD
[0.0.13]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.12...v0.0.13
[0.0.12]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.11...v0.0.12
[0.0.11]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.10...v0.0.11
[0.0.10]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.9...v0.0.10
[0.0.9]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.8...v0.0.9
[0.0.8]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.7...v0.0.8
[0.0.7]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/FerroHEALTH/FerroBRIDGE/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.1
