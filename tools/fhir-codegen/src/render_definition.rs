// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral read of `ElementDefinition`, one impl per version.
//!
//! The trait and its read structs are the crate-level `definition.rs`
//! (`templates/definition.rs`). This module writes each version's impl from
//! that version's lowered `ElementDefinition`: every accessor names a FHIR
//! element, and the emitter spells the read from the field the package
//! defines for it, so a single `type.profile` (STU3) and a list (R4) both read
//! as a list, a `valueSet[x]` choice (STU3) and a `valueSet` canonical (R4)
//! both read as the value set's URL, and an element the version lacks reads as
//! absent (<https://hl7.org/fhir/STU3/elementdefinition.html>,
//! <https://hl7.org/fhir/R4/elementdefinition.html>). Nothing here names a
//! version.

use std::fmt::{self, Write};

use crate::lower::{Cardinality, Field, Scalar, Target, TypeDef, TypeKind, VersionModule};

/// The type the read is implemented for.
const ELEMENT_DEFINITION: &str = "ElementDefinition";

/// The codec module from a version file.
const C: &str = "super::super::codec";

/// The definition module from a version file.
const D: &str = "super::super::definition";

/// What an accessor reads out of a primitive's `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Read {
    /// Text, as `&str`.
    Text,
    /// A scalar copied out (`u32`, `i32`, `bool`).
    Copy,
}

/// Whether `model` defines `ElementDefinition`, so its version gets the
/// module this file renders.
#[must_use]
pub fn defines_element_definition(model: &VersionModule) -> bool {
    model.types.contains_key(ELEMENT_DEFINITION)
}

/// The `cfg` of the version's `definition` module: the scope of its
/// `ElementDefinition`.
#[must_use]
pub fn definition_cfg(model: &VersionModule) -> String {
    model
        .types
        .get(ELEMENT_DEFINITION)
        .map(|ty| ty.scope.cfg())
        .unwrap_or_default()
}

/// Renders the `definition.rs` module of `model`.
///
/// # Errors
///
/// Returns [`fmt::Error`] only if writing to the string fails, which `String` never does.
pub fn render_definition(model: &VersionModule) -> Result<String, fmt::Error> {
    let mut out = crate::render::banner(model);
    writeln!(
        out,
        "//! The release-neutral read of this version's `ElementDefinition`, emitted from\n//! its package (<https://hl7.org/fhir/{}/elementdefinition.html>).\n",
        model.name.to_uppercase()
    )?;
    let reader = Reader { model };
    let own = ELEMENT_DEFINITION;
    writeln!(
        out,
        "impl {D}::ElementDefinitionRead for super::element_definition::ElementDefinition {{"
    )?;
    method(
        &mut out,
        "element_id",
        "Option<&str>",
        &reader.text_opt(own, "id", "self"),
    )?;
    method(
        &mut out,
        "path",
        "Option<&str>",
        &reader.text_opt(own, "path", "self"),
    )?;
    method(
        &mut out,
        "slice_name",
        "Option<&str>",
        &reader.text_opt(own, "sliceName", "self"),
    )?;
    method(
        &mut out,
        "min",
        "Option<u32>",
        &reader.copy_opt(own, "min", "self"),
    )?;
    method(
        &mut out,
        "max",
        "Option<&str>",
        &reader.text_opt(own, "max", "self"),
    )?;
    method(
        &mut out,
        "max_length",
        "Option<i32>",
        &reader.copy_opt(own, "maxLength", "self"),
    )?;
    method(
        &mut out,
        "content_reference",
        "Option<&str>",
        &reader.text_opt(own, "contentReference", "self"),
    )?;
    render_structured(&mut out, &reader)?;
    writeln!(out, "}}")?;
    Ok(out)
}

/// Writes the methods that read a backbone element of `ElementDefinition`
/// (the types, the binding, the constraints and the slicing) and the two
/// choice values.
fn render_structured(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    let own = ELEMENT_DEFINITION;
    let ty = "ElementDefinition.type";
    let type_read = format!(
        "{D}::TypeRead {{ code: {}, profiles: {}, target_profiles: {}, aggregation: {}, versioning: {} }}",
        reader.text_opt(ty, "code", "item"),
        reader.text_list(ty, "profile", "item"),
        reader.text_list(ty, "targetProfile", "item"),
        reader.text_list(ty, "aggregation", "item"),
        reader.text_opt(ty, "versioning", "item"),
    );
    method(
        out,
        "types",
        &format!("Vec<{D}::TypeRead<'_>>"),
        &reader.struct_list(own, "type", "self", &type_read),
    )?;
    let binding = "ElementDefinition.binding";
    let binding_read = format!(
        "{D}::BindingRead {{ strength: {}, value_set: {} }}",
        reader.text_opt(binding, "strength", "item"),
        reader.text_opt(binding, "valueSet", "item"),
    );
    method(
        out,
        "binding",
        &format!("Option<{D}::BindingRead<'_>>"),
        &reader.struct_opt(own, "binding", "self", &binding_read),
    )?;
    let constraint = "ElementDefinition.constraint";
    let constraint_read = format!(
        "{D}::ConstraintRead {{ key: {}, severity: {}, human: {}, expression: {}, xpath: {}, source: {} }}",
        reader.text_opt(constraint, "key", "item"),
        reader.text_opt(constraint, "severity", "item"),
        reader.text_opt(constraint, "human", "item"),
        reader.text_opt(constraint, "expression", "item"),
        reader.text_opt(constraint, "xpath", "item"),
        reader.text_opt(constraint, "source", "item"),
    );
    method(
        out,
        "constraints",
        &format!("Vec<{D}::ConstraintRead<'_>>"),
        &reader.struct_list(own, "constraint", "self", &constraint_read),
    )?;
    let discriminator = "ElementDefinition.slicing.discriminator";
    let discriminator_read = format!(
        "{D}::DiscriminatorRead {{ kind: {}, path: {} }}",
        reader.text_opt(discriminator, "type", "item"),
        reader.text_opt(discriminator, "path", "item"),
    );
    let slicing = "ElementDefinition.slicing";
    let slicing_read = format!(
        "{D}::SlicingRead {{ discriminators: {}, description: {}, ordered: {}, rules: {} }}",
        reader.struct_list(slicing, "discriminator", "item", &discriminator_read),
        reader.text_opt(slicing, "description", "item"),
        reader.copy_opt(slicing, "ordered", "item"),
        reader.text_opt(slicing, "rules", "item"),
    );
    method(
        out,
        "slicing",
        &format!("Option<{D}::SlicingRead<'_>>"),
        &reader.struct_opt(own, "slicing", "self", &slicing_read),
    )?;
    for name in ["fixed", "pattern"] {
        method(
            out,
            name,
            &format!("Result<Option<{D}::ChoiceValue>, {C}::EncodeError>"),
            &reader.choice_value(own, name),
        )?;
    }
    Ok(())
}

/// Writes one trait method returning `ty` as `body`.
fn method(out: &mut String, name: &str, ty: &str, body: &str) -> fmt::Result {
    writeln!(
        out,
        "    fn {name}(&self) -> {ty} {{\n        {body}\n    }}\n"
    )
}

/// Spells reads of the lowered model.
struct Reader<'m> {
    model: &'m VersionModule,
}

impl Reader<'_> {
    /// The struct type lowered from the element at `path`.
    fn type_at(&self, path: &str) -> Option<&TypeDef> {
        self.model
            .types
            .values()
            .find(|ty| ty.path == path && matches!(ty.kind, TypeKind::Struct { .. }))
    }

    /// The field of the struct at `path` for the FHIR element `name`, a
    /// choice found by its stem.
    fn field(&self, path: &str, name: &str) -> Option<&Field> {
        let TypeKind::Struct { fields } = &self.type_at(path)?.kind else {
            return None;
        };
        fields
            .iter()
            .find(|field| field.fhir_name == name || field.fhir_name == format!("{name}[x]"))
    }

    /// An `Option<&str>` read of the element `name` of the struct at `path`
    /// on `receiver`.
    fn text_opt(&self, path: &str, name: &str, receiver: &str) -> String {
        self.optional(path, name, receiver, Read::Text)
    }

    /// An `Option<scalar>` read of the element `name` of the struct at
    /// `path` on `receiver`.
    fn copy_opt(&self, path: &str, name: &str, receiver: &str) -> String {
        self.optional(path, name, receiver, Read::Copy)
    }

    /// The `Option` read of one element, absent when the version lacks it.
    ///
    /// A repeating element reads its first value; no element this read names
    /// repeats where it reads one value.
    fn optional(&self, path: &str, name: &str, receiver: &str, read: Read) -> String {
        let Some(field) = self.field(path, name) else {
            return String::from("None");
        };
        let place = format!("{receiver}.{}", field.name);
        if let Target::Inline(scalar) = field.ty.target
            && field.ty.card != Cardinality::Many
        {
            return inline(scalar, &place, read, field.ty.card);
        }
        match field.ty.card {
            Cardinality::One => self.value(&field.ty.target, &place, read),
            Cardinality::Optional => format!(
                "{place}.as_ref().and_then(|value| {})",
                self.value(&field.ty.target, "value", read)
            ),
            Cardinality::Many => format!(
                "{place}.first().and_then(|value| {})",
                self.value(&field.ty.target, "value", read)
            ),
        }
    }

    /// A `Vec<&str>` read of the element `name` of the struct at `path` on
    /// `receiver`: every value of a list, the one value of a single element.
    fn text_list(&self, path: &str, name: &str, receiver: &str) -> String {
        let Some(field) = self.field(path, name) else {
            return String::from("Vec::new()");
        };
        let place = format!("{receiver}.{}", field.name);
        let value = self.value(&field.ty.target, "value", Read::Text);
        match field.ty.card {
            Cardinality::One => format!(
                "{}.into_iter().collect()",
                self.value(&field.ty.target, &place, Read::Text)
            ),
            Cardinality::Optional | Cardinality::Many => {
                format!("{place}.iter().filter_map(|value| {value}).collect()")
            }
        }
    }

    /// A `Vec` of `read` over the struct-valued element `name` at `path`,
    /// `read` spelled over `item`.
    fn struct_list(&self, path: &str, name: &str, receiver: &str, read: &str) -> String {
        match self.field(path, name) {
            Some(field) => format!(
                "{receiver}.{}.iter().map(|item| {read}).collect()",
                field.name
            ),
            None => String::from("Vec::new()"),
        }
    }

    /// An `Option` of `read` over the struct-valued element `name` at
    /// `path`, `read` spelled over `item`.
    fn struct_opt(&self, path: &str, name: &str, receiver: &str, read: &str) -> String {
        match self.field(path, name) {
            Some(field) => format!("{receiver}.{}.as_ref().map(|item| {read})", field.name),
            None => String::from("None"),
        }
    }

    /// The JSON form of the choice element `name` of `ElementDefinition`.
    fn choice_value(&self, path: &str, name: &str) -> String {
        match self.field(path, name) {
            Some(field) => format!(
                "self.{}.as_ref().map(|value| value.to_json_parts().map(|(suffix, value, element)| {D}::ChoiceValue {{ suffix, value, element }})).transpose()",
                field.name
            ),
            None => String::from("Ok(None)"),
        }
    }

    /// An `Option` read of one value of the type `target`, held by
    /// reference in `value`.
    ///
    /// A primitive reads its `value`, a choice reads whichever alternative it
    /// holds, and a `Reference` reads its `reference`, the URL of what it
    /// points at (<https://hl7.org/fhir/STU3/references.html>), which is how
    /// STU3's `valueSetReference` names its value set.
    fn value(&self, target: &Target, value: &str, read: Read) -> String {
        match target {
            Target::Inline(scalar) => inline(*scalar, value, read, Cardinality::One),
            Target::Named(name) => match self.model.types.get(name) {
                Some(ty) if ty.is_primitive => Self::primitive_value(ty, value, read),
                Some(TypeDef {
                    kind: TypeKind::Choice { variants, .. },
                    module,
                    ..
                }) => {
                    let arms: Vec<String> = variants
                        .iter()
                        .map(|variant| {
                            format!(
                                "super::{module}::{name}::{}(held) => {}",
                                variant.name,
                                self.value(&variant.target, "held", read)
                            )
                        })
                        .collect();
                    format!("match &{value} {{ {}, }}", arms.join(", "))
                }
                Some(ty) if ty.path == "Reference" && read == Read::Text => {
                    self.optional("Reference", "reference", value, Read::Text)
                }
                _ => String::from("None"),
            },
        }
    }

    /// The read of the primitive `ty`'s `value`, held by reference in `value`.
    fn primitive_value(ty: &TypeDef, value: &str, read: Read) -> String {
        let TypeKind::Struct { fields } = &ty.kind else {
            return String::from("None");
        };
        let Some(field) = fields.iter().find(|field| field.name == "value") else {
            return String::from("None");
        };
        let Target::Inline(scalar) = field.ty.target else {
            return String::from("None");
        };
        inline(scalar, &format!("{value}.value"), read, field.ty.card)
    }
}

/// The read of a scalar of `card` at `place`; a text read of a number and a
/// copy read of text are absent.
fn inline(scalar: Scalar, place: &str, read: Read, card: Cardinality) -> String {
    match (scalar, read, card) {
        (Scalar::Str, Read::Text, Cardinality::One) => format!("Some({place}.as_str())"),
        (Scalar::Str, Read::Text, _) => format!("{place}.as_deref()"),
        (Scalar::Bool | Scalar::I32 | Scalar::U32, Read::Copy, Cardinality::One) => {
            format!("Some({place})")
        }
        (Scalar::Bool | Scalar::I32 | Scalar::U32, Read::Copy, _) => place.to_owned(),
        _ => String::from("None"),
    }
}
