// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constructs a party context refuses at load.
//!
//! A party context is a context whose operational template is rooted at a
//! `PERSON`, an `ORGANISATION` or a `ROLE`, read from the RM type of the
//! template root. FHIRconnect v1.0.0 admits an `openEHR-DEMOGRAPHIC-*`
//! archetype and defines no mapping rooted at a party, so every rule in this
//! module is our own design: no specification governs it. What a party
//! context refuses is what names a composition: `$composition`, the
//! composition attributes the defaults and the `ctx/` vocabulary set, the
//! `PARTICIPATION` lists, `link`, and an entered context whose `start` slots
//! a composition model.

use std::path::Path;

use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::position::Position;
use openehr_rm::v1_2::paths::RmPath;

use crate::model::ast::Mapping;
use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::diagnostic;

use crate::resolve::compile::Compiler;

/// The `COMPOSITION` attributes the composition defaults and the `ctx/`
/// vocabulary set, which a party context refuses.
///
/// `engine/defaults-for-fields.adoc` defaults the composer, the context and
/// the language, territory and category, and Simplified Formats master06 sets
/// the same attributes through `ctx/`. A party has none of them
/// (RM Demographic §PARTY).
const COMPOSITION_FIELDS: [&str; 5] = ["composer", "context", "language", "territory", "category"];

/// The site of one refusal: the file, the mapping that owns it, the YAML
/// position and the path into the document model.
pub(super) struct Site<'site> {
    /// The file the refused construct is written in.
    pub(super) file: &'site Path,
    /// The `metadata.name` of that file.
    pub(super) owner: &'site MappingName,
    /// The position of the refused construct.
    pub(super) position: Position,
    /// The path into the document model.
    pub(super) path: &'site ModelPath,
}

impl Compiler<'_> {
    /// Refuses a `$composition` path in a party context, answering whether it
    /// did.
    ///
    /// `$composition` is the "Path of the root composition"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/basics/Variables.adoc`),
    /// and a party context has none.
    pub(super) fn party_refuses_composition(&mut self, site: &Site<'_>, written: &str) -> bool {
        let Some(party) = self.party else {
            return false;
        };
        self.refuse(
            site,
            ResolveCode::PartyCompositionVariable,
            format!(
                "`{written}` opens with `$composition` in a party context rooted at a `{party}`, \
                 which has no composition"
            ),
        );
        true
    }

    /// Refuses a path that names a composition attribute the defaults or the
    /// `ctx/` vocabulary set, answering whether it did.
    pub(super) fn party_refuses_field(&mut self, site: &Site<'_>, resolved: &RmPath) -> bool {
        let Some(party) = self.party else {
            return false;
        };
        let Some(field) = resolved
            .segments
            .first()
            .map(|segment| segment.attribute.as_str())
            .filter(|attribute| resolved.absolute && COMPOSITION_FIELDS.contains(attribute))
        else {
            return false;
        };
        self.refuse(
            site,
            ResolveCode::PartyCompositionField,
            format!(
                "`{resolved}` names the composition attribute `{field}`, which the composition \
                 defaults and the `ctx/` vocabulary set, in a party context rooted at a `{party}`"
            ),
        );
        true
    }

    /// Refuses a `participationsFunction` in a party context, answering
    /// whether it did.
    ///
    /// `EVENT_CONTEXT.participations` and `ENTRY.other_participations` are the
    /// lists it writes (RM EHR IM, the `EVENT_CONTEXT` and `ENTRY` classes), and a party holds
    /// neither.
    pub(super) fn party_refuses_participation(
        &mut self,
        file: &Path,
        owner: &MappingName,
        method: &Mapping,
        path: &ModelPath,
    ) -> bool {
        let (Some(party), Some(function)) = (self.party, method.participations_function.as_ref())
        else {
            return false;
        };
        let site = Site {
            file,
            owner,
            position: function.position(),
            path: &path.field("participationsFunction"),
        };
        self.refuse(
            &site,
            ResolveCode::PartyParticipation,
            format!(
                "`{}` writes a `participationsFunction` in a party context rooted at a \
                 `{party}`, and a party holds no `EVENT_CONTEXT` or `PARTICIPATION` list",
                method.name.value()
            ),
        );
        true
    }

    /// Refuses a `link` in a party context, answering whether it did.
    ///
    /// `LINK.target` is a `DV_EHR_URI` naming a composition in an EHR
    /// (RM Common IM, the `LINK` class), and a party lives in no EHR.
    pub(super) fn party_refuses_link(
        &mut self,
        file: &Path,
        owner: &MappingName,
        method: &Mapping,
        path: &ModelPath,
    ) -> bool {
        let (Some(party), Some(link)) = (self.party, method.link.as_ref()) else {
            return false;
        };
        let site = Site {
            file,
            owner,
            position: link.position,
            path: &path.field("link"),
        };
        self.refuse(
            &site,
            ResolveCode::PartyLink,
            format!(
                "`{}` writes a `link` in a party context rooted at a `{party}`, and a party \
                 lives in no EHR for `LINK.target` to name",
                method.name.value()
            ),
        );
        true
    }

    /// Refuses entering `context` from a party context when its `start`
    /// slots a composition model, answering whether it did.
    pub(super) fn party_refuses_entry(
        &mut self,
        site: &Site<'_>,
        context: &MappingName,
        start: &MappingName,
        slots_composition: bool,
    ) -> bool {
        let Some(party) = self.party.filter(|_| slots_composition) else {
            return false;
        };
        self.refuse(
            site,
            ResolveCode::PartySubContextComposition,
            format!(
                "`{context}` starts at `{start}`, which slots a composition model, and the party \
                 context rooted at a `{party}` has no composition"
            ),
        );
        true
    }

    /// Records one refusal at `site`.
    fn refuse(&mut self, site: &Site<'_>, code: ResolveCode, message: String) {
        self.diagnostics.push(diagnostic(
            site.file,
            site.owner,
            code,
            site.position,
            site.path,
            message,
        ));
    }
}
