<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->
<div align="center">

<img src="assets/brand/ferrobridge-lockup-auto.svg" alt="FerroBRIDGE" width="290">

**The bridge from openEHR health records to HL7 FHIR and the OMOP research model, driven by mapping files.**

[Documentation](https://ferrobridge.eu/docs/) ·
[Quick start](#quick-start) ·
[What it implements](#what-it-implements) ·
[Licensing](https://ferrobridge.eu/docs/evaluate/licensing.html)

<!-- badges:begin -->
**Project**<br>
[![Release](https://img.shields.io/github/v/release/FerroHEALTH/FerroBRIDGE?sort=semver&logo=github&label=release)](https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroBRIDGE/ci.yml?branch=main&logo=githubactions&logoColor=white&label=CI)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml)
[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-ferrobridge.eu-blue?logo=mdbook)](https://ferrobridge.eu/docs/)
[![GHCR](https://img.shields.io/badge/ghcr.io-ferrobridge-2496ED?logo=docker&logoColor=white)](https://github.com/FerroHEALTH/FerroBRIDGE/pkgs/container/ferrobridge)

**Quality and security**<br>
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroBRIDGE/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroBRIDGE)
[![Quality gate](https://img.shields.io/sonar/quality_gate/FerroHEALTH_FerroBRIDGE?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=quality%20gate)](https://sonarcloud.io/summary/overall?id=FerroHEALTH_FerroBRIDGE)
[![Coverage](https://img.shields.io/sonar/coverage/FerroHEALTH_FerroBRIDGE?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=coverage)](https://sonarcloud.io/summary/new_code?id=FerroHEALTH_FerroBRIDGE)
[![CodeQL](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroBRIDGE/codeql.yml?branch=main&logo=github&label=CodeQL)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml)
[![Signed releases](https://img.shields.io/badge/releases-signed-brightgreen?logo=github)](SECURITY.md#verifying-a-release)
[![SLSA Build L3](https://slsa.dev/images/gh-badge-level3.svg)](SECURITY.md#verifying-a-release)

**Conformance and compliance**<br>
[![FHIRconnect mapping library](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirconnect-mapping-lib.json)](conformance/fhirconnect-mapping-lib/pass-list.txt)
[![FHIRconnect REST API](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Frest-api.json)](conformance/rest-api/pass-list.txt)
[![OMOCL mapping library](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fomocl.json)](conformance/omocl/pass-list.txt)
[![FHIR STU3](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-stu3.json)](conformance/fhir-stu3/pass-list.txt)
[![FHIR R4](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4.json)](conformance/fhir-r4/pass-list.txt)
[![FHIR R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4b.json)](conformance/fhir-r4b/pass-list.txt)
[![FHIR R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r5.json)](conformance/fhir-r5/pass-list.txt)
[![FHIR R6](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r6.json)](conformance/fhir-r6/pass-list.txt)
[![FHIR round-trip laws](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Froundtrip.json)](conformance/roundtrip/pass-list.txt)
[![FHIR R4 facade](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4-facade.json)](conformance/fhir-r4-facade/pass-list.txt)
[![FHIRPath R4 test suite, N1](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirpath-r4.json)](conformance/fhirpath-r4/pass-list.txt)
[![FHIRPath R4 test suite, 3.0.0 STU functions](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirpath-r4-stu3.json)](conformance/fhirpath-r4-stu3/pass-list.txt)
[![R4 profile validator, agreement with the HL7 FHIR Validator](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fvalidator-agreement.json)](conformance/validator-agreement/pass-list.txt)
[![HL7 v2 message corpora](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 smoke corpora (NIST, AIRA)](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-smoke.json)](conformance/hl7v2-smoke/pass-list.txt)
<br>
[![CRA](https://img.shields.io/badge/CRA-information%20for%20users-blue?logo=europeanunion)](https://ferrobridge.eu/docs/evaluate/cra-user-information.html)

<details>
<summary>HL7 v2 by message family and by version, across both corpora</summary>

[![HL7 v2 ACK](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-ack.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 ADT](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-adt.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 BAR](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-bar.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 DFT](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-dft.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 MDM](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-mdm.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 OMG](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-omg.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 OML](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-oml.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 ORL](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-orl.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 ORM](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-orm.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 ORU](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-oru.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 OUL](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-oul.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 PPR](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-ppr.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 RDE](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-rde.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 RDS](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-rds.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 REF](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-ref.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 SIU](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-siu.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 VXU](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-family-vxu.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.3](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.3.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.3.1](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.3.1.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.4](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.4.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.5](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.5.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.5.1](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.5.1.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.6](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.6.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2.8](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-version-2.8.json)](conformance/hl7v2/pass-list.txt)

</details>
<!-- badges:end -->

</div>


## What FerroBRIDGE is

Health data rarely lives in one format. A hospital may keep its records in
openEHR, an open standard for structured health records, while the apps that
need them speak HL7 FHIR, the exchange format most modern health software
uses, and the researchers who study them work in OMOP, the common data model
of the international OHDSI research network.

FerroBRIDGE translates between them, in pure Rust. It reads records from any
openEHR CDR it reaches over the ITS-REST API and offers them as FHIR, so a
FHIR app reads and writes them without knowing openEHR. It copies records
into an OMOP CDM database, so studies run over them. HL7 v2 messages, the
format many hospital systems send, come in through it as well. It stores no
clinical data of its own: the record stays in the CDR.

Every translation is a mapping file, written in one of two published
languages: FHIRconnect for FHIR and OMOCL for OMOP. A clinical informatician
reads and changes a mapping without a programmer, and FerroBRIDGE checks each
file against the published schema when it loads it. Nothing is hand-coded per
FHIR resource or per OMOP table.

It is for hospitals and regions that keep records in openEHR and share them
with FHIR apps, national exchanges or partners; for research groups and data
offices that want openEHR data in an OMOP database; and for vendors and
integrators that build FHIR applications over an openEHR back end or use its
FHIR and HL7 v2 libraries on their own.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="website/book/src/operate/img/console/dashboard-dark.png">
  <img src="website/book/src/operate/img/console/dashboard.png" alt="The FerroBRIDGE operator console dashboard">
</picture>

## What it implements

| Lane | Specifications | What it does |
|---|---|---|
| [The FHIR facade](https://ferrobridge.eu/docs/integrate/fhir-facade.html) | FHIRconnect 1.0.0, HL7 FHIR R4, SMART App Launch 2 | A FHIR R4 server over the CDR: read, vread, create, update, search, transaction, batch, `$validate` and `Composition/$document`, every request driven by the mappings and every access token checked against the SMART v2 scope its interaction needs |
| [The FHIRconnect operations](https://ferrobridge.eu/docs/integrate/operations.html) | FHIRconnect REST API | `$tofhir` and `$toopenehr`, which run a mapping as a pure transformation with no CDR |
| [The OMOP ETL](https://ferrobridge.eu/docs/integrate/omop-etl.html) | OMOCL 1.0.0, OMOP CDM v5.4 | `cdm init` creates a CDM v5.4 database and `etl run` fills it from the CDR over AQL, with [concept resolution](https://ferrobridge.eu/docs/integrate/omop-etl.html#concept-resolution-is-sql-not-terminology) as SQL over the OHDSI vocabulary you load, and [FHIR R4 loaded with no CDR](https://ferrobridge.eu/docs/integrate/omop-etl.html#loading-fhir-with-no-cdr) |
| [The HL7 v2 face](https://ferrobridge.eu/docs/operate/hl7v2-face.html) | HL7 v2 over MLLP, the HL7 v2-to-FHIR guide | An MLLP listener that maps each message through the guide's ConceptMaps and writes it into the CDR as one transaction, acknowledged once the CDR commits or refuses it |
| [The change feed](https://ferrobridge.eu/docs/integrate/change-feed.html) | openEHR ITS-REST 1.1.0 | Hands each commit of the CDR to [the events lane](https://ferrobridge.eu/docs/integrate/events.html), the incremental OMOP load and [the outbound FHIR lane](https://ferrobridge.eu/docs/integrate/fhir-outbound.html), which posts one transaction Bundle per changed composition |
| [The interoperability component](https://ferrobridge.eu/docs/evaluate/interoperability-component.html) | Regulation (EU) 2025/327 | The `eehrxf` crate: the European exchange format in FHIR R4 for prescriptions and dispensations, laboratory, imaging and discharge reports, mapped from openEHR through FHIRconnect |

[Writing and loading mappings](https://ferrobridge.eu/docs/integrate/mappings.html)
covers the model, extension and context files, `ferrobridge mapping check`,
and the operator API that reloads a running bridge.
[The operator console](https://ferrobridge.eu/docs/operate/operator-console.html)
shows the running mapping set, edits and checks a draft, and runs a mapping
in a playground.

## Quick start

The quickstart runs the published image with every lane off. You need
Docker Compose 2.23.1 or newer and `curl`, and no checkout. Every release
attaches the quickstart as one self-contained `compose.yaml`; download the
latest release's into an empty directory:

```sh
curl -fsSLO https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest/download/compose.yaml
```

Create the secret files it reads, empty for a lane you leave off:

<!-- tour:secrets -->
```sh
mkdir -p secrets && umask 077
touch secrets/cdm_url secrets/cdm_password secrets/cdr_password \
  secrets/terminology_password secrets/console_oidc_client_secret
```

Start the bridge and ask for liveness:

<!-- tour:start -->
```sh
docker compose up --wait
curl --fail-with-body -s http://127.0.0.1:8080/health/liveness
```

Every command above is a step of
[`scripts/quickstart/tour.sh`](scripts/quickstart/tour.sh), which CI runs
against `compose.yaml`. Turn a lane on by setting its variable:
`FERROBRIDGE__CDR__ENDPOINT__URL` reaches your openEHR CDR and
`FERROBRIDGE__TERMINOLOGY__ENDPOINT__URL` your terminology server. The header
of `compose.yaml` names every setting, and the `cdm` and `console` profiles
add an OMOP database and the operator console.
[Configuring the server](https://ferrobridge.eu/docs/operate/configuration.html)
continues from here.

## Deploy it

Every release publishes, from one tag:

- the bridge image `ghcr.io/ferrohealth/ferrobridge` and the console image
  `ghcr.io/ferrohealth/ferrobridge-console`, for `linux/amd64` and
  `linux/arm64`, on a distroless base;
- the static `ferrobridge` binary for Linux and the quickstart
  `compose.yaml` on the [GitHub release](https://github.com/FerroHEALTH/FerroBRIDGE/releases);
- the library crates whose version moved, on crates.io.

The latest release is [v0.0.24](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.24), cut on 2026-11-11.
Its image is `ghcr.io/ferrohealth/ferrobridge:0.0.24`.
The bridge runs beside an openEHR CDR over ITS-REST, a FHIR terminology
server and, for the OMOP side, a CDM v5.4 database with the OHDSI vocabulary
loaded ([What FerroBRIDGE runs beside](https://ferrobridge.eu/docs/operate/deployment-shape.html)).
[The container image](https://ferrobridge.eu/docs/operate/container.html)
covers the images, the probes and the compose profiles.

## Security and conformance

- **Who may call it:** the facade and the operator API are a SMART App
  Launch 2 resource server, and every access is written to the
  [access record](https://ferrobridge.eu/docs/operate/access-record.html)
  of Regulation (EU) 2025/327.
- **How it fails:** an upstream failure is reported with its status, never
  turned into an empty answer
  ([Failure and identity behaviour](https://ferrobridge.eu/docs/operate/failure-and-identity.html)).
- **What you install:** every release is signed and carries an SBOM and
  SLSA Build Level 3 provenance, and every scan finding an OpenVEX
  judgement ([verifying a release](SECURITY.md#verifying-a-release)).
- **Conformance:** each conformance badge above links to its pass list, the
  cases that pass against the published FHIRconnect and OMOCL mapping
  libraries, the HL7 FHIR examples per version, the HL7 FHIRPath test suite
  and the HL7 v2 message sets
  ([Checks and gates](https://ferrobridge.eu/docs/contribute/checks-and-gates.html)).

The source is public and every feature is in this repository; there is no
paid edition with extra functions.

## Crates

The mapping languages, the FHIR and HL7 v2 models and the clients the
bridge is built from are published as Rust crates. Pin a crate exactly:
every change moves it one patch
([Crate versions](https://ferrobridge.eu/docs/contribute/crate-versions.html)).

| Crate | What it does | Version | Docs | Downloads |
|---|---|---|---|---|
| [`fhir-types`](https://crates.io/crates/fhir-types) | The FHIR model for STU3, R4, R4B, R5 and R6, generated from the HL7 packages (Apache-2.0) | [![crates.io](https://img.shields.io/crates/v/fhir-types?logo=rust&label=crates.io)](https://crates.io/crates/fhir-types) | [![docs.rs](https://img.shields.io/docsrs/fhir-types?logo=docsdotrs&label=docs.rs)](https://docs.rs/fhir-types) | [![downloads](https://img.shields.io/crates/d/fhir-types?logo=rust&label=downloads)](https://crates.io/crates/fhir-types) |
| [`hl7v2-types`](https://crates.io/crates/hl7v2-types) | The HL7 v2 message structures, segments and data types, generated (Apache-2.0) | [![crates.io](https://img.shields.io/crates/v/hl7v2-types?logo=rust&label=crates.io)](https://crates.io/crates/hl7v2-types) | [![docs.rs](https://img.shields.io/docsrs/hl7v2-types?logo=docsdotrs&label=docs.rs)](https://docs.rs/hl7v2-types) | [![downloads](https://img.shields.io/crates/d/hl7v2-types?logo=rust&label=downloads)](https://crates.io/crates/hl7v2-types) |
| [`openehr-mapping-core`](https://crates.io/crates/openehr-mapping-core) | The mapping header, loader and registry that FHIRconnect and OMOCL share | [![crates.io](https://img.shields.io/crates/v/openehr-mapping-core?logo=rust&label=crates.io)](https://crates.io/crates/openehr-mapping-core) | [![docs.rs](https://img.shields.io/docsrs/openehr-mapping-core?logo=docsdotrs&label=docs.rs)](https://docs.rs/openehr-mapping-core) | [![downloads](https://img.shields.io/crates/d/openehr-mapping-core?logo=rust&label=downloads)](https://crates.io/crates/openehr-mapping-core) |
| [`fhirconnect`](https://crates.io/crates/fhirconnect) | The FHIRconnect mapping language: file model, validation, compiler and engine | [![crates.io](https://img.shields.io/crates/v/fhirconnect?logo=rust&label=crates.io)](https://crates.io/crates/fhirconnect) | [![docs.rs](https://img.shields.io/docsrs/fhirconnect?logo=docsdotrs&label=docs.rs)](https://docs.rs/fhirconnect) | [![downloads](https://img.shields.io/crates/d/fhirconnect?logo=rust&label=downloads)](https://crates.io/crates/fhirconnect) |
| [`omocl`](https://crates.io/crates/omocl) | The OMOCL mapping language and its engine, from openEHR to OMOP CDM rows | [![crates.io](https://img.shields.io/crates/v/omocl?logo=rust&label=crates.io)](https://crates.io/crates/omocl) | [![docs.rs](https://img.shields.io/docsrs/omocl?logo=docsdotrs&label=docs.rs)](https://docs.rs/omocl) | [![downloads](https://img.shields.io/crates/d/omocl?logo=rust&label=downloads)](https://crates.io/crates/omocl) |
| [`omop-cdm`](https://crates.io/crates/omop-cdm) | The OMOP CDM v5.4 row types, the OHDSI DDL, the vocabulary loader and the writer | [![crates.io](https://img.shields.io/crates/v/omop-cdm?logo=rust&label=crates.io)](https://crates.io/crates/omop-cdm) | [![docs.rs](https://img.shields.io/docsrs/omop-cdm?logo=docsdotrs&label=docs.rs)](https://docs.rs/omop-cdm) | [![downloads](https://img.shields.io/crates/d/omop-cdm?logo=rust&label=downloads)](https://crates.io/crates/omop-cdm) |
| [`ferrobridge-term`](https://crates.io/crates/ferrobridge-term) | The FHIR terminology client: $lookup, $translate, $validate-code, $expand and $subsumes | [![crates.io](https://img.shields.io/crates/v/ferrobridge-term?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-term) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-term?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-term) | [![downloads](https://img.shields.io/crates/d/ferrobridge-term?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-term) |
| [`ferrobridge-hl7v2`](https://crates.io/crates/ferrobridge-hl7v2) | The HL7 v2 face: MLLP, parsing, acknowledgments and the v2-to-FHIR map | [![crates.io](https://img.shields.io/crates/v/ferrobridge-hl7v2?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-hl7v2) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-hl7v2?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-hl7v2) | [![downloads](https://img.shields.io/crates/d/ferrobridge-hl7v2?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-hl7v2) |
| [`ferrobridge-fhirpath`](https://crates.io/crates/ferrobridge-fhirpath) | A FHIRPath 2.0.0 evaluator with the FHIR R4 additions | [![crates.io](https://img.shields.io/crates/v/ferrobridge-fhirpath?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-fhirpath) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-fhirpath?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-fhirpath) | [![downloads](https://img.shields.io/crates/d/ferrobridge-fhirpath?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-fhirpath) |
| [`ferrobridge-validator`](https://crates.io/crates/ferrobridge-validator) | A FHIR R4 and STU3 profile validator over packages read offline | [![crates.io](https://img.shields.io/crates/v/ferrobridge-validator?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-validator) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-validator?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-validator) | [![downloads](https://img.shields.io/crates/d/ferrobridge-validator?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-validator) |
| [`fhir-search`](https://crates.io/crates/fhir-search) | The FHIR R4 search value grammar: escaped value lists, tokens, string matching and dates with their prefixes and ranges | [![crates.io](https://img.shields.io/crates/v/fhir-search?logo=rust&label=crates.io)](https://crates.io/crates/fhir-search) | [![docs.rs](https://img.shields.io/docsrs/fhir-search?logo=docsdotrs&label=docs.rs)](https://docs.rs/fhir-search) | [![downloads](https://img.shields.io/crates/d/fhir-search?logo=rust&label=downloads)](https://crates.io/crates/fhir-search) |
| [`eehrxf`](https://crates.io/crates/eehrxf) | The European interoperability component of an EHR system under the EHDS Regulation: the exchange format in FHIR R4, mapped from openEHR | [![crates.io](https://img.shields.io/crates/v/eehrxf?logo=rust&label=crates.io)](https://crates.io/crates/eehrxf) | [![docs.rs](https://img.shields.io/docsrs/eehrxf?logo=docsdotrs&label=docs.rs)](https://docs.rs/eehrxf) | [![downloads](https://img.shields.io/crates/d/eehrxf?logo=rust&label=downloads)](https://crates.io/crates/eehrxf) |
| [`ferrobridge-operator-api`](https://crates.io/crates/ferrobridge-operator-api) | The health and operator documents the bridge and its console share | [![crates.io](https://img.shields.io/crates/v/ferrobridge-operator-api?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-operator-api) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-operator-api?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-operator-api) | [![downloads](https://img.shields.io/crates/d/ferrobridge-operator-api?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-operator-api) |

## Part of FerroHEALTH

FerroBRIDGE is one of five [FerroHEALTH](https://ferrohealth.eu) servers.
Each does one job and reaches the others only over published standards:
FerroEHR stores the record, FerroBRIDGE translates it to FHIR and the OMOP
research model, FerroFED connects records across organisations and borders,
FerroPIX matches patient identities, and FerroTERM serves the code systems.
FerroBRIDGE works with any openEHR CDR and any FHIR terminology server.

## Documentation

- [The FerroBRIDGE book](https://ferrobridge.eu/docs/): evaluate,
  integrate, operate and contribute, and the
  [FerroHEALTH book](https://ferrohealth.eu/docs/) for what the family
  shares
- [Architecture](https://ferrobridge.eu/docs/evaluate/architecture.html) and
  [the two mapping specifications](https://ferrobridge.eu/docs/evaluate/mapping-specifications.html)
- [Releases](https://ferrobridge.eu/docs/operate/releases.html), the
  [changelog](CHANGELOG.md) and the
  [GitHub releases](https://github.com/FerroHEALTH/FerroBRIDGE/releases)

## Support and contributing

- [Support](SUPPORT.md) and the [security policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md), [code of conduct](CODE_OF_CONDUCT.md),
  [governance](GOVERNANCE.md), [maintainers](MAINTAINERS.md) and the
  [AI statement](AI_STATEMENT.md)

## Licence

Business Source License 1.1 ([LICENSE](LICENSE)): free outside production
and for non-commercial production use, a commercial licence from Cadasto
B.V. for other production use, and Apache License 2.0 four years after each
version; `fhir-types` and `hl7v2-types` are Apache-2.0 today. The
[licensing page](https://ferrobridge.eu/docs/evaluate/licensing.html) says
which applies to you.
