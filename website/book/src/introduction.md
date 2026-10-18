<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroBRIDGE is a pure-Rust, standalone bridge between openEHR and two
interoperability targets: HL7 FHIR, driven by the FHIRconnect specification,
and the OMOP Common Data Model, driven by the OMOCL specification. Mappings are
specification-conformant YAML validated against published schemas, never
hand-coded per FHIR resource or per OMOP table. FerroBRIDGE runs as its own
server beside any openEHR CDR it reaches over the openEHR ITS-REST API, and it
asks a FHIR terminology server for code systems and value sets.

## Where the project is

FerroBRIDGE is on the 0.0.x release line. Each release ships the `ferrobridge`
binary for Linux (x86_64 and aarch64, glibc and musl), the container image
and the published library crates, and the [changelog](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/CHANGELOG.md)
says what each one added. The design is recorded in the
[architecture](evaluate/architecture.md), and the build order there names what
the next releases add.

What the design settles: one shared mapping foundation with two interpreters
and two sinks, FHIRconnect v1.0.0 on FHIR R4 and OMOCL v1.0.0 on OMOP CDM v5.4,
a FHIR facade over the CDR on one side and a batch ETL into a CDM database on
the other. The [build order](evaluate/build-order.md) page lists what each
release shipped and what the next milestones hold.

## What this book leaves to the family book

FerroBRIDGE is one product of the FerroHEALTH family, and Cadasto B.V. is the
manufacturer of every product. This book documents FerroBRIDGE alone. The
manufacturer, the shared security policy and support period, the Cyber
Resilience Act's manufacturer side, how the products fit together, and the
licensing and trademark terms are written once, in the
[FerroHEALTH book](https://ferrohealth.eu/docs/).

## How this book is organised

The four parts follow what you came to do.

- **Evaluate** answers whether FerroBRIDGE fits your problem: the design, the
  two mapping languages it reads, the version pins, the build order, and the
  licence.
- **Operate** covers what the running bridge needs around it and how it behaves
  when a mapping or an upstream call fails.
- **Integrate** covers the two target surfaces and the mapping files that drive
  them.
- **Contribute** covers how the work is tracked and which checks a change has
  to pass.

The tracker is the scope. Open issues are the worklist, milestones are
releases, and the
[roadmap](https://github.com/FerroHEALTH/FerroBRIDGE/milestones) is the
public view of both.
