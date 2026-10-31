<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Contributing to FerroBRIDGE

FerroBRIDGE is a pure-Rust, standalone bridge between openEHR and two
interoperability targets: HL7 FHIR, driven by the FHIRconnect specification,
and the OMOP Common Data Model, driven by the OMOCL specification. The design
of record is `docs/architecture.md`, and the working discipline is
[`CLAUDE.md`](CLAUDE.md). Read both before making a change.

This file carries what is particular to FerroBRIDGE: how to build and test it,
its rules for a change, and the parts it publishes under another licence. The
contribution terms every FerroHEALTH product shares are in the family book at
<https://ferrohealth.eu/docs/licensing.html#contributions>.

A contribution need not be code. A specification citation, a measurement,
or first-hand experience with FHIRconnect, OMOCL or an openEHR CDR in
production is as useful.

## The two layers

- **The generated model layers**: `crates/fhir-types` from the vendored HL7
  FHIR packages, `crates/hl7v2-types` from the HL7 v2 definitions, and the
  OMOP CDM row types in `crates/omop-cdm`. Never hand-edit a file marked
  `// @generated`: change the generator and regenerate
  (`.claude/rules/codegen.md`).
- **The hand-written bridge engine**, which is the product: the mapping
  foundation, the FHIRconnect and OMOCL parsers and interpreters, the openEHR
  ITS-REST and terminology clients, the FHIR facade and the ETL runner. Modern
  idiomatic Rust, with the specifications as the authority
  (`.claude/rules/rust-style.md`).

**Mappings are specification-conformant YAML**, validated against the published
schemas. A hand-coded converter per FHIR resource or per OMOP table is refused
in review.

## Build and test

The local gate is `scripts/checks/fast.sh`. It runs rustfmt, clippy, nextest
and the doctests on the packages your change reaches, the same selection the
CI `detect` job computes, after fetching the build-time inputs those packages
read:

```
scripts/checks/fast.sh             # the packages your change reaches
scripts/checks/fast.sh --dry-run   # print the plan without running it
scripts/checks/fast.sh --all       # every package
```

A change to a workflow or a shell script also needs the tier-1 linters CI
runs, with the same flags:

```
zizmor --min-severity=low .github/
actionlint
shellcheck --severity=style <the shell files you touched>
```

Every cargo invocation uses `--locked`, and `Cargo.lock` is committed. The
full list of checks, the conformance pass lists and the build-time fetches are
on the book's
[checks and gates](https://ferrobridge.eu/docs/contribute/checks-and-gates.html)
page. The `conclusion` job of `ci.yml` aggregates every lane and is the single
required check on `main` (`docs/ci-cd.md`).

## Specifications are the authority

Five sources, one per surface: the HL7 FHIR specification for the version in
play, FHIRconnect, the OMOP Common Data Model, OMOCL, and openEHR ITS-REST.
Read the governing section before implementing spec-facing behaviour, and cite
it in the pull request. Never resolve a specification question from memory, and
never from another implementation's behaviour: openFHIR, Eos, and the EHRbase
FHIR Bridge are prior art, a running CDR or terminology server is a test
deployment, and none of them is an oracle (`.claude/rules/spec-adherence.md`).

Where the specifications are silent, say so in the text you write: "no
specification governs this: our own design".

## Branches and commits

- Branch names use conventional types: `feat/<slug>`, `fix/<slug>`,
  `chore/<slug>`, `docs/<slug>`, `refactor/<slug>`, `perf/<slug>`,
  `test/<slug>`, `ci/<slug>`, `build/<slug>`, `release/<slug>`. Never
  force-push `main`.
- Commit messages describe only the change. Do **not** add AI or assistant
  attribution, co-author trailers, or "generated with" lines anywhere.
- **Sign your commits.** Configure commit signing (GPG, SSH, or S/MIME) so
  every commit is verified.

## Pull requests

- Declare `Closes #N`, one keyword per issue.
- Keep changes compiling and tested at every step; do not defer compilation.
- Never weaken, skip, or delete a test to make a build pass.
- Add one fragment, `changelog.d/<issue>-<kebab-slug>.<section>.md`, for any
  user-visible change, and leave `CHANGELOG.md` alone (`changelog.d/README.md`).
  The release cut assembles the fragments into the changelog.
- Every workflow `uses:` is pinned to a full commit SHA with a trailing version
  comment; keep it that way. `permissions:` is `{}` at the workflow level with
  the minimum granted per job, and no untrusted context is interpolated into a
  `run:` block; pass it through `env:`.
- Write prose to `.claude/rules/writing-style.md`: no em dashes, no
  "not X but Y", no decorative triads, no filler buzzwords.

## Using AI tools

You may. If a contribution has AI-generated content, say so in the pull-request
description: which tool, and what it did. Disclosure lives in the description
and never in a commit trailer. You remain responsible for the submission in
full: understood, explained on request, tested, and honest. See
[`AI_STATEMENT.md`](AI_STATEMENT.md).

## Security

Report vulnerabilities privately. See [`SECURITY.md`](SECURITY.md), and the
family policy it links for how a report is handled. Do not open a public issue
for a security problem.

## Licensing of contributions

FerroBRIDGE's own code is licensed under the Business Source License 1.1
([`LICENSE`](LICENSE)). These are the terms the family book states for every
FerroHEALTH product
([Contributions](https://ferrohealth.eu/docs/licensing.html#contributions)),
with the part FerroBRIDGE publishes under another licence named in point 2.
By opening a pull request you:

1. certify that you wrote it, or otherwise have the right to submit it under
   these terms;
2. license it under the Business Source License 1.1 as applied to the version it
   lands in, including that version's Change License, so it becomes Apache 2.0
   with the rest of that version. A contribution to `crates/fhir-types`, the generated
   FHIR model, is licensed under that crate's Apache License 2.0 instead; and
3. grant the Licensor named in `LICENSE`, Cadasto B.V., a perpetual,
   irrevocable, worldwide, royalty-free, transferable right to use, reproduce,
   modify, distribute, sublicense and relicense the contribution as part of the
   Licensed Work under any terms, including commercial licences.

You keep your copyright. Point 3 is what lets the Licensed Work stay one work
with one licensor: a commercial licence, a change of the licence parameters, or a
transfer of the project can then cover every line, not only the maintainer's own.
There is no separate agreement to sign and no box to tick: opening the pull
request is your acceptance of these terms.
