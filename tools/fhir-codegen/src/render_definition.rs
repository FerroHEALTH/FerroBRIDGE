// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral read of `ElementDefinition` and `StructureDefinition`,
//! one impl each per version.
//!
//! The trait and its read structs are the crate-level `definition.rs`
//! (`templates/definition.rs`). This module writes each version's impl from
//! that version's lowered `ElementDefinition`: every accessor names a FHIR
//! element, and the emitter spells the read from the field the package
//! defines for it, so a single `type.profile` (STU3) and a list (R4) both read
//! as a list, a `valueSet[x]` choice (STU3) and a `valueSet` canonical (R4)
//! both read as the value set's URL, and an element the version lacks reads as
//! absent (<https://hl7.org/fhir/STU3/elementdefinition.html>,
//! <https://hl7.org/fhir/R4/elementdefinition.html>). An extension-carried
//! fact reads the value of the extension with its url, and an STU3 context
//! (a `contextType` beside a list of strings) reads in the shape of an R4
//! context entry. Nothing here names a version.

use std::fmt::{self, Write};

use crate::lower::{Cardinality, Field, Scalar, Target, TypeDef, TypeKind, VersionModule};

/// The type the element read is implemented for.
const ELEMENT_DEFINITION: &str = "ElementDefinition";

/// The type the structure read is implemented for.
const STRUCTURE_DEFINITION: &str = "StructureDefinition";

/// The extension naming a binding's maximum value set.
const MAX_VALUE_SET: &str = "http://hl7.org/fhir/StructureDefinition/elementdefinition-maxValueSet";

/// The extension marking a constraint as a best practice.
const BEST_PRACTICE: &str =
    "http://hl7.org/fhir/StructureDefinition/elementdefinition-bestpractice";

/// The lint expectation over the element impl: an extension's `value[x]` is read by
/// a `match` with one arm per type the version admits, and the arms of the
/// types that cannot carry the fact all read as absent.
const SAME_ARMS: &str = "#[expect(\n    clippy::match_same_arms,\n    reason = \"an extension value is read by one arm per type the version admits, and every arm of a type that cannot carry the fact reads as absent\"\n)]\n";

/// The extension stating a primitive type's lexical form.
const REGEX: &str = "http://hl7.org/fhir/StructureDefinition/regex";

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
    /// A `bool` copied out, where a choice also offers other scalars.
    Bool,
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
        "{SAME_ARMS}impl {D}::ElementDefinitionRead for super::element_definition::ElementDefinition {{"
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
        "is_modifier",
        "Option<bool>",
        &reader.copy_opt(own, "isModifier", "self"),
    )?;
    method(
        &mut out,
        "content_reference",
        "Option<&str>",
        &reader.text_opt(own, "contentReference", "self"),
    )?;
    render_structured(&mut out, &reader)?;
    writeln!(out, "}}")?;
    if let Some(structure) = model.types.get(STRUCTURE_DEFINITION) {
        writeln!(out)?;
        out.push_str(&structure.scope.cfg());
        render_structure(&mut out, &reader)?;
    }
    Ok(out)
}

/// Writes the `StructureDefinitionRead` impl.
fn render_structure(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    let own = STRUCTURE_DEFINITION;
    writeln!(
        out,
        "impl {D}::StructureDefinitionRead for super::structure_definition::StructureDefinition {{\n    type Element = super::element_definition::ElementDefinition;\n"
    )?;
    for (name, element) in [
        ("url", "url"),
        ("version", "version"),
        ("type_name", "type"),
        ("base_definition", "baseDefinition"),
        ("kind", "kind"),
        ("derivation", "derivation"),
    ] {
        method(
            out,
            name,
            "Option<&str>",
            &reader.text_opt(own, element, "self"),
        )?;
    }
    method(
        out,
        "is_abstract",
        "Option<bool>",
        &reader.copy_opt(own, "abstract", "self"),
    )?;
    let snapshot = match (
        reader.field(own, "snapshot"),
        reader.field("StructureDefinition.snapshot", "element"),
    ) {
        (Some(snapshot), Some(element)) => format!(
            "self.{}.as_ref().map(|item| item.{}.as_slice())",
            snapshot.name, element.name
        ),
        _ => String::from("None"),
    };
    method(out, "snapshot", "Option<&[Self::Element]>", &snapshot)?;
    method(
        out,
        "contexts",
        &format!("Vec<{D}::ContextRead<'_>>"),
        &reader.contexts(),
    )?;
    method(
        out,
        "context_invariants",
        "Vec<&str>",
        &reader.text_list(own, "contextInvariant", "self"),
    )?;
    writeln!(out, "}}")
}

/// Writes the methods that read a backbone element of `ElementDefinition`
/// (the types, the binding, the constraints and the slicing) and the two
/// choice values.
fn render_structured(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    let own = ELEMENT_DEFINITION;
    let ty = "ElementDefinition.type";
    let type_read = format!(
        "{D}::TypeRead {{ code: {}, profiles: {}, target_profiles: {}, aggregation: {}, versioning: {}, regex: {} }}",
        reader.text_opt(ty, "code", "item"),
        reader.text_list(ty, "profile", "item"),
        reader.text_list(ty, "targetProfile", "item"),
        reader.text_list(ty, "aggregation", "item"),
        reader.text_opt(ty, "versioning", "item"),
        reader.extension(ty, "item", REGEX, Read::Text),
    );
    method(
        out,
        "types",
        &format!("Vec<{D}::TypeRead<'_>>"),
        &reader.struct_list(own, "type", "self", &type_read),
    )?;
    let binding = "ElementDefinition.binding";
    let binding_read = format!(
        "{D}::BindingRead {{ strength: {}, value_set: {}, max_value_set: {} }}",
        reader.text_opt(binding, "strength", "item"),
        reader.text_opt(binding, "valueSet", "item"),
        reader.extension(binding, "item", MAX_VALUE_SET, Read::Text),
    );
    method(
        out,
        "binding",
        &format!("Option<{D}::BindingRead<'_>>"),
        &reader.struct_opt(own, "binding", "self", &binding_read),
    )?;
    let constraint = "ElementDefinition.constraint";
    let constraint_read = format!(
        "{D}::ConstraintRead {{ key: {}, severity: {}, human: {}, expression: {}, xpath: {}, source: {}, best_practice: {} }}",
        reader.text_opt(constraint, "key", "item"),
        reader.text_opt(constraint, "severity", "item"),
        reader.text_opt(constraint, "human", "item"),
        reader.text_opt(constraint, "expression", "item"),
        reader.text_opt(constraint, "xpath", "item"),
        reader.text_opt(constraint, "source", "item"),
        reader.extension(constraint, "item", BEST_PRACTICE, Read::Bool),
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

    /// The read of the value of the extension whose url is `url`, among the
    /// extensions of the struct at `path` on `receiver`; absent where the
    /// struct carries none with that url.
    fn extension(&self, path: &str, receiver: &str, url: &str, read: Read) -> String {
        let Some(field) = self.field(path, "extension") else {
            return String::from("None");
        };
        let url_read = self.text_opt("Extension", "url", "extension");
        let value = self.optional("Extension", "value", "extension", read);
        format!(
            "{receiver}.{}.iter().filter(|extension| {url_read} == Some({url:?})).find_map(|extension| {value})",
            field.name
        )
    }

    /// The read of `StructureDefinition.context`: each R4 context entry's
    /// `type` and `expression`, or each STU3 `context` string with the one
    /// `contextType` beside it.
    fn contexts(&self) -> String {
        let own = STRUCTURE_DEFINITION;
        let entry = "StructureDefinition.context";
        if self.type_at(entry).is_some() {
            let read = format!(
                "{D}::ContextRead {{ kind: {}, expression: {} }}",
                self.text_opt(entry, "type", "item"),
                self.text_opt(entry, "expression", "item"),
            );
            return self.struct_list(own, "context", "self", &read);
        }
        let Some(field) = self.field(own, "context") else {
            return String::from("Vec::new()");
        };
        format!(
            "self.{}.iter().map(|item| {D}::ContextRead {{ kind: {}, expression: {} }}).collect()",
            field.name,
            self.text_opt(own, "contextType", "self"),
            self.value(&field.ty.target, "item", Read::Text),
        )
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
                            let read = self.value(&variant.target, "held", read);
                            let held = if read == "None" { "_" } else { "held" };
                            format!(
                                "super::{module}::{name}::{}({held}) => {read}",
                                variant.name
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
        (Scalar::Bool | Scalar::I32 | Scalar::U32, Read::Copy, Cardinality::One)
        | (Scalar::Bool, Read::Bool, Cardinality::One) => format!("Some({place})"),
        (Scalar::Bool | Scalar::I32 | Scalar::U32, Read::Copy, _)
        | (Scalar::Bool, Read::Bool, _) => place.to_owned(),
        _ => String::from("None"),
    }
}
