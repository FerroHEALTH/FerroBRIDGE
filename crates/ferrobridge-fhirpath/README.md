<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-fhirpath

A FHIRPath evaluator: FHIRPath Normative Release 2.0.0 with the additions
FHIR R4 makes to it, over the lexical document model and the generated
element table of [`fhir-types`](https://docs.rs/fhir-types).

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.

## What it does

- Parses an expression once (`Expression::parse`) into a syntax tree, with
  typed errors that carry the byte offset.
- Checks an expression against the type its context will have
  (`Expression::check`): a member no reachable type defines, or a leading type
  name that is not the context's, is refused before any document is read.
- Evaluates it any number of times (`Expression::evaluate`) on a resource, an
  element, or the items of an earlier evaluation, with `%resource`,
  `%rootResource`, `%context`, `%ucum`, `%sct`, `%loinc`, `%"vs-"` and
  `%"ext-"` defined, and the caller's own variables beside them.

Decimals are exact. Dates, datetimes and times keep the precision they were
written to, and a comparison across precisions answers empty. An evaluation
error and an empty result are distinct outcomes.

`resolve()`, `memberOf()`, `subsumes()`, `subsumedBy()`, `conformsTo()`,
`elementDefinition()` and `slice()` call seams the caller supplies on the
`Environment`; without one, the call is an error naming the function.
`htmlChecks()` reads the narrative rules of FHIR R4. `trace()` logs through
`tracing` the name, the count and the item types, never a value.

Quantities with the same unit, and calendar durations among themselves,
compare and compute; other pairs of different UCUM units answer empty, as
FHIRPath permits for an implementation without UCUM conversion.

## Conformance

The HL7 FHIRPath test suite for FHIR R4 runs as the `fhirpath-r4` corpus of
the FerroBRIDGE conformance gate; every failing test records its reason.
Every invariant of the FHIR R4 core package parses and passes the static check
against the element it constrains.

## Licence

Business Source License 1.1; see `LICENSE`.
