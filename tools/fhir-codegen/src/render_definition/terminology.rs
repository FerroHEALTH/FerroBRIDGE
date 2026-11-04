// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral `ValueSet` and `CodeSystem` reads: the composition,
//! the expansion and the concepts a validator decides a binding from.

use std::fmt::{self, Write as _};

use super::{C, D, Read, Reader, method};
/// Writes the `ValueSetRead`, `ContainsRead`, `CodeSystemRead` and
/// `ConceptRead` impls, each where the version defines its type.
pub(super) fn render_terminology(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    render_value_set(out, reader)?;
    render_code_system(out, reader)
}

/// Writes the `ValueSetRead` and `ContainsRead` impls.
fn render_value_set(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    let filter = "ValueSet.compose.include.filter";
    let filter_read = format!(
        "{D}::FilterRead {{ property: {}, op: {}, value: {} }}",
        reader.optional(filter, "property", "item", Read::Text),
        reader.optional(filter, "op", "item", Read::Text),
        reader.optional(filter, "value", "item", Read::Text),
    );
    let include = "ValueSet.compose.include";
    let concept_codes = match reader.field(include, "concept") {
        Some(field) => format!(
            "item.{}.iter().filter_map(|item| {}).collect()",
            field.name,
            reader.optional(
                "ValueSet.compose.include.concept",
                "code",
                "item",
                Read::Text
            )
        ),
        None => String::from("Vec::new()"),
    };
    let include_read = format!(
        "{D}::IncludeRead {{ system: {}, version: {}, concepts: {concept_codes}, filters: {}, value_sets: {} }}",
        reader.optional(include, "system", "item", Read::Text),
        reader.optional(include, "version", "item", Read::Text),
        reader.struct_list(include, "filter", "item", &filter_read),
        reader.text_list(include, "valueSet", "item"),
    );
    let compose = "ValueSet.compose";
    let compose_read = format!(
        "{D}::ComposeRead {{ inactive: {}, includes: {}, excludes: {} }}",
        reader.optional(compose, "inactive", "item", Read::Copy),
        reader.struct_list(compose, "include", "item", &include_read),
        reader.struct_list(compose, "exclude", "item", &include_read),
    );
    let expansion = "ValueSet.expansion";
    let expansion_read = format!(
        "{D}::ExpansionRead {{ total: {}, offset: {}, contains: {} }}",
        reader.optional(expansion, "total", "item", Read::Copy),
        reader.optional(expansion, "offset", "item", Read::Copy),
        reader.slice(expansion, "contains", "item"),
    );
    let contains = "ValueSet.expansion.contains";
    if let (Some(own), Some(entry)) = (reader.type_at("ValueSet"), reader.type_at(contains)) {
        writeln!(out)?;
        out.push_str(&own.scope.cfg());
        writeln!(
            out,
            "impl {D}::ValueSetRead for super::{}::{} {{\n    type Contains = super::{}::{};\n",
            own.module, own.name, entry.module, entry.name
        )?;
        method(
            out,
            "compose",
            &format!("Option<{D}::ComposeRead<'_>>"),
            &reader.struct_opt("ValueSet", "compose", "self", &compose_read),
        )?;
        method(
            out,
            "expansion",
            &format!("Option<{D}::ExpansionRead<'_, Self::Contains>>"),
            &reader.struct_opt("ValueSet", "expansion", "self", &expansion_read),
        )?;
        writeln!(out, "}}\n")?;
        out.push_str(&entry.scope.cfg());
        writeln!(
            out,
            "impl {D}::ContainsRead for super::{}::{} {{",
            entry.module, entry.name
        )?;
        for (name, element) in [
            ("system", "system"),
            ("version", "version"),
            ("code", "code"),
        ] {
            method(
                out,
                name,
                "Option<&str>",
                &reader.optional(contains, element, "self", Read::Text),
            )?;
        }
        method(
            out,
            "is_abstract",
            "Option<bool>",
            &reader.optional(contains, "abstract", "self", Read::Copy),
        )?;
        method(
            out,
            "contains",
            "&[Self]",
            &reader.slice(contains, "contains", "self"),
        )?;
        writeln!(out, "}}")?;
    }
    Ok(())
}

/// Writes the `CodeSystemRead` and `ConceptRead` impls.
fn render_code_system(out: &mut String, reader: &Reader<'_>) -> fmt::Result {
    let concept = "CodeSystem.concept";
    if let (Some(own), Some(entry)) = (reader.type_at("CodeSystem"), reader.type_at(concept)) {
        writeln!(out)?;
        out.push_str(&own.scope.cfg());
        writeln!(
            out,
            "impl {D}::CodeSystemRead for super::{}::{} {{\n    type Concept = super::{}::{};\n",
            own.module, own.name, entry.module, entry.name
        )?;
        for (name, element) in [
            ("content", "content"),
            ("hierarchy_meaning", "hierarchyMeaning"),
        ] {
            method(
                out,
                name,
                "Option<&str>",
                &reader.optional("CodeSystem", element, "self", Read::Text),
            )?;
        }
        method(
            out,
            "case_sensitive",
            "Option<bool>",
            &reader.optional("CodeSystem", "caseSensitive", "self", Read::Copy),
        )?;
        let property = "CodeSystem.property";
        let property_read = format!(
            "{D}::PropertyRead {{ code: {}, uri: {} }}",
            reader.optional(property, "code", "item", Read::Text),
            reader.optional(property, "uri", "item", Read::Text),
        );
        method(
            out,
            "properties",
            &format!("Vec<{D}::PropertyRead<'_>>"),
            &reader.struct_list("CodeSystem", "property", "self", &property_read),
        )?;
        method(
            out,
            "concepts",
            "&[Self::Concept]",
            &reader.slice("CodeSystem", "concept", "self"),
        )?;
        writeln!(out, "}}\n")?;
        out.push_str(&entry.scope.cfg());
        writeln!(
            out,
            "impl {D}::ConceptRead for super::{}::{} {{",
            entry.module, entry.name
        )?;
        for (name, element) in [("code", "code"), ("display", "display")] {
            method(
                out,
                name,
                "Option<&str>",
                &reader.optional(concept, element, "self", Read::Text),
            )?;
        }
        let designations = match reader.field(concept, "designation") {
            Some(field) => format!(
                "self.{}.iter().filter_map(|item| {}).collect()",
                field.name,
                reader.optional(
                    "CodeSystem.concept.designation",
                    "value",
                    "item",
                    Read::Text
                )
            ),
            None => String::from("Vec::new()"),
        };
        method(out, "designations", "Vec<&str>", &designations)?;
        let value = "CodeSystem.concept.property";
        let properties = match reader.field(concept, "property") {
            Some(field) => format!(
                "self.{}.iter().map(|item| Ok({D}::ConceptPropertyRead {{ code: {}, value: {}? }})).collect()",
                field.name,
                reader.optional(value, "code", "item", Read::Text),
                reader.choice_value(value, "value", "item"),
            ),
            None => String::from("Ok(Vec::new())"),
        };
        method(
            out,
            "properties",
            &format!("Result<Vec<{D}::ConceptPropertyRead<'_>>, {C}::EncodeError>"),
            &properties,
        )?;
        method(
            out,
            "concepts",
            "&[Self]",
            &reader.slice(concept, "concept", "self"),
        )?;
        writeln!(out, "}}")?;
    }
    Ok(())
}
