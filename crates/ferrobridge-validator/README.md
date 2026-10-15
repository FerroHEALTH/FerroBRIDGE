<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-validator

A FHIR R4 profile validator over FHIR packages read offline, the generated
model and element table of [`fhir-types`](https://docs.rs/fhir-types).

Part of [FerroBRIDGE](https://ferrobridge.eu), a pure-Rust bridge between
openEHR and two interoperability targets: HL7 FHIR through the FHIRconnect
specification, and the OMOP Common Data Model through the OMOCL specification.

## What it does

- Reads FHIR packages from `.tgz` files and unpacked directories
  (`<dir>/package/package.json`), refuses a set whose manifests name a
  dependency it does not hold, and resolves StructureDefinitions by
  canonical, `url|version` or `url` alone for the latest version. Every
  definition decodes into the generated `fhir_types::r4` model.
- Validates a resource, held in the lexical `fhir_types::codec::Value`
  document model, against the core definition of its type or a profile the
  package set carries. The R4 element table gives the base structure; the
  profile's snapshot gives the constraints.
- Reports typed issues with a severity, an R4 issue type, the FHIRPath
  location (`Patient.name[0].given[1]`), a message id and a message, and
  converts the report into an R4 `OperationOutcome`.

The structural checks are unknown members, the JSON shapes (no empty object,
array or string, aligned primitive arrays), cardinality per parent occurrence,
the types a choice may take, `contentReference`, the lexical form of every
primitive read from the `regex` extension of its own definition, the integer
ranges, the length a primitive's definition allows and a profile's
`maxLength`, `fixed[x]`, `pattern[x]`, and extensions resolved by `url` and
walked against their definition.

Slicing is evaluated as FHIR R4 profiling defines it: the discriminator types
`value`, `exists`, `pattern`, `type` and `profile` over paths in the simple
FHIRPath subset (`resolve()` through contained resources and Bundle entries,
`extension(url)`, `ofType()` and `as()`), a required binding over a value set
the package set enumerates, slicing without a discriminator, the rules
`closed`, `open` and `openAtEnd`, ordered slicing, the `@default` slice,
re-slicing, extension slices by `url`, and the cardinality of each slice and
of the slicing entry.
An item matched to a slice is held to the slice's own constraints. Content
is validated against the profiles its type names and passes when it conforms
to one of them.

Terminology bindings are checked on `code`, `string`, `uri`, `Coding`,
`CodeableConcept` and quantity units. A value with no code from the value set
is an error under a `required` binding and under the maximum value set of the
`elementdefinition-maxValueSet` extension, a warning under `extensible` and
information under `preferred`; `example` is not checked. Membership is read
from the package set: listed concepts, whole code systems, excludes, imported
value sets, version pins, the subsumption and property filters a carried code
system supports, and complete expansions. A coding whose display is none its
code system gives the code is a warning. A binding the package set cannot
decide is kept as a pending check, which `Report::resolve_bindings` sends to a
terminology server through
[`ferrobridge-term`](https://docs.rs/ferrobridge-term) as one call or one
batch; without a server, or when the server fails, it stays listed with the
reason.

References resolve to a contained resource by `#id` or to an entry of the
Bundle around the resource, by the method of the R4 Bundle page (absolute,
relative against the entry's `fullUrl`, `urn:uuid:`, version-specific
through `meta.versionId`); a reference matching two entries is an error. The
resolved target is held to the element's target profiles, passing when it
conforms to one, and the reference to its aggregation (`contained`,
`referenced`, `bundled`). Every Bundle entry is held to the profile its slice
names, and every resource to each profile it claims in `meta.profile`. A
document Bundle is held to the content rules R4 gives documents: the
resources the Composition references are in the Bundle, and the Bundle holds
only the Composition, what it references directly or indirectly, a CSS
stylesheet `Binary` and the `Provenance` of what it includes.

Invariants are not evaluated yet, and neither is a discriminator, target
profile or aggregation the validator cannot decide (a path outside the
subset, a reference outside the resources it holds, a value set it cannot
evaluate, a profile the package set lacks). Each one the validator meets is listed in the report and in
the `OperationOutcome` as a `warning` of type `not-supported`, never passed in
silence.

## Message ids

R4 defines no validator messages, so the ids are this crate's own vocabulary.
An `OperationOutcome` issue carries its id as a `details.coding` in the system
`https://ferrobridge.eu/fhir/CodeSystem/validator-message`, and its message in
`details.text`.

## Licence

Business Source License 1.1; see `LICENSE`.
