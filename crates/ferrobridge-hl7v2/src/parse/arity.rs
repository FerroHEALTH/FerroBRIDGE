// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constituents of a placed segment beyond what its definition expects:
//! a repetition past a field's cardinality, a component past its data
//! type's components, and a subcomponent past its component's data type.
//!
//! HL7 v2 chapter 2 §2.6, "Rules for the recipient", tells a receiver to
//! "ignore segments, fields, components, subcomponents, and extra
//! repetitions of a field that are present but were not expected", and its
//! version compatibility rules say the receiver "SHALL ignore any message
//! or message constituent it is not expecting without generating an
//! application failure" (HL7 v2+ 2021Jan, Control). So none of these
//! refuses: each valued one is counted on the result and stays in the
//! parsed message, where a map can still read it.
//!
//! Components and subcomponents are checked only against a field whose data
//! type the v2.9.1 definitions define: the legacy tables a message of an
//! earlier version is read with carry no component tables, so for those
//! fields only the repetitions are checked.

use hl7v2_types::model::{DataType, DataTypeRef, Field, Max, SegmentRef};

use crate::parse::{Component, Location, Parsed, Unplaced};

/// The MSH fields the lexer reads as one value each: the field separator
/// and the encoding characters.
const MSH_DELIMITER_FIELDS: [u16; 2] = [1, 2];

/// Counts every valued repetition, component and subcomponent of the
/// segment at `index` beyond what the definition its node places expects.
pub(super) fn check_arity(parsed: &mut Parsed, index: usize, node: &'static SegmentRef) {
    let location = parsed.location(index);
    let Some(segment) = parsed.message.segments.get(index) else {
        return;
    };
    let definition = node.segment;
    let mut found = Vec::new();
    for field in definition.fields {
        if definition.id == "MSH" && MSH_DELIMITER_FIELDS.contains(&field.position) {
            continue;
        }
        let Some(value) = segment.field(usize::from(field.position)) else {
            continue;
        };
        let at = location.clone().with_field(usize::from(field.position));
        for (offset, repetition) in value.repetitions().iter().enumerate() {
            let number = offset.saturating_add(1);
            let mut place = at.clone();
            place.repetition = Some(number);
            if !fits(number, field) {
                if repetition.is_valued() {
                    found.push(Unplaced::ExtraRepetition {
                        location: place,
                        field: field.id,
                    });
                }
                continue;
            }
            let Some(DataTypeRef::Defined(data_type)) = field.data_type else {
                continue;
            };
            components(&place, data_type, repetition.components(), &mut found);
        }
    }
    parsed.unplaced.extend(found);
}

/// Whether repetition `number`, from 1, is within the field's cardinality.
fn fits(number: usize, field: &Field) -> bool {
    match field.cardinality.max {
        Max::Unbounded => true,
        Max::Bounded(max) => usize::try_from(max).map_or(true, |max| number <= max),
    }
}

/// The number of components a value of `data_type` holds: its components,
/// or one for a primitive.
fn arity(data_type: &DataType) -> usize {
    data_type.components.len().max(1)
}

/// Counts the valued components of one repetition beyond `data_type`, and
/// the valued subcomponents of each expected component beyond its type.
fn components(
    at: &Location,
    data_type: &'static DataType,
    values: &[Component],
    found: &mut Vec<Unplaced>,
) {
    for (offset, value) in values.iter().enumerate() {
        let number = offset.saturating_add(1);
        let mut place = at.clone();
        place.component = Some(number);
        if number > arity(data_type) {
            if value.is_valued() {
                found.push(Unplaced::ExtraComponent {
                    location: place,
                    data_type: data_type.code,
                });
            }
            continue;
        }
        let inner = if data_type.components.is_empty() {
            Some(data_type)
        } else {
            data_type
                .components
                .get(offset)
                .and_then(|component| component.data_type)
        };
        let Some(inner) = inner else {
            continue;
        };
        for (sub_offset, subcomponent) in value.subcomponents().iter().enumerate() {
            let sub_number = sub_offset.saturating_add(1);
            if sub_number > arity(inner) && crate::parse::valued(subcomponent) {
                let mut sub_place = place.clone();
                sub_place.subcomponent = Some(sub_number);
                found.push(Unplaced::ExtraSubcomponent {
                    location: sub_place,
                    data_type: inner.code,
                });
            }
        }
    }
}
