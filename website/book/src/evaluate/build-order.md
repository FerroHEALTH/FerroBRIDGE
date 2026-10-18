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
| [v0.0.8](https://github.com/FerroHEALTH/FerroBRIDGE/releases/tag/v0.0.8) | 2026-10-18 | The operator console and its image, the operator API, `ferrobridge mapping check`, and the profile validator's agreement corpus against the HL7 FHIR Validator. |

## Next

The milestones after v0.0.8, each one theme:

1. **v0.0.9, the profile validator finished.** The extension context and
   reference versioning checks, and STU3 profiles.
2. **v0.0.10, openEHR demographics.** The patient-to-EHR link and Patient,
   Practitioner and Organization over the Demographic API.
3. **v0.0.11, FHIRconnect breadth.** The full data-type matrix, the remaining
   concept types and `mappingCode` functions, and the open outcomes of the
   HL7 v2 face.
4. **v0.0.12, OMOP breadth.** The remaining OMOCL targets, the Athena
   vocabulary loader, and the data quality checks.
5. **v0.0.13, security.** The caller's identity reaching the CDR, token
   introspection, and `private_key_jwt` client authentication.
6. **v0.0.14, the EU profile targets.** HL7 Europe Base and Core, then the
   Laboratory Report, then Medication Prescription and Dispense.
7. **v0.0.15, the change feed.** The change-feed adapter driving the
   incremental OMOP load and an outbound FHIR lane.
8. **v0.0.16, console breadth.** Mapping-set revisions with history and
   rollback, and more of the console.

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
