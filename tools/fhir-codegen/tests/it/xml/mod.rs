// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XML codec: every root-set resource round-trips JSON to XML to JSON,
//! refusals name their element, and a few examples are pinned as golden XML
//! (<https://hl7.org/fhir/R4B/xml.html>).

use fhir_types::codec::Value;
use fhir_types::schema::Schemas;

mod namespace;
mod narrative;
mod refusals;
mod round_trip;

/// The member named `key`, which the document under test carries.
fn member<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).expect("the member exists")
}

/// The first item of an array the document under test carries.
fn first(value: &Value) -> &Value {
    value
        .as_array()
        .and_then(<[Value]>::first)
        .expect("a non-empty array")
}

/// The three versions the namespace rules are pinned in.
fn namespace_versions() -> [&'static Schemas; 3] {
    [
        &fhir_types::stu3::schema::SCHEMAS,
        &fhir_types::r4::schema::SCHEMAS,
        &fhir_types::r5::schema::SCHEMAS,
    ]
}
