# ferrobridge-fhirpath

The FHIRPath evaluator, hand-written over the lexical document model and the
generated element table of `fhir-types`. The oracles are FHIRPath Normative
Release 2.0.0 (<https://hl7.org/fhirpath/N1/>, never the unversioned URL,
which serves 3.0.0) with its ANTLR grammar as the source of truth for syntax,
and the FHIR R4 FHIRPath page (<https://hl7.org/fhir/R4/fhirpath.html>
§2.9.1 to §2.9.3) for the variables, the functions and the `~` changes FHIR
adds. Other FHIRPath engines are prior art only.

## The document model is `fhir_types::codec::Value`

Every document this crate reads is the lexical `Value` of `fhir-types`, so a
decimal keeps the text it was written in and becomes a `rust_decimal` value
without passing through a float. Never introduce a second JSON model.

## The element table is the type source, behind `model::Model`

What an element is, which types a choice admits and where a content reference
lands come from `fhir_types::schema::Schemas`; `model::r4::R4` is the R4
table, and another version plugs in by implementing `Model` over its own.
`Model::base` reads each type's base from the table (`Schemas::base_of`), so
`is`, `as` and `ofType` see `Age` as a `Quantity` and `Patient` as a
`DomainResource` and a `Resource`, the abstract types included; a gap in the
table is a generator change, never a constant here. A FHIR primitive maps
onto the system types by the table of R4 §2.9.1.2: `date` is a
`System.DateTime`, and every primitive is its own type for `is`.

## Strict, and an error is never an empty result

A member no type of the input defines, a leading type name that is not the
context's, a function N1 and R4 do not define, an unresolvable type
specifier and every singleton breach of N1 §4.5 are typed errors. A function
of a later FHIRPath release (`lowBoundary()`, `sort()`, `trim()`) is not
defined here: N1 is the version R4 cites. Navigation over system values,
untyped nodes and the results of `children()`, `descendants()` and
`resolve()` stays lenient, as N1 §11 says a compiler cannot type them.

A choice element is typed by every type it admits (R4 §2.9.1.1). The static
check of a path from a choice element, or of an expression whose context is
one (`Expression::check_any` over `model::path_types`), accepts a member any
admitted type defines. On an occurrence of a choice element, reached through
it or named with `Focus::defined_by`, a member another admitted type defines
and its own type lacks navigates to empty (N1 §3.2 "all kinds of value", §4.4);
a member no admitted type defines stays the N1 §3 error.

`Expression::check` is the static half: it follows the reachable FHIR types
and refuses what the model refuses, and passes anything it cannot type. A type
test of the focus narrows it for the operand of `and`, `or`, `implies` and
the branch of `iif()` whose outcome the test fixes, and for the output of
`where()`; a branch no type reaches refuses nothing.

## What an evaluation calls out to is a seam

`seam` holds the traits the caller supplies; `Environment::with_*` installs
them. With no seam, the function is `EvalError::NoSeam`; a seam's failure is
`EvalError::Seam` with the failure as its source. `htmlChecks()` and
`checkModifiers()` need no seam.

## Settled decisions are `// NOTE:` comments at their site

Comparison across precisions walks the components as N1 §6.1.1's equality
does; a datetime with an offset and one without compare as unknown; the
fixed-length UCUM time units count as calendar durations in date arithmetic
while `'mo'` and `'a'` are refused; a quoted calendar keyword is a calendar
unit (N1 §4.1.8); transcendental results keep 8 decimal places; `now()` reads
UTC; `trace()` logs shapes, never values; `iif()` keeps the surrounding
`$index` and `$total`. Change one only with a citation.

## Tests

`tests/it/conformance.rs` runs the vendored HL7 FHIRPath test suite as the
`fhirpath-r4` corpus (`docs/specs/fhirpath-tests/`), never special-casing a
test by name. `tests/it/constraints.rs` parses every invariant of the vendored
packages and checks the R4 core ones against their elements; an invariant a
package publishes with a defect is an expected refusal with the evidence.
Fixtures are synthetic.
