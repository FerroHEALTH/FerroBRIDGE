<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-validator

The FHIR R4 profile validator (#432), hand-written over `fhir-types`. The
oracles are the FHIR R4 pages: validation.html (informative) for what a
validator checks, profiling.html and elementdefinition.html for the profile
rules, json.html for the shapes, datatypes.html for the primitives,
extensibility.html for extensions, references.html#canonical for resolution,
and the package format of <https://hl7.org/fhir/packages.html>. The HL7 FHIR
Validator is prior art and the agreement target (#442), never the oracle.

## No second model

A StructureDefinition decodes into `fhir_types::r4::structure_definition::
StructureDefinition`, an instance stays a `fhir_types::codec::Value`, and the
base structure comes from `fhir_types::r4::schema::SCHEMAS`. When the
generated model cannot carry what a check needs, that is a generator change
(`.claude/rules/codegen.md`), never a local struct.

## Layout

- `package`: one package from a `.tgz` or a directory. With a `.index.json`
  the resources are indexed from it and a directory's files are read on first
  use; without one, every JSON file is read for its header.
- `set`: the closure check (a missing dependency is refused by name) and
  canonical resolution, decoding each definition once.
- `issue` and `report`: the typed issue, the unevaluated list and the
  `OperationOutcome` conversion.
- `validate`: the walk (`walk`: the object and member walk, `members` for
  the JSON shapes and cardinality, `content` per kind), the primitive rules
  (`primitive`), `fixed[x]` and `pattern[x]` (`matching`), extension
  resolution (`extension`), the snapshot index with its slices
  (`structure`), slicing (`slicing`: `path` holds a discriminator path to the
  FHIRPath subset through `ferrobridge-fhirpath`'s parser, `instance` and
  `definition` follow it through the item and the slice, `discriminate`
  decides one discriminator), the profiles a type names (`profiles`),
  terminology (`terminology`: `index` holds a code system by code, `compose`
  decides membership in a value set three-valued, `binding` holds a value to
  an element's binding) and the `resolve()` seam with its contained-resource
  resolver (`resolver`).
- `terminology` (the public module): the `Terminology` seam over
  `ferrobridge-term`, the pending binding checks, and
  `Report::resolve_bindings`.

## The sync walk and the async server

The walk stays synchronous and reads only the package set. A binding it
cannot decide becomes a `PendingBinding` in the report and is listed as
unevaluated with its local reason. `Report::resolve_bindings(&terminology)`
is the one async step: it sends the distinct questions as one call or one
batch, turns answers into issues, lifts the listing of each binding it
answered, and leaves a refused or failed one listed with
`Reason::TerminologyFailed`. A slicing discriminator never asks a server.

## Never a silent pass

Every invariant, target profile, claimed profile and extension context the
walk meets is listed as `Unevaluated` with its reason, once per declaring
definition, element and constraint, and so is every binding the package set
cannot decide, every discriminator the slicing cannot decide and every type
profile the set does not carry. An item a discriminator cannot decide is
placed in no slice and refused by no rule. The sibling issues fill these in:
invariants #439 through `ferrobridge-fhirpath`, target and claimed profiles
and Bundle resolution #441. A check that lands removes its listing in the
same change.

## Settled decisions are `// NOTE:` comments at their site

The lexical forms are the package's informative regexes, compiled anchored
with Unicode mode off; an unknown non-modifier extension is a warning; a
`fixed[x]` comparison ignores an instance `id`, `extension` or `_` sibling the
fixed value lacks; an example binding is not listed; a bare canonical resolves
to the latest version by Semantic Versioning precedence, within the
dependency closure of the package whose definition names it and in the whole
set when that closure does not carry it; one definition carried
byte for byte by two packages is no conflict; one canonical carried by two
versions of one package resolves to the higher package version; a leading byte-order mark is
stripped; a discriminator path may use `ofType()` or `as()` (#443); an item
matching two slices is an error and counts in the first; an item in no slice
may sit between ordered open slices; a `@default` slice under rules other
than `closed` is named and not used; an unresolvable `#id` under `resolve()`
is a warning and leaves the item undecided; an extensible binding missed is a
warning and a preferred one information; imported value sets intersect (R4
valueset.html §4.9.5 over the `include.valueSet` definition's "union"); a
`Coding` without a system is in no value set; subsumption filters need an
`is-a` hierarchy meaning and the property `concept`; a display matches the
concept's display or a designation exactly and a mismatch is a warning; a
`deprecated` status under `compose.inactive` false is undecided. Change one
only with a citation.

## Tests

`tests/it/` drives the public API over synthetic packages and profiles
written into temporary directories, with the vendored R4 core package beside
them. `eu.rs` validates against HL7 Europe `patient-eu-core` 2.0.1 when
`scripts/vendor/profile-packages.sh` has fetched the profile packages, and
returns early otherwise; so do the EPS slicing and binding suites and the
scoped-resolution unit test in `set`. Under CI (`CI=true`) an absent tree
fails them instead, through `support::profile_packages`, the one helper every
package-gated integration test goes through (the unit test repeats its
check), and the CI `test` lane and the Sonar lane fetch the packages first.
Fixtures are synthetic.
