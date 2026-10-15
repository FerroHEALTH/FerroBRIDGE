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

Slicing, invariants, terminology bindings, the profiles an element names and
the profiles a resource claims are not evaluated yet. Each one the validator
meets is listed in the report and in the `OperationOutcome` as a `warning` of
type `not-supported`, never passed in silence.

## Message ids

R4 defines no validator messages, so the ids are this crate's own vocabulary.
An `OperationOutcome` issue carries its id as a `details.coding` in the system
`https://ferrobridge.eu/fhir/CodeSystem/validator-message`, and its message in
`details.text`.

## Licence

Business Source License 1.1; see `LICENSE`.
