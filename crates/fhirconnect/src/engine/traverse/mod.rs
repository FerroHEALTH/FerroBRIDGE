// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The interpreter over a compiled program, one traversal for both
//! directions.
//!
//! The walk is top-down in program order. For each mapping it evaluates the
//! input-side conditions, reads the input occurrences, runs the data-type cell
//! the two classes fix, and writes the result where the recurrence rules put
//! it. A `followedBy` child runs once per occurrence its parent bound, which
//! is what pairs two repeating lists against each other
//! (`recurrence/PopulatingAnEntry.adoc`).
//!
//! Where a mapping writes is decided by one rule, applied on whichever side is
//! the output: the axes the parent bound keep their instance, and every
//! repeating element below them takes a fresh instance per input occurrence,
//! because "FHIRconnect always appends to a `0..n` path if it's not iterated"
//! (`recurrence/PopulatingAnEntry.adoc` §Wrong mapping). An output with no
//! repeating element below the parent holds one value, so a second write
//! overwrites the first (`recurrence/Overwriting.adoc`) and a `0..n` input
//! leaves only its last occurrence, with the rest declared lost
//! (`recurrence/TransformingList.adoc`).

pub mod build;
pub mod convert;
pub mod demographic;
pub mod display;
pub mod error;
pub mod functions;
pub mod links;
pub mod manual;
pub mod party;
pub mod place;
pub mod programmed;
pub mod reference;
pub mod split;
pub mod tail;
pub mod translate;
pub mod walk;

use std::collections::BTreeMap;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::coding::Coding;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::CanonicalDocument;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::FlatId;

use crate::engine::context::CallContext;
use crate::engine::family;
use crate::engine::invariant;
use crate::engine::origin::Origin;
use crate::engine::outcome::Location;
use crate::engine::outcome::Outcome;
use crate::engine::outcome::Warning;
use crate::engine::rm::RmValue;
use crate::engine::seam::Seams;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::Program;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::Occurrence;
use crate::tree::locate::locate;

use crate::tree::element::Table;

use crate::engine::traverse::error::EngineError;

/// How many instances of one repeating node a read probes for.
///
/// The composition carries no instance count of its own, so the read stops at
/// the first absent instance and this is the ceiling on that probe. No
/// specification governs this: our own design.
const INSTANCE_CEILING: u32 = 1024;

/// The `LINK.type` a `link` mapping that names none writes.
///
/// `LINK.type` is mandatory
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_link_class>)
/// and the `link` key leaves it optional. No specification governs the
/// value: our own design.
const LINK_TYPE: &str = "reference";

/// The composition fields the engine fills when no mapping does.
///
/// `engine/defaults-for-fields.adoc` puts the composer and the context start
/// time on the engine and the rest on "the project performing the mapping", so
/// only those two carry a value here unless the caller sets more. Every field
/// the engine fills is recorded as a
/// [`Loss::Defaulted`](crate::engine::outcome::Loss::Defaulted) warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defaults {
    composer: String,
    start_time: String,
    setting: Setting,
    language: Option<String>,
    territory: Option<String>,
    origin: Option<Origin>,
}

impl Defaults {
    /// Creates the defaults, with `now` as the context start time.
    ///
    /// "For the `context_start_time`, the default mapping time can be used",
    /// and the composer is defaulted "with a value such as `FHIRconnect`
    /// party" (`engine/defaults-for-fields.adoc`). This crate reads no clock,
    /// so the caller passes the time.
    #[must_use]
    pub fn at(now: impl Into<String>) -> Self {
        Self {
            composer: String::from("FHIRconnect"),
            start_time: now.into(),
            setting: Setting {
                code: String::from("238"),
                value: String::from("other care"),
            },
            language: None,
            territory: None,
            origin: None,
        }
    }

    /// Returns these defaults with `name` as the composer.
    #[must_use]
    pub fn with_composer(mut self, name: impl Into<String>) -> Self {
        self.composer = name.into();
        self
    }

    /// Returns these defaults with the `EVENT_CONTEXT.setting` the project
    /// performing the mapping defines.
    ///
    /// `code` and `value` are one concept of the openEHR terminology's
    /// `setting` group, which "should be defined by the project performing the
    /// mapping" and "can be defaulted to one of the valid values"
    /// (`engine/defaults-for-fields.adoc`, which links the group in
    /// `openehr_terminology.xml`). The default is `238` "other care".
    #[must_use]
    pub fn with_setting(mut self, code: impl Into<String>, value: impl Into<String>) -> Self {
        self.setting = Setting {
            code: code.into(),
            value: value.into(),
        };
        self
    }

    /// Returns these defaults with the composition language `code`.
    ///
    /// The code set is ISO 639-1
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_composition_class>).
    #[must_use]
    pub fn with_language(mut self, code: impl Into<String>) -> Self {
        self.language = Some(code.into());
        self
    }

    /// Returns these defaults with the composition territory `code`.
    ///
    /// The code set is ISO 3166-1
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_composition_class>).
    #[must_use]
    pub fn with_territory(mut self, code: impl Into<String>) -> Self {
        self.territory = Some(code.into());
        self
    }

    /// Returns these defaults with `origin` recorded in the composition's
    /// `FEEDER_AUDIT`.
    ///
    /// The system and the source resource travel as `originating_system_audit`
    /// and `originating_system_item_ids`, and every field the engine defaults
    /// travels as one `feeder_system_item_ids` entry in the order it was
    /// defaulted, so a reader of the stored composition sees what the bridge
    /// supplied
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
    /// Which attribute carries which fact is our own design: no specification
    /// governs the allocation.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Returns the origin the run records, when the caller set one.
    #[must_use]
    pub const fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
}

/// One concept of the openEHR terminology's `setting` group.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Setting {
    code: String,
    value: String,
}

/// The occurrence a mapping is bound to, on both sides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Binding {
    fhir: Occurrence,
    openehr: Vec<RmPosition>,
}

/// One openEHR value the run produced.
#[derive(Debug, Clone, PartialEq)]
struct Written {
    flat_id: FlatId,
    positions: Vec<RmPosition>,
    value: Held,
}

/// What the run holds at one place of the composition.
#[derive(Debug, Clone, PartialEq)]
enum Held {
    /// A whole value a cell produced.
    Value(RmValue),
    /// A value written one tail attribute at a time, as canonical JSON, read
    /// as its class when the run builds the composition.
    Partial(serde_json::Map<String, serde_json::Value>),
    /// The canonical JSON of a whole value a registered function produced,
    /// which travels to the builder as written.
    Canonical(serde_json::Value),
}

/// The value at the end of an openEHR target, the node's own or its tail's.
#[derive(Debug, Clone, PartialEq)]
enum Leaf {
    /// A data value a cell reads.
    Value(Box<RmValue>),
    /// A scalar attribute, as its text.
    Scalar(String),
}

/// Runs `program` over a FHIR document, producing a composition.
///
/// `seams` carries what the run calls out to: the `mappingCode` registry, the
/// source a `reference` mapping fetches the referenced resource from, and the
/// terminology server a `conceptmap` translates through, which every
/// translation reaches before the composition is built.
/// `context` carries the per-call values a `manual` path reads through
/// `$context`; pass [`CallContext::new`] when the caller supplied none.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, and for a
/// set of values the template does not admit as a composition.
pub fn to_openehr<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    document: &Value,
    seams: &Seams<'_>,
    defaults: &Defaults,
    context: &CallContext,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    composition_program(program)?;
    let mut run = Run::new(
        program,
        table,
        index,
        Direction::FhirToOpenehr,
        *seams,
        context,
        document.clone(),
        None,
    );
    // NOTE: basics/Variables.adoc, `$archetype` is the root as `$resource` is, so one
    // resource maps into one instance of the start archetype and its axes are bound.
    run.pins = program
        .root_axes()
        .iter()
        .map(|axis| (axis.clone(), RmPosition::first()))
        .collect();
    run.admits_context()?;
    run.split_or_run(
        program.mappings(),
        program.hierarchy(),
        program.start().as_str(),
        &Binding::default(),
    )?;
    let defaulted = run.apply_defaults(defaults)?;
    if let Some(origin) = defaults.origin() {
        run.families
            .extend(family::feeder_audit(index.root(), origin, &defaulted));
    }
    let (values, routed) = run.node_values()?;
    let built = index
        .build_composition(&values, &defaults.start_time)
        .map_err(|source| EngineError::Build {
            source: Box::new(source),
        })?;
    run.carried_whole(&built, &routed)?;
    Ok(Outcome::new(built, run.warnings).with_consumed(run.consumed))
}

/// Runs `program` over a composition, producing a FHIR resource.
///
/// `seams` carries what the run calls out to: the `mappingCode` registry and
/// the sink that assigns the id of every resource a `reference` mapping or a
/// `hierarchy.split` creates. Those resources travel as
/// [`Outcome::created`]. A `conceptmap` translates the code the composition
/// holds through the seams' terminology server. `context` carries the per-call values a `manual`
/// path reads through `$context`; pass [`CallContext::new`] when the caller
/// supplied none.
///
/// # Errors
///
/// Returns [`EngineError::NotApplicable`] when the program's own
/// `openehrCondition` does not hold over the composition,
/// [`EngineError::Invariant`] when the produced resource or one it created
/// breaks a constraint [`crate::engine::invariant::check`] holds it to, and
/// [`EngineError`]
/// for any element the program cannot map.
pub fn to_fhir<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<Outcome<Value>, EngineError> {
    composition_program(program)?;
    read_out(program, table, index, composition, seams, context)
}

/// Runs `program` over a canonical document, a composition or a party,
/// producing a FHIR resource.
fn read_out<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    document: &dyn CanonicalDocument,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<Outcome<Value>, EngineError> {
    let mut run = Run::new(
        program,
        table,
        index,
        Direction::OpenehrToFhir,
        *seams,
        context,
        empty_resource(program.resource().as_str()),
        Some(document),
    );
    run.admits_context()?;
    run.split_or_run(
        program.mappings(),
        program.hierarchy(),
        program.start().as_str(),
        &Binding::default(),
    )?;
    let created = core::mem::take(&mut run.created);
    for document in core::iter::once(&run.fhir).chain(&created) {
        invariant::check(document).map_err(|breach| EngineError::Invariant {
            context: String::from(program.context().as_str()),
            breach,
        })?;
    }
    Ok(Outcome::new(run.fhir, run.warnings).with_created(created))
}

/// Refuses a party program at an entry point that runs over a composition.
///
/// A party program reads and builds a `PARTY`, and the composition builder
/// applies the composition defaults and the `ctx/` vocabulary, so a party
/// program never reaches it; it runs through [`party::to_party`] and
/// [`party::from_party`]. No specification governs this: our own design.
fn composition_program(program: &Program) -> Result<(), EngineError> {
    match program.party() {
        Some(party) => Err(EngineError::PartyProgram {
            context: String::from(program.context().as_str()),
            party,
        }),
        None => Ok(()),
    }
}

/// Returns a resource of `resource_type` that holds nothing else yet.
fn empty_resource(resource_type: &str) -> Value {
    let mut resource = Object::new();
    resource.insert(
        String::from("resourceType"),
        Value::String(String::from(resource_type)),
    );
    Value::Object(resource)
}

/// One run of one program, in one direction.
struct Run<'a, T: Table + ?Sized> {
    program: &'a Program,
    table: &'a T,
    index: &'a WebTemplateIndex,
    direction: Direction,
    seams: Seams<'a>,
    context: &'a CallContext,
    warnings: Vec<Warning>,
    /// The FHIR document the mappings read or write: the run's own resource,
    /// or the referenced or created one a nested run swapped in.
    fhir: Value,
    composition: Option<&'a dyn CanonicalDocument>,
    written: Vec<Written>,
    families: Vec<NodeValue>,
    /// The defaults the run writes as `ctx/` keys.
    context_keys: Vec<NodeValue>,
    /// The values the built composition must carry at the nodes a `ctx/`
    /// default sets, where the key holds less than the value.
    expected: Vec<Routed>,
    counters: BTreeMap<(String, String), usize>,
    chain: Vec<String>,
    /// The references the current reference chain entered, outermost first.
    references: Vec<String>,
    /// The resources the run created beside its own.
    created: Vec<Value>,
    /// The literal references a `slotContext` consumed.
    consumed: Vec<String>,
    /// The instance a `hierarchy.split` fixes for each openEHR axis it pins.
    pins: Vec<(FlatId, RmPosition)>,
    /// The FHIR element a split out of FHIR fixes, as its repeating axes and
    /// its occurrence.
    fhir_pin: Option<(Vec<String>, Occurrence)>,
    /// The concept each distinct code translated to, so the seam is asked
    /// once per code in a run.
    translations: BTreeMap<translate::TranslationKey, Option<Coding>>,
}

impl<'a, T: Table + ?Sized> Run<'a, T> {
    /// Creates a run with nothing written yet.
    #[expect(
        clippy::too_many_arguments,
        reason = "a run is assembled once per entry point from exactly the inputs the two entry points take"
    )]
    fn new(
        program: &'a Program,
        table: &'a T,
        index: &'a WebTemplateIndex,
        direction: Direction,
        seams: Seams<'a>,
        context: &'a CallContext,
        fhir: Value,
        composition: Option<&'a dyn CanonicalDocument>,
    ) -> Self {
        Self {
            program,
            table,
            index,
            direction,
            seams,
            context,
            warnings: Vec::new(),
            fhir,
            composition,
            written: Vec::new(),
            families: Vec::new(),
            context_keys: Vec::new(),
            expected: Vec::new(),
            counters: BTreeMap::new(),
            chain: Vec::new(),
            references: Vec::new(),
            created: Vec::new(),
            consumed: Vec::new(),
            pins: Vec::new(),
            fhir_pin: None,
            translations: BTreeMap::new(),
        }
    }
}

impl<T: Table + ?Sized> Run<'_, T> {
    /// Returns where a loss at `mapping` happened, its FHIR side at
    /// `occurrence`.
    ///
    /// The FHIR occurrence a binding carries is the input occurrence going
    /// into openEHR and the output occurrence coming out to FHIR, so the one
    /// rule locates the element the loss concerns in both directions.
    fn locate(&self, mapping: &Mapping, occurrence: &Occurrence) -> Location {
        self.locate_targets(mapping.fhir(), occurrence, mapping.openehr())
    }

    /// Returns the location of a FHIR and an openEHR target, either absent.
    fn locate_targets(
        &self,
        fhir: Option<&FhirTarget>,
        occurrence: &Occurrence,
        openehr: Option<&OpenehrTarget>,
    ) -> Location {
        let mut location = Location::default();
        if let Some(target) = fhir {
            location = location.with_fhir(locate(&self.fhir, target.resolved(), occurrence));
        }
        if let Some(target) = openehr {
            location = location.with_openehr(openehr_path(target));
        }
        location
    }
}

/// Returns the archetype path of an openEHR target: its node's `aqlPath` with
/// the attribute tail appended.
///
/// The `aqlPath` carries the node ids and the names the template gives the
/// node, so the path holds no value of the composition.
fn openehr_path(target: &OpenehrTarget) -> String {
    let mut path = String::from(target.node().aql_path().as_str());
    for segment in &target.tail().segments {
        path.push('/');
        path.push_str(segment.attribute.as_str());
    }
    if path.is_empty() {
        path.push('/');
    }
    path
}

/// A value written at a composition attribute the FLAT builder sets from the
/// context vocabulary, beside the node it belongs to.
type Routed = (FlatId, RmValue);
