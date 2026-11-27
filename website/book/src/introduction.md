<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroBRIDGE

FerroBRIDGE is a standalone bridge between openEHR and two interoperability
targets: HL7 FHIR R4, driven by FHIRconnect mappings, and the OMOP Common
Data Model v5.4, driven by OMOCL mappings. It runs as its own server beside
any openEHR CDR it reaches over openEHR ITS-REST 1.1.0, and asks a FHIR
terminology server for code systems and value sets. Mappings are
specification-conformant YAML, never code written per FHIR resource or per
OMOP table.

## Start here

| You are | You want to | Start with | Then |
|---|---|---|---|
| An integrator | Connect a FHIR client, a source system or an analytics pipeline to openEHR data | [Your first mapping](tutorials/first-mapping.md) | [The FHIR facade](integrate/fhir-facade.md), [The inbound intake](integrate/inbound.md), [The change feed](integrate/change-feed.md), [The OMOP ETL](integrate/omop-etl.md) |
| A mapping author | Write and check FHIRconnect and OMOCL files | [Your first mapping](tutorials/first-mapping.md) | [Writing and loading mappings](integrate/mappings.md), [The two mapping specifications](evaluate/mapping-specifications.md), [How FerroBRIDGE maps](explanation/how-ferrobridge-maps.md) |
| An operator | Deploy, configure, upgrade and watch the bridge | [What FerroBRIDGE runs beside](operate/deployment-shape.md) | [The container image](operate/container.md), [Configuration](operate/configuration.md), [The state store and high availability](operate/high-availability.md), [Upgrading](operate/upgrades.md) |
| A compliance or procurement reader | Judge the bridge against the EHDS Regulation, the CRA and the licence | [How FerroBRIDGE maps](explanation/how-ferrobridge-maps.md) | [EHDS readiness](evaluate/ehds-readiness.md), [CRA information for users](evaluate/cra-user-information.md), [Licensing](evaluate/licensing.md) |

## How this book is organised

The parts follow the Diátaxis framework (<https://diataxis.fr/>):

- **Tutorials** take you through a first result, step by step.
- **How-to guides** answer one task each, for integrators and for operators.
- **Reference** lists the configuration, the command line, the operator API
  and the problem types, generated from the code where it can be.
- **Explanation** says how the parts fit and which specification governs
  each.
- **Compliance** holds the EHDS and CRA material.
- **Contribute** covers how the work is tracked and which checks a change
  passes.

FerroBRIDGE is one product of the FerroHEALTH family. The manufacturer, the
shared security policy and support period, and the licensing and trademark
terms every product shares are written once, in the [FerroHEALTH
book](https://ferrohealth.eu/docs/).
