<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

FerroBRIDGE is a bridge between openEHR and two interoperability targets, HL7
FHIR and the OMOP Common Data Model. This document carries what is particular
to FerroBRIDGE: the versions that receive security fixes, where to report, the
response times it commits to, its scope, and how to verify a release.

The vulnerability-handling policy every FerroHEALTH product shares is on one
page of the family book, written by the manufacturer, Cadasto B.V.:
<https://ferrohealth.eu/docs/security.html>. It covers what happens after you
report, coordinated disclosure and credit, how a fix and its advisory reach
you, the support period, and what the manufacturer reports to
the authorities under the Cyber Resilience Act.

## Supported versions

Security fixes apply to the latest released version only; there is no
back-port line.

| Version             | Supported          |
| ------------------- | ------------------ |
| Latest release      | :white_check_mark: |
| Older releases      | :x:                |
| `main` (unreleased) | best effort        |

Once the project reaches 1.0 this table will name a supported minor line.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report it
privately through GitHub's private vulnerability reporting:

1. Open the private advisory form:
   <https://github.com/FerroHEALTH/FerroBRIDGE/security/advisories/new>. You
   can also reach it from the repository's **Security** tab under **Report a
   vulnerability**.
2. Describe the issue, the affected version or commit, and a reproduction if
   you have one.

This opens a private advisory visible only to you and the maintainer. If you
cannot use GitHub's form, contact the maintainer through their GitHub profile
and ask for a private channel before sending any details. The family policy
lists what a report should carry.

**Never include patient data in a report.** A bridge sits in the path of
clinical data, so a reproduction is easy to build from real content by
accident. Redact it, or synthesize an equivalent case.

## Response times

FerroBRIDGE is solo-maintained, so these are best effort rather than a
contractual service-level agreement: an acknowledgement within about 5
business days, and an initial assessment (is it valid, how severe) within
about 10. A confirmed vulnerability is fixed as a priority in a new patch
version; the family policy says how that release and its advisory reach you.

## Scope

In scope: the FerroBRIDGE server and libraries in this repository, its mapping
handling, its build and release pipeline, and its published artifacts.

Out of scope: a vulnerability in a third-party dependency with no
FerroBRIDGE-specific impact (report those upstream; Dependabot and `cargo deny`
track advisories here), a vulnerability in the openEHR CDR or the terminology
server a deployment points the bridge at (report those to that project), and an
issue that requires an already-compromised host or a misconfiguration outside
FerroBRIDGE's control.

## Verifying a release

Every binary and every image is built in an isolated reusable workflow, which
is what puts the signing identity out of reach of the build steps (SLSA v1.2
Build Level 3). The commands below are how you check that, and they need
[GitHub CLI](https://cli.github.com/) 2.49 or later.

The repository moved to the `FerroHEALTH` organization on 2026-10-01. Releases
up to and including v0.0.4 were built and signed while it was
`rubentalstra/FerroBRIDGE`, and their images live at
`ghcr.io/rubentalstra/ferrobridge`. To verify one of those, use that image
reference, `--repo rubentalstra/FerroBRIDGE`, and a `--signer-workflow` under
`rubentalstra/FerroBRIDGE`, because the signature records where it was built.

**The tarball came from this repository's release-build lane.**

```sh
gh attestation verify ferrobridge-vX.Y.Z-<target>.tar.gz \
  --repo FerroHEALTH/FerroBRIDGE \
  --signer-workflow FerroHEALTH/FerroBRIDGE/.github/workflows/release-build.yml
```

This proves that the exact bytes you downloaded were built by that workflow, in
this repository, at a commit the attestation names. `--signer-workflow` is the
load-bearing half: without it, any workflow in the repository would satisfy the
check.

**The image came from this repository's release-image lane.**

```sh
gh attestation verify oci://ghcr.io/ferrohealth/ferrobridge:X.Y.Z \
  --repo FerroHEALTH/FerroBRIDGE \
  --signer-workflow FerroHEALTH/FerroBRIDGE/.github/workflows/release-image.yml
```

The same proof for the image index the tag resolves to. Each platform manifest
carries its own provenance and an SPDX SBOM. To check one of those, pass
`oci://ghcr.io/ferrohealth/ferrobridge@sha256:<manifest digest>` in place of
the tag, and add `--predicate-type https://spdx.dev/Document/v2.3` to read the
SBOM attestation in place of the provenance.

**The operator console came from this repository's release-viewer lane.**

```sh
gh attestation verify oci://ghcr.io/ferrohealth/ferrobridge-viewer:X.Y.Z \
  --repo FerroHEALTH/FerroBRIDGE \
  --signer-workflow FerroHEALTH/FerroBRIDGE/.github/workflows/release-viewer.yml
```

The same check holds for the console's release assets,
`ferrobridge-viewer-vX.Y.Z-<target>.tar.gz` and
`ferrobridge-viewer-vX.Y.Z-site.tar.gz`, with the file in place of the
`oci://` reference. Each platform manifest of the console image carries its
provenance and an SPDX SBOM, as the bridge image's do.

**The download is not corrupt.**

```sh
sha256sum -c ferrobridge-vX.Y.Z-<target>.tar.gz.sha256sum
```

This proves only that the file matches the checksum published beside it. It is
not a signature, and it says nothing about who built the file: an attacker who
can replace the tarball can replace the checksum. Use it to catch a truncated
download, and use `gh attestation verify` for everything else.

Each release also carries two SBOMs per target, a CycloneDX document of the
source dependency graph (`.cdx.json`) and an SPDX document read from the
shipped binary (`.spdx.json`), both attested; the Sigstore bundles
(`.sigstore.json`) and the provenance envelope (`.intoto.jsonl`) for offline
verification; and the dependency list embedded in the binary itself by
`cargo auditable`, which `syft`, `trivy`, `grype` and `osv-scanner` read
directly. The lane that produces them is `docs/release.md`.
