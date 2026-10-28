<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->
<div align="center">

<img src="assets/brand/ferrobridge-lockup-auto.svg" alt="FerroBRIDGE" width="290">

**The bridge from openEHR health records to HL7 FHIR and the OMOP research model, driven by mapping files.**

[Documentation](https://ferrobridge.eu/docs/) ·
[Quick start](#quick-start) ·
[What ships today](#what-ships-today) ·
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

## What FerroBRIDGE does

Health data rarely lives in one format. A hospital may keep its records in
openEHR, an open standard for structured health records, while the apps that
need them speak HL7 FHIR, the exchange format most modern health software
uses, and the researchers who study them work in OMOP, the common data model
of the international OHDSI research network.

FerroBRIDGE translates between them. It reads records from any openEHR
record server and offers them as FHIR, so a FHIR app can read and write them
without knowing openEHR. It also copies records into an OMOP research
database, so studies can run over them. Hospital messages in HL7 v2, the
older format many hospital systems still send, come in through it as well.

Every translation is a mapping file, written in two published languages:
FHIRconnect for FHIR and OMOCL for OMOP. A clinical informatician can read
and change a mapping without a programmer, and FerroBRIDGE checks each file
against the published rules when it loads it. It stores no clinical data of
its own: the record stays in the record server.

### Who it is for

- **Hospitals and regions** that keep records in openEHR and need to share
  them with FHIR apps, national exchanges or partners.
- **Research groups and data offices** that want openEHR data in an OMOP
  database for observational studies.
- **Software vendors and integrators** that build FHIR applications over an
  openEHR back end, or need the FHIR and HL7 v2 libraries on their own.

### Why you can trust what it claims

Every release is signed and comes with a software bill of materials (the
list of every component inside) and SLSA build provenance, so you can check
that what you install is what was built here. The source is public, and
every feature is in this repository: there is no paid edition with extra
functions. Each conformance badge above links to its pass list, the cases
that pass against the published test suites of FHIRconnect, OMOCL, FHIR,
FHIRPath and HL7 v2, so you can check a result against the standard
yourself.

## Part of FerroHEALTH

FerroBRIDGE is one of five [FerroHEALTH](https://ferrohealth.eu) servers.
Each does one job and reaches the others only over published standards:
FerroEHR stores the record, FerroBRIDGE translates it to FHIR and the OMOP
research model, FerroFED connects records across organisations and borders,
FerroPIX matches patient identities, and FerroTERM serves the code systems
(the shared lists of medical terms). FerroBRIDGE works with any openEHR
record server and any FHIR terminology server, not only FerroEHR and
FerroTERM. Sign-in, the audit trail and the web consoles come from three
shared libraries: FerroAUTH, FerroAUDIT and FerroUI.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="website/book/src/operate/img/console/dashboard-dark.png">
  <img src="website/book/src/operate/img/console/dashboard.png" alt="The FerroBRIDGE operator console dashboard">
</picture>

## Quick start

The latest release is [v0.0.13](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.13), cut on 2026-10-25.
Download its `compose.yaml`, create the four secret files it reads (empty
for a lane you leave off), and start the bridge:

```sh
curl -LO https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest/download/compose.yaml
mkdir -p secrets
touch secrets/cdm_url secrets/cdm_password secrets/cdr_password secrets/terminology_password
docker compose up
```

In a second terminal, check that it runs:

```sh
curl http://127.0.0.1:8080/health/liveness
```

Every lane is off until you configure it: set `FERROBRIDGE__CDR__BASE_URL`
to reach your openEHR record server and `FERROBRIDGE__TERMINOLOGY__BASE_URL`
for your terminology server. The header of `compose.yaml` names every
setting, and the `cdm` and `console` profiles add an OMOP database and the
operator console. The image is `ghcr.io/ferrohealth/ferrobridge:0.0.13`.
[Configuring the server](https://ferrobridge.eu/docs/operate/configuration.html)
and [the container image](https://ferrobridge.eu/docs/operate/container.html)
continue from here.

## What ships today

### FHIR

- [The FHIR R4 facade](https://ferrobridge.eu/docs/integrate/fhir-facade.html) over the record server: create, read, update, transaction, batch, search and `$validate`
- [The FHIRconnect operations](https://ferrobridge.eu/docs/integrate/operations.html) `$tofhir` and `$toopenehr`, which run a mapping without a record server
- [SMART App Launch](https://ferrobridge.eu/docs/integrate/fhir-facade.html): every access token checked, with SMART v2 scopes per interaction
- [Mappings in FHIRconnect](https://ferrobridge.eu/docs/integrate/mappings.html), checked against the published schemas when they load

### OMOP

- [The OMOP ETL](https://ferrobridge.eu/docs/integrate/omop-etl.html): `cdm init` creates a CDM v5.4 database, `etl run` fills it from the record server through OMOCL
- [FHIR R4 into OMOP with no record server](https://ferrobridge.eu/docs/integrate/omop-etl.html#loading-fhir-with-no-cdr)
- [Concept resolution](https://ferrobridge.eu/docs/integrate/omop-etl.html#concept-resolution-is-sql-not-terminology) over the OHDSI vocabulary you load

### HL7 v2

- [The HL7 v2 face](https://ferrobridge.eu/docs/operate/hl7v2-face.html): an MLLP listener that maps each message through HL7's v2-to-FHIR guide and writes it as one transaction

### Operations

- [The operator console](https://ferrobridge.eu/docs/operate/operator-console.html): engine state, mapping files, a transform playground and a validation dry run
- [The operator API](https://ferrobridge.eu/docs/operate/operator-api.html) behind the same sign-in
- [Failure and identity behaviour](https://ferrobridge.eu/docs/operate/failure-and-identity.html): an upstream failure is always reported, never hidden
- [Container images and release binaries](https://ferrobridge.eu/docs/operate/container.html) for amd64 and arm64, signed, with a software bill of materials

### Libraries

- The FHIR model for STU3 to R6, HL7 v2 tables, a FHIRPath evaluator, a FHIR R4 profile validator and a terminology client, published as Rust crates (listed below)

<details>
<summary><b>Published crates</b> (12)</summary>

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
| [`eehrxf`](https://crates.io/crates/eehrxf) | The European interoperability component of an EHR system under the EHDS Regulation: the exchange format in FHIR R4, mapped from openEHR | [![crates.io](https://img.shields.io/crates/v/eehrxf?logo=rust&label=crates.io)](https://crates.io/crates/eehrxf) | [![docs.rs](https://img.shields.io/docsrs/eehrxf?logo=docsdotrs&label=docs.rs)](https://docs.rs/eehrxf) | [![downloads](https://img.shields.io/crates/d/eehrxf?logo=rust&label=downloads)](https://crates.io/crates/eehrxf) |
| [`ferrobridge-operator-api`](https://crates.io/crates/ferrobridge-operator-api) | The health and operator documents the bridge and its console share | [![crates.io](https://img.shields.io/crates/v/ferrobridge-operator-api?logo=rust&label=crates.io)](https://crates.io/crates/ferrobridge-operator-api) | [![docs.rs](https://img.shields.io/docsrs/ferrobridge-operator-api?logo=docsdotrs&label=docs.rs)](https://docs.rs/ferrobridge-operator-api) | [![downloads](https://img.shields.io/crates/d/ferrobridge-operator-api?logo=rust&label=downloads)](https://crates.io/crates/ferrobridge-operator-api) |

</details>

## Learn more

- [Documentation](https://ferrobridge.eu/docs/) and the [FerroHEALTH book](https://ferrohealth.eu/docs/)
- [Architecture](https://ferrobridge.eu/docs/evaluate/architecture.html) and [the two mapping specifications](https://ferrobridge.eu/docs/evaluate/mapping-specifications.html)
- [Crate versions](https://ferrobridge.eu/docs/contribute/crate-versions.html): pin a crate exactly, every change moves it one patch
- [Changelog](CHANGELOG.md) and [milestones](https://github.com/FerroHEALTH/FerroBRIDGE/milestones)
- [Security policy](SECURITY.md), with [verifying a release](SECURITY.md#verifying-a-release)
- [Contributing](CONTRIBUTING.md), [code of conduct](CODE_OF_CONDUCT.md), [support](SUPPORT.md), [governance](GOVERNANCE.md), [maintainers](MAINTAINERS.md) and the [AI statement](AI_STATEMENT.md)

## Licence

Business Source License 1.1 ([LICENSE](LICENSE)), with Apache License 2.0 as
the Change License four years after each version; `fhir-types` and
`hl7v2-types` are Apache-2.0 today. The
[licensing page](https://ferrobridge.eu/docs/evaluate/licensing.html) says
when you need a commercial licence from Cadasto B.V.
