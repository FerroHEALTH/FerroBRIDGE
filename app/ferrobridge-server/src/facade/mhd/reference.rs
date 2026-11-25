// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DocumentReference`: Find Document References [ITI-67] and the read of
//! one description.
//!
//! The description of a document is the one [`DocumentMetadata::describe`]
//! writes from the document and the composition it carries: draft `EEHRxF`
//! Annex III Table 1 on the MyHealth@EU `DocumentReference-minimal` shape,
//! its `id` the Composition's, its document category the Composition's first
//! `category`. Its `content.attachment.url` is the full URL of the document,
//! `[base]/Composition/[id]/$document`, which Retrieve Document [ITI-68]
//! reads (ITI TF-2: 3.67.4.2.2.1).

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use serde_json::Value;

use eehrxf::document::metadata::DocumentMetadata;

use super::DOCUMENT_REFERENCE;
use super::Found;
use super::listing;
use super::params;
use super::params::Span;
use super::params::Values;
use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::handlers;
use crate::facade::handlers::read::COMPOSITION;
use crate::facade::handlers::read::Located;
use crate::facade::reply::Refusal;

/// The `rest.resource.documentation` of `DocumentReference`.
pub(super) const DOCUMENTATION: &str = "The IHE MHD 4.2.3 Document Responder: one DocumentReference per document a Composition context assembles, the draft EEHRxF Annex III description of Composition/[id]/$document under the Composition's id. Read, and Find Document References [ITI-67] by GET and POST; a parameter this server does not support is ignored unless Prefer: handling=strict asks for a refusal.";

/// The search parameters of `DocumentReference` (ITI TF-2: 3.67.4.1.2.1).
pub(super) const DECLARED: &[params::Declared] = &[
    (
        "_id",
        "http://hl7.org/fhir/SearchParameter/Resource-id",
        "token",
    ),
    (
        "_lastUpdated",
        "http://hl7.org/fhir/SearchParameter/Resource-lastUpdated",
        "date",
    ),
    (
        "author.given",
        "http://hl7.org/fhir/SearchParameter/individual-given",
        "string",
    ),
    (
        "author.family",
        "http://hl7.org/fhir/SearchParameter/individual-family",
        "string",
    ),
    (
        "category",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-category",
        "token",
    ),
    (
        "creation",
        "https://profiles.ihe.net/ITI/MHD/SearchParameter/DocumentReference-Creation",
        "date",
    ),
    (
        "date",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-date",
        "date",
    ),
    (
        "event",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-event",
        "token",
    ),
    (
        "facility",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-facility",
        "token",
    ),
    (
        "format",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-format",
        "token",
    ),
    (
        "identifier",
        "http://hl7.org/fhir/SearchParameter/clinical-identifier",
        "token",
    ),
    (
        "patient",
        "http://hl7.org/fhir/SearchParameter/clinical-patient",
        "reference",
    ),
    (
        "patient.identifier",
        "http://hl7.org/fhir/SearchParameter/Patient-identifier",
        "token",
    ),
    (
        "period",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-period",
        "date",
    ),
    (
        "related",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-related",
        "reference",
    ),
    (
        "security-label",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-security-label",
        "token",
    ),
    (
        "setting",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-setting",
        "token",
    ),
    (
        "status",
        "http://hl7.org/fhir/SearchParameter/DocumentReference-status",
        "token",
    ),
    (
        "type",
        "http://hl7.org/fhir/SearchParameter/clinical-type",
        "token",
    ),
];

/// The R4 code system of `DocumentReference.status`.
const STATUS: &str = "http://hl7.org/fhir/document-reference-status";

/// `GET [base]/DocumentReference/[id]`: the description of the document of
/// `Composition/[id]`.
pub(super) async fn read(
    facade: &Facade,
    access: &Access,
    client: &CdrClient,
    id: &str,
) -> Result<Object, Refusal> {
    let Ok(internal) = crate::facade::identity::FhirResourceId::new(id) else {
        return Err(super::unknown(DOCUMENT_REFERENCE, id));
    };
    let Some(binding) = handlers::read::binding(facade, COMPOSITION, &internal).await? else {
        return Err(super::unknown(DOCUMENT_REFERENCE, id));
    };
    let ehr = binding
        .ehr(&internal)
        .map_err(|error| handlers::read::stored_identifier(&error))?;
    crate::facade::compartment::admit_ehr(
        facade,
        access,
        Permission::Read,
        DOCUMENT_REFERENCE,
        &ehr,
    )
    .await?;
    let assembled = handlers::read::assembled(facade, access, client, id).await?;
    let (described, _) = describe(facade, id, &assembled.bundle, &assembled.composition)?;
    Ok(described)
}

/// Find Document References [ITI-67]: the descriptions of every document
/// of the EHRs the search reaches that match every parameter applied.
pub(super) async fn search(
    facade: &Facade,
    access: &Access,
    client: &CdrClient,
    pairs: &[(String, String)],
    strict: bool,
) -> Result<Found, Refusal> {
    let request = params::read(pairs, DECLARED, strict)?;
    let named = super::list::patients(facade, client, &request).await?;
    let ehrs = super::reach(facade, access, DOCUMENT_REFERENCE, named).await?;
    let mut found = Vec::new();
    for ehr_id in &ehrs {
        for listed in listing::listed(facade, client, listing::Scope::Ehr(ehr_id)).await? {
            let bundle = handlers::read::assemble(
                facade,
                access,
                client,
                Located {
                    program: &listed.program,
                    ehr_id: &listed.ehr_id,
                    id: &listed.id,
                    binding: &listed.binding,
                    version: &listed.version,
                    composition: &listed.composition,
                },
            )
            .await?;
            let (described, json) =
                describe(facade, listed.id.as_str(), &bundle, &listed.composition)?;
            let document = serde_json::to_value(&bundle)
                .map_err(|error| super::internal(format!("a document does not encode: {error}")))?;
            if matches_all(&request, &json, &document) {
                found.push(described);
            }
        }
    }
    Ok(Found::paged(found, request))
}

/// Returns the description of `bundle`, the document of `Composition/[id]`
/// carrying `composition`, and its JSON for matching.
///
/// A document category is required (draft `EEHRxF` Annex III Table 1
/// `documentCategory`), so a Composition with no `category` cannot be
/// described and answers `500` naming it.
fn describe(
    facade: &Facade,
    id: &str,
    bundle: &Object,
    composition: &openehr_mapping_core::composition::CanonicalComposition,
) -> Result<(Object, Value), Refusal> {
    let undescribed = |text: String| {
        super::internal(format!(
            "Composition/{id} cannot be described as a DocumentReference: {text}"
        ))
    };
    let category = bundle
        .get("entry")
        .and_then(fhir_types::codec::Value::as_array)
        .and_then(|entries| entries.first())
        .and_then(fhir_types::codec::Value::as_object)
        .and_then(|entry| entry.get("resource"))
        .and_then(fhir_types::codec::Value::as_object)
        .and_then(|resource| resource.get("category"))
        .and_then(fhir_types::codec::Value::as_array)
        .and_then(|categories| categories.first())
        .and_then(fhir_types::codec::Value::as_object)
        .ok_or_else(|| undescribed(String::from("its Composition carries no category")))?;
    let category = CodeableConcept::from_json(category, &mut Path::root("CodeableConcept"))
        .map_err(|error| undescribed(error.to_string()))?;
    let typed = Bundle::from_json(bundle, &mut Path::root("Bundle"))
        .map_err(|error| undescribed(error.to_string()))?;
    let metadata = DocumentMetadata::from_composition(composition.value(), category)
        .map_err(|error| undescribed(ferrohealth_server::run::chain(&error)))?;
    let mut described = metadata
        .describe(&typed, id)
        .map_err(|error| undescribed(ferrohealth_server::run::chain(&error)))?;
    let url = format!(
        "{}/{COMPOSITION}/{id}/$document",
        facade.settings().base_url.trim_end_matches('/')
    );
    for content in &mut described.content {
        content.attachment.url = Some(url.as_str().into());
    }
    let object = described
        .to_json()
        .map_err(|error| undescribed(error.to_string()))?;
    let json = serde_json::to_value(&object).map_err(|error| undescribed(error.to_string()))?;
    Ok((object, json))
}

/// Whether the description `json` of `document` matches every parameter
/// `request` applies; `patient` and `patient.identifier` scope the EHRs the
/// search lists.
fn matches_all(request: &params::Read, json: &Value, document: &Value) -> bool {
    request
        .applied
        .iter()
        .all(|occurrence| matches(occurrence.name, &occurrence.values, json, document))
}

/// Whether the description `json` matches one occurrence of `name`.
fn matches(name: &str, values: &Values, json: &Value, document: &Value) -> bool {
    let at = |pointer: &str| json.pointer(pointer);
    let each = |pointer: &str| -> Vec<&Value> {
        match at(pointer) {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(item) => vec![item],
            None => Vec::new(),
        }
    };
    match (name, values) {
        ("_id", Values::Tokens(tokens)) => tokens
            .iter()
            .any(|token| token.code() == at("/id").and_then(Value::as_str)),
        ("_lastUpdated", Values::Dates(dates)) => params::dates(
            dates,
            at("/meta/lastUpdated")
                .and_then(Value::as_str)
                .and_then(Span::of_date),
        ),
        ("author.given" | "author.family", Values::Texts(matching, texts)) => {
            let part = if name == "author.given" {
                "given"
            } else {
                "family"
            };
            let names = author_names(&each("/author"), document, part);
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            params::texts(*matching, texts, &names)
        }
        ("category", Values::Tokens(tokens)) => {
            params::tokens_code(tokens, each("/category").into_iter())
        }
        ("creation", Values::Dates(dates)) => each("/content")
            .iter()
            .filter_map(|content| content.pointer("/attachment/creation"))
            .filter_map(Value::as_str)
            .any(|creation| params::dates(dates, Span::of_date(creation))),
        ("date", Values::Dates(dates)) => params::dates(
            dates,
            at("/date").and_then(Value::as_str).and_then(Span::of_date),
        ),
        ("event", Values::Tokens(tokens)) => {
            params::tokens_code(tokens, each("/context/event").into_iter())
        }
        ("facility", Values::Tokens(tokens)) => {
            params::tokens_code(tokens, each("/context/facilityType").into_iter())
        }
        ("format", Values::Tokens(tokens)) => params::tokens_code(
            tokens,
            each("/content")
                .into_iter()
                .filter_map(|content| content.get("format")),
        ),
        ("identifier", Values::Tokens(tokens)) => params::tokens_identifier(
            tokens,
            each("/masterIdentifier")
                .into_iter()
                .chain(each("/identifier")),
        ),
        ("period", Values::Dates(dates)) => {
            params::dates(dates, at("/context/period").and_then(Span::of_period))
        }
        ("related", Values::References(references)) => each("/context/related")
            .iter()
            .filter_map(|related| related.get("reference").and_then(Value::as_str))
            .any(|reference| references.iter().any(|wanted| wanted == reference)),
        ("security-label", Values::Tokens(tokens)) => {
            params::tokens_code(tokens, each("/securityLabel").into_iter())
        }
        ("setting", Values::Tokens(tokens)) => {
            params::tokens_code(tokens, each("/context/practiceSetting").into_iter())
        }
        ("status", Values::Tokens(tokens)) => {
            params::tokens_plain(tokens, STATUS, at("/status").and_then(Value::as_str))
        }
        ("type", Values::Tokens(tokens)) => params::tokens_code(tokens, each("/type").into_iter()),
        ("patient" | "patient.identifier", _) => true,
        _ => false,
    }
}

/// Returns the name parts `part` of every author `authors` names that the
/// document holds as an entry, read from its `name`.
fn author_names(authors: &[&Value], document: &Value, part: &str) -> Vec<String> {
    let entries = document
        .get("entry")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut names = Vec::new();
    for author in authors {
        let Some(reference) = author.get("reference").and_then(Value::as_str) else {
            continue;
        };
        let held = entries.iter().find(|entry| {
            entry.get("fullUrl").and_then(Value::as_str) == Some(reference)
                || entry.get("resource").is_some_and(|resource| {
                    let kind = resource.get("resourceType").and_then(Value::as_str);
                    let id = resource.get("id").and_then(Value::as_str);
                    matches!((kind, id), (Some(kind), Some(id)) if reference == format!("{kind}/{id}"))
                })
        });
        let Some(names_of) = held
            .and_then(|entry| entry.pointer("/resource/name"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for name in names_of {
            match name.get(part) {
                Some(Value::Array(items)) => {
                    names.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                Some(Value::String(text)) => names.push(text.clone()),
                _ => {}
            }
        }
    }
    names
}
