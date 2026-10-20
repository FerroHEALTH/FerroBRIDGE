// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR time mapped onto an `EVENT` node itself.
//!
//! "The time that maps to the event time in FHIRconnect, determines if the
//! `EVENT` is an `POINT_EVENT` or `INTERVAL_EVENT`"
//! (`types-of-mappings/data-type/EVENT.adoc` §`POINT_EVENT` and `INTERVAL_EVENT`).
//! A `dateTime`, `instant` or `date` is the `time` of a `POINT_EVENT`. A
//! `Period` is an `INTERVAL_EVENT`: its `time` is "the time point of the
//! trailing edge of the event", `interval_start_time = time - width`, and
//! `math_function` defaults to `640|actual|` (openEHR RM Release 1.1.0, data
//! structures, the `EVENT` and `INTERVAL_EVENT` classes). The width is the
//! definite difference of BASE `Iso8601_date_time`.

use openehr_base::v1_3::foundation_types::time::iso8601_date_time::Iso8601DateTime;
use openehr_base::v1_3::foundation_types::time::iso8601_duration::Iso8601Duration;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::paths::FlatId;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use serde_json::json;

use crate::engine::cell;
use crate::engine::fhir::FhirKind;
use crate::engine::fhir::FhirValue;
use crate::engine::outcome::Loss;
use crate::engine::outcome::SkipReason;
use crate::engine::outcome::Warning;
use crate::engine::rm::RmValue;
use crate::resolve::program::mapping::EventTime;
use crate::resolve::program::mapping::Mapping;
use crate::tree::Occurrence;
use crate::tree::element::Table;
use crate::tree::read::read;

use crate::engine::traverse::Binding;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::error::EventRefusal;

/// The `_type` of an interval event (RM data structures, `INTERVAL_EVENT`).
const INTERVAL_EVENT: &str = "INTERVAL_EVENT";

/// The `event math function` code the RM gives `INTERVAL_EVENT.math_function`
/// as its default, `640|actual|`.
const ACTUAL: &str = "640";

/// The rubric of [`ACTUAL`] the RM default names.
const ACTUAL_RUBRIC: &str = "actual";

/// The class of the RM value an event's `time` holds.
const DV_DATE_TIME: &str = "DV_DATE_TIME";

/// The class of the RM value a period converts through.
const DATE_TIME_INTERVAL: &str = "DV_INTERVAL<DV_DATE_TIME>";

/// One interval event a run wrote, whose class, width and function the
/// engine sets on the built composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Interval {
    /// The mapping that wrote it, for a refusal.
    mapping: String,
    /// The event node.
    node: FlatId,
    /// The instance of each repeating node down to the event.
    positions: Vec<RmPosition>,
    /// The width, an ISO 8601 duration.
    width: String,
}

/// The class of event a FHIR type makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// A `POINT_EVENT`, from a `dateTime`, `instant` or `date`.
    Point,
    /// A `POINT_EVENT` from a `time`, which has no date to carry.
    TimeOfDay,
    /// An `INTERVAL_EVENT`, from a `Period`.
    Interval,
}

/// Returns the class of event the FHIR type `code` (or the suffix a choice
/// alternative writes it with) makes, `None` for a type `EVENT.adoc` pairs
/// with no class.
fn class_of(code: &str) -> Option<Class> {
    let named = |name: &str| code.eq_ignore_ascii_case(name);
    if named("dateTime") || named("instant") || named("date") {
        Some(Class::Point)
    } else if named("time") {
        Some(Class::TimeOfDay)
    } else if named("Period") {
        Some(Class::Interval)
    } else {
        None
    }
}

/// Returns the refusal of `mapping` carrying `element` for `reason`.
fn refused(mapping: &Mapping, element: &str, reason: EventRefusal) -> EngineError {
    EngineError::EventTime {
        mapping: String::from(mapping.name()),
        element: String::from(element),
        reason,
    }
}

impl<T: Table + ?Sized> Run<'_, T> {
    /// Writes the event time one input occurrence carries.
    pub(super) fn event_in(
        &mut self,
        mapping: &Mapping,
        event: &EventTime,
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let read_at = event.read();
        let matches = read(self.table, self.document_for(read_at), read_at.expression()).map_err(
            |source| EngineError::Read {
                mapping: String::from(mapping.name()),
                expression: String::from(read_at.expression().as_str()),
                source: Box::new(source),
            },
        )?;
        let Some(found) = matches.iter().find(|matched| matched.occurrence() == input) else {
            return Ok(());
        };
        let Some(value) = found.value() else {
            return Ok(());
        };
        let element = read_at.resolved().leaf();
        let refuse = |reason| refused(mapping, element, reason);
        let code = found
            .alternative()
            .map(|(suffix, _)| suffix)
            .or_else(|| read_at.resolved().type_code())
            .ok_or_else(|| refuse(EventRefusal::Type))?;
        match class_of(code).ok_or_else(|| refuse(EventRefusal::Type))? {
            Class::TimeOfDay => Err(refuse(EventRefusal::NoDate)),
            Class::Point => {
                // NOTE: R4 datatypes §dateTime: a `date` and an `instant` are forms of the
                // `dateTime` lexical space, so the text goes in unchanged as a `dateTime`.
                let view = Self::event_view(mapping, FhirKind::DateTime, element, value)?;
                let time = Self::event_put(mapping, element, &view, DV_DATE_TIME)?;
                self.put_openehr_at(event.time(), binding, time);
                Ok(())
            }
            Class::Interval => {
                let view = Self::event_view(mapping, FhirKind::Period, element, value)?;
                let RmValue::DateTimeInterval(interval) =
                    Self::event_put(mapping, element, &view, DATE_TIME_INTERVAL)?
                else {
                    return Err(refuse(EventRefusal::Type));
                };
                let (Some(start), Some(end)) = (interval.lower, interval.upper) else {
                    return Err(refuse(EventRefusal::OpenPeriod));
                };
                let width = Iso8601DateTime {
                    value: end.value.clone(),
                }
                .diff(&Iso8601DateTime {
                    value: start.value.clone(),
                })
                .filter(|width| !width.value.starts_with('-'))
                .ok_or_else(|| refuse(EventRefusal::NoWidth))?;
                let node = mapping
                    .openehr()
                    .ok_or_else(|| refuse(EventRefusal::Type))?
                    .node();
                self.intervals.push(Interval {
                    mapping: String::from(mapping.name()),
                    node: node.flat_id().clone(),
                    positions: binding.openehr.clone(),
                    width: width.value,
                });
                self.put_openehr_at(event.time(), binding, RmValue::DateTime(Box::new(end)));
                Ok(())
            }
        }
    }

    /// Writes the FHIR time one event instance holds.
    pub(super) fn event_out(
        &mut self,
        mapping: &Mapping,
        event: &EventTime,
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let Some(node) = mapping.openehr() else {
            return Ok(());
        };
        let positions = Self::positions_of(mapping, input, node)?;
        let Some(held) = self.node_value(mapping.name(), node, &positions)? else {
            return Ok(());
        };
        let element = event.read().resolved().leaf();
        let refuse = |reason| refused(mapping, element, reason);
        let aql_path = node.node().aql_path().as_str();
        let date_time = |value: &serde_json::Value| {
            RmValue::from_canonical(DV_DATE_TIME, aql_path, value).map_err(|source| {
                EngineError::Rm {
                    mapping: String::from(mapping.name()),
                    node: String::from(aql_path),
                    source: Box::new(source),
                }
            })
        };
        let time = held
            .get("time")
            .ok_or_else(|| refuse(EventRefusal::Unreadable))?;
        let RmValue::DateTime(end) = date_time(time)? else {
            return Err(refuse(EventRefusal::Unreadable));
        };
        let interval =
            held.get("_type").and_then(serde_json::Value::as_str) == Some(INTERVAL_EVENT);
        let (target, view) = if interval {
            let target = event
                .interval()
                .ok_or_else(|| refuse(EventRefusal::OtherClass))?;
            let width = held
                .get("width")
                .and_then(|width| width.get("value"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| refuse(EventRefusal::Unreadable))?;
            let start = Iso8601DateTime {
                value: end.value.clone(),
            }
            .subtract(&Iso8601Duration {
                value: String::from(width),
            })
            .ok_or_else(|| refuse(EventRefusal::Unreadable))?;
            let period = DvInterval {
                lower: Some(DvDateTime {
                    value: start.value,
                    ..(*end).clone()
                }),
                upper: Some(*end),
                lower_unbounded: false,
                upper_unbounded: false,
                lower_included: true,
                upper_included: true,
            };
            let view = Self::event_get(
                mapping,
                element,
                &RmValue::DateTimeInterval(Box::new(period)),
                FhirKind::Period,
            )?;
            let function = held
                .pointer("/math_function/defining_code/code_string")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| refuse(EventRefusal::Unreadable))?;
            if function != ACTUAL {
                let location = self.locate(mapping, input);
                self.warnings.push(Warning::new(
                    Loss::Skipped {
                        mapping: String::from(mapping.name()),
                        reason: SkipReason::EventFunctionUncarried {
                            code: String::from(function),
                        },
                    },
                    location,
                ));
            }
            (target, view)
        } else {
            let target = event
                .point()
                .ok_or_else(|| refuse(EventRefusal::OtherClass))?;
            let view = Self::event_get(
                mapping,
                element,
                &RmValue::DateTime(end),
                FhirKind::DateTime,
            )?;
            (target, view)
        };
        self.put_fhir_at(mapping, target, binding, &view)
    }

    /// Sets the class, the width and the function of every interval event the
    /// run wrote on the built composition.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the edited composition does not
    /// validate against the template.
    pub(super) fn amend_intervals(
        &self,
        mut built: CanonicalComposition,
    ) -> Result<CanonicalComposition, EngineError> {
        for interval in &self.intervals {
            let refuse = |source| EngineError::Template {
                mapping: interval.mapping.clone(),
                node: interval.node.as_str().to_owned(),
                source: Box::new(source),
            };
            let node = self.index.node_by_flat_id(&interval.node).map_err(refuse)?;
            built = self
                .index
                .amend(built, node, &interval.positions, |event| {
                    event.insert(String::from("_type"), json!(INTERVAL_EVENT));
                    event.insert(
                        String::from("width"),
                        json!({"_type": "DV_DURATION", "value": interval.width}),
                    );
                    event.insert(
                        String::from("math_function"),
                        json!({
                            "_type": "DV_CODED_TEXT",
                            "value": ACTUAL_RUBRIC,
                            "defining_code": {
                                "_type": "CODE_PHRASE",
                                "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr"},
                                "code_string": ACTUAL
                            }
                        }),
                    );
                })
                .map_err(refuse)?;
        }
        Ok(built)
    }

    /// Reads a FHIR value as `kind`.
    fn event_view(
        mapping: &Mapping,
        kind: FhirKind,
        element: &str,
        value: &fhir_types::codec::Value,
    ) -> Result<FhirValue, EngineError> {
        FhirValue::read(kind, element, value).map_err(|source| EngineError::Fhir {
            mapping: String::from(mapping.name()),
            element: String::from(element),
            source: Box::new(source),
        })
    }

    /// Converts a FHIR value into the RM class `rm_type` through its cell.
    fn event_put(
        mapping: &Mapping,
        element: &str,
        view: &FhirValue,
        rm_type: &str,
    ) -> Result<RmValue, EngineError> {
        cell::put(view, rm_type, None, None)
            .map(|produced| produced.value)
            .map_err(|source| EngineError::Cell {
                mapping: String::from(mapping.name()),
                element: String::from(element),
                source: Box::new(source),
            })
    }

    /// Converts an RM value into the FHIR element `kind` through its cell.
    fn event_get(
        mapping: &Mapping,
        element: &str,
        value: &RmValue,
        kind: FhirKind,
    ) -> Result<FhirValue, EngineError> {
        cell::get(value, kind, None).map_err(|source| EngineError::Cell {
            mapping: String::from(mapping.name()),
            element: String::from(element),
            source: Box::new(source),
        })
    }
}
