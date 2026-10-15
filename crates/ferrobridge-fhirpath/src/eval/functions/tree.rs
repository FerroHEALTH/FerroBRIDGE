// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Tree navigation (N1 §5.8), the utility functions (§5.9) and reflection
//! (§10.2).

use super::Call;
use crate::eval::convert::describe;
use crate::eval::error::EvalError;
use crate::temporal::{Date, DateTime, Time};
use crate::value::{Collection, Item, TypeInfo};

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let model = call.model();
    match call.name {
        "children" => {
            call.arity(0, 0, "no")?;
            children(call, &call.input).map(Some)
        }
        "descendants" => {
            call.arity(0, 0, "no")?;
            let mut out = Vec::new();
            let mut level = children(call, &call.input)?;
            while !level.is_empty() {
                let next = children(call, &level)?;
                out.extend(level);
                level = next;
            }
            Ok(Some(out))
        }
        "trace" => {
            call.arity(1, 2, "1 or 2")?;
            let name = crate::eval::convert::string(model, &call.arg_or_empty(0)?, "trace")?
                .unwrap_or_default();
            let traced = if call.args.len() == 2 {
                let mut projected = Vec::new();
                for (index, item) in call.input.iter().enumerate() {
                    projected.extend(call.lambda(1, item, index, None)?);
                }
                projected
            } else {
                call.input.clone()
            };
            // NOTE: N1 §5.9.1 leaves the log's form open; no specification governs this: our
            // own design, the log carries the name and the item types, never a clinical value.
            tracing::trace!(
                target: "ferrobridge_fhirpath::trace",
                name = %name,
                count = traced.len(),
                types = %traced.iter().map(describe).collect::<Vec<_>>().join(", "),
                "trace()"
            );
            Ok(Some(call.input.clone()))
        }
        "now" | "today" | "timeOfDay" => {
            call.arity(0, 0, "no")?;
            let civil = call
                .engine
                .now()
                .to_zoned(jiff::tz::TimeZone::UTC)
                .datetime();
            let date = format!(
                "{:04}-{:02}-{:02}",
                civil.year(),
                civil.month(),
                civil.day()
            );
            let time = format!(
                "{:02}:{:02}:{:02}.{:03}",
                civil.hour(),
                civil.minute(),
                civil.second(),
                civil.millisecond()
            );
            // NOTE: N1 §5.9.2 leaves the clock to the implementation; no specification
            // governs this: our own design, the instant is read in UTC.
            let item = match call.name {
                "now" => DateTime::parse(&format!("{date}T{time}Z")).map(Item::DateTime),
                "today" => Date::parse(&date).map(Item::Date),
                _ => Time::parse(&time).map(Item::Time),
            };
            Ok(Some(item.into_iter().collect()))
        }
        "type" => {
            call.arity(0, 0, "no")?;
            Ok(Some(
                call.input
                    .iter()
                    .map(|item| {
                        Item::Type(match item {
                            Item::Node(node) => TypeInfo {
                                namespace: "FHIR",
                                name: node
                                    .fhir_type()
                                    .filter(|ty| !ty.is_backbone())
                                    .map(|ty| String::from(ty.name)),
                            },
                            Item::Type(_) => TypeInfo {
                                namespace: "System",
                                name: Some(String::from("TypeInfo")),
                            },
                            other => TypeInfo {
                                namespace: "System",
                                name: other.system_type().map(String::from),
                            },
                        })
                    })
                    .collect(),
            ))
        }
        _ => Ok(None),
    }
}

/// The immediate children of every node of `items` (N1 §5.8.1).
fn children<'a>(
    call: &Call<'_, '_, 'a>,
    items: &Collection<'a>,
) -> Result<Collection<'a>, EvalError> {
    let mut out = Vec::new();
    for item in items {
        if let Item::Node(node) = item {
            out.extend(
                node.named_children(call.model())?
                    .into_iter()
                    .map(|(_, child)| child),
            );
        }
    }
    Ok(out)
}
