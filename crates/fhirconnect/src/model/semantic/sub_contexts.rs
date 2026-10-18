// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rules about how contexts enter one another
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`):
//! a `slotContext` names a loaded context and never `openehr: $reference`,
//! the `contexts` lists are acyclic, and a `scope: sub` context is listed by
//! some context.

use core::str::FromStr;
use std::collections::BTreeSet;
use std::path::Path;

use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::position::Located;

use crate::model::ast::ContextMappingFile;
use crate::model::ast::Mapping;
use crate::model::ast::keyword::ContextScope;
use crate::model::ast::keyword::Variable;
use crate::model::error::ModelCode;
use crate::model::load::MappingSet;

use super::FileRules;
use super::error;

/// Applies the rules that hold for a `slotContext` mapping: it names a loaded
/// context and does not write `openehr: $reference`.
pub(super) fn check_slot_context(
    rules: FileRules<'_>,
    mapping: &Mapping,
    path: &ModelPath,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(ref slot) = mapping.slot_context else {
        return;
    };
    let file = rules.file.file();
    let name = rules.name;
    require_context(
        rules.set,
        file,
        name,
        slot,
        &path.field("slotContext"),
        diagnostics,
    );
    let is_reference_variable = mapping
        .with
        .as_ref()
        .and_then(|with| with.openehr.as_ref())
        .is_some_and(|value| {
            Variable::from_str(value.value()).is_ok_and(|variable| variable == Variable::Reference)
        });
    // NOTE: sub-contexts.adoc §Rules: "A `slotContext` mapping with `openehr: $reference` is a
    // validation error", since a sub-context maps into the parent composition.
    if is_reference_variable {
        diagnostics.push(error(
            file,
            name,
            ModelCode::SubContextReferenceVariable,
            slot.position(),
            &path.field("with").field("openehr"),
            format!(
                "`slotContext: {}` maps into the parent composition, and `openehr: $reference` \
                 would create a separate composition, which is not supported",
                slot.value()
            ),
        ));
    }
}

/// Refuses a reference to a context that names no loaded context mapping.
pub(super) fn require_context(
    set: &MappingSet,
    file: &Path,
    owner: &MappingName,
    reference: &Located<MappingName>,
    path: &ModelPath,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if set.context(reference.value()).is_some() {
        return;
    }
    diagnostics.push(error(
        file,
        owner,
        ModelCode::UnknownMappingReference,
        reference.position(),
        path,
        format!(
            "`{}` is no context mapping of the loaded set",
            reference.value()
        ),
    ));
}

/// Applies the rules about how the loaded contexts enter one another
/// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`,
/// §Rules): the `contexts` lists are acyclic, and every `scope: sub` context
/// is listed under the `contexts` of at least one context.
pub(super) fn validate_sub_contexts(set: &MappingSet, diagnostics: &mut Vec<Diagnostic>) {
    let listed: BTreeSet<&MappingName> = set
        .contexts()
        .flat_map(|file| file.context().contexts.iter().map(Located::value))
        .collect();
    for file in set.contexts() {
        let name = file.header().name().value();
        let context = file.context();
        if context.scope() == ContextScope::Sub && !listed.contains(name) {
            let position = context
                .scope
                .as_ref()
                .map_or(context.position, Located::position);
            diagnostics.push(error(
                file.file(),
                name,
                ModelCode::UnreferencedSubContext,
                position,
                &ModelPath::root().field("context").field("scope"),
                format!("`{name}` is `scope: sub` and no context lists it under `contexts`"),
            ));
        }
        if let Some((reference, index)) = closes_cycle(set, file) {
            diagnostics.push(error(
                file.file(),
                name,
                ModelCode::CyclicContextReference,
                reference.position(),
                &ModelPath::root()
                    .field("context")
                    .field("contexts")
                    .index(index),
                format!(
                    "`{name}` lists `{}` under `contexts`, which reaches `{name}` again",
                    reference.value()
                ),
            ));
        }
    }
}

/// Returns the entry of `file`'s `contexts`, with its index, from which the
/// `contexts` lists of the loaded set lead back to `file`.
fn closes_cycle<'a>(
    set: &MappingSet,
    file: &'a ContextMappingFile,
) -> Option<(&'a Located<MappingName>, usize)> {
    let start = file.header().name().value();
    for (index, reference) in file.context().contexts.iter().enumerate() {
        let mut seen: BTreeSet<&MappingName> = BTreeSet::new();
        let mut pending = vec![reference.value()];
        while let Some(next) = pending.pop() {
            if next == start {
                return Some((reference, index));
            }
            if !seen.insert(next) {
                continue;
            }
            if let Some(entered) = set.context(next) {
                pending.extend(entered.context().contexts.iter().map(Located::value));
            }
        }
    }
    None
}
