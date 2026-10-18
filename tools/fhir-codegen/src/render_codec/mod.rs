// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Rendering the JSON codec impls beside each generated type.
//!
//! The runtime lives in the generated `codec` module (from the template in
//! `templates/codec.rs`); this module writes, per type, the `Json` or
//! `Primitive` impl and the serde bridges, following the FHIR JSON
//! representation (<https://hl7.org/fhir/R4B/json.html>).

mod builder;
mod choice;
mod json;
mod lexical;
mod primitive;
mod serialize;

use std::fmt::{self, Write};

use crate::lower::{Field, Scalar, Target, TypeDef, TypeKind, VersionModule};
use crate::render_codec::choice::render_choice;
use crate::render_codec::choice::render_resource_enum;
use crate::render_codec::json::render_struct;
use crate::render_codec::primitive::render_primitive;
use crate::render_codec::serialize::render_resource_serialize;
use crate::render_codec::serialize::render_serialize;

/// The codec module path from a type file (`<version>/<module>.rs`).
const C: &str = "super::super::codec";

/// What a field holds, from the codec's point of view.
enum Shape<'a> {
    /// A `FHIRPath` system scalar (`id`, `url`): a bare JSON value.
    Scalar(Scalar),
    /// A FHIR primitive: value plus `_name` sibling.
    Primitive,
    /// A complex type, backbone struct, or the `Resource` enum: a JSON object.
    Complex,
    /// A choice enum: one of several keys.
    Choice(&'a TypeDef),
}

fn shape<'a>(model: &'a VersionModule, field: &Field) -> Shape<'a> {
    match &field.ty.target {
        Target::Inline(scalar) => Shape::Scalar(*scalar),
        Target::Named(name) => match model.types.get(name) {
            Some(ty) if ty.is_primitive => Shape::Primitive,
            Some(ty) if matches!(ty.kind, TypeKind::Choice { .. }) => Shape::Choice(ty),
            _ => Shape::Complex,
        },
    }
}

/// The scalar's JSON constructor expression over `v` (a reference).
fn scalar_to_value(scalar: Scalar, v: &str) -> String {
    match scalar {
        Scalar::Bool => format!("{C}::Value::Bool(*{v})"),
        Scalar::I32 | Scalar::U32 => format!("{C}::Value::from(*{v})"),
        Scalar::I64 => format!("{C}::Value::String({v}.to_string())"),
        Scalar::Str => format!("{C}::Value::String({v}.clone())"),
    }
}

/// The scalar's JSON constructor expression over `v` (an owned place).
fn scalar_to_value_owned(scalar: Scalar, v: &str) -> String {
    match scalar {
        Scalar::Bool => format!("{C}::Value::Bool({v})"),
        Scalar::I32 | Scalar::U32 => format!("{C}::Value::from({v})"),
        Scalar::I64 => format!("{C}::Value::String({v}.to_string())"),
        Scalar::Str => format!("{C}::Value::String({v}.clone())"),
    }
}

/// The scalar's decoder expression over `value` (a `&Value`) and `path` (a `&Path`).
fn scalar_from_value(scalar: Scalar, primitive: Option<&str>) -> String {
    match (scalar, primitive) {
        (Scalar::Bool, _) => format!("{C}::expect_bool(value, path)"),
        (Scalar::I32, _) => format!("{C}::expect_i32(value, path)"),
        (Scalar::U32, Some("PositiveInt")) => format!("{C}::expect_u32(value, 1, path)"),
        (Scalar::U32, _) => format!("{C}::expect_u32(value, 0, path)"),
        (Scalar::I64, _) => format!("{C}::expect_i64_string(value, path)"),
        (Scalar::Str, Some("Decimal")) => format!("{C}::expect_decimal(value, path)"),
        (Scalar::Str, _) => format!("{C}::expect_string(value, path)"),
    }
}

/// Renders the codec impls for `ty`.
///
/// # Errors
///
/// Returns [`fmt::Error`] only if writing to the string fails, which `String` never does.
pub fn render_codec(model: &VersionModule, out: &mut String, ty: &TypeDef) -> fmt::Result {
    match &ty.kind {
        TypeKind::Struct { fields } if ty.is_primitive => render_primitive(model, out, ty, fields),
        TypeKind::Struct { fields } => {
            render_struct(model, out, ty, fields)?;
            render_serialize(model, out, ty, fields)?;
            render_deserialize(out, &ty.name)
        }
        TypeKind::Choice { variants, .. } => render_choice(model, out, ty, variants),
        TypeKind::ResourceEnum { resources } => {
            render_resource_enum(model, out, ty, resources)?;
            render_resource_serialize(model, out, ty, resources)?;
            render_deserialize(out, &ty.name)
        }
        TypeKind::UnknownResource => Ok(()),
    }
}

/// Renders `check_value`, which holds a JSON value to the primitive a type
/// code names, through that primitive's own decoder.
///
/// A consumer that writes a value into an element knows the element's type
/// code and nothing else, so the check is keyed by the code
/// (<https://hl7.org/fhir/R4/datatypes.html#primitive>) and decodes the value
/// as the primitive's JSON value (<https://hl7.org/fhir/R4/json.html#primitive>).
///
/// # Errors
///
/// Returns [`fmt::Error`] only if writing to the string fails, which `String` never does.
pub fn render_value_check(
    model: &VersionModule,
    out: &mut String,
    types: &[&TypeDef],
) -> fmt::Result {
    let version = model.name.to_uppercase();
    writeln!(
        out,
        "\n/// Checks `value` as the JSON value of the FHIR primitive `code`: its JSON type,"
    )?;
    writeln!(
        out,
        "/// its lexical form and, for an integer type, its range, exactly as the decoder does"
    )?;
    writeln!(
        out,
        "/// (<https://hl7.org/fhir/{version}/datatypes.html#primitive>)."
    )?;
    writeln!(out, "///")?;
    writeln!(
        out,
        "/// Answers `None` when this version defines no primitive named `code`."
    )?;
    writeln!(out, "///")?;
    writeln!(out, "/// # Errors")?;
    writeln!(out, "///")?;
    writeln!(
        out,
        "/// The inner result is the decoder's [`{C}::DecodeError`] at the path `code` for a value"
    )?;
    writeln!(out, "/// the primitive refuses.")?;
    writeln!(out, "#[must_use]")?;
    writeln!(
        out,
        "pub fn check_value(code: &str, value: &{C}::Value) -> Option<Result<(), {C}::DecodeError>> {{"
    )?;
    writeln!(out, "    let mut path = {C}::Path::root(code);")?;
    writeln!(out, "    let checked = match code {{")?;
    let mut primitives: Vec<&&TypeDef> = types
        .iter()
        .filter(|ty| ty.is_primitive && matches!(ty.kind, TypeKind::Struct { .. }))
        .collect();
    primitives.sort_by(|a, b| a.path.cmp(&b.path));
    for ty in primitives {
        out.push_str(&ty.scope.cfg());
        writeln!(
            out,
            "        {:?} => <{} as {C}::Primitive>::from_json_parts(Some(value), None, &mut path).map(|_| ()),",
            ty.path, ty.name
        )?;
    }
    writeln!(out, "        _ => return None,")?;
    writeln!(out, "    }};")?;
    writeln!(out, "    Some(checked)")?;
    writeln!(out, "}}")
}

fn render_deserialize(out: &mut String, name: &str) -> fmt::Result {
    writeln!(out, "\nimpl<'de> serde::Deserialize<'de> for {name} {{")?;
    writeln!(
        out,
        "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{"
    )?;
    writeln!(
        out,
        "        let value = <{C}::Value as serde::Deserialize>::deserialize(deserializer)?;"
    )?;
    writeln!(out, "        let mut path = {C}::Path::root({name:?});")?;
    writeln!(
        out,
        "        let object = {C}::expect_object(&value, &path).map_err(serde::de::Error::custom)?;"
    )?;
    writeln!(
        out,
        "        {C}::Json::from_json(object, &mut path).map_err(serde::de::Error::custom)"
    )?;
    writeln!(out, "    }}\n}}")
}
