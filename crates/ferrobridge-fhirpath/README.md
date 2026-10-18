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

The default evaluator is FHIRPath N1 2.0.0, the release FHIR R4 cites, so a
function only a later release defines is an error.
`Environment::with_fhirpath_stu3_functions` admits the functions FHIRPath
3.0.0 adds as Standard for Trial Use that `eval::STU3_FUNCTIONS` lists, of
which `matchesFull()`, `encode()`, `decode()`, `escape()`, `unescape()`,
`trim()`, `split()` and `join()` are implemented.

Quantities compare, convert and add across commensurable UCUM units, in the
more granular of the two, and `toQuantity(unit)` converts to the unit asked
for. A product or a quotient carries the UCUM unit with identical units
merged (`12 'cm' * 3 'cm'` is `36 'cm2'`). The `ucum` module reads the UCUM 2.2 essence, embedded whole: prefixes,
exponents, annotations, the `10*` and `10^` forms and parenthesized terms,
with degree Celsius, degree Fahrenheit and degree Réaumur converted by their
functions. Other special units and every arbitrary unit convert to nothing,
and units of different dimensions answer empty. Calendar durations keep the
FHIRPath rules: they convert among themselves and meet the UCUM time units
only at and below the second.

The UCUM essence is copyright 1999-2024, Regenstrief Institute, Inc., and
ships under the UCUM Copyright Notice and License
(<https://unitsofmeasure.org/license>) in `vendor/ucum/`, provided "AS IS",
without warranties or conditions of any kind; it is not covered by the
licence of this crate.

## Conformance

The HL7 FHIRPath test suite for FHIR R4 runs as two corpora of the
FerroBRIDGE conformance gate: `fhirpath-r4-stu3` holds the tests that call a
function FHIRPath 3.0.0 adds, run with the opt-in, and `fhirpath-r4` every
other test, run as N1. Every failing test records its reason.
Every invariant of the FHIR R4 core package parses and passes the static check
against the element it constrains.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
