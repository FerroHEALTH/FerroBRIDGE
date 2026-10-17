// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The field builders a struct carries.

use std::fmt::{self, Write};

use crate::lower::{Cardinality, Field, Scalar, TypeDef, VersionModule};
use crate::naming::type_name;
use crate::render_codec::C;
use crate::render_codec::Shape;
use crate::render_codec::lexical::lexical_form;
use crate::render_codec::lexical::lexical_form_path;
use crate::render_codec::scalar_from_value;
use crate::render_codec::shape;

pub(super) fn render_field_builders(
    model: &VersionModule,
    out: &mut String,
    fields: &[Field],
) -> fmt::Result {
    // Build each field from its raw slot into a `field_` local, so a FHIR element
    // named `path` or `object` cannot shadow the decoder's own bindings.
    for field in fields {
        match shape(model, field) {
            Shape::Scalar(scalar) => render_scalar_builder(model, out, field, scalar)?,
            Shape::Primitive => render_primitive_builder(out, field)?,
            Shape::Complex => render_complex_builder(out, field)?,
            Shape::Choice(choice) => render_choice_builder(out, field, choice)?,
        }
    }
    Ok(())
}

/// The lexical-form check of the primitive an inline scalar field spells out.
///
/// An element typed with a `FHIRPath` system type holds the primitive's value
/// directly (<https://hl7.org/fhir/R4B/json.html#primitive>), so it keeps the
/// lexical form of the FHIR type the element's own type extension names.
pub(super) fn inline_lexical_form(
    model: &VersionModule,
    field: &Field,
    scalar: Scalar,
) -> Option<String> {
    let code = field.types.first()?;
    let ty = model.types.get(&type_name(code))?;
    if !ty.is_primitive {
        return None;
    }
    lexical_form(ty, scalar)?;
    Some(lexical_form_path(&ty.name))
}

/// The decoder lines for a `FHIRPath` system scalar field.
fn render_scalar_builder(
    model: &VersionModule,
    out: &mut String,
    field: &Field,
    scalar: Scalar,
) -> fmt::Result {
    let slot = field.name.trim_start_matches("r#");
    let key = &field.fhir_name;
    let name = format!("field_{slot}");
    let decode = match inline_lexical_form(model, field, scalar) {
        Some(check) => format!("{check}({}?, path)", scalar_from_value(scalar, None)),
        None => scalar_from_value(scalar, None),
    };
    match field.ty.card {
        Cardinality::Optional => writeln!(
            out,
            "        let {name} = raw_{slot}.map(|value| path.with({key:?}, |path| {{ let value = {C}::expect_single(value, path)?; {decode} }})).transpose()?;"
        ),
        Cardinality::One => writeln!(
            out,
            "        let {name} = path.with({key:?}, |path| {{ let value = raw_{slot}.ok_or_else(|| path.error({C}::DecodeErrorKind::MissingProperty))?; let value = {C}::expect_single(value, path)?; {decode} }})?;"
        ),
        Cardinality::Many => {
            writeln!(out, "        let mut {name} = Vec::new();")?;
            writeln!(out, "        if let Some(raw) = raw_{slot} {{")?;
            writeln!(
                out,
                "            for (index, value) in {C}::expect_array(raw, path)?.iter().enumerate() {{"
            )?;
            writeln!(
                out,
                "                {name}.push(path.with_index({key:?}, index, |path| {decode})?);"
            )?;
            writeln!(out, "            }}")?;
            writeln!(out, "        }}")?;
            render_minimum(out, field, &name)
        }
    }
}

/// The decoder lines holding a repeating field's items, read into `name`, to
/// the element's minimum cardinality.
///
/// The minimum is `ElementDefinition.min`
/// (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.min>),
/// and an array is never empty (<https://hl7.org/fhir/R4/json.html#2.6.2>), so a
/// required repeating element is either absent, refused as a missing property
/// at its own path, or present with fewer items than the minimum, refused as a
/// cardinality mismatch there. A lenient path reads either as it stands, as it
/// reads a required primitive stating no value.
fn render_minimum(out: &mut String, field: &Field, name: &str) -> fmt::Result {
    let key = &field.fhir_name;
    let min = field.min;
    if min == 0 {
        return Ok(());
    }
    writeln!(
        out,
        "        if {name}.is_empty() && !path.is_lenient() {{ return path.with({key:?}, |path| Err(path.error({C}::DecodeErrorKind::MissingProperty))); }}"
    )?;
    if min > 1 {
        writeln!(
            out,
            "        if {name}.len() < {min} && !path.is_lenient() {{ return path.with({key:?}, |path| Err(path.error({C}::DecodeErrorKind::WrongCardinality))); }}"
        )?;
    }
    Ok(())
}

/// The decoder lines for a FHIR primitive field, value plus `_name` sibling.
fn render_primitive_builder(out: &mut String, field: &Field) -> fmt::Result {
    let slot = field.name.trim_start_matches("r#");
    let key = &field.fhir_name;
    let name = format!("field_{slot}");
    match field.ty.card {
        Cardinality::Optional => {
            writeln!(
                out,
                "        let {name} = match (raw_{slot}, raw_{slot}_element) {{"
            )?;
            writeln!(out, "            (None, None) => None,")?;
            writeln!(
                out,
                "            (value, element) => Some(path.with({key:?}, |path| {C}::Primitive::from_json_parts(value, element, path))?),"
            )?;
            writeln!(out, "        }};")
        }
        Cardinality::One => writeln!(
            out,
            "        let {name} = path.with({key:?}, |path| {C}::Primitive::from_json_parts(raw_{slot}, raw_{slot}_element, path))?;"
        ),
        Cardinality::Many => {
            writeln!(out, "        let mut {name} = Vec::new();")?;
            writeln!(
                out,
                "        for (index, (value, element)) in {C}::pair_arrays(raw_{slot}, raw_{slot}_element, path)?.into_iter().enumerate() {{"
            )?;
            writeln!(
                out,
                "            {name}.push(path.with_index({key:?}, index, |path| {C}::Primitive::from_json_parts(value, element, path))?);"
            )?;
            writeln!(out, "        }}")?;
            render_minimum(out, field, &name)
        }
    }
}

/// The decoder lines for a complex, backbone, or resource field.
fn render_complex_builder(out: &mut String, field: &Field) -> fmt::Result {
    let slot = field.name.trim_start_matches("r#");
    let key = &field.fhir_name;
    let name = format!("field_{slot}");
    let wrap_map = if field.ty.boxed { ".map(Box::new)" } else { "" };
    let decode = format!(
        "{C}::Json::from_json({C}::expect_object({C}::expect_single(value, path)?, path)?, path){wrap_map}"
    );
    match field.ty.card {
        Cardinality::Optional => writeln!(
            out,
            "        let {name} = raw_{slot}.map(|value| path.with({key:?}, |path| {decode})).transpose()?;"
        ),
        Cardinality::One => writeln!(
            out,
            "        let {name} = path.with({key:?}, |path| {{ let value = raw_{slot}.ok_or_else(|| path.error({C}::DecodeErrorKind::MissingProperty))?; {decode} }})?;"
        ),
        Cardinality::Many => {
            writeln!(out, "        let mut {name} = Vec::new();")?;
            writeln!(out, "        if let Some(raw) = raw_{slot} {{")?;
            writeln!(
                out,
                "            for (index, value) in {C}::expect_array(raw, path)?.iter().enumerate() {{"
            )?;
            writeln!(
                out,
                "                {name}.push(path.with_index({key:?}, index, |path| {C}::Json::from_json({C}::expect_object(value, path)?, path){wrap_map})?);"
            )?;
            writeln!(out, "            }}")?;
            writeln!(out, "        }}")?;
            render_minimum(out, field, &name)
        }
    }
}

/// The decoder lines for a choice field, keyed by the suffix its raw slot found.
fn render_choice_builder(out: &mut String, field: &Field, choice: &TypeDef) -> fmt::Result {
    let slot = field.name.trim_start_matches("r#");
    let name = format!("field_{slot}");
    let stem = field.fhir_name.trim_end_matches("[x]");
    let decode = format!(
        "{}::from_json_parts(suffix, raw_{slot}.value, raw_{slot}.element, path)",
        choice.name
    );
    match field.ty.card {
        Cardinality::Optional => {
            writeln!(out, "        let {name} = match raw_{slot}.suffix {{")?;
            writeln!(out, "            None => None,")?;
            writeln!(
                out,
                "            Some(suffix) => Some(path.with({stem:?}, |path| {decode})?),"
            )?;
            writeln!(out, "        }};")
        }
        _ => writeln!(
            out,
            "        let {name} = path.with({stem:?}, |path| {{ let suffix = raw_{slot}.suffix.ok_or_else(|| path.error({C}::DecodeErrorKind::MissingProperty))?; {decode} }})?;"
        ),
    }
}
