<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# <img src="assets/brand/ferrobridge-lockup-auto.svg" alt="FerroBRIDGE" width="284" height="64">

[Documentation](#documentation) · [**Do you need a commercial licence?**](#do-you-need-a-commercial-licence) · [Contributing](#contributing) · [Security](SECURITY.md)

<!-- badges:begin -->
[![CI](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml/badge.svg)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/ci.yml)
[![CodeQL](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml/badge.svg)](https://github.com/FerroHEALTH/FerroBRIDGE/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroBRIDGE/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroBRIDGE)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=FerroHEALTH_FerroBRIDGE&metric=alert_status)](https://sonarcloud.io/summary/overall?id=FerroHEALTH_FerroBRIDGE)
[![Coverage](https://sonarcloud.io/api/project_badges/measure?project=FerroHEALTH_FerroBRIDGE&metric=coverage)](https://sonarcloud.io/summary/new_code?id=FerroHEALTH_FerroBRIDGE)
[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)
[![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/FerroHEALTH/FerroBRIDGE?sort=semver)](https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest)

**Conformance**, measured by the corpus tests under `conformance/`; each badge links to its pass list.

FHIRconnect 1.0.0:
[![FHIRconnect mapping library](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirconnect-mapping-lib.json)](conformance/fhirconnect-mapping-lib/pass-list.txt)
[![FHIRconnect REST API](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Frest-api.json)](conformance/rest-api/pass-list.txt)

OMOCL 1.0.0:
[![OMOCL mapping library](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fomocl.json)](conformance/omocl/pass-list.txt)

FHIR, the model per version through fhir-types, then R4 through the mappings and the facade:
[![FHIR STU3](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-stu3.json)](conformance/fhir-stu3/pass-list.txt)
[![FHIR R4](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4.json)](conformance/fhir-r4/pass-list.txt)
[![FHIR R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4b.json)](conformance/fhir-r4b/pass-list.txt)
[![FHIR R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r5.json)](conformance/fhir-r5/pass-list.txt)
[![FHIR R6](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r6.json)](conformance/fhir-r6/pass-list.txt)
[![FHIR round-trip laws](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Froundtrip.json)](conformance/roundtrip/pass-list.txt)
[![FHIR R4 facade](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhir-r4-facade.json)](conformance/fhir-r4-facade/pass-list.txt)

FHIRPath with the FHIR R4 additions, N1 2.0.0, then the tests of the functions 3.0.0 adds as STU, behind their opt-in:
[![FHIRPath R4 test suite, N1](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirpath-r4.json)](conformance/fhirpath-r4/pass-list.txt)
[![FHIRPath R4 test suite, 3.0.0 STU functions](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Ffhirpath-r4-stu3.json)](conformance/fhirpath-r4-stu3/pass-list.txt)

The R4 profile validator against the HL7 FHIR Validator 6.10.4 on a synthetic corpus, every disagreement adjudicated:
[![R4 profile validator, agreement with the HL7 FHIR Validator](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fvalidator-agreement.json)](conformance/validator-agreement/pass-list.txt)

HL7 v2, by corpus:
[![HL7 v2 message corpora](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 smoke corpora (NIST, AIRA)](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-smoke.json)](conformance/hl7v2-smoke/pass-list.txt)

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

FerroBRIDGE is a standalone bridge between openEHR and two interoperability
targets. It maps openEHR to HL7 FHIR through the FHIRconnect specification and
to the OMOP Common Data Model through the OMOCL specification. Mappings are
specification-conformant YAML files that FerroBRIDGE validates against the
published schemas; nothing is hand-coded per FHIR resource or per OMOP table.

It is written in Rust and runs as its own server. It reaches any openEHR CDR
over the openEHR ITS-REST API and any FHIR terminology server for code systems
and value sets. FerroEHR is the reference CDR and FerroTERM the reference
terminology server; neither is a compile-time dependency.

The latest release is
[v0.0.8](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.8),
cut on 2026-10-18. [`CHANGELOG.md`](CHANGELOG.md) lists what every release
added, changed and fixed.

## What ships

- **The `ferrobridge` binary and its container image**,
  `ghcr.io/ferrohealth/ferrobridge`. Each release attaches a Linux binary per
  architecture and pushes a multi-platform image, each with provenance and an
  SBOM you can verify ([`SECURITY.md`](SECURITY.md#verifying-a-release)).
- **A FHIR R4 facade over the CDR**, mounted under `/fhir`. It answers
  `create`, `read`, `vread`, `update`, `transaction`, `batch` and `$validate`,
  and search over the AQL projections a `*.search.yml` file declares beside a
  context mapping. It stores no clinical data. It can act as a SMART App
  Launch 2.x resource server that checks every access token, with SMART v2
  scopes per interaction and the patient compartment.
- **The `$tofhir` and `$toopenehr` operations**, which run a FHIRconnect
  mapping without a CDR.
- **The OMOP ETL**: `ferrobridge cdm init` creates a CDM v5.4 schema, and
  `ferrobridge etl run` reads compositions over AQL and writes OMOP rows
  through the OMOCL engine.
- **An HL7 v2 face**: an MLLP listener that maps each message through the
  HL7 v2-to-FHIR guide's ConceptMaps and writes it through the facade as one
  transaction.
- **A FHIRPath evaluator and a FHIR R4 profile validator**, as library crates.
  The evaluator implements FHIRPath N1 2.0.0 with the FHIR R4 additions. The
  validator holds a resource to the profiles of a package set it reads offline
  and lists every constraint it could not evaluate.
- **The operator console**, `ghcr.io/ferrohealth/ferrobridge-viewer`: a web
  console behind OpenID Connect sign-in. It shows the engine state, lists and
  edits the mapping files, and runs the transform playground and the
  validation dry run.
- **Library crates on crates.io**, listed [below](#crates).

The conformance badges at the top of this page measure each part against its
corpus; every badge links to its pass list.

## Run it

[`compose.yaml`](compose.yaml) is a standalone quickstart. Download it, create
the secret files it lists in its header, and start the bridge:

```bash
docker compose up
curl http://127.0.0.1:8080/health/liveness
```

Every lane is off until you configure it, so an unconfigured server answers
`/` and the health routes and nothing else. You turn a lane on with
environment variables: `FERROBRIDGE__CDR__BASE_URL` for the CDR,
`FERROBRIDGE__TERMINOLOGY__BASE_URL` for the terminology server and
`FERROBRIDGE__CDM__URL_FILE` for the CDM database. The file names every key.
Three profiles add services:

- `cdm` starts a PostgreSQL CDM database and applies the CDM v5.4 DDL;
- `vocab` runs `vocab load` over an OHDSI Athena export you downloaded
  yourself. That job does not load yet: it exits naming
  [#233](https://github.com/FerroHEALTH/FerroBRIDGE/issues/233);
- `viewer` runs the operator console beside the bridge.

```bash
docker compose --profile cdm up
docker compose --profile viewer up
```

To run the image without compose, pull
`ghcr.io/ferrohealth/ferrobridge:0.0.8`. To run the binary, download the
archive for your architecture from the
[release](https://github.com/FerroHEALTH/FerroBRIDGE/releases/latest). The
binary's subcommands are `serve`, `cdm init`, `etl run`, `mapping check` and
`vocab load`. The book's
[configuration](https://ferrobridge.eu/docs/operate/configuration.html) and
[container](https://ferrobridge.eu/docs/operate/container.html) pages cover
both in full.

## Crates

The library crates are published on crates.io. `fhir-types`,
`openehr-mapping-core`, `fhirconnect`, `ferrobridge-term`,
`ferrobridge-fhirpath` and `ferrobridge-validator` are on the crate line; the
other crates hold their names at 0.0.0 until their first release on it. The book's
[crate versions](https://ferrobridge.eu/docs/contribute/crate-versions.html)
page explains how they move: every change moves a crate one patch, a breaking
change included, so pin a crate exactly (for example `fhirconnect = "=0.1.136"`).

| Crate | What it is | crates.io |
|---|---|---|
| `fhir-types` | the FHIR model STU3 to R6, generated from the HL7 packages (Apache-2.0) | [![fhir-types](https://img.shields.io/crates/v/fhir-types.svg)](https://crates.io/crates/fhir-types) |
| `openehr-mapping-core` | the mapping header, loader and registry FHIRconnect and OMOCL share | [![openehr-mapping-core](https://img.shields.io/crates/v/openehr-mapping-core.svg)](https://crates.io/crates/openehr-mapping-core) |
| `fhirconnect` | the FHIRconnect language: its file model, path model, compiler and engine | [![fhirconnect](https://img.shields.io/crates/v/fhirconnect.svg)](https://crates.io/crates/fhirconnect) |
| `omocl` | the OMOCL language and its engine | [![omocl](https://img.shields.io/crates/v/omocl.svg)](https://crates.io/crates/omocl) |
| `omop-cdm` | the OMOP CDM v5.4 row types, DDL and writer | [![omop-cdm](https://img.shields.io/crates/v/omop-cdm.svg)](https://crates.io/crates/omop-cdm) |
| `ferrobridge-term` | the FHIR terminology client | [![ferrobridge-term](https://img.shields.io/crates/v/ferrobridge-term.svg)](https://crates.io/crates/ferrobridge-term) |
| `hl7v2-types` | the HL7 v2 tables, generated (Apache-2.0) | [![hl7v2-types](https://img.shields.io/crates/v/hl7v2-types.svg)](https://crates.io/crates/hl7v2-types) |
| `ferrobridge-hl7v2` | the HL7 v2 face: MLLP, parsing and the v2-to-FHIR map | [![ferrobridge-hl7v2](https://img.shields.io/crates/v/ferrobridge-hl7v2.svg)](https://crates.io/crates/ferrobridge-hl7v2) |
| `ferrobridge-fhirpath` | the FHIRPath evaluator | [![ferrobridge-fhirpath](https://img.shields.io/crates/v/ferrobridge-fhirpath.svg)](https://crates.io/crates/ferrobridge-fhirpath) |
| `ferrobridge-validator` | the FHIR R4 profile validator | [![ferrobridge-validator](https://img.shields.io/crates/v/ferrobridge-validator.svg)](https://crates.io/crates/ferrobridge-validator) |

## Documentation

The site is <https://ferrobridge.eu/>, and the book lives under
<https://ferrobridge.eu/docs/>. It is organised by what you came to do:

- [Architecture](https://ferrobridge.eu/docs/evaluate/architecture.html) and
  [the two mapping specifications](https://ferrobridge.eu/docs/evaluate/mapping-specifications.html),
  to judge whether FerroBRIDGE fits your problem.
- [What FerroBRIDGE runs beside](https://ferrobridge.eu/docs/operate/deployment-shape.html)
  and [failure and identity behaviour](https://ferrobridge.eu/docs/operate/failure-and-identity.html),
  to plan a deployment.
- [The FHIR facade](https://ferrobridge.eu/docs/integrate/fhir-facade.html),
  [the OMOP ETL](https://ferrobridge.eu/docs/integrate/omop-etl.html), and
  [writing and loading mappings](https://ferrobridge.eu/docs/integrate/mappings.html),
  to integrate with it.
- [How the work is organised](https://ferrobridge.eu/docs/contribute/how-the-work-is-organised.html)
  and [checks and gates](https://ferrobridge.eu/docs/contribute/checks-and-gates.html),
  to contribute.

FerroBRIDGE is one product of the FerroHEALTH family, made by Cadasto B.V.
What holds for every product is written once, in the family book at
<https://ferrohealth.eu/docs/>:
[the manufacturer](https://ferrohealth.eu/docs/manufacturer.html),
[the security policy](https://ferrohealth.eu/docs/security.html) and
[post-market support](https://ferrohealth.eu/docs/post-market.html),
[the Cyber Resilience Act](https://ferrohealth.eu/docs/cra.html),
[how the products fit together](https://ferrohealth.eu/docs/architecture.html),
and [licensing and trademarks](https://ferrohealth.eu/docs/licensing.html).

## How it is built

- **A standalone server.** FerroBRIDGE is its own process and its own image.
  It reaches the CDR over ITS-REST like any other client, so it works against
  a CDR it did not build.
- **Mapping-driven.** A FHIRconnect or OMOCL file is validated against its
  schema at load, and a file the engine cannot compile is refused at load,
  never on the first request that touches it.
- **One foundation, two interpreters.** FHIRconnect and OMOCL share a header,
  a loader and a registry (`openehr-mapping-core`); each language keeps its
  own parser, validator and engine.
- **Pure Rust.** One binary and no JVM, like the rest of the FerroHEALTH
  family.

[`docs/architecture.md`](docs/architecture.md) records every decision with its
ground, including where no specification governs a choice and FerroBRIDGE
makes its own. Its section 14 is the build order, release by release.

## Do you need a commercial licence?

FerroBRIDGE is source-available under the Business Source License 1.1, which
is not an OSI-approved open-source licence. [`LICENSE`](LICENSE) is the
authority and names the Licensor, the Licensed Work, the Additional Use Grant
and the Change Date; this table is the same boundary in the order people ask
about it, and the `Why` column names the part of `LICENSE` that decides each
row.

| What you are doing | What you need | Why |
|---|---|---|
| Reading, building, modifying or redistributing the source | Free | Terms: "the right to copy, modify, create derivative works, redistribute". |
| Development, testing, evaluation, prototyping | Free | Terms: the right to "make non-production use of the Licensed Work". |
| Production use for Non-Commercial Purposes | Free | Additional Use Grant: personal use, academic or scientific research, teaching, and use by a non-profit organisation or public body that is not in the course of a business, does not deliver a service for payment, and is not for commercial advantage. |
| A hospital, clinic or care provider running it for its patients | Commercial licence | Additional Use Grant: "the delivery of health care or any other service for payment, requires a commercial license". |
| A vendor or integrator, or any company running it in production | Commercial licence | Additional Use Grant: production use in the course of a business is outside Non-Commercial Purposes, and "Any other production use" needs a commercial license. |
| Offering it, or a work derived from it, to third parties as a hosted, managed or embedded service | Commercial licence | Additional Use Grant (a): a service "through which anyone other than you and your affiliates maps, converts, or exchanges health data", excluded whoever you are. |
| Selling, sublicensing or otherwise distributing it for a fee | Commercial licence | Additional Use Grant (b): excluded whoever you are, on its own or as a component of another product. |

The last two rows hold whatever else you are: they need a commercial licence
even for an organisation the rows above would otherwise leave free.

**Each version becomes Apache License 2.0 four years after that version is
published** (`LICENSE`, Change Date and Change License). A commercial licence
is arranged with Cadasto B.V., the Licensor, which handles the business side
of FerroBRIDGE: write to [info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Technical questions go to the maintainer
named in [MAINTAINERS.md](MAINTAINERS.md).

There is no open-core tier: the bridge, the server and the tools are in this
repository under the one licence. Two published crates are under the Apache
License 2.0 instead, because they are generated models meant for any Rust
project: `fhir-types` and `hl7v2-types`, each with its own `LICENSE`. Every
other published crate is BUSL-1.1 on the terms above, which its `Cargo.toml`
`license` field and its README state.

Contributions carry the terms in
[CONTRIBUTING.md](CONTRIBUTING.md#licensing-of-contributions): you keep your
copyright, and you grant the Licensor the relicensing right that keeps the work
one work under one licensor. There is no separate agreement to sign. Vendored
specifications and third-party material keep their upstream terms, recorded
beside them. The licensing and trademark terms every FerroHEALTH product
shares are on the family book's
[licensing page](https://ferrohealth.eu/docs/licensing.html).

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the rules, and the open issues are the
worklist, grouped into release milestones. A specification citation, a
measurement, or first-hand experience with FHIRconnect, OMOCL or an openEHR CDR
in production is as useful as code.

- [`CLAUDE.md`](CLAUDE.md): the working discipline, in full.
- [`AI_STATEMENT.md`](AI_STATEMENT.md): how AI tools are used to build this,
  and what they are not allowed to do.
- [`GOVERNANCE.md`](GOVERNANCE.md), [`MAINTAINERS.md`](MAINTAINERS.md): who
  decides, and the honest answer about the bus factor.
- [`SECURITY.md`](SECURITY.md): report a vulnerability privately.
