<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# VEX statements

Vulnerability Exploitability eXchange documents, in
[OpenVEX](https://openvex.dev) format, asserting the exploitability of
specific findings in the artifacts FerroBRIDGE publishes: the `ferrobridge`
server binary, the `ferrobridge` image built from it, and the
`ferrobridge-console` image.

A VEX document is the machine-readable form of an argument, and it carries
the argument with it: each statement names the vulnerability, the product, a
`status`, a controlled-vocabulary `justification`, and an `impact_statement`
a reader can check. An ignore list records the decision without the
reasoning, which decays into a list nobody can re-evaluate.

## Rules

- **`not_affected` needs a justification from the OpenVEX vocabulary**, and
  the `impact_statement` says concretely why the code is unreachable in
  FerroBRIDGE's usage. "Low risk" is not a justification.
- **A finding we can fix is fixed, not VEXed.** A statement exists for a
  finding whose fix belongs to someone else: an inherited base-image layer,
  or a dependency with no patched release.
- **Re-check on every base-image bump.** When the distroless base changes,
  statements about its packages may become obsolete, and a stale
  `not_affected` is worse than no VEX at all.
- **The scanners consume these files** (`trivy --vex`), so a statement that
  stops being true stops being invisible: the finding returns in the weekly
  published-image scan.
- **An `affected` statement leaves its finding visible.** `trivy --vex`
  removes a finding only for `not_affected` or `fixed`; an `affected` finding
  stays in every scan with its action statement beside it in the release
  record, which is intended: it applies to the reader until the fix ships.
- **Every finding a release ships carries a statement.** The release lane
  scans each image and server tarball with no severity floor and unfixed
  findings included, and `scripts/checks/vex-coverage.sh` refuses the release
  while a finding has no statement here that judges it: `not_affected` with a
  `justification` or `impact_statement`, `affected` with an
  `action_statement`, or `fixed`. Products are named
  `pkg:oci/ferrobridge`, `pkg:oci/ferrobridge-console` and
  `pkg:cargo/ferrobridge-server`, and a statement matches a finding by its
  `vulnerability.name` or an `aliases` entry.

## Documents

| File | Subject | Authored |
|---|---|---|
| `rust-advisories.openvex.json` | The Rust dependency advisories the advisory gate accepts. Each statement also carries a `ferrobridge:reachability` block, our own extension since OpenVEX defines none, naming the crates the affected package is reached through. | **generated** |

There is no OS document. Both images are built on
`gcr.io/distroless/static-debian13:nonroot` and carry one statically linked
musl binary each, so the only Debian packages they hold are the base's data
packages (`base-files`, `ca-certificates`, `media-types`, `netbase`, `tzdata`,
`tzdata-legacy`). A full Trivy
scan of the current base reports no finding, so there is nothing to judge. When
a release scan reports one that cannot be fixed by a rebuild, its statement
goes in a hand-written `ferrobridge-os.openvex.json` naming both images.

## The generated document

`rust-advisories.openvex.json` is produced by
`scripts/security/vex-generate.sh` from two inputs, and is never edited by
hand:

- **`deny.toml`** `[advisories].ignore`: the authoritative set of advisory
  ids. It is the gate that decides whether a build passes, so it is the only
  place the id list may live.
- **`security/vex/rust-advisories.toml`**: the reasoning, with the OpenVEX
  `status`, the controlled-vocabulary `justification`, and the
  `impact_statement` for each id.

The generator refuses to emit anything unless the two sets match in **both**
directions, and `scripts/checks/vex-advisories.sh`, in the `vex` CI job,
regenerates the document and fails on any difference. Adding an ignore to
`deny.toml` without publishing its justification is a red build.

Agreement is not truth: an ignore and its justification stay in agreement
while a dependency upgrade resolves the advisory underneath both.
`scripts/checks/advisory-exceptions.sh`, in the same job, promotes
cargo-deny's `advisory-not-detected` diagnostic to an error, so an exception
that has outlived its finding fails the build.

A statement can also agree, describe an advisory that still fires, and argue
from a dependency path that is not the real one. So each statement carries its
path as data (`carriers`, `workspace_roots`) and
`scripts/checks/vex-reachability.sh` compares both sets against the graph cargo
resolves, in both directions. They travel into the published document as the
`ferrobridge:reachability` block, so a consumer reads the claim the gate
checks.

To change a statement, edit `rust-advisories.toml` (and `deny.toml` if the id
set changes), bump the document's `version` and `timestamp`, then run
`bash scripts/security/vex-generate.sh`. `carriers` and `workspace_roots` come
from `cargo tree -i <crate>[@<version>] --workspace --all-features --target all
-e normal,build,dev`; the gate accepts no set that is not on the graph.
