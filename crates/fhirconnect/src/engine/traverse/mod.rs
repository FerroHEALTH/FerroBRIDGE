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
pub mod event;
pub mod functions;
pub mod links;
pub mod manual;
pub mod occurrence;
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
use crate::engine::lens::null_flavour::NullFlavour;
use crate::engine::origin::Origin;
use crate::engine::outcome::Location;
use crate::engine::outcome::Outcome;
use crate::engine::outcome::Warning;
use crate::engine::rm::RmValue;
use crate::engine::seam::Seams;
use crate::engine::subject::SubjectIdentifier;
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
    /// How many references deep the document the FHIR occurrence indexes
    /// lies, the program's own resource being `0`.
    level: usize,
}

/// A document a `reference` mapping left to run over the resource it
/// resolved or creates.
///
/// A `^` run can climb back out of the referenced resource
/// (`basics/path_operators.adoc`, §Recurrence and parent elements), so the
/// document stays reachable with the occurrence the reference was entered at.
#[derive(Debug)]
struct Frame {
    /// The document the reference mapping was written in.
    document: Value,
    /// The occurrence of the reference element in that document.
    occurrence: Occurrence,
    /// The split pin that document held.
    pin: Option<(Vec<String>, Occurrence)>,
    /// The instance counters of that document.
    counters: BTreeMap<(String, String), usize>,
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
    /// An `ELEMENT`'s null flavour with its reason.
    NullFlavour(Box<NullFlavour>),
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
    let built = run.amend_intervals(built)?;
    run.carried_whole(&built, &routed)?;
    Ok(Outcome::new(built, run.warnings)
        .with_consumed(run.consumed)
        .with_subject(run.subject)
        .with_reads(run.reads))
}

/// Runs `program` over a composition, producing a FHIR resource.
///
/// The run maps the first occurrence of the context's start archetype. When
/// the composition holds it more than once, the further occurrences are
/// declared as [`crate::engine::outcome::Loss::UnmappedOccurrences`], never
/// mixed into the resource; [`to_fhir_each`] maps every occurrence on its own.
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
    let instances = root_instances(program, index, composition)?;
    let first = instances.first().cloned().unwrap_or_default();
    let mut outcome = read_out(program, table, index, composition, seams, context, &first)?;
    let further = instances.len().saturating_sub(1);
    if further > 0 {
        outcome.warn(Warning::new(
            crate::engine::outcome::Loss::UnmappedOccurrences {
                archetype: start_archetype(program),
                count: further,
            },
            Location::default(),
        ));
    }
    Ok(outcome)
}

/// One occurrence of a context's start archetype, run on its own by
/// [`to_fhir_each`].
#[derive(Debug)]
pub enum Each {
    /// The occurrence mapped to a resource.
    Mapped {
        /// The 1-based instance of each repeating axis down to the start
        /// archetype.
        occurrence: Vec<u32>,
        /// The path that names the occurrence in the composition
        /// ([`openehr_mapping_core::index::WebTemplateIndex::entry_path`]),
        /// the entry path its deterministic id derives from
        /// ([`crate::engine::identity::deterministic_id`]).
        path: String,
        /// The resource and what its run declared.
        outcome: Outcome<Value>,
    },
    /// The context's `openehrCondition` does not hold at the occurrence.
    NotApplicable {
        /// The 1-based instance of each repeating axis down to the start
        /// archetype.
        occurrence: Vec<u32>,
        /// The path that names the occurrence in the composition.
        path: String,
    },
}

/// Runs `program` once per occurrence of its start archetype in a
/// composition, producing one resource per occurrence.
///
/// Each run reads its own occurrence's subtree alone, the start archetype's
/// axes pinned to it the way a `hierarchy.split` pins its node
/// (`types-of-mappings/concept-type/HierarchyMappings.adoc`, §split), and the
/// context's `openehrCondition` is evaluated at that occurrence, so one
/// occurrence may map while another is not applicable. FHIRconnect v1.0.0
/// does not say how a context binds to a start archetype a composition holds
/// more than once: no specification governs this, our own design. A
/// composition that holds no occurrence answers an empty list.
///
/// # Errors
///
/// [`EngineError`] for any element one occurrence cannot map, and the
/// [`EngineError::Invariant`] of a resource that breaks a constraint; a
/// condition that does not hold is [`Each::NotApplicable`], never an error.
pub fn to_fhir_each<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<Vec<Each>, EngineError> {
    composition_program(program)?;
    let mut each = Vec::new();
    for instance in root_instances(program, index, composition)? {
        let occurrence: Vec<u32> = instance.iter().map(|position| position.get()).collect();
        // NOTE: an instance of the axes that holds no start archetype root is
        // no occurrence of it, a legitimate absence; no specification governs
        // this, our own design.
        let Some(path) = entry_path(program, index, composition, &instance)? else {
            continue;
        };
        match read_out(
            program,
            table,
            index,
            composition,
            seams,
            context,
            &instance,
        ) {
            Ok(outcome) => each.push(Each::Mapped {
                occurrence,
                path,
                outcome,
            }),
            Err(EngineError::NotApplicable { .. }) => {
                each.push(Each::NotApplicable { occurrence, path });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(each)
}

/// Returns every instance of the program's start archetype the composition
/// holds, as the positions of its repeating axes, in document order.
///
/// A start archetype on no repeating axis has the one instance `[]`.
fn root_instances(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Vec<Vec<RmPosition>>, EngineError> {
    let mut found: Vec<Vec<RmPosition>> = vec![Vec::new()];
    for axis in program.root_axes() {
        let refuse = |source| EngineError::Template {
            mapping: String::from(program.context().as_str()),
            node: String::from(axis.as_str()),
            source: Box::new(source),
        };
        let node = index.node_by_flat_id(axis).map_err(refuse)?;
        let mut deeper = Vec::new();
        for prefix in found {
            let mut instance = 1u32;
            while instance <= INSTANCE_CEILING {
                let mut positions = prefix.clone();
                positions.push(RmPosition::new(instance).map_err(|source| {
                    EngineError::Position {
                        mapping: String::from(program.context().as_str()),
                        source,
                    }
                })?);
                if !index.holds(composition, node, &positions).map_err(refuse)? {
                    break;
                }
                deeper.push(positions);
                instance = instance.saturating_add(1);
            }
        }
        found = deeper;
    }
    Ok(found)
}

/// Returns the path that names one instance of the program's start archetype
/// root in `composition`, or `None` when the instance holds no root.
///
/// A program with no root node in its template, a context whose start is the
/// composition itself, names [`crate::engine::identity::ROOT_PATH`].
fn entry_path(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    instance: &[RmPosition],
) -> Result<Option<String>, EngineError> {
    let Some(root) = program.root() else {
        return Ok(Some(String::from(crate::engine::identity::ROOT_PATH)));
    };
    let refuse = |source| EngineError::Template {
        mapping: String::from(program.context().as_str()),
        node: String::from(root.as_str()),
        source: Box::new(source),
    };
    let node = index.node_by_flat_id(root).map_err(refuse)?;
    let named = index
        .entry_path(composition, node, instance)
        .map_err(refuse)?;
    Ok(named.map(|path| {
        let rendered = path.to_string();
        if rendered.is_empty() {
            String::from(crate::engine::identity::ROOT_PATH)
        } else {
            rendered
        }
    }))
}

/// Returns the archetype id the context's start model mapping declares.
pub(crate) fn start_archetype(program: &Program) -> String {
    program
        .models()
        .iter()
        .find(|model| model.name() == program.start())
        .and_then(|model| model.archetype())
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Runs `program` over a canonical document, a composition or a party,
/// producing a FHIR resource, with the start archetype's axes pinned to
/// `instance` when it names one.
fn read_out<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    document: &dyn CanonicalDocument,
    seams: &Seams<'_>,
    context: &CallContext,
    instance: &[RmPosition],
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
    run.pins = program
        .root_axes()
        .iter()
        .cloned()
        .zip(instance.iter().copied())
        .collect();
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
    /// The documents the current reference chain left, outermost first, so
    /// the frame at index `n` is the document of level `n`.
    frames: Vec<Frame>,
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
    /// The identifiers a `mappingCode` function offered as the subject of the EHR.
    subject: Vec<SubjectIdentifier>,
    /// The interval events the run wrote, set on the built composition.
    intervals: Vec<event::Interval>,
    /// The FHIR elements the run read going into openEHR.
    reads: Vec<Location>,
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
            frames: Vec::new(),
            created: Vec::new(),
            consumed: Vec::new(),
            pins: Vec::new(),
            fhir_pin: None,
            translations: BTreeMap::new(),
            subject: Vec::new(),
            intervals: Vec::new(),
            reads: Vec::new(),
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
            location = location.with_fhir(locate(
                self.document_for(target),
                target.resolved(),
                occurrence,
            ));
        }
        if let Some(target) = openehr {
            location = location.with_openehr(openehr_path(target));
        }
        location
    }

    /// Returns the FHIR document `target` is rooted in: the current one, or
    /// the one an enclosing `reference` mapping left.
    fn document_for(&self, target: &FhirTarget) -> &Value {
        self.frames
            .get(target.level())
            .map_or(&self.fhir, |frame| &frame.document)
    }

    /// Returns the FHIR document a condition's `targetRoot` is rooted in.
    fn condition_document(&self, gate: &crate::resolve::program::condition::Condition) -> &Value {
        match *gate.target() {
            crate::resolve::program::target::Target::Fhir(ref target) => self.document_for(target),
            crate::resolve::program::target::Target::Openehr(_) => &self.fhir,
        }
    }

    /// Returns the FHIR document `target` is rooted in, to write.
    fn document_for_mut(&mut self, target: &FhirTarget) -> &mut Value {
        match self.frames.get_mut(target.level()) {
            Some(frame) => &mut frame.document,
            None => &mut self.fhir,
        }
    }

    /// Returns the split pin of the document `target` is rooted in.
    fn pin_for(&self, target: &FhirTarget) -> Option<&(Vec<String>, Occurrence)> {
        self.frames
            .get(target.level())
            .map_or(self.fhir_pin.as_ref(), |frame| frame.pin.as_ref())
    }

    /// Returns the split pin of the document of `level`, to set.
    fn pin_at(&mut self, level: usize) -> &mut Option<(Vec<String>, Occurrence)> {
        match self.frames.get_mut(level) {
            Some(frame) => &mut frame.pin,
            None => &mut self.fhir_pin,
        }
    }

    /// Returns the instance counters of the document of `level`.
    fn counters_at(&mut self, level: usize) -> &mut BTreeMap<(String, String), usize> {
        match self.frames.get_mut(level) {
            Some(frame) => &mut frame.counters,
            None => &mut self.counters,
        }
    }

    /// Returns the binding a mapping whose FHIR side is `target` runs under.
    ///
    /// A parent at the target's level binds it. A target a `^` run bound in
    /// an enclosing document lies under the occurrence that document's
    /// reference was entered at, whatever the parent inside the reference
    /// bound.
    fn scoped(&self, target: Option<&FhirTarget>, parent: &Binding) -> Binding {
        let Some(target) = target.filter(|target| target.level() != parent.level) else {
            return parent.clone();
        };
        self.frames.get(target.level()).map_or_else(
            || parent.clone(),
            |frame| Binding {
                fhir: frame.occurrence.clone(),
                openehr: parent.openehr.clone(),
                level: target.level(),
            },
        )
    }

    /// Enters a document of its own for a `reference` mapping, keeping the
    /// current one as a frame, and returns the level of the entered one.
    ///
    /// Going into openEHR the FHIR side is the input, so the frame keeps the
    /// split pin and the counters stay with the openEHR output; coming out
    /// the FHIR side is the output, so the frame keeps the instance counters
    /// of the document left.
    fn enter_frame(&mut self, document: Value, occurrence: Occurrence) -> usize {
        // NOTE: no specification governs this: our own design, the document a
        // `^` run climbs back into is kept whole beside the occurrence it left.
        let document = core::mem::replace(&mut self.fhir, document);
        let (pin, counters) = match self.direction {
            Direction::FhirToOpenehr => (self.fhir_pin.take(), BTreeMap::new()),
            Direction::OpenehrToFhir => (None, core::mem::take(&mut self.counters)),
        };
        self.frames.push(Frame {
            document,
            occurrence,
            pin,
            counters,
        });
        self.frames.len()
    }

    /// Leaves the document [`Self::enter_frame`] entered, and returns it.
    #[expect(
        clippy::expect_used,
        reason = "every caller pairs this with the enter_frame it made in the same function, \
                  so a frame is always there to leave"
    )]
    fn leave_frame(&mut self) -> Value {
        let frame = self
            .frames
            .pop()
            .expect("a reference should leave the frame it entered");
        match self.direction {
            Direction::FhirToOpenehr => self.fhir_pin = frame.pin,
            Direction::OpenehrToFhir => self.counters = frame.counters,
        }
        core::mem::replace(&mut self.fhir, frame.document)
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
