<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-market procedures

The working checklist for FerroBRIDGE's side of the vulnerability procedure
that Cadasto B.V., the manufacturer of each tagged FerroBRIDGE release, runs
for every FerroHEALTH product. The procedure itself (the channels, how a
report is classified, when an advisory's details may wait, the CRA Art 14
notification, reporting to the maintainer of an integrated component) is the
manufacturer's and is published in the FerroHEALTH book:
[Security](https://ferrohealth.eu/docs/security.html). The duties come from
Regulation (EU) 2024/2847, the Cyber Resilience Act, Annex I Part II.

This file keeps the steps bound to this repository: its tracker, its
changelog and its release lane (`release.md`).

## Who does what in this repository

| Who | What |
|---|---|
| Cadasto B.V., the manufacturer | Receives reports at `info@cadasto.com`, decides whether a vulnerability is actively exploited, and signs every report to an authority (the FerroHEALTH procedure). |
| The maintainer | Reads FerroBRIDGE's private vulnerability reports, the reports Cadasto B.V. forwards from `info@cadasto.com`, and the tracker; drafts the advisory, prepares the fixing release, and publishes the advisory with it. |

FerroBRIDGE's channels are GitHub private vulnerability reporting on this
repository and `info@cadasto.com` with the subject "FerroBRIDGE
vulnerability" ([`SECURITY.md`](../SECURITY.md)). An emailed report gets a
draft security advisory on this repository on receipt, exactly as a private
GitHub report does.

## A fixed vulnerability's advisory

Every fixed vulnerability in FerroBRIDGE gets a GitHub security advisory on
this repository, published with the release that fixes it: one in
FerroBRIDGE's own code, and one for a dependency or base-image fix that
changes what a shipped artefact (a server tarball, the `ferrobridge` or
`ferrobridge-console` image, a published crate) contains or how it behaves. A
dependency finding that an OpenVEX statement under `security/vex/` shows does
not affect the artefact gets no advisory; the changelog entry names that
statement's path instead.

1. **On the report or the finding:** open a draft advisory on this
   repository. It is private, and it is the record until publication.
2. **Ship the fix** as a security-only patch release. If a functional change
   has to ride with it, open the release's `### Security` section with
   `Shipped with functional changes because: <reason>`
   (`.claude/rules/changelog.md`).
3. **The changelog entry** under `### Security` carries the advisory's `GHSA-`
   identifier; it lands as a `security` fragment under `changelog.d/`.
   `scripts/checks/changelog-security.sh` refuses an entry without one in CI
   over `[Unreleased]` with every fragment folded in, and the release `plan`
   job refuses it over the section being cut.
4. **Publish the advisory** in the same hour as the GitHub release, and
   request a CVE through it where the vulnerability warrants one.

## Vulnerabilities in integrated components

A vulnerability the maintainer confirms in a component FerroBRIDGE integrates
(a Rust crate, the distroless base image, a vendored asset) is reported
upstream as the FerroHEALTH procedure says (CRA Art 13(6)). For a Rust crate
the channel is its `SECURITY.md` or private vulnerability reporting on its
repository, then a RustSec advisory (`rustsec/advisory-db`) once it can be
public, so that `cargo audit` and `cargo deny` users see it. FerroBRIDGE's own
side is handled like any other vulnerability: a draft advisory and a fixing
release when FerroBRIDGE is affected, or an OpenVEX statement when it is not.

The `upstream-report` label on this tracker is for defects in the
specifications FerroBRIDGE implements, not for component vulnerabilities.
