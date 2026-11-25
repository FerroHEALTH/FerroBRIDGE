// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document of one patient, written from the patient
//! summary compositions of the patient's EHR.
//!
//! Each composition runs through the EPS project of `eehrxf`, every code in
//! no preferred system is translated on the terminology server
//! (`eehrxf::terminology::translation`), and the sections of every run are
//! placed in the form's slots. A section the project feeds and no run filled
//! is `nilknown`; a required section no openEHR content feeds is
//! `unavailable`, and its narrative says the bridge does not supply it
//! (FHIR R4 `list-empty-reason`). A term mapping the run wrote as no coding
//! and every element it could not carry are written into the section's
//! narrative. No specification governs these rules: our own design, against
//! the `Composition.section` elements of the form's profile.

use std::collections::BTreeMap;

use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use eehrxf::crosswalk::patient_summary::SOURCES;
use eehrxf::document::Document;
use eehrxf::document::DocumentError;
use eehrxf::document::form::Form;
use eehrxf::document::form::SectionRule;
use eehrxf::document::form::Slot;
use eehrxf::document::section::EmptyReason;
use eehrxf::document::section::MappedSection;
use eehrxf::document::section::Section;
use eehrxf::mapping::Mapping;
use eehrxf::mapping::MappingError;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::ehn;
use eehrxf::terminology::system::ARCHETYPE_LOCAL;
use eehrxf::terminology::translation::Binding;
use eehrxf::terminology::translation::TranslationError;
use eehrxf::terminology::translation::Translator;
use eehrxf::terminology::withheld::Withheld;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::bundle::BundleEntry;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::device::Device;
use fhir_types::r4::device::DeviceDeviceName;
use fhir_types::r4::device::DeviceVersion;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run::Settings;

/// The narrative of a section the record holds no entry for.
const NOTHING_RECORDED: &str = "The record holds no entry for this section.";

/// The narrative of a required section no openEHR content feeds.
const NOT_SUPPLIED: &str = "This server does not supply this section: no openEHR content it reads feeds it, so the section says nothing about whether the patient has any.";

/// The narrative of a section the record holds entries for.
const RECORDED: &str = "This section lists what the record holds.";

/// What one summary is written from.
#[derive(Debug)]
pub(super) struct Inputs<'a> {
    /// The document form.
    pub(super) form: &'static Form,
    /// The value-set bindings the document's codes meet through the
    /// terminology server.
    pub(super) bindings: &'static [Binding],
    /// The absolute FHIR base every entry is named under.
    pub(super) base: &'a str,
    /// The document identifier, a `urn:uuid:` URN.
    pub(super) identifier: String,
    /// The instant it is written, a FHIR `instant`.
    pub(super) timestamp: String,
    /// The patient, as the party context renders it.
    pub(super) patient: Patient,
    /// The name of the organisation that operates this server.
    pub(super) operator: &'a str,
    /// The patient summary compositions of the patient's EHR, canonical JSON.
    pub(super) compositions: &'a [String],
}

/// Why a summary cannot be written.
#[derive(Debug, thiserror::Error)]
pub(super) enum AssemblyError {
    /// The document breaks a rule of its form.
    #[error("the patient summary cannot be written")]
    Document(#[from] DocumentError),
    /// The EPS project refused a composition.
    #[error("the patient summary project refused a composition")]
    Mapping(#[source] MappingError),
    /// A code could not be translated.
    #[error("a code of the patient summary could not be translated")]
    Translation(#[source] TranslationError),
}

/// The document, and the authors this server wrote into it.
#[derive(Debug)]
pub(super) struct Assembled {
    /// The document `Bundle`.
    pub(super) bundle: Bundle,
    /// The `fullUrl` of this server's `Device` and of its operator.
    pub(super) authored: Vec<String>,
}

/// One slot as the runs filled it.
#[derive(Debug, Default)]
struct Filled {
    /// The `fullUrl` of every entry, in run order.
    entries: Vec<String>,
    /// The section the first run that placed it wrote, with its empty
    /// reason.
    mapped: Option<MappedSection>,
    /// The narrative lines of what the runs withheld or dropped.
    lines: Vec<String>,
}

/// Writes the summary `inputs` describe, mapping each composition with
/// `mapping` and translating its codes through `terminology`.
///
/// # Errors
///
/// [`AssemblyError::Mapping`] when the project refuses a composition,
/// [`AssemblyError::Translation`] when the terminology server gives no
/// answer, and [`AssemblyError::Document`] for a document its form refuses.
pub(super) async fn assemble(
    inputs: Inputs<'_>,
    mapping: &Mapping,
    terminology: &Terminology,
) -> Result<Assembled, AssemblyError> {
    let mut document = Document::of(
        inputs.form,
        inputs.base,
        inputs.identifier,
        inputs.timestamp.clone(),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(inputs.patient)?;
    let operator_url = document.full_url("Organization", "operator");
    let device = document.with_author(&Resource::Device(Box::new(device(&operator_url))))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("operator")),
        name: Some(inputs.operator.into()),
        ..Organization::default()
    })))?;
    let settings = Settings::new(device.clone(), inputs.timestamp);
    let mut translator =
        Translator::new(terminology, &ehn::PATIENT_SUMMARY).with_bindings(inputs.bindings);
    let mut filled: BTreeMap<&'static str, Filled> = BTreeMap::new();
    for composition in inputs.compositions {
        let context = CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device));
        let (answer, warnings) =
            match mapping.to_fhir_with_warnings(composition, &settings, context) {
                Ok(ran) => ran,
                Err(refused) if not_applicable(&refused) => continue,
                Err(refused) => return Err(AssemblyError::Mapping(refused)),
            };
        let mut entry = Vec::with_capacity(answer.bundle().entry.len());
        for resource in answer
            .bundle()
            .entry
            .iter()
            .filter_map(|held| held.resource.clone())
        {
            let resource = match resource {
                Resource::Composition(_)
                | Resource::Provenance(_)
                | Resource::OperationOutcome(_) => resource,
                other => translator
                    .translate(&other, preferred)
                    .await
                    .map_err(AssemblyError::Translation)?,
            };
            entry.push(BundleEntry {
                resource: Some(resource),
                ..BundleEntry::default()
            });
        }
        let run = Bundle {
            entry,
            ..answer.bundle().clone()
        };
        let lines = narrated(&warnings);
        for section in document.with_mapped_composition(&run)? {
            let slot = filled.entry(section.slot.slice).or_default();
            slot.entries.extend(section.entries.iter().cloned());
            slot.lines.extend(lines.iter().cloned());
            if slot.mapped.is_none() {
                slot.mapped = Some(section);
            }
        }
    }
    if !translator.unbound().is_empty() {
        tracing::warn!(
            unbound = translator.unbound().len(),
            "a patient summary holds bound concepts the terminology server maps into no bound system"
        );
    }
    let fed = fed_slices();
    for slot in inputs.form.slots {
        if let Some(section) = written(inputs.form, slot, filled.remove(slot.slice), &fed) {
            document.with_section(section)?;
        }
    }
    Ok(Assembled {
        bundle: document.into_bundle()?,
        authored: vec![device, operator_url],
    })
}

/// The section `slot` takes from what the runs `filled` it with, `None` for
/// a slot the form does not require and no run filled.
fn written(
    form: &Form,
    slot: &'static Slot,
    filled: Option<Filled>,
    fed: &[&str],
) -> Option<Section> {
    let required = form.rules.iter().any(|rule| match rule {
        SectionRule::Required { slices } => slices.contains(&slot.slice),
        _ => false,
    });
    let Some(filled) = filled else {
        return required.then(|| {
            if fed.contains(&slot.slice) {
                Section::new(slot, vec![String::from(NOTHING_RECORDED)])
                    .empty_because(EmptyReason::NilKnown)
            } else {
                Section::new(slot, vec![String::from(NOT_SUPPLIED)])
                    .empty_because(EmptyReason::Unavailable)
            }
        });
    };
    if !filled.entries.is_empty() {
        let mut paragraphs = vec![String::from(RECORDED)];
        paragraphs.extend(filled.lines);
        return Some(
            filled
                .entries
                .into_iter()
                .fold(Section::new(slot, paragraphs), Section::with_entry),
        );
    }
    let mut paragraphs = vec![String::from(NOTHING_RECORDED)];
    paragraphs.extend(filled.lines);
    Some(match filled.mapped {
        Some(mapped) if mapped.empty_reason.is_some() => mapped.into_section(paragraphs),
        _ => Section::new(slot, paragraphs).empty_because(EmptyReason::NilKnown),
    })
}

/// The slices the EPS project feeds, read from the crosswalk rows of its
/// sources.
fn fed_slices() -> Vec<&'static str> {
    let mut slices: Vec<&'static str> = SOURCES
        .iter()
        .filter_map(|source| PATIENT_SUMMARY.row(source.path))
        .filter_map(|row| row.eps.strip_prefix("Composition.section:"))
        .map(|rest| rest.split_once('.').map_or(rest, |(slice, _)| slice))
        .collect();
    slices.sort_unstable();
    slices.dedup();
    slices
}

/// The narrative lines of what one run withheld or could not carry, by
/// element path, never by value.
fn narrated(warnings: &[fhirconnect::engine::outcome::Warning]) -> Vec<String> {
    let mut lines: Vec<String> = warnings
        .iter()
        .filter_map(Withheld::of)
        .map(|withheld| withheld.narrative())
        .collect();
    let unread = warnings
        .iter()
        .filter(|warning| matches!(warning.loss(), Loss::Unread { .. }))
        .count();
    for warning in warnings {
        if Withheld::of(warning).is_none() && !matches!(warning.loss(), Loss::Unread { .. }) {
            lines.push(format!(
                "Not carried into this summary: {}.",
                warning.loss()
            ));
        }
    }
    if unread > 0 {
        lines.push(format!(
            "The record holds {unread} element(s) this summary has no place for."
        ));
    }
    lines
}

/// Whether a coding of `system` needs no translation: a preferred system of
/// the eHealth Network Guidelines on Patient Summary §4, or an
/// archetype-local code system rendered from the operational template.
fn preferred(system: &str) -> bool {
    ehn::PATIENT_SUMMARY.contains(&system) || system.starts_with(ARCHETYPE_LOCAL)
}

/// Whether `refused` is the engine's answer that the context's condition
/// does not hold over the composition (FHIRconnect 1.0.0
/// `basics/preprocessor.adoc`).
fn not_applicable(refused: &MappingError) -> bool {
    let MappingError::Run { source } = refused else {
        return false;
    };
    let OperationError::Mapping { source } = &**source else {
        return false;
    };
    matches!(**source, EngineError::NotApplicable { .. })
}

/// The `Device` that writes the summary: this server, owned by its operator.
fn device(operator: &str) -> Device {
    Device {
        id: Some(String::from("ferrobridge")),
        device_name: vec![DeviceDeviceName {
            name: crate::build_info::PRODUCT.into(),
            r#type: "manufacturer-name".into(),
            ..DeviceDeviceName::default()
        }],
        version: vec![DeviceVersion {
            value: crate::build_info::VERSION.into(),
            ..DeviceVersion::default()
        }],
        // NOTE: eHN Guidelines on Patient Summary A.1.4.3: the author device carries the nature of the summary.
        r#type: Some(CodeableConcept {
            text: Some("Software that generates the patient summary automatically".into()),
            ..CodeableConcept::default()
        }),
        owner: Some(reference(operator)),
        ..Device::default()
    }
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}
