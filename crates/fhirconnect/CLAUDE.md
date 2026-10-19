# fhirconnect

The FHIRconnect half of the bridge, hand-written over the shared foundation in
`openehr-mapping-core` and the generated FHIR model in `fhir-types`. The
FHIRconnect specification and the HL7 FHIR specification for the version in
play are the oracles; openFHIR and the retired CDR connector are prior art.

## The document model is `fhir_types::codec::Value`, never `serde_json::Value`

FHIR forbids reading a decimal through a binary float
(<https://hl7.org/fhir/R4/datatypes.html#decimal>), so every tree this crate
reads or writes is the lexical `Value` of `fhir-types`, which keeps a number in
the text the document carried. A `serde_json::Value` conversion belongs at the
HTTP edge and nowhere inside the model. Never introduce a second JSON model,
and never route a clinical value through `f64`.

## The element table is the authority, and it is generated

What an element is, whether it repeats, which types a choice admits and where a
content reference lands all come from `fhir_types::schema::Schemas`, the element
table the generator emits from the vendored HL7 packages. That includes the two
members the JSON representation gives a primitive's sibling object, `id` and
`extension`, which the table carries as its own `Element` entry
(<https://hl7.org/fhir/R4/element.html>). Nothing here restates a fact about
FHIR that the table already carries. A gap in the table is a generator change
plus a regeneration, never a hand-written constant in this crate
(`.claude/rules/codegen.md`).

`tree::element::Table` is what the path model asks of a version's table, so
R4B and R5 plug in through the same trait. Only R4 is wired today.

## Writable or read-only is decided when the expression is parsed

A step that selects among the values a document already holds cannot be written
through: `where()`, `first()`, `last()`, an index filter and `resolve()` all
make the whole expression read-only, and a write through one is a typed
refusal naming the offending step. `extension(url)` stays writable, because the
url is the identity of the extension and so names one element to create
(<https://hl7.org/fhir/R4/extensibility.html>). `value` on a primitive names the
primitive itself (R4 defines `code.value` and its kin), so it adds no move and
nothing lies below it.

The point is a defect the retired CDR connector had: it let filtering paths
through and its reverse direction then wrote nothing, silently.

## A refusal names the element, never only the expression

Every refusal from `tree::element`, `tree::read` and `tree::write` carries the
element path the table holds (`Condition.onset[x]`), so a diagnostic points at
the definition. A shape the element table contradicts is refused rather than
read leniently, and a write applies to a copy so a refusal leaves no half-built
element behind.

## The program is immutable; the engine never parses a path

`resolve::compile` is the one place a path is parsed, an anchor is bound, an
extension is applied and a version selector is checked. What it returns is a
`Program` with private fields, no `&mut self` method and an `Arc` around it, so
nothing downstream can change what a context compiled to. The interpreter reads
a resolved FHIR target, a resolved Web Template node and a structured
occurrence axis; it never sees a path string and never reads a mapping file. A
mapping that cannot be resolved is a refusal at load, never a failure on the
request that first touches it.

A `slotContext` is compiled where it stands: the entered context's `start`
is slotted as a `slotArchetype` would slot it, with the compiler's extension
set swapped for the entered context's for the duration, and the composition
slot of a root context entered this way skipped (`Method::SlotContext`). A
`scope: sub` context is refused as a root; a caller loads only roots. The
entered context's `template.id` is compared with
`WebTemplateIndex::embedded_template` at the slot node: a different template
refuses, and a slot node whose OPT states none is a warning.

A party context is recognised from the RM type of the template root
(`PERSON`, `ORGANISATION`, `ROLE`), never from a key in the file, and the
program carries it as `Program::party`
(`openehr_mapping_core::party::PartyClass`). `resolve::compile::party` holds
what it refuses at load, one code each: what names a composition
(`$composition`, the composition attributes the defaults and `ctx/` set, the
`PARTICIPATION` lists, `link`, an entered context whose `start` slots a
composition). The engine's composition entry points refuse a party program
(`EngineError::PartyProgram`); it runs through `engine::traverse::party`
(`to_party`, `from_party`), which builds through
`WebTemplateIndex::build_party` and reads through the same `read`. One RM
attribute of the party root that no template node holds is mappable,
`ROLE.performer` (`engine::rm::PARTY_ROOTS`); its `PARTY_REF` travels
through `engine::lens::party_ref`, the reference resolved through the
demographics seam and its type read from the element's `targetProfile` in
the `fhir-types` table, and a foreign, typed-otherwise or decorated
reference is refused. No specification governs a party context: our own
design.

Every diagnostic the compiler raises is collected. A run reports every
disagreement it found, because a mapping author fixing one refusal at a time is
how a load loop turns into an afternoon.

## A mapping with no `type` key converts through a derived pair

The `type` key is deprecated because the type is "derivable from the
instances" (`data-mappings.adoc` §Deprecated), so the compiler derives it once
and the program carries it as `Mapping::derived`; the engine never looks at a
type code or a class name to decide a cell. `resolve::derive::PAIRS` is the one
table: per openEHR class, the FHIR type codes a cell of the engine carries,
in the order the class's page of the data-type chapter lists them. That order
is our own design, and it is the whole tie-break: an element's own type wins
when the class pairs with it, a text primitive that is no pair converts as a
`string`, and a choice element with no type filter is written as the first
pair of the class the choice admits (going in, the alternative the document
carries decides). An `Extension` against a data value converts through its
`value[x]`. An untyped mapping onto a polymorphic `ELEMENT` (the Web Template
gives each admitted data type a child node) converts into the alternative
whose class lists the FHIR type earliest in `PAIRS` (`Derived::Value`) and
anchors its children at the `ELEMENT`, because `FollowedBy.adoc` runs the
children after the parent "has been executed". Any other untyped mapping
onto a structural (`PATHABLE`) node anchors its children, and one with no
child is a warning, never a silent nothing. Every
pair in `PAIRS` must name a `FhirKind`; `engine::fhir` tests that.

## Direction enters the engine at three points only

A condition is evaluated on the input side, a `unidirectional` marker skips a
mapping in the other direction, and the composition defaults apply going into
openEHR. Everything else is written once, which is why a data-type cell is a
lens: `get` and `put` live beside each other and are property-tested against
each other both ways, so the two directions cannot drift apart. Two
independently written direction functions are the duplication this shape
exists to prevent.

A skipped element is a typed outcome carried to the caller, never a log line,
and the declared set of losses is closed: the round-trip tests assert it
exactly. An element the program cannot map refuses the unit, and so does a
produced FHIR document that breaks a base constraint `engine::invariant`
checks (`ext-1` first): `to_fhir` answers `EngineError::Invariant` with the
key and the instance path. A new check there is one constraint key, a
structural test, and a case that produces its breach.

Every warning carries the `Location` the engine knew when it raised it: the
FHIR target rendered by `tree::locate` at the binding's FHIR occurrence (the
input going in, the output coming out) and the openEHR node's `aqlPath` with
its tail. No value of either document enters a location, so a consumer can
log it; a new warning site passes one, and the tests in
`tests/it/locations.rs` pin each kind both ways.

The concept-type methods that create or resolve a document are the one place
each direction has a branch of its own, because the specification names a
different act per side: `hierarchy.split` runs `split.fhir` going out of
openEHR and `split.openehr` going in, and `reference` resolves a resource
going in and creates one going out. `link` and `participationsFunction` keep
the two branches beside each other over the same family, so they cannot
drift.

## What a run calls out to is a seam

The engine makes no call of its own. `engine::seam::Seams` carries the
`mappingCode` registry, the `ReferenceSource` a `reference` mapping fetches
from, and the `IdentitySink` that names every resource a run creates;
`Seams::default()` registers nothing, resolves nothing and derives ids by a
digest over the `IdentityRequest`, so the same composition yields the same
ids. Every seam trait is `Send + Sync`, and a compile-time assertion keeps
`Seams` `Send`, so an async caller holds the seams across an `.await`. A reference that resolves to nothing is a `Loss::Skipped` warning, never a
silent nothing; a reference chain that reaches itself refuses. Created
resources travel as `Outcome::created`: the operations lane answers them as
Bundle entries and the facade binds each under an id of its own and serves it
at `<Type>/<id>`. `operations::run` takes the caller's `Seams`, and `$toopenehr`
resolves a reference against the request Bundle first and the seams'
reference source after it; the server passes none, so it reads the Bundle
only.

The fifth seam is `engine::demographics::Demographics`, the demographics
endpoint a party reference resolves through. The default `NoDemographics`
refuses, so a reference with no endpoint fails closed; `IssuerDemographics`
is the stateless fallback over one FHIR server base. `engine::traverse::
demographic` puts the identity in `identifiers` where FLAT carries it and in
`external_ref` on the composer and the participations, and reads both shapes
back through `reference_of`. Never drop a reference.

The fourth seam is `engine::terminology::Terminology`, the server a
`conceptmap` translates through, over the generated R4 `$translate`
contract. The default `NoTerminology` refuses, so a `conceptmap` with no
server fails closed. `engine::traverse::translate` asks once per distinct
code during the walk, which runs before the composition is built; only an
`equivalent` or `equal` match counts, an untranslated optional element is a
`SkipReason::Untranslated` and a required one refuses. Never pass a source
code through under the target when the translation fails.

## Reference-model attributes beside the template nodes

`FEEDER_AUDIT`, `LINK` and `PARTICIPATION` are no Web Template node, so the
engine writes them as the `_feeder_audit`, `_link:i` and
`_other_participation:i` FLAT families of the node they belong to
(`engine::family`); `participationsFunction` writes
`ENTRY.other_participations` and `EVENT_CONTEXT.participations`
(`_participation:i` under `context`), the same `PARTICIPATION` list.
`ENTRY.provider` is a tail of the entry node whose value the cell writes as
the `_provider` family (`rm::Carried::Family`, `family::provider`), and
`EVENT_CONTEXT.health_care_facility` the `_health_care_facility` family of
the `context` node the same way. These
families are the one place the engine still spells FLAT keys, because the
builder takes no `|raw` on a `_`-prefixed family (the `TODO(#618)` in
`engine::family`). Reading them back decodes typed `LINK` and
`PARTICIPATION` values, and a list that does not decode refuses the run. A
run whose `Defaults` carry
an `engine::origin::Origin` records the originating system, the source
resource and every defaulted field in the composition's
`FEEDER_AUDIT`, in the same order as the `Loss::Defaulted` warnings. A
`link` target must be an `ehr:` URI, because `LINK.target` is a `DV_EHR_URI`;
the linked composition of a `link` whose FHIR side is no reference is
refused, since one run produces one composition.

## A value travels whole, and the attribute model decides a tail

Every data value reaches the FLAT builder as its canonical JSON under the
`|raw` suffix (Simplified Formats master04 §Raw canonical JSON), written and
read through `openehr_its::json`, so the engine spells no per-class suffix and
every attribute the reference model gives a value survives. Never add a
per-class FLAT table back: what a class carries is `openehr_rm::v1_2::model`.

The resolver records the RM class of the last attribute a tail names
(`OpenehrTarget::leaf_class`). The engine reads the tail from the node's
canonical JSON with `paths::item_at_path` and writes it by merging into the
value the place holds. `engine::rm::carried` looks the tail up in the
attribute model: it is carried when every attribute exists on the node's
class or a concrete descendant, no step is a list, and the node is a data
value (or the tail is a `FAMILIES` entry such as `ENTRY.provider`). A `manual`
path must also end on a `String` attribute, because `merge` writes text. A
tail outside that is refused at load (`fc-uncarried-tail`, naming the mapping,
the class and the tail); `EngineError::UnsupportedTail` in both directions is
only the backstop for a program the compiler never saw. A tail that ends on a
data value runs the cell of the tail's class, not the node's.

Three nodes are the exception: the builder routes `COMPOSITION.composer`,
`language` and `territory` through the `ctx/` vocabulary (master06) and takes
no `|raw` there, so the engine writes them as `ctx/composer_name`,
`ctx/language` and `ctx/territory` and refuses the run
(`EngineError::ContextAttribute`) when the built value differs from the one
written. The composition defaults travel as `ctx/` keys the same way; which
fields default stays `engine/defaults-for-fields.adoc`'s. The one fill the
builder makes beside them, the mandatory `INSTRUCTION.narrative` no mapping
wrote, is declared as a `Loss::Defaulted` per instruction instance
(`Run::declare_narratives`, our own design), never left silent. The FLAT spellings
are Simplified Formats, vendored at
`docs/specs/its-rest/docs/simplified_formats/`.

## Where a mapping writes is one rule, applied to whichever side is the output

The axes the parent mapping bound keep their instance; every repeating element
below them takes a fresh instance per input occurrence, because FHIRconnect
appends to a `0..n` path it does not iterate. An output with no repeating
element below the parent holds one value, so a later mapping overwrites an
earlier one and a `0..n` input leaves only its last occurrence. That one rule
is what makes the specification's three recurrence examples come out as the
specification draws them, in both directions, and the counter-examples come out
wrong in the documented way rather than in some other way. Going into openEHR
the repeating nodes down to the start archetype are bound to their first
instance before any mapping runs (`Program::root_axes`), because `$archetype`
is the root as `$resource` is: one resource, one archetype instance. A FHIR
axis is named by the document path the walk took, so two elements that share a
definition count their instances apart.

A `hierarchy.split` adds pins to that rule and nothing else: each group of
occurrences (one per occurrence, or one per distinct `unique` tuple) runs the
whole mapping set with the split node's openEHR axis pinned to the group's
instance, and going into openEHR with the FHIR `with` element pinned too. A
pinned axis counts as bound, so every mapping under the node writes into the
group's element and every mapping outside it runs as it would unsplit.

## Tests

The corpus under `docs/specs/fhirconnect-mapping-lib/` is evidence of what real
mappings write, never an oracle. A path form found there is covered by a test;
a form it uses that the FHIR specification does not define is adjudicated
against the specification and recorded, never accepted because the corpus
writes it. Fixtures are synthetic content invented for the test.
