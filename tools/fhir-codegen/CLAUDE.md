# fhir-codegen

The generator: vendored FHIR packages in, `crates/fhir-types` out, and the
fetched HL7 v2 definitions in, `crates/hl7v2-types` out. Hand-written
tooling; the vendored `StructureDefinition` and `OperationDefinition`
resources are the authority for what it emits (`.claude/rules/codegen.md`).

- Inputs live under `vendor/<package>/`, fetched only by
  `scripts/vendor/fhir-packages.sh`, verbatim, with a `PROVENANCE.md` each
  (`.claude/rules/vendored-inputs.md`). Never hand-edit a vendored file.
- The pipeline is `package` (read), `snapshot` (resolve), `roots` (select),
  `closure` (the root-set closure), `lower` (the Rust model: structs per
  type and backbone element, enums per choice, boxed cycle edges), `render`
  (source text), `emit` (write or `--check`). Two root sets are declared,
  `roots::RootScope::Terminology` and `roots::RootScope::Resources`; the
  emitter emits one tree holding the union with the complete closure of the
  types each references, per version, and marks every item with the `cfg` of
  the narrowest feature that selects it. A shape the consumer lacks is fixed
  here, never shadowed downstream.
- `render_definition` (#836) writes each version's `ElementDefinitionRead`
  impl (the trait is `templates/definition.rs`) by finding the field the
  version defines for each read element and spelling its read from the
  field's cardinality and type: a primitive reads its `value`, a choice reads
  each alternative, a `Reference` its `reference`, and a missing element
  reads as absent. An extension-carried fact reads the value of the
  extension with its url (a lexical form from either url the snapshot reader
  accepts, listed once in the module as `LEXICAL_FORMS`, #875), and the
  `StructureDefinitionRead` impl (#859)
  reads a context either as R4 entries or, where the version states context
  strings, as those strings with the version's `contextType`.
  `render_definition::terminology` (#876) writes the `ValueSetRead`,
  `ContainsRead`, `CodeSystemRead` and `ConceptRead` impls the same way, a
  nested entry's read being its own type's slice. No version is named.
- `search` (#522) emits the search parameter registry per version behind
  `resources`: every `SearchParameter` of the package apart from the
  specification's examples (`package::Package` leaves out the ids `example`
  and `example-*`), in canonical order, and the standard parameter each
  concrete resource type answers by code, its own over a base type's. Two
  definitions of one code on one base fail the emit, except the pairs
  `search::TOLERATED_AMBIGUITIES` lists, which a test asserts are still in
  their packages. The crate-level `search.rs` carries the shape and the
  `SearchParamType` enum over the codes every version uses.
- The `v2` root set (`src/v2/`, #253) reads the HL7 v2 definitions that
  `scripts/vendor/v2ig.sh` fetches into the ignored `vendor/hl7-v2ig/` and
  emits `crates/hl7v2-types`: `v2::corpus` (the manifest-less loader over the
  directories `roots` declares), `roots::V2RootSet` (every message structure,
  segment, data type and message definition), `v2::lower` (the differential
  read as the snapshot, the position-prefixed id rule, the extensions, the
  component tables, and each message definition linked to its structure by
  name), `v2::render` and `v2::emit`.
  `v2::definition` is the v2 files' own serde projection, refusing every
  member it does not name, so the FHIR projection in `fhir.rs` stays strict.
  A defect of the definitions is tolerated only in the files
  `v2::lower::Defect::tolerated_in` lists, and a test asserts each listed
  file carries it. `emit` and `emit --check` cover both crates, and both need
  the fetched tree.
- `v2::legacy` (#303) reads the IGAMT export of HL7's v2 database that
  `scripts/vendor/v2-legacy.sh` fetches into the ignored
  `vendor/hl7-v2-legacy/` (`legacy::source`, one directory of JSON tables
  per version, and table 0354 of `hl7.terminology`) and lowers every
  structure of every version into the `lower` shapes (`legacy::lower`,
  #333), marking withdrawn those whose code v2.9.1 lacks. Nothing is emitted
  twice (`legacy::lower::Owner`): a segment links to the agreeing v2.9.1
  static or to an earlier version's identical one, and a tree identical to an
  earlier version's links its `<NAME>_NODES` (`legacy::lower::tree_owner`).
  `v2::render` emits the rest under `legacy/`. `legacy::lower::interactions`
  (#1016) reads each version's `interactions.json` into `LEGACY_EVENTS`:
  every other code and event the version sends one of its structures under
  (`REF^I14` sends `REF_I12`), refusing two structures under one pair. `legacy::datatype` gives each data type code those
  segments' fields name a `LegacyDataType` static per version, with the base
  type a version-specific code stands for (`CM_MSG` is `MSG`, `CE_0051` is
  `CE` bound to table 0051, `TS` is `DTM`), derived from the code since the
  export carries no base column. Its defects are tolerated only where
  `legacy::lower::LegacyDefect::tolerated_in` lists them.
- A primitive whose JSON form is a string carries its lexical form into the
  output: `lower` reads the `regex` extension of `<primitive>.value` from the
  version's own package and compiles it there, so an uncompilable form fails
  the emit rather than the crate, and `render_codec` writes it anchored behind
  a `LazyLock` with the function that refuses a value outside it
  (<https://hl7.org/fhir/R5/datatypes.html#primitive>). The forms are XML
  Schema patterns, so both sides compile them with Unicode mode off
  (`lower::compile_lexical_form`), and they differ between versions, so each is
  read from its own package and never copied.
- Output is byte-deterministic: iterate `BTreeMap` and sorted vectors, never
  a hash map (`.claude/rules/reliability.md`); `rustfmt` from the pinned
  toolchain formats the output so `cargo fmt --check` and the emitter agree.
- `cargo run -p fhir-codegen -- emit` regenerates; `-- emit --check`
  is the drift check CI runs.
- The larger modules are folders with one child per concern: `lower/`
  (`structure`, `cycle`), `operations/` (`field`, `render`), `render_codec/`
  (`lexical`, `primitive`, `choice`, `json`, `builder`, `serialize`),
  `v2/lower/`, `v2/render/` and `v2/legacy/lower/`.
- `main.rs` is thin over `lib.rs` so the loader and emitter are tested
  through the library.
- This crate is a tool, so it may write to stdout and stderr; every such site
  carries a scoped `#[expect]` with a reason.

## Naming

Names say what a thing is in its domain, never which language or pipeline
stage produced it. The FHIR side keeps FHIR's names verbatim
(`fhir::StructureDefinition`, `fhir::ElementDefinition`,
`snapshot::ResolvedStructure`); the generated side names the artefact
(`lower::VersionModule` is the generated module for one FHIR version,
`lower::TypeDef` one type definition, `lower::Cardinality` a cardinality). A
`Rust`, `Fhir`, or `Gen` prefix on a type name is a smell: if two views of one
thing collide, the module path disambiguates (`fhir::` versus `lower::`).

## The full-emit tests run behind a gate

Five integration tests emit a whole generated crate (`emit.rs` and `v2/emit.rs`:
emitting twice is byte-identical, the check reports drift, the committed
crate is in sync). A full emit takes minutes in a debug build and longer
under coverage instrumentation, so they run only when
`FERROBRIDGE_CODEGEN_EMIT` is exactly `1`, which the `codegen-emit-tests` CI
job sets over the `emit` profile (the emitter optimised, the two generated
crates the test binary links left at opt-level 0); elsewhere they return
early and pass. `scripts/checks/fast.sh` runs them when your change reaches
this crate; by hand it is `FERROBRIDGE_CODEGEN_EMIT=1 cargo nextest run
--cargo-profile emit -p fhir-codegen`. The drift check itself
(`emit --check`) stays the guard.
