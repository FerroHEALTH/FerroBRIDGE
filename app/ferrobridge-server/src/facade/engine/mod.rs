// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Running one program, and reading the identity of the entry it maps.
//!
//! The facade is a client of the same in-process engine the `$tofhir` and
//! `$toopenehr` operations call, so the two surfaces cannot disagree (no
//! specification governs this: our own design). This module is the thin seam: it hands the
//! engine the element table and the template index, and it reads back out of
//! the produced composition the two facts identity needs, the entry's path and
//! its `LOCATABLE.uid`.

pub mod entry;

use fhir_types::codec::Value;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::demographics::Demographics;
use fhirconnect::engine::origin::Origin;
use fhirconnect::engine::origin::SourceItem;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::IdentitySink;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::terminology::Terminology;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::party::from_party;
use fhirconnect::engine::traverse::party::to_party;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::party::CanonicalParty;

/// The composer the facade records when no mapping sets one.
///
/// "The composer … could be defaulted with a value such as `FHIRconnect`
/// party" (`engine/defaults-for-fields.adoc`); this deployment names itself so
/// a reader can tell which bridge wrote the composition.
pub const COMPOSER: &str = "FerroBRIDGE";

/// Maps one FHIR document into a composition.
///
/// `now` is the one instant this ingest defaults every composition field
/// from, so a single request cannot carry two clock readings. The composition
/// carries a `FEEDER_AUDIT` naming this bridge as the system, the document's
/// `resourceType`, `id` and `meta.versionId` as the source item, and every
/// field the engine defaulted, because `FEEDER_AUDIT` "describes the origin of
/// data that have been transformed into openEHR form and committed to the
/// system" (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
/// Every `conceptmap` code is translated through `terminology`, which
/// [`crate::terminology::Translator::run`] supplies with the codes resolved,
/// and every party reference resolves through `demographics`.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, for a
/// code `terminology` does not translate, and for a set of values the
/// template does not admit as a composition.
pub fn inbound(
    program: &Program,
    index: &WebTemplateIndex,
    document: &Value,
    now: &str,
    settings: &crate::facade::Settings,
    terminology: &dyn Terminology,
    demographics: &dyn Demographics,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    inbound_from(
        program,
        index,
        document,
        now,
        settings,
        SourceItem::of(document),
        &Seams::default()
            .with_terminology(terminology)
            .with_demographics(demographics),
    )
}

/// Maps one FHIR document into a composition that names `source` as its
/// origin.
///
/// This is [`inbound`] with the originating item given by the caller, so a
/// face that received something other than the resource itself, such as a
/// message, names what it received. `None` records no source item. Every
/// `conceptmap` code translates through the terminology `seams` carries, and
/// a `reference` mapping reads the resource it points at from its reference
/// source, where one it holds no resource for is a declared skip.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, for a
/// code the terminology does not translate, and for a set of values the
/// template does not admit as a composition.
pub fn inbound_from(
    program: &Program,
    index: &WebTemplateIndex,
    document: &Value,
    now: &str,
    settings: &crate::facade::Settings,
    source: Option<SourceItem>,
    seams: &Seams<'_>,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    // TODO(#95): resolve the displays the concept mappings need here, before
    // any value is built, once the PROGRAMMED registry lands.
    let mut origin = Origin::new(&settings.system_id);
    if let Some(source) = source {
        origin = origin.with_source(source);
    }
    let defaults = Defaults::at(now)
        .with_composer(COMPOSER)
        .with_language(&settings.language)
        .with_territory(&settings.territory)
        .with_origin(origin);
    to_openehr(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        document,
        seams,
        &defaults,
        &CallContext::new(),
    )
}

/// Maps one composition into a FHIR document, translating every
/// `conceptmap` code through `terminology` and reading every party reference
/// back through `demographics`.
///
/// Every resource the run creates beside the one it maps (a `hierarchy.split`
/// occurrence or a `reference`) takes the id `identities` assigns.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, for a
/// created resource `identities` gives no id, and for a code `terminology`
/// does not translate.
pub fn outbound(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    identities: &dyn IdentitySink,
    terminology: &dyn Terminology,
    demographics: &dyn Demographics,
) -> Result<Outcome<Value>, EngineError> {
    to_fhir(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        composition,
        &Seams::default()
            .with_identities(identities)
            .with_terminology(terminology)
            .with_demographics(demographics),
        &CallContext::new(),
    )
}

/// Runs a party program over one inbound resource, building its party.
///
/// A party carries no composition default (no specification governs a party
/// context: our own design).
///
/// # Errors
///
/// Returns the engine's refusal, a party the values do not build included.
pub fn party_inbound(
    program: &Program,
    index: &WebTemplateIndex,
    document: &Value,
    terminology: &dyn Terminology,
    demographics: &dyn Demographics,
) -> Result<Outcome<CanonicalParty>, EngineError> {
    to_party(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        document,
        &Seams::default()
            .with_terminology(terminology)
            .with_demographics(demographics),
        &CallContext::new(),
    )
}

/// Runs a party program over a party, producing its resource.
///
/// # Errors
///
/// Returns the engine's refusal.
pub fn party_outbound(
    program: &Program,
    index: &WebTemplateIndex,
    party: &CanonicalParty,
    terminology: &dyn Terminology,
    demographics: &dyn Demographics,
) -> Result<Outcome<Value>, EngineError> {
    from_party(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        party,
        &Seams::default()
            .with_terminology(terminology)
            .with_demographics(demographics),
        &CallContext::new(),
    )
}
