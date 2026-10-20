// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One resolved mapping, its concept method and the pair a mapping with no
//! `type` converts through.

use core::fmt;

use openehr_mapping_core::header::metadata::MappingName;

use crate::model::ast::keyword::DataType;
use crate::model::ast::keyword::Direction;

use crate::resolve::program::binding::ResourceType;
use crate::resolve::program::condition::Condition;
use crate::resolve::program::condition::render_condition;
use crate::resolve::program::hierarchy::Create;
use crate::resolve::program::hierarchy::Preprocessor;
use crate::resolve::program::manual::Manual;
use crate::resolve::program::manual::render_manual;
use crate::resolve::program::port::Port;
use crate::resolve::program::port::render_ports;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;

/// What a compiled mapping does beyond mapping its two paths against each
/// other.
///
/// The concept-type chapter names eight kinds of mapping and each is decided
/// by the key the mapping writes
/// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/concept-mappings.adoc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// The two paths are mapped against each other.
    Value,
    /// The mapping hands over to another model mapping, expanded here.
    Slot {
        /// The model mapping the slot names.
        model: MappingName,
        /// The preprocessor of that file and of every extension applied to it,
        /// in application order, which gates the slotted mappings.
        preprocessors: Vec<Preprocessor>,
        /// Its compiled mappings, under this mapping's anchors.
        mappings: Vec<Mapping>,
    },
    /// The mapping enters a context and maps from its `start` with that
    /// context's extensions, expanded here
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`,
    /// §`slotContext`).
    SlotContext {
        /// The context the slot enters.
        context: MappingName,
        /// The model mapping that context starts at.
        model: MappingName,
        /// The preprocessor of that file and of every extension applied to it,
        /// in application order, which gates the slotted mappings.
        preprocessors: Vec<Preprocessor>,
        /// Its compiled mappings, under this mapping's anchors and with the
        /// entered context's extensions applied.
        mappings: Vec<Mapping>,
    },
    /// The mapping initializes another resource, whose mappings follow.
    Reference {
        /// The resource type the reference names.
        resource: ResourceType,
        /// The mappings that populate it.
        mappings: Vec<Mapping>,
    },
    /// The mapping writes an openEHR `LINK`.
    Link {
        /// The meaning the link carries.
        meaning: Option<String>,
        /// The type the link carries.
        link_type: Option<String>,
    },
    /// The mapping calls a registered function.
    Programmed {
        /// The name of the function.
        code: String,
        /// The openEHR ports its signature declares, resolved against the
        /// template.
        ports: Vec<Port>,
    },
    /// The mapping fills a participation function.
    Participation {
        /// The function the participation carries.
        function: String,
    },
}

/// The alternative of a choice element a mapping writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    code: &'static str,
    target: FhirTarget,
}

impl Alternative {
    /// Pairs the FHIR type code of the alternative with the target that
    /// writes it.
    #[must_use]
    pub const fn new(code: &'static str, target: FhirTarget) -> Self {
        Self { code, target }
    }

    /// Returns the FHIR type code of the alternative.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the target that writes the alternative, the choice resolved to
    /// it.
    #[must_use]
    pub const fn target(&self) -> &FhirTarget {
        &self.target
    }
}

/// What the data-type cell of a mapping with no `type` key converts, as the
/// compiler derived it from the two sides.
///
/// The `type` key is deprecated because the type "is derivable from the
/// instances of FHIR and openEHR"
/// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/data-mappings.adoc`,
/// §Deprecated); [`crate::resolve::derive`] holds the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derived {
    /// The FHIR type code the cell converts through.
    Element(&'static str),
    /// A choice element no type filter resolved, read as the alternative the
    /// document carries and written as the one the node's class selects.
    ///
    /// An `Extension` against a data value converts through its `value[x]`,
    /// the one element of an extension that carries data
    /// (<https://hl7.org/fhir/R4/extensibility.html>), so `read` is that
    /// choice below the mapping's own element.
    Choice {
        /// The choice element the value is read at.
        read: FhirTarget,
        /// The alternative written, `None` for a mapping that never writes
        /// FHIR.
        write: Option<Alternative>,
    },
    /// The node holds other nodes, so the mapping anchors its children and
    /// converts nothing, as `type: NONE` does.
    Anchor,
    /// The node is an `ELEMENT` whose value admits several data types, so
    /// the mapping converts into the value alternative whose class pairs
    /// with the FHIR element, and anchors its children at the `ELEMENT`.
    Value {
        /// The FHIR type code the cell converts through.
        code: &'static str,
        /// The value alternative the cell reads and writes.
        target: OpenehrTarget,
    },
    /// A choice element against an `ELEMENT` whose value admits several data
    /// types: each pair of a choice alternative and a value alternative the
    /// data-type chapter defines, in the order the engine prefers them.
    ///
    /// Going into openEHR the alternative the document carries selects the
    /// value node; coming out the value node the composition holds selects
    /// the alternative written.
    Polymorphic {
        /// The choice element the value is read at.
        read: FhirTarget,
        /// The pairs, value node by value node in template order, each node's
        /// pairs in the order its page of the chapter lists them.
        pairings: Vec<Pairing>,
    },
    /// A choice element the `type` key resolves, read and written as the
    /// alternative of that type.
    Declared(Alternative),
    /// A FHIR time against an `EVENT` node itself, whose class the time
    /// decides (`types-of-mappings/data-type/EVENT.adoc` §`POINT_EVENT` and
    /// `INTERVAL_EVENT`).
    Event(Box<EventTime>),
}

/// The event time a FHIR time maps onto an `EVENT` node as, in a
/// [`Derived::Event`] mapping.
///
/// Going into openEHR the type of the element (or the alternative of a choice
/// the document carries) decides the class; coming out the class of the event
/// picks the target written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTime {
    read: FhirTarget,
    time: OpenehrTarget,
    point: Option<FhirTarget>,
    interval: Option<FhirTarget>,
}

impl EventTime {
    /// Pairs the FHIR element the time is read at with the event's `time`
    /// node, and the targets a `POINT_EVENT` and an `INTERVAL_EVENT` are
    /// written through.
    #[must_use]
    pub const fn new(
        read: FhirTarget,
        time: OpenehrTarget,
        point: Option<FhirTarget>,
        interval: Option<FhirTarget>,
    ) -> Self {
        Self {
            read,
            time,
            point,
            interval,
        }
    }

    /// Returns the FHIR element the time is read at going into openEHR.
    #[must_use]
    pub const fn read(&self) -> &FhirTarget {
        &self.read
    }

    /// Returns the event's `time` node.
    #[must_use]
    pub const fn time(&self) -> &OpenehrTarget {
        &self.time
    }

    /// Returns the `dateTime` target a `POINT_EVENT` is written through,
    /// `None` when the element admits none.
    #[must_use]
    pub const fn point(&self) -> Option<&FhirTarget> {
        self.point.as_ref()
    }

    /// Returns the `Period` target an `INTERVAL_EVENT` is written through,
    /// `None` when the element admits none.
    #[must_use]
    pub const fn interval(&self) -> Option<&FhirTarget> {
        self.interval.as_ref()
    }
}

/// One pair of a choice alternative and the `ELEMENT` value node it converts
/// with, in a [`Derived::Polymorphic`] mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    alternative: Alternative,
    suffix: &'static str,
    rank: usize,
    target: OpenehrTarget,
}

impl Pairing {
    /// Pairs the choice alternative written through `alternative`, whose JSON
    /// suffix is `suffix`, with the value node `target`, at position `rank`
    /// among the pairs of the node's class.
    #[must_use]
    pub const fn new(
        alternative: Alternative,
        suffix: &'static str,
        rank: usize,
        target: OpenehrTarget,
    ) -> Self {
        Self {
            alternative,
            suffix,
            rank,
            target,
        }
    }

    /// Returns the choice alternative and the target that writes it.
    #[must_use]
    pub const fn alternative(&self) -> &Alternative {
        &self.alternative
    }

    /// Returns the suffix the choice alternative carries in a document.
    #[must_use]
    pub const fn suffix(&self) -> &'static str {
        self.suffix
    }

    /// Returns the position of the pair among the pairs of the node's class.
    #[must_use]
    pub const fn rank(&self) -> usize {
        self.rank
    }

    /// Returns the value node the pair converts into and out of.
    #[must_use]
    pub const fn target(&self) -> &OpenehrTarget {
        &self.target
    }
}

impl fmt::Display for Derived {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Element(code) => write!(f, "{code}"),
            Self::Choice {
                ref read,
                ref write,
            } => {
                write!(f, "choice read {read}")?;
                if let Some(written) = write.as_ref() {
                    write!(f, " write {} as {}", written.code, written.target)?;
                }
                Ok(())
            }
            Self::Anchor => f.write_str("anchor"),
            Self::Value { code, ref target } => {
                write!(f, "{code} into {}", target.node().rm_type())
            }
            Self::Declared(ref written) => {
                write!(f, "{} as {}", written.code, written.target)
            }
            Self::Polymorphic {
                ref read,
                ref pairings,
            } => {
                write!(f, "choice read {read}")?;
                for pairing in pairings {
                    write!(
                        f,
                        " {} into {}",
                        pairing.alternative.code,
                        pairing.target.node().rm_type()
                    )?;
                }
                Ok(())
            }
            Self::Event(ref event) => {
                write!(f, "event time read {} into {}", event.read, event.time)?;
                if let Some(ref point) = event.point {
                    write!(f, " point as {point}")?;
                }
                if let Some(ref interval) = event.interval {
                    write!(f, " interval as {interval}")?;
                }
                Ok(())
            }
        }
    }
}

/// One compiled mapping method, with everything the interpreter needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    name: String,
    model: MappingName,
    fhir: Option<FhirTarget>,
    openehr: Option<OpenehrTarget>,
    data_type: Option<DataType>,
    derived: Option<Derived>,
    value: Option<String>,
    direction: Option<Direction>,
    fhir_condition: Option<Condition>,
    openehr_condition: Option<Condition>,
    manual: Vec<Manual>,
    conceptmap: Option<String>,
    method: Method,
    followed_by: Vec<Mapping>,
}

/// Everything a compiled mapping carries, as the compiler assembles it.
///
/// The compiler fills one of these and hands it to [`Mapping::new`], which
/// keeps a function from taking a dozen positional arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingParts {
    /// The dotted name of the mapping inside its model mapping.
    pub name: String,
    /// The model mapping the method was read from.
    pub model: MappingName,
    /// The resolved FHIR side.
    pub fhir: Option<FhirTarget>,
    /// The resolved openEHR side.
    pub openehr: Option<OpenehrTarget>,
    /// The data type the mapping pins.
    pub data_type: Option<DataType>,
    /// The conversion the compiler derived for a mapping with no `type` key.
    pub derived: Option<Derived>,
    /// The literal value the mapping writes.
    pub value: Option<String>,
    /// The one direction the mapping runs in, when it is pinned to one.
    pub direction: Option<Direction>,
    /// The condition the FHIR to openEHR direction evaluates.
    pub fhir_condition: Option<Condition>,
    /// The condition the openEHR to FHIR direction evaluates.
    pub openehr_condition: Option<Condition>,
    /// The manual entries of the mapping.
    pub manual: Vec<Manual>,
    /// The `ConceptMap.url` the codes of this method are translated through,
    /// the method's own or the one its file's header attaches.
    pub conceptmap: Option<String>,
    /// What the mapping does beyond mapping its two paths.
    pub method: Method,
    /// The mappings that run after this one, under its anchors.
    pub followed_by: Vec<Mapping>,
}

impl Mapping {
    /// Assembles a compiled mapping.
    #[must_use]
    pub fn new(parts: MappingParts) -> Self {
        Self {
            name: parts.name,
            model: parts.model,
            fhir: parts.fhir,
            openehr: parts.openehr,
            data_type: parts.data_type,
            derived: parts.derived,
            value: parts.value,
            direction: parts.direction,
            fhir_condition: parts.fhir_condition,
            openehr_condition: parts.openehr_condition,
            manual: parts.manual,
            conceptmap: parts.conceptmap,
            method: parts.method,
            followed_by: parts.followed_by,
        }
    }

    /// Returns the dotted name of the mapping inside its model mapping.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the model mapping the method was read from.
    #[must_use]
    pub const fn model(&self) -> &MappingName {
        &self.model
    }

    /// Returns the resolved FHIR side.
    #[must_use]
    pub const fn fhir(&self) -> Option<&FhirTarget> {
        self.fhir.as_ref()
    }

    /// Returns the resolved openEHR side.
    #[must_use]
    pub const fn openehr(&self) -> Option<&OpenehrTarget> {
        self.openehr.as_ref()
    }

    /// Returns the data type the mapping pins.
    #[must_use]
    pub const fn data_type(&self) -> Option<DataType> {
        self.data_type
    }

    /// Returns the conversion the compiler derived, for a mapping with no
    /// `type` key.
    #[must_use]
    pub const fn derived(&self) -> Option<&Derived> {
        self.derived.as_ref()
    }

    /// Returns the literal value the mapping writes.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Returns the one direction the mapping runs in, when it is pinned.
    #[must_use]
    pub const fn direction(&self) -> Option<Direction> {
        self.direction
    }

    /// Returns the condition the FHIR to openEHR direction evaluates.
    #[must_use]
    pub const fn fhir_condition(&self) -> Option<&Condition> {
        self.fhir_condition.as_ref()
    }

    /// Returns the condition the openEHR to FHIR direction evaluates.
    #[must_use]
    pub const fn openehr_condition(&self) -> Option<&Condition> {
        self.openehr_condition.as_ref()
    }

    /// Returns the manual entries of the mapping.
    #[must_use]
    pub fn manual(&self) -> &[Manual] {
        &self.manual
    }

    /// Returns the concept map the codes of this mapping translate through.
    ///
    /// The method's own `conceptmap` wins and a `spec.conceptmap` of the file
    /// that contributed the method is the fallback: "the concept map can also
    /// be directly attached inside the header, this way all codes contained in
    /// the conceptmap will be transformed using the conceptmap"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/manual.adoc`,
    /// §`ConceptMaps`).
    #[must_use]
    pub fn conceptmap(&self) -> Option<&str> {
        self.conceptmap.as_deref()
    }

    /// Returns what the mapping does beyond mapping its two paths.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the mappings that run after this one.
    #[must_use]
    pub fn followed_by(&self) -> &[Mapping] {
        &self.followed_by
    }

    /// Writes this mapping and everything under it at `depth`.
    pub(super) fn render(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let pad = "  ".repeat(depth);
        writeln!(f, "{pad}mapping {} from {}", self.name, self.model)?;
        let inner = "  ".repeat(depth.saturating_add(1));
        if let Some(ref fhir) = self.fhir {
            writeln!(f, "{inner}fhir {fhir}")?;
        }
        if let Some(ref openehr) = self.openehr {
            writeln!(f, "{inner}openehr {openehr}")?;
        }
        if let Some(data_type) = self.data_type {
            writeln!(f, "{inner}type {data_type}")?;
        }
        if let Some(ref derived) = self.derived {
            writeln!(f, "{inner}derived {derived}")?;
        }
        if let Some(ref value) = self.value {
            writeln!(f, "{inner}value {value}")?;
        }
        if let Some(direction) = self.direction {
            writeln!(f, "{inner}unidirectional {direction}")?;
        }
        if let Some(ref conceptmap) = self.conceptmap {
            writeln!(f, "{inner}conceptmap {conceptmap}")?;
        }
        for (label, condition) in [
            ("fhirCondition", self.fhir_condition.as_ref()),
            ("openehrCondition", self.openehr_condition.as_ref()),
        ] {
            if let Some(condition) = condition {
                render_condition(f, &inner, label, condition)?;
            }
        }
        for manual in &self.manual {
            render_manual(f, &inner, manual)?;
        }
        match self.method {
            Method::Value => {}
            Method::Slot {
                ref model,
                ref preprocessors,
                ref mappings,
            } => {
                writeln!(f, "{inner}slotArchetype {model}")?;
                let under = "  ".repeat(depth.saturating_add(2));
                for preprocessor in preprocessors {
                    render_preprocessor(f, &under, preprocessor)?;
                }
                for mapping in mappings {
                    mapping.render(f, depth.saturating_add(2))?;
                }
            }
            Method::SlotContext {
                ref context,
                ref model,
                ref preprocessors,
                ref mappings,
            } => {
                writeln!(f, "{inner}slotContext {context} start {model}")?;
                let under = "  ".repeat(depth.saturating_add(2));
                for preprocessor in preprocessors {
                    render_preprocessor(f, &under, preprocessor)?;
                }
                for mapping in mappings {
                    mapping.render(f, depth.saturating_add(2))?;
                }
            }
            Method::Reference {
                ref resource,
                ref mappings,
            } => {
                writeln!(f, "{inner}reference {resource}")?;
                for mapping in mappings {
                    mapping.render(f, depth.saturating_add(2))?;
                }
            }
            Method::Link {
                ref meaning,
                ref link_type,
            } => {
                writeln!(
                    f,
                    "{inner}link meaning {} type {}",
                    meaning.as_deref().unwrap_or("-"),
                    link_type.as_deref().unwrap_or("-")
                )?;
            }
            Method::Programmed {
                ref code,
                ref ports,
            } => writeln!(f, "{inner}mappingCode {code}{}", render_ports(ports))?,
            Method::Participation { ref function } => {
                writeln!(f, "{inner}participationsFunction {function}")?;
            }
        }
        for child in &self.followed_by {
            child.render(f, depth.saturating_add(1))?;
        }
        Ok(())
    }
}

/// Writes one file's compiled preprocessor.
pub(super) fn render_preprocessor(
    f: &mut fmt::Formatter<'_>,
    pad: &str,
    preprocessor: &Preprocessor,
) -> fmt::Result {
    if preprocessor.is_empty() {
        return Ok(());
    }
    writeln!(f, "{pad}preprocessor {}", preprocessor.model())?;
    let inner = format!("{pad}  ");
    for (label, condition) in [
        ("fhirCondition", preprocessor.fhir_condition()),
        ("openehrCondition", preprocessor.openehr_condition()),
    ] {
        if let Some(condition) = condition {
            render_condition(f, &inner, label, condition)?;
        }
    }
    if let Some(hierarchy) = preprocessor.hierarchy() {
        writeln!(f, "{inner}hierarchy")?;
        if let Some(fhir) = hierarchy.fhir() {
            writeln!(f, "{inner}  fhir {fhir}")?;
        }
        if let Some(openehr) = hierarchy.openehr() {
            writeln!(f, "{inner}  openehr {openehr}")?;
        }
        for (label, split) in [
            ("split fhir", hierarchy.split_fhir()),
            ("split openehr", hierarchy.split_openehr()),
        ] {
            let Some(split) = split else {
                continue;
            };
            writeln!(
                f,
                "{inner}  {label} create {}",
                split.create().map_or("-", Create::as_str)
            )?;
            if let Some(path) = split.path() {
                writeln!(f, "{inner}    path {path}")?;
            }
            for unique in split.unique() {
                writeln!(f, "{inner}    unique {unique}")?;
            }
        }
    }
    Ok(())
}
