---
paths: ["crates/**", "scripts/release/**", ".github/workflows/publish-crates.yml"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Published crates discipline (crates.io)

The `crates/*` members are published on crates.io under plain names
(`fhir-types`, `hl7v2-types`, `openehr-mapping-core`, `fhirconnect`, `omocl`,
`omop-cdm`, `ferrobridge-term`, `ferrobridge-hl7v2`, `ferrobridge-fhirpath`,
`ferrobridge-validator`, `fhir-search`, `eehrxf`) so other projects can
depend on them. The server (`app/*`) and the tools (`tools/*`) are never
published.
Every member is BUSL-1.1 except the two generated crates, which are
Apache-2.0: `fhir-types` (`docs/architecture.md` §4.1), so the generated FHIR
model stays usable by any Rust project, and `hl7v2-types` (owner ruling on
#251: generated Rust is the project's own code; the HL7 v2 definitions it is
generated from are never packaged). Published versions are immutable, so version hygiene is a hard
rule, machine-enforced by the `crate-version-guard` CI job.

## Two version lines

- **The product version** is the workspace `version` in the root `Cargo.toml`
  (the server, the tools, the release tag `vX.Y.Z`).
- **The crate line** is the `version` in each `crates/*/Cargo.toml`. It never
  adopts the product version or a specification version; it is the crates' own
  SemVer line. `fhir-types` carries the line (its pin is the `fhir-types` row
  of `docs/VERSIONS.md`); `openehr-mapping-core` and `fhirconnect` joined it
  at their first publish (#397), `ferrobridge-term`, `ferrobridge-fhirpath`
  and `ferrobridge-validator` at 0.1.126 (#432), `hl7v2-types` and
  `ferrobridge-hl7v2` at 0.1.127 (#1131), `eehrxf` at 0.1.128 when it moved
  here from FerroFED (#1314; its crates.io name already held 0.0.0 and its two
  Trusted Publisher entries name this repository), `fhir-search` at 0.1.128
  when it was created (#1718), and every other member still
  holds its name at the 0.0.0 placeholder until it joins the line at its own
  first publish.

## The bump rule

- **A PR that changes any packaged content of a `crates/*` member bumps THAT
  member's version in the same PR.** Packaged content is what the crate's
  `include` ships: `src/**`, `README.md`, `LICENSE`, and `Cargo.toml`. Tests,
  benches and `CLAUDE.md` are not packaged and need no bump. A root
  `[workspace.dependencies]` entry a member consumes is packaged content too,
  because `cargo package` renders the concrete requirement.
- The member's own `Cargo.toml`, any internal requirement in the root
  `[workspace.dependencies]` table, and `Cargo.lock` move together
  (`cargo update -w` in the same PR). The guard fails a half-done bump and a
  stale lock.
- Escape: the `no-crate-bump` PR label, only when the diff provably alters no
  packaged bytes.
- Not every bumped version is published; gaps in the published sequence are
  normal. Publishing different content under an existing version is what is
  forbidden, and crates.io refuses it.
- **A published crate follows the Cargo SemVer rules and no rule of its
  own** (owner decision 2026-11-10, #1830, over #1745): The Cargo Book,
  "SemVer Compatibility"
  (<https://doc.rust-lang.org/cargo/reference/semver.html>), is the one
  authority. A 0.y.z release "can treat changes in "y" as a major release,
  and "z" as a minor release", so a breaking change bumps the minor while
  the crate is 0.x (0.1.240 to 0.2.0) and a compatible change may bump
  either. That is the rule Cargo reads a caret requirement by
  (<https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#caret-requirements>).
  `cargo-semver-checks` is the judge (`scripts/checks/semver.sh`, pinned in
  `docs/VERSIONS.md`). CI judges every crate that moved from a published
  line against the target branch, and the release plan judges every crate
  it publishes against crates.io. Each crate is judged on the judge's
  default feature set and, when it has features, on
  `--only-explicit-features`, and `fhir-types` on each FHIR version module
  with its terminology set. The `cargo-semver-checks` README says the judge
  does not catch "breaking changes that exist when only a subset of all
  crate features are activated". `crate-version-guard` refuses two steps
  only: a minor step whose patch is not 0, which SemVer 2.0.0 §7 forbids
  (<https://semver.org/#spec-item-7>), and a step out of 0.x, which is an
  owner decision. A new variant goes at the end of an enum, since the judge
  counts a moved discriminant as a break.
- **A public dependency moved to an incompatible version is a breaking
  change** (coordinator ruling, 2026-11-09, #1777; grounds corrected by
  #1830). The Cargo Book chapter has no entry for it. Its intro counts as
  breaking the "changes that will cause cargo and rustc to fail to build
  something that previously worked", and a type of the dependency in the
  crate's API becomes another type on the new line. The `cargo-semver-checks`
  README lists "breaking type changes, for example in the type of a field or
  function parameter" among what the judge does not catch. For a 0.x
  dependency, the incompatible step is the left-most non-zero component
  (`openehr-rm` 0.0.135 to 0.0.136, `fhirconnect` 0.1 to 0.2). The crate
  bumps its minor. A public dependency is any normal dependency of the
  `openehr-*`, `fhir-types`, `hl7v2-types` and `ferrohealth-*` families or a
  sibling of this workspace (`scripts/checks/lib/public-deps.sh`), optional
  ones included. `crate-version-guard` refuses a patch step for a member
  whose public dependency moved line in the diff. `semver.sh` refuses it
  against the target branch or crates.io, and accepts the minor step it
  answers. A sibling's minor step therefore moves every published member
  that requires it to its next minor too.
- **A consumer outside this workspace requires a crate of the line by a caret
  range** (`fhirconnect = "0.1"`, or `"0.1.240"` for a floor), so one
  compatible release serves every crate in its build that uses it, and two
  family crates built against different patches resolve together. Each
  published crate's README states that range, and `crate-version-guard`
  checks it names the line the manifest is on. A PR that breaks a public API
  says so in its body and its CHANGELOG entry, naming what a consumer
  changes.
- **A crate of this line requires a sibling of this workspace exactly**
  (#1736): `ferrobridge-validator` requires `ferrobridge-term = "=0.1.147"`,
  in the root `[workspace.dependencies]` and wherever else a sibling is
  named with a version. The siblings are built, tested and released
  together from one commit, so a published crate names the one sibling
  release it was verified with, and a consumer's lockfile cannot pair it
  with a later sibling this workspace never built it against. A sibling's
  bump then moves every member that requires it in the same PR, which the
  bump rule above already demands, since the root entry is packaged
  content. `crate-version-guard` fails a range between workspace crates on
  every run. The caret ranges of the rule above are for consumers outside
  this workspace.
- **A crate of this line takes a caret requirement on another family shared
  crate** (owner decision 2026-10-26): `ferrobridge-term` requires
  `ferrohealth-smart = "0.8.3"`, never `=0.8.3`, because the FerroAUTH,
  FerroAUDIT and FerroSYS lines never break on a patch. A smart patch then
  reaches a product through its lockfile without a release of this line
  first. A product still pins every shared crate exactly.
- `crates/fhir-types/src/**` and `crates/hl7v2-types/src/**` are generated, so a bump there follows a generator
  change and a regeneration, never a hand-edit (`codegen.md`).

## The publish lane is per crate, resumable, and verified

`scripts/release/publish-crates.sh` (`publish` / `verify` / `version`) uploads
the members one at a time in dependency order, counts "already exists" as done,
and reads the registry back before reporting success. Two lanes call it: the
`crates` leg of `release.yml` on a `v*` tag (the primary path, paused by the
`crates-io` environment's required reviewer) and `publish-crates.yml` on a
manual dispatch (a dry run by default; `publish = true` is the recovery path).
Both authenticate with crates.io Trusted Publishing (OIDC through
`rust-lang/crates-io-auth-action`); no long-lived crates.io token exists in the
repository. Neither lane restores a build cache (`ci-cd.md`), and the
`codegen-drift` gate runs ahead of both, so a published generated crate never
disagrees with its generator.

`cargo publish --workspace --dry-run --locked` runs on every pull request as
the `publish-dry-run` job, so a packaging failure is found before a release
reaches the registry.

## Owner steps (once per crate, and once for the environment)

1. The first-ever version of a crate cannot use Trusted Publishing: the owner
   runs `cargo login` locally and `scripts/release/publish-crates.sh publish`
   from the merged `main`, then
   `scripts/release/publish-crates.sh verify`. `fhir-types` already exists on
   crates.io, published by the sibling terminology server, so its Trusted
   Publisher entries move to this repository instead.
2. On crates.io, each crate's Settings, Trusted Publishing: two GitHub entries,
   repository owner `FerroHEALTH`, repository `FerroBRIDGE`, workflow
   `release.yml` and workflow `publish-crates.yml`, environment `crates-io`.
3. The `crates-io` GitHub environment carries the owner as required reviewer.

## Before publishing: the C-STABLE adjudication

`reliability.md` deviates from C-STABLE while the crates are unpublished. Every
pre-1.0 dependency that appears in a published crate's public API is
adjudicated before the first publish from here, and again whenever the line
graduates past `0.x`.

## Official documentation (durable citations)

- The Cargo Book, SemVer Compatibility: <https://doc.rust-lang.org/cargo/reference/semver.html>
- `cargo-semver-checks` README, v0.51.0: <https://github.com/obi1kenobi/cargo-semver-checks/blob/v0.51.0/README.md>
- Trusted Publishing on crates.io: <https://crates.io/docs/trusted-publishing>
- `cargo publish`: <https://doc.rust-lang.org/cargo/commands/cargo-publish.html>
