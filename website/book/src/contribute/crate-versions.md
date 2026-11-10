<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Crate versions and the bump rule

The library crates under `crates/` are published on crates.io, and a published
version is immutable. That single fact produces one rule you have to follow in
every pull request that touches them. The two version lines it sits inside are
described on the
[version lines and releases](../operate/versions-and-releases.md) page.

<!-- toc -->

## The rule

**A pull request that changes any packaged content of a `crates/*` member bumps
that member's version in the same pull request.**

Packaged content is what the crate's `include` ships: `src/**`, `README.md`,
`LICENSE`, and `Cargo.toml`. A root `[workspace.dependencies]` entry the member
consumes counts too, because `cargo package` renders the concrete requirement
into the packaged manifest. Tests, benches, and `CLAUDE.md` are not packaged
and need no bump.

Three things move together:

1. the `version` in that member's `crates/*/Cargo.toml`,
2. any internal requirement naming it in the root `Cargo.toml`
   `[workspace.dependencies]` table,
3. `Cargo.lock`, refreshed with `cargo update -w` and committed.

A half-done bump fails the same way a missing one does.

## Which number moves

The crates follow Cargo's SemVer rules and no rule of their own
([SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)).
A breaking change bumps the minor while the crate is 0.x (0.1.240 to 0.2.0),
because Cargo reads a caret requirement on `0.1.240` as any 0.1.x at or above
it. A change that keeps every consumer compiling bumps the patch (0.1.240 to
0.1.241) or the minor. Two steps are refused: a minor step that keeps its
patch, since the patch resets to 0 when the minor moves
([SemVer 2.0.0 §7](https://semver.org/#spec-item-7)), and a step out of 0.x,
which is the owner's decision.

`cargo-semver-checks` decides which of the two your change needs. Run it
before you push, against the branch you target:

```bash
scripts/checks/semver.sh --baseline-rev origin/main fhirconnect
```

It refuses a break on a patch bump. It judges each crate on the judge's
default feature set and, when the crate has features, with no default
features, because a break can exist in a feature subset alone. `fhir-types` is
judged on each FHIR version module with its terminology set as well. Add a new
enum variant at the end of the enum: the judge counts a moved discriminant as
a break.

Consumers outside the workspace then require a crate by a caret range,
`fhirconnect = "0.1"`, so one compatible release serves every crate in their
build. Each crate's README states that range, and the guard fails when it does
not name the line the manifest is on, so a minor bump moves the README with
it.

## What is enforced, and where

| Where | What it does |
|---|---|
| `scripts/checks/crate-version-guard.sh` | compares a head ref with its merge base with a base ref, and fails naming the member whose packaged content moved without its version, moved by a step SemVer does not take, or whose README states another range |
| `scripts/checks/semver.sh` | runs the pinned `cargo-semver-checks` over the named crates against a git revision or crates.io |
| the `crate-version-guard` CI job | runs the guard over the pull request's target branch as fetched now and its head, then `semver.sh` over every crate that moved from a published line; it is one of the jobs the `conclusion` check reads |
| the release `plan` job | runs `semver.sh` over every crate the release publishes, against its newest release on crates.io |
| `.claude/hooks/crate_version_bump_guard.sh` | runs the same script before a `git commit` or a `git push`, against the merge base with `origin/main`, and refuses the command with the findings |

The hook checks a commit against the working tree, because the change it is
about to record is not in `HEAD` yet, and a push against `HEAD`. It is the
early copy of the CI job, not a second rule.

Run the guard yourself the way CI does:

```bash
scripts/checks/crate-version-guard.sh origin/main HEAD
```

## The escape, and its one condition

A pull request carrying the `no-crate-bump` label skips the CI job. Use it only
when the diff provably alters no packaged bytes. The script itself reads no
labels, so the hook still reports; that is the intended asymmetry, since the
label is a reviewed decision rather than a local one.

## Two things that are normal

**A 0.0.0 version is a name reservation.** A member that has not joined the
crate line holds its crates.io name at 0.0.0, and the guard says so rather than
demanding a bump: content under a reservation changes until the first real
version.

**A gap in the published sequence is fine.** Not every bumped version is
published. Publishing different content under a version that already exists is
the forbidden case, and crates.io refuses it anyway.

## `fhir-types` is generated

`crates/fhir-types/src/**` is emitted by `tools/fhir-codegen` and carries a
`@generated` banner. A change there is a generator change plus a regeneration,
never an edit of the output, and the bump follows the regeneration. The
`codegen-drift` CI job re-runs the emitter and fails on any difference.
