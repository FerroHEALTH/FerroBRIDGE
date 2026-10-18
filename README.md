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

HL7 v2, by corpus, then by message family and by version across both corpora:
[![HL7 v2 message corpora](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2.json)](conformance/hl7v2/pass-list.txt)
[![HL7 v2 smoke corpora (NIST, AIRA)](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroBRIDGE%2Fmain%2Fconformance%2Fbadges%2Fhl7v2-smoke.json)](conformance/hl7v2-smoke/pass-list.txt)
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
<!-- badges:end -->

A pure-Rust, standalone bridge between openEHR and two interoperability
targets: HL7 FHIR, driven by the FHIRconnect specification (model-mapping and
contextual-mapping YAML validated against its published JSON schemas), and the
OMOP Common Data Model, driven by the OMOCL specification. Mappings are
specification-conformant YAML, never hand-coded per resource or table. It runs
as its own server beside any openEHR CDR reached over the openEHR ITS-REST API,
never as a compile-time dependency of one, and uses any FHIR terminology server
for code systems and value sets.

## Status: designed, not yet built

The research pass over the specifications and the prior art is done and its
result is [`docs/architecture.md`](docs/architecture.md): FHIRconnect and OMOCL
are two mapping languages sharing one header, so FerroBRIDGE is one shared
foundation with two interpreters and two sinks; it pins FHIRconnect v1.0.0 on
FHIR R4 and OMOCL v1.0.0 on OMOP CDM v5.4; the FHIR side is a REST facade over
the CDR and the OMOP side a batch ETL into a CDM database. The first milestone is
one verbatim round trip per target on upstream mapping files. Follow
[issue #1](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1) for the
decisions and their citations.

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

## What is decided

- **A standalone server.** FerroBRIDGE is its own process and its own image. It
  is not a crate inside a CDR, not a plugin, and not a library a CDR links. It
  reaches the CDR over ITS-REST like any other client, which is what lets it
  work against a CDR it did not build.
- **Mapping-driven, never hand-coded.** Mappings are specification-conformant
  YAML validated against the published schemas. A hand-written per-resource or
  per-table converter is refused in review, because it is the thing this
  project exists to avoid.
- **Pure Rust.** No JVM, and a single binary, like the rest of the family.
- **Business Source License 1.1.** Free for every non-production use and for
  non-commercial production use, a commercial licence for production use in a
  business: [do you need a commercial
  licence?](#do-you-need-a-commercial-licence)

## What is open

The crate layout, the engine design, whether a generated model layer exists on
the FHIR side or the openEHR side, the version pins for every specification,
and the acceptance instrument. Each is a question for issue #1 rather than an
assumption, and the answer arrives with its citation.

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
worklist. While the project is in its design phase the most useful contribution
is evidence: a specification citation, a measurement, or first-hand experience
with FHIRconnect, OMOCL, or an openEHR CDR in production.

- [`CLAUDE.md`](CLAUDE.md): the working discipline, in full.
- [`AI_STATEMENT.md`](AI_STATEMENT.md): how AI tools are used to build this,
  and what they are not allowed to do.
- [`GOVERNANCE.md`](GOVERNANCE.md), [`MAINTAINERS.md`](MAINTAINERS.md): who
  decides, and the honest answer about the bus factor.
- [`SECURITY.md`](SECURITY.md): report a vulnerability privately.
