// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pair a mapping with no `type` converts through.

use core::str::FromStr;

use fhir_types::schema::Kind;
use fhir_types::schema::ValueKind;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::position::Position;
use openehr_rm::v1_2::model;

use crate::model::ast::ModelMappingFile;
use crate::model::ast::keyword::Direction;
use crate::resolve::derive;
use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::diagnostic;
use crate::resolve::program::mapping::Alternative;
use crate::resolve::program::mapping::Derived;
use crate::resolve::program::mapping::EventTime;
use crate::resolve::program::mapping::Pairing;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::element::Location;
use crate::tree::element::Move;
use crate::tree::element::resolve as resolve_element;
use crate::tree::path::FhirPath;

use crate::resolve::compile::Compiler;
use crate::resolve::compile::Derivation;

/// The two resolved sides of a mapping with no `type` key, as its conversion
/// is derived from them.
#[derive(Debug, Clone, Copy)]
struct Sides<'target> {
    fhir: &'target FhirTarget,
    openehr: &'target OpenehrTarget,
    direction: Option<Direction>,
    children: bool,
}

/// The FHIR type every extension is (<https://hl7.org/fhir/R4/extensibility.html>).
const EXTENSION_TYPE: &str = "Extension";

/// The element of an extension that carries its value, `Extension.value[x]`.
const EXTENSION_VALUE: &str = "value";

/// The reference-model class of a leaf of an item structure, whose `value`
/// holds one data value (openEHR RM, the data structures `ELEMENT` class).
const ELEMENT: &str = "ELEMENT";

/// The abstract class of an event of a history (openEHR RM, the data
/// structures `EVENT` class).
const EVENT: &str = "EVENT";

/// The event a point in time makes (RM data structures, `POINT_EVENT`).
const POINT_EVENT: &str = "POINT_EVENT";

/// The event a period makes (RM data structures, `INTERVAL_EVENT`).
const INTERVAL_EVENT: &str = "INTERVAL_EVENT";

/// The attribute of an event that carries its time (RM `EVENT.time`).
const TIME: &str = "time";

/// The FHIR type a `POINT_EVENT` is written as.
const DATE_TIME: &str = "dateTime";

/// The FHIR type an `INTERVAL_EVENT` is written as.
const PERIOD: &str = "Period";

/// The FHIR types `EVENT.adoc` pairs with a `POINT_EVENT` besides `dateTime`,
/// which go in and have no event class to come out as.
const POINT_CODES: &[&str] = &["instant", "date", "time"];

impl<'a> Compiler<'a> {
    /// Derives what the data-type cell of a mapping converts, when the
    /// mapping leaves it to the compiler.
    ///
    /// A plain value mapping with no `type` key derives its pair from its two
    /// sides, and a mapping whose `type` key names the type of a choice element
    /// no filter resolved takes that alternative. Every other mapping carries
    /// what it wrote.
    pub(super) fn derived_of(
        &mut self,
        file: &'a ModelMappingFile,
        derivation: &Derivation<'_>,
        position: Position,
        path: &ModelPath,
    ) -> Option<Derived> {
        let fhir = derivation.fhir?;
        match (derivation.openehr, derivation.data_type) {
            (Some(openehr), None) if derivation.plain => self.derive(
                file,
                Sides {
                    fhir,
                    openehr,
                    direction: derivation.direction,
                    children: derivation.children,
                },
                position,
                path,
            ),
            (_, Some(data_type))
                if derivation.plain
                    && matches!(*fhir.resolved().location(), Location::Choice(_)) =>
            {
                let code = derive::declared(data_type)?;
                self.declared_alternative(file, fhir, code, position, path)
            }
            _ => None,
        }
    }

    /// Derives the conversion of a mapping with no `type` key from its two
    /// resolved sides.
    ///
    /// A structural node anchors the children and converts nothing, as the
    /// `type: NONE` of `types-of-mappings/concept-type/FollowedBy.adoc` does,
    /// and one with no child writes nothing, which is refused. A choice
    /// element, and an `Extension` against a data value, read the alternative
    /// the document carries and write the first pair of the node's class the
    /// choice admits (`crate::resolve::derive`).
    fn derive(
        &mut self,
        file: &'a ModelMappingFile,
        sides: Sides<'_>,
        position: Position,
        path: &ModelPath,
    ) -> Option<Derived> {
        let owner = file.header().name().value();
        let Sides {
            fhir,
            openehr,
            direction,
            children,
        } = sides;
        let class = openehr.leaf_class().unwrap_or(openehr.node().rm_type());
        if derive::is_structural(class) {
            if let Some(event) = self.event_time(file, fhir, openehr, position, path) {
                return Some(event);
            }
            if let Some(polymorphic) = self.element_choice(file, fhir, openehr, position, path) {
                return Some(polymorphic);
            }
            if let Some(value) = self.element_value(fhir, openehr) {
                return Some(value);
            }
            if children {
                return Some(Derived::Anchor);
            }
            // NOTE: no specification governs this: our own design, the mapping is kept
            // with no derived pair so a run that carries its element refuses with no cell.
            self.diagnostics.push(
                Diagnostic::warning(
                    file.file().to_path_buf(),
                    ResolveCode::AnchorWithoutChildren.into(),
                    format!(
                        "`{}` names the {class} node `{}`, which holds other nodes, and no \
                         `type` or child says what to write into it, so a run that carries \
                         the element refuses",
                        fhir.expression(),
                        openehr.path()
                    ),
                )
                .with_position(position)
                .with_mapping_name(owner.clone())
                .with_model_path(path.clone()),
            );
            return None;
        }
        let read = match *fhir.resolved().location() {
            Location::Choice(_) => fhir.clone(),
            Location::Complex(schema)
                if schema.name == EXTENSION_TYPE && !derive::pairs(class).is_empty() =>
            {
                self.derived_target(file, fhir, EXTENSION_VALUE, position, path)?
            }
            Location::Complex(_) | Location::Primitive(_) | Location::Attribute => {
                let code = fhir.resolved().type_code()?;
                let text = matches!(
                    *fhir.resolved().location(),
                    Location::Primitive(ValueKind::Text) | Location::Attribute
                );
                return Some(Derived::Element(derive::element(class, code, text)));
            }
            Location::PrimitiveElement | Location::Resource | Location::Deferred => return None,
        };
        if direction == Some(Direction::FhirToOpenehr) {
            return Some(Derived::Choice { read, write: None });
        }
        let variants = match read.resolved().moves().last() {
            Some(Move::Choice { schema, .. }) => match schema.kind {
                Kind::Choice(variants) => variants,
                _ => &[],
            },
            _ => &[],
        };
        let Some(suffix) = derive::alternative(class, variants) else {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::UnderivedAlternative,
                position,
                path,
                format!(
                    "`{}` is written from the {class} node `{}`, and the choice admits none of \
                     the types {class} pairs with ({}); a `type` or an `as()` says which to write",
                    read.resolved().leaf(),
                    openehr.path(),
                    derive::pairs(class).join(", ")
                ),
            ));
            return None;
        };
        let written = self.derived_target(file, &read, &format!("as({suffix})"), position, path)?;
        let code = written.resolved().type_code().unwrap_or(suffix);
        Some(Derived::Choice {
            read,
            write: Some(Alternative::new(code, written)),
        })
    }

    /// Returns the value alternative of a polymorphic `ELEMENT` the FHIR
    /// element converts into, when the node is one and an alternative pairs.
    ///
    /// The Web Template gives each data type an `ELEMENT` admits a child node
    /// of its own, so the node holds other nodes; `FollowedBy.adoc` runs the
    /// children after the parent "has been executed", so the parent converts
    /// its own value first. A class that pairs with the FHIR type itself
    /// outranks one reached through the text fallback of [`derive::element`]:
    /// `types-of-mappings/data-type/DV_DATE_TIME.adoc` §`DateTime` pairs
    /// `dateTime` with `DV_DATE_TIME`, while `DV_TEXT.adoc` §`String` pairs only
    /// `string`. Among the rest the alternative is the one whose class pairs
    /// with the FHIR type earliest in [`derive::PAIRS`], the first in template
    /// order on a tie (no specification governs the tie-break: our own design).
    fn element_value(&self, fhir: &FhirTarget, openehr: &OpenehrTarget) -> Option<Derived> {
        if openehr.node().rm_type() != ELEMENT || !openehr.tail().segments.is_empty() {
            return None;
        }
        let location = fhir.resolved().location();
        let text = match *location {
            Location::Primitive(ValueKind::Text) | Location::Attribute => true,
            Location::Complex(_) | Location::Primitive(_) => false,
            Location::Choice(_)
            | Location::PrimitiveElement
            | Location::Resource
            | Location::Deferred => return None,
        };
        let code = fhir.resolved().type_code()?;
        let mut chosen: Option<((bool, usize), &ResolvedNode, &'static str)> = None;
        for flat_id in openehr.node().children() {
            let Ok(child) = self.template.node_by_flat_id(flat_id) else {
                continue;
            };
            let class = child.rm_type();
            let pair = derive::element(class, code, text);
            let Some(rank) = derive::pairs(class)
                .iter()
                .position(|&paired| paired == pair)
            else {
                continue;
            };
            let key = (pair != code, rank);
            if chosen.is_none_or(|(best, _, _)| key < best) {
                chosen = Some((key, child, pair));
            }
        }
        let (_, child, pair) = chosen?;
        // NOTE: openEHR RM data_structures §ELEMENT: `value` holds one data value, so the
        // alternative repeats exactly where the `ELEMENT` does and takes its axes.
        let target = OpenehrTarget::new(
            child.rm_path().clone(),
            child.clone(),
            openehr.tail().clone(),
            openehr.occurrences().to_vec(),
        );
        Some(Derived::Value { code: pair, target })
    }

    /// Returns the event time a FHIR time maps onto an `EVENT` node as, when
    /// the node is one and the FHIR element is a time, a `Period` or a choice
    /// admitting either.
    ///
    /// `types-of-mappings/data-type/EVENT.adoc` §`POINT_EVENT` and
    /// `INTERVAL_EVENT` pairs `time`, `date`, `dateTime` and `instant` with a
    /// `POINT_EVENT` and `Period` with an `INTERVAL_EVENT`. Coming out a
    /// `POINT_EVENT` is written as `dateTime` and an `INTERVAL_EVENT` as
    /// `Period`, and a node the template narrows to one class takes only its
    /// own (no specification governs which alternative is written: our own
    /// design).
    fn event_time(
        &mut self,
        file: &'a ModelMappingFile,
        fhir: &FhirTarget,
        openehr: &OpenehrTarget,
        position: Position,
        path: &ModelPath,
    ) -> Option<Derived> {
        let class = openehr.node().rm_type();
        if !model::is_a(class, EVENT) || !openehr.tail().segments.is_empty() {
            return None;
        }
        let points = model::is_a(POINT_EVENT, class);
        let intervals = model::is_a(INTERVAL_EVENT, class);
        let time = self.event_time_node(openehr)?;
        let (point, interval) = match *fhir.resolved().location() {
            Location::Choice(_) => {
                let variants = match fhir.resolved().moves().last() {
                    Some(Move::Choice { schema, .. }) => match schema.kind {
                        Kind::Choice(variants) => variants,
                        _ => &[],
                    },
                    _ => &[],
                };
                let point = derive::suffix_of(DATE_TIME, variants).filter(|_| points);
                let interval = derive::suffix_of(PERIOD, variants).filter(|_| intervals);
                if point.is_none() && interval.is_none() {
                    return None;
                }
                let mut written = |suffix: Option<&str>| match suffix {
                    Some(suffix) => self
                        .derived_target(file, fhir, &format!("as({suffix})"), position, path)
                        .map(Some),
                    None => Some(None),
                };
                (written(point)?, written(interval)?)
            }
            Location::Complex(_) | Location::Primitive(_) => match fhir.resolved().type_code()? {
                PERIOD if intervals => (None, Some(fhir.clone())),
                DATE_TIME if points => (Some(fhir.clone()), None),
                code if points && POINT_CODES.contains(&code) => (None, None),
                _ => return None,
            },
            Location::Attribute
            | Location::PrimitiveElement
            | Location::Resource
            | Location::Deferred => return None,
        };
        Some(Derived::Event(Box::new(EventTime::new(
            fhir.clone(),
            time,
            point,
            interval,
        ))))
    }

    /// Returns the `time` node of an `EVENT` node, with the event's axes.
    ///
    /// `EVENT.time` is 1..1 (openEHR RM data structures, the `EVENT` class), so
    /// the node repeats exactly where the event does.
    fn event_time_node(&self, openehr: &OpenehrTarget) -> Option<OpenehrTarget> {
        let child = openehr.node().children().iter().find_map(|flat_id| {
            self.template.node_by_flat_id(flat_id).ok().filter(|child| {
                child
                    .rm_path()
                    .segments
                    .last()
                    .is_some_and(|segment| segment.attribute == TIME)
            })
        })?;
        Some(OpenehrTarget::new(
            child.rm_path().clone(),
            child.clone(),
            openehr.tail().clone(),
            openehr.occurrences().to_vec(),
        ))
    }

    /// Returns the pairs a choice element converts through against a
    /// polymorphic `ELEMENT`, when the node is one and an alternative pairs.
    ///
    /// The Web Template gives each data type an `ELEMENT` admits a child node,
    /// and the data-type chapter pairs each class with FHIR types
    /// ([`derive::PAIRS`]), so every alternative of the choice that pairs with
    /// a child's class is kept, child by child in template order and each
    /// child's pairs in its page's order (no specification governs the order:
    /// our own design). The document's alternative or the composition's value
    /// node picks one at run time.
    fn element_choice(
        &mut self,
        file: &'a ModelMappingFile,
        fhir: &FhirTarget,
        openehr: &OpenehrTarget,
        position: Position,
        path: &ModelPath,
    ) -> Option<Derived> {
        if openehr.node().rm_type() != ELEMENT
            || !openehr.tail().segments.is_empty()
            || !matches!(*fhir.resolved().location(), Location::Choice(_))
        {
            return None;
        }
        let variants = match fhir.resolved().moves().last() {
            Some(Move::Choice { schema, .. }) => match schema.kind {
                Kind::Choice(variants) => variants,
                _ => &[],
            },
            _ => &[],
        };
        let mut pairings = Vec::new();
        for flat_id in openehr.node().children() {
            let Ok(child) = self.template.node_by_flat_id(flat_id) else {
                continue;
            };
            // NOTE: openEHR RM data_structures §ELEMENT: `value` holds one data value, so
            // each value alternative repeats exactly where the `ELEMENT` does.
            let target = OpenehrTarget::new(
                child.rm_path().clone(),
                child.clone(),
                openehr.tail().clone(),
                openehr.occurrences().to_vec(),
            );
            for (rank, code) in derive::pairs(child.rm_type()).iter().enumerate() {
                let Some(suffix) = derive::suffix_of(code, variants) else {
                    continue;
                };
                let written =
                    self.derived_target(file, fhir, &format!("as({suffix})"), position, path)?;
                let code = written.resolved().type_code().unwrap_or(code);
                pairings.push(Pairing::new(
                    Alternative::new(code, written),
                    suffix,
                    rank,
                    target.clone(),
                ));
            }
        }
        (!pairings.is_empty()).then(|| Derived::Polymorphic {
            read: fhir.clone(),
            pairings,
        })
    }

    /// Resolves the alternative of a choice element the `type` key names.
    ///
    /// The key names the FHIR type of the element, the Type ID column of the
    /// table in `types-of-mappings/data-type/data-mappings.adoc` §Deprecated, so
    /// on a choice it fixes the alternative both directions read and write.
    fn declared_alternative(
        &mut self,
        file: &'a ModelMappingFile,
        fhir: &FhirTarget,
        code: &'static str,
        position: Position,
        path: &ModelPath,
    ) -> Option<Derived> {
        let variants = match fhir.resolved().moves().last() {
            Some(Move::Choice { schema, .. }) => match schema.kind {
                Kind::Choice(variants) => variants,
                _ => &[],
            },
            _ => &[],
        };
        let Some(suffix) = derive::suffix_of(code, variants) else {
            self.diagnostics.push(diagnostic(
                file.file(),
                file.header().name().value(),
                ResolveCode::UnderivedAlternative,
                position,
                path,
                format!(
                    "the `type` of `{}` names {code}, which the choice `{}` does not admit",
                    fhir.expression(),
                    fhir.resolved().leaf()
                ),
            ));
            return None;
        };
        let written = self.derived_target(file, fhir, &format!("as({suffix})"), position, path)?;
        Some(Derived::Declared(Alternative::new(code, written)))
    }

    /// Resolves `target` extended by one step the compiler derived.
    ///
    /// The step is one the grammar admits below any element of the kind the
    /// target ends on, so a refusal is an element-table disagreement and is
    /// reported as one for a written path would be.
    fn derived_target(
        &mut self,
        file: &'a ModelMappingFile,
        target: &FhirTarget,
        step: &str,
        position: Position,
        path: &ModelPath,
    ) -> Option<FhirTarget> {
        let written = format!("{}.{step}", target.expression());
        let (code, reason) = match FhirPath::from_str(&written) {
            Ok(expression) => {
                match resolve_element(self.table, target.resolved().resource(), &expression) {
                    Ok(resolved) => {
                        return Some(
                            FhirTarget::new(expression, resolved).at_level(target.level()),
                        );
                    }
                    Err(error) => (ResolveCode::UnknownFhirElement, error.to_string()),
                }
            }
            Err(error) => (ResolveCode::MalformedFhirPath, error.to_string()),
        };
        self.diagnostics.push(diagnostic(
            file.file(),
            file.header().name().value(),
            code,
            position,
            path,
            format!("the derived `{written}` does not resolve: {reason}"),
        ));
        None
    }
}
