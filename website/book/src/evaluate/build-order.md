<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Build order

FerroBRIDGE ships on the 0.0.x line. A milestone on the tracker is a release:
it holds one theme, and the release is cut when the milestone has no open issue
left. This page lists what each release shipped and what the next milestones
hold, in the order
[`docs/architecture.md`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/docs/architecture.md)
section 14 sets. [`CHANGELOG.md`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/main/CHANGELOG.md)
has every change of every release.

## Released

| Release | Date | What it shipped |
|---|---|---|
| [v0.0.1](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.1) | 2026-09-05 | The release lane, the brand assets and the working discipline. No code yet. |
| [v0.0.2](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.2) | 2026-09-13 | The foundation: the Cargo workspace, `fhir-types` and its generator moved in from the sibling terminology server, the generated OMOP CDM v5.4 layer, `openehr-mapping-core`, the ITS-REST and terminology clients, the `ferrobridge` binary, the container image and the release lane with its crates.io leg. |
| [v0.0.3](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.3) | 2026-09-25 | The FHIR round trip: the `fhirconnect` crate, the `$tofhir` and `$toopenehr` operations, the FHIR R4 facade and its identity store. |
| [v0.0.4](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.4) | 2026-09-26 | The OMOP round trip: the `omocl` crate, the concept resolver, `cdm init` and `etl run`; the conformance gate and its badges; and the HL7 v2 face. |
| [v0.0.5](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.5) | 2026-10-01 | The repository's move to the FerroHEALTH organization, with the image at `ghcr.io/ferrohealth/ferrobridge`. |
| [v0.0.6](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.6) | 2026-10-17 | FHIR STU3 beside R4 to R6; the FHIRPath evaluator with UCUM; the profile validator's foundation, slicing, bindings, bundles and invariants; FHIR search over AQL projections; SMART App Launch on the facade and OAuth toward the CDR; the OMOP PERSON rows and the deletion check. |
| [v0.0.7](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.7) | 2026-10-17 | Facade hardening: search answers the entry that matched, every error answers an `OperationOutcome`, and the identity store gains its repair paths. |
| [v0.0.8](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.8) | 2026-10-18 | The operator console and its image, the operator API and `ferrobridge mapping check`; and most of the profile validator work planned for v0.0.9: the agreement corpus against the HL7 FHIR Validator, and the extension context and reference versioning checks. |
| [v0.0.9](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.9) | 2026-10-19 | The profile validator finished: STU3 profiles validating STU3 resources over the release-neutral definition read, and the validator, terminology client and FHIRPath crates on the crate line at 0.1.126. FHIRconnect sub-contexts, and the openEHR RM pinned to Release 1.1.0. And the first of the openEHR demographics work planned for v0.0.10: the Demographic API behind `[cdr] demographic`, the configured patient-to-EHR link, and demographic references resolved on the way into openEHR. |
| [v0.0.10](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.10) | 2026-10-19 | openEHR demographics: FHIRconnect party contexts that build a `PERSON`, `ORGANISATION` or `ROLE`, and `Patient`, `Practitioner`, `PractitionerRole` and `Organization` on the facade over the Demographic API, searched by identifier and name through the Cadasto AQL demographic extension. Clinical searches chained through the patient identifier, a created `Patient` given its EHR, OAuth 2.0 client credentials toward the terminology server, and the static bearer token removed. |
| [v0.0.11](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.11) | 2026-10-24 | The operator console rebuilt on FerroUI 0.3.6 and renamed `ferrobridge-console`. Release images scanned before they are tagged, a vulnerability record with every finding judged, the support period and the CRA user information in the notes. The last `mappingCode` functions of the published FHIRconnect library, display resolution, `NULL_FLAVOUR`, the `EVENT` choice and the quantity, temporal, URI, string and interval data-type pairs. |
| [v0.0.12](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.12) | 2026-10-25 | FHIRconnect and facade outcomes: a write shows the client the losses its mapping declared, a transaction names why it refuses a linked resource, the operations answer `502` on a failed terminology server, a `DV_TEXT` display looked up through `$lookup`, a direct data-type pair over the text fallback, the `DV_ORDINAL` options of a template node, and the XML narrative `div` held to XHTML. The terminology client gains `$expand`, `$subsumes`, the `date` input, mutual TLS and context-aware `$validate-code`. |
| [v0.0.13](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.13) | 2026-10-25 | Security and OMOP breadth: the authenticated caller stated as the committer of every facade write, and token introspection. FHIR R4 into the CDM with no CDR through `etl fhir`, every row naming its resource, the Athena vocabulary loader, the OMOCL `DEATH`, provider and visit detail targets, pseudonymised identifiers with a per-person date shift, and the Data Quality Dashboard's conformance, completeness and temporal plausibility checks in Rust behind `dq check`. FHIRconnect `DV_ORDINAL` and `ACTION.time`, and the first HL7 Europe profile contexts. |
| [v0.0.14](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.14) | 2026-10-29 | The EU profile targets: round trips of the HL7 Europe Base and Core `medicalTestResult-eu-core`, the Laboratory Report `Composition-eu-lab` and the Medication Prescription and Dispense profiles, and `Composition/$document` on the facade. The `eehrxf` interoperability component taken in from FerroFED and checked with HL7's FHIR Validator. The server and the console on the FerroSYS crates, with `config check`, `healthcheck` and `GET /operator/readiness`. One deterministic resource id on the facade and `$tofhir`, and RFC 9457 problem details read from the CDR and answered outside the FHIR routes. |

## Next

The milestones after v0.0.14, each one theme:

1. **v0.0.15, the change feed.** The change-feed adapter driving the
   incremental OMOP load and an outbound FHIR lane.
2. **v0.0.16, console breadth.** Mapping-set revisions with history and
   rollback, and more of the console.
3. **v0.0.17, the interoperability component.** The openEHR mappings of the
   medication, laboratory, discharge and imaging categories, and the
   documents received in each.

## How correctness is measured

The published FHIRconnect and OMOCL mapping libraries are vendored verbatim
by committed fetch scripts with provenance and run as conformance corpora,
beside the HL7 FHIR examples packages per version, the HL7 FHIRPath test
suite, the HL7 v2 message sets, the R4 facade, and the profile validator's
agreement with the HL7 FHIR Validator. Each corpus keeps a pass list, and a
case that stops passing fails CI. [Checks and gates](../contribute/checks-and-gates.md)
describes each corpus and what a passing case means.

## What is outside the design on purpose

FHIR Subscriptions, which FHIRconnect never mentions; a materialised FHIR
store, which would contradict the facade decision; and OMOP CDM versions
other than 5.4. Each one is a tracker issue, so you can read the reasoning and
argue with it.

The current picture is the
[milestone list](https://github.com/FerroHEALTH/FerroBRIDGE/milestones), and
the [open issues](https://github.com/FerroHEALTH/FerroBRIDGE/issues) are the
worklist behind it.
