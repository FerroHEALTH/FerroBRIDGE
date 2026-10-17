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
  resolution (`extension`), the contexts and context invariants an
  extension's definition declares (`context`), the snapshot index with its slices
  (`structure`), slicing (`slicing`: `path` holds a discriminator path to the
  FHIRPath subset through `ferrobridge-fhirpath`'s parser, `instance` and
  `definition` follow it through the item and the slice, `discriminate`
  decides one discriminator), the profiles a type names (`profiles`),
  terminology (`terminology`: `index` holds a code system by code, `compose`
  decides membership in a value set three-valued, `binding` holds a value to
  an element's binding), the resolution seam with its contained-resource and
  Bundle resolvers (`resolver`, bundle.html §2.36.4.1, each Bundle
  indexed once per validation), the target profiles
  and aggregation of a reference (`reference`), the versioning a reference
  takes (`versioning`), the document rules
  (`document`, documents.html §3.3.1), and the invariants (`invariant`: the
  evaluation on each occurrence, the expression and check caches on the
  `Validator`, and `seams`, which fills `resolve()`, `memberOf()` and
  `conformsTo()` from the walk). The claimed profiles sit in `profiles`.
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

Every extension context the walk cannot decide (and none it decides admits
the extension), every context invariant that does not parse or evaluate,
every invariant whose expression
does not parse, check or evaluate (a seam the run cannot answer included),
every target profile whose target it cannot reach or whose profile the set
lacks, and every claimed profile the set lacks, is listed as `Unevaluated`
with its reason, once per declaring definition, element and constraint, and
so is every binding the package set cannot decide, every discriminator the
slicing cannot decide and every type profile the set does not carry. An item
a discriminator cannot decide is placed in no slice and refused by no rule.
A published invariant with a known defect (`cid-0`, `inv-1`, `dom-3`) is
evaluated like any other and listed with its error and upstream report, never
special-cased to pass. A check that lands removes its listing in the same
change.

## Settled decisions are `// NOTE:` comments at their site

The lexical forms are the package's informative regexes, compiled anchored
with Unicode mode off; an unknown non-modifier extension is a warning, and an
extension url the set cannot resolve (absent, a `#` fragment, two
definitions under one canonical) is unknown and never ends the run; a
`fixed[x]` comparison ignores an instance `id`, `extension` or `_` sibling the
fixed value lacks; an example binding is not listed; a bare canonical resolves
to the latest version by Semantic Versioning precedence, within the
dependency closure of the package whose definition names it and in the whole
set when that closure does not carry it; one definition carried
byte for byte by two packages is no conflict; one canonical carried by two
versions of one package resolves to the higher package version; a leading
byte-order mark is
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
`deprecated` status under `compose.inactive` false is undecided; an
ambiguous Bundle reference is an error; a target failing every checked
target profile is not refused while another is unknown; a target profile
check under
way further up the walk counts as passing (a reference cycle); every
`meta.profile` claim is validated; the core `Resource` and
`DomainResource` admit any target; the stylesheet `Binary` of a document is
one whose `contentType` is `text/css`; an invariant whose expression answers
empty is listed, neither passed nor failed; an invariant on a choice element
is checked against every type the base choice admits and evaluated on the
occurrence named as one of it; a Bundle entry is its own `%rootResource`; an
invariant the
snapshot repeats is evaluated once per location, key and declaring
canonical; `resolve()` from the Bundle element reaches entries by an
absolute reference; an element context naming a type admits that type and
its specializations, and `Element` admits a resource too; a `url#id` context
on an element held to no definition deriving from `url`, or on an item in no
slice of the entry it names a slice of, is undecided; a child a snapshot
omits is held to the core definition of the type that declares it; a slice
that falls short of its minimum after a validation kept an item out of it (a
`profile` discriminator, or no discriminator) names that validation's errors
for the rejected item with the fewest, as `slice-rejection` information; a
reference that names no FHIR REST server (a contained `#id`, a `urn:`) is
taken as version specific under `type.versioning`, so `specific` admits it
and `independent` lists it. Change one only with a citation.

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
