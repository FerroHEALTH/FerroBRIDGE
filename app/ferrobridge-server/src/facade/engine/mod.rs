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
use fhirconnect::engine::library::Library;
use fhirconnect::engine::origin::ItemId;
use fhirconnect::engine::origin::Origin;
use fhirconnect::engine::origin::Party;
use fhirconnect::engine::origin::SourceItem;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::seam::identity::IdentitySink;
use fhirconnect::engine::terminology::Terminology;
use fhirconnect::engine::traverse::defaults::ComposerId;
use fhirconnect::engine::traverse::defaults::Defaults;
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
        origin_of(settings, SourceItem::of(document), None),
        &Seams::default()
            .with_functions(&Library)
            .with_terminology(terminology)
            .with_demographics(demographics),
    )
}

/// What a run into openEHR records of where its content came from: the
/// origin its `FEEDER_AUDIT` names, and the health care facility its
/// `EVENT_CONTEXT` names when no mapping gave one.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The origin the `FEEDER_AUDIT` names.
    pub origin: Origin,
    /// The facility the run defaults, when the face named one.
    pub facility: Option<Party>,
    /// The composer the run defaults, when the face named one.
    pub composer: Option<Composer>,
}

/// The composer a run defaults when no mapping sets one (RM EHR IM,
/// `COMPOSITION.composer`, "The person primarily responsible for the content
/// of the Composition").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composer {
    /// The name the composer carries.
    pub name: String,
    /// The reference the composer carries, `PARTY_PROXY.external_ref`, when the
    /// face names one.
    pub id: Option<ComposerId>,
    /// The formal identifiers the composer carries, each with its issuer
    /// (`PARTY_IDENTIFIED.identifiers`).
    pub identifiers: Vec<ItemId>,
}

/// Returns the origin of a run under `settings`: the bridge as the system the
/// content passed through, `source` as the item it came from, and
/// `originating`, when given, as the system it originated in.
#[must_use]
pub fn origin_of(
    settings: &crate::facade::Settings,
    source: Option<SourceItem>,
    originating: Option<&str>,
) -> Recorded {
    let mut origin = Origin::new(&settings.system_id);
    if let Some(originating) = originating {
        origin = origin.with_originating_system(originating);
    }
    if let Some(source) = source {
        origin = origin.with_source(source);
    }
    Recorded {
        origin,
        facility: None,
        composer: None,
    }
}

/// Maps one FHIR document into a composition that names `origin` as its
/// origin.
///
/// This is [`inbound`] with the origin given by the caller ([`origin_of`]),
/// so a face that received something other than the resource itself, such
/// as a message, names what it received, and a system caller is named as the
/// originating system. Every
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
    recorded: Recorded,
    seams: &Seams<'_>,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    let mut defaults = Defaults::at(now)
        .with_composer(COMPOSER)
        .with_language(&settings.language)
        .with_territory(&settings.territory)
        .with_origin(recorded.origin);
    if let Some(facility) = recorded.facility {
        defaults = defaults.with_facility(facility);
    }
    if let Some(composer) = recorded.composer {
        defaults = defaults
            .with_composer(composer.name)
            .with_composer_identifiers(composer.identifiers);
        if let Some(id) = composer.id {
            defaults = defaults.with_composer_id(id);
        }
    }
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
            .with_functions(&Library)
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
            .with_functions(&Library)
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
            .with_functions(&Library)
            .with_terminology(terminology)
            .with_demographics(demographics),
        &CallContext::new(),
    )
}
