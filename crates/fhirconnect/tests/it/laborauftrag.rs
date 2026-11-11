// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Organization` round trip: the requester of the published KDS
//! laboratory request chain of the FHIRconnect mapping library, under a
//! FerroBRIDGE project context compiled against the published
//! `KDS_Laborauftrag` operational template.
//!
//! The service request model reaches `CLUSTER.organisation.v1` through a
//! `reference` to an `Organization`
//! (`types-of-mappings/concept-type/Reference.adoc`): going in the requester
//! resolves to the resource it names, going out the run creates the
//! `Organization` and references it. The published organisation model loads
//! verbatim. Where another published file does not compile or run, its
//! refusal is asserted here and the project directory carries a stand-in
//! that says what it departs from.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::origin::Origin;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::seam::BundleReferences;
use fhirconnect::engine::seam::NoReferences;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use ferrobridge_testkit::laws::declared;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;
use crate::support::rows;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published files the project context loads, as the library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "model/cluster/org.openehr/organisation.v1.yml",
    "model/cluster/org.openehr/specimen.v1.yml",
];

/// The project directory this crate authored for the round trip.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_laborauftrag";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_laborauftrag.context.yml",
    "ferrobridge_service_request.v1.yml",
    "ferrobridge_request.v1.ServiceRequest.yml",
    "ferrobridge_kds_laborauftrag.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_laborauftrag.context";

/// The published files of the chain the stand-ins replace or leave out, as
/// the library ships them.
const REPLACED: &[&str] = &[
    "model/instruction/org.openehr/service_request.v1.yml",
    "model/composition/org.openehr/request.v1.ServiceRequest.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "projects/org.highmed/KDS/laborauftrag/KDS_laborauftrag.yml",
    "projects/org.highmed/KDS/laborauftrag/KDS_composition.yml",
];

/// The `id` the synthetic request's contained requester carries.
const CONTAINED_ID: &str = "einsender";

/// The `id` the synthetic request's contained specimen carries.
const SPECIMEN_ID: &str = "probe";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Laborauftrag` template.
fn kds_laborauftrag_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::KDS_LABORAUFTRAG_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_laborauftrag_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_laborauftrag_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_laborauftrag_template()?;
    let set = kds_laborauftrag_set()?;
    let program = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_laborauftrag_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_laborauftrag_program()?;
    Ok((PUBLISHED, program))
}

/// Returns the synthetic MII laboratory `ServiceRequest` of the testkit.
fn kds_service_request() -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_str(
        ferrobridge_testkit::fixtures::KDS_LABORAUFTRAG_SERVICE_REQUEST,
    )?)
}

/// Returns the resource the synthetic request contains under `id`.
fn contained(request: &serde_json::Value, id: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    request
        .get("contained")
        .and_then(serde_json::Value::as_array)
        .and_then(|contained| {
            contained
                .iter()
                .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(id))
        })
        .cloned()
        .ok_or_else(|| format!("the synthetic request contains no `{id}`").into())
}

/// Builds the synthetic `KDS_Laborauftrag` composition from its FLAT fixture.
fn kds_laborauftrag_composition(
    index: &WebTemplateIndex,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let text = std::fs::read_to_string(project("kds_laborauftrag.flat.json"))?;
    let flat: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
    Ok(index.build_from_flat(&flat, laws::NOW)?)
}

/// Renders a document of the engine's model as JSON, rooted at its type.
fn json_of(document: &Value, resource_type: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(document.to_serde_json(&mut fhir_types::codec::Path::root(resource_type))?)
}

/// The resources a run out of openEHR creates for the request, by type: the
/// requester and the specimen.
const CREATED: [&str; 2] = ["Organization", "Specimen"];

/// Returns the one resource of `resource_type` a run out of openEHR created,
/// after checking the run created exactly one of each type in [`CREATED`].
fn created_of(created: &[Value], resource_type: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let types: Vec<Option<&str>> = created
        .iter()
        .map(|resource| resource.get("resourceType").and_then(Value::as_str))
        .collect();
    if types != CREATED.map(Some) {
        return Err(format!("the run created {types:?}, not {CREATED:?}").into());
    }
    let resource = created
        .iter()
        .find(|resource| {
            resource.get("resourceType").and_then(Value::as_str) == Some(resource_type)
        })
        .ok_or_else(|| format!("the run created no {resource_type}"))?;
    json_of(resource, resource_type)
}

/// Returns the losses one leg declared, as the messages a reader reviews.
fn messages(warnings: &[fhirconnect::engine::outcome::Warning]) -> Vec<String> {
    warnings.iter().map(ToString::to_string).collect()
}

/// Maps `request` into a `KDS_Laborauftrag` composition with no reference
/// source.
fn inbound(
    program: &Program,
    index: &WebTemplateIndex,
    request: serde_json::Value,
) -> Result<
    fhirconnect::engine::outcome::Outcome<CanonicalComposition>,
    fhirconnect::engine::traverse::error::EngineError,
> {
    to_openehr(
        program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(request),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )
}

#[test]
fn the_kds_laborauftrag_context_compiles_against_the_published_template()
-> Result<(), Box<dyn Error>> {
    let (program, _) = kds_laborauftrag_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Laborauftrag");
    assert_eq!(program.resource().as_str(), "ServiceRequest");
    insta::assert_snapshot!("kds_laborauftrag_program", program.to_string());
    Ok(())
}

#[test]
fn the_kds_laborauftrag_flat_fixture_builds_a_composition_the_template_admits()
-> Result<(), Box<dyn Error>> {
    let index = kds_laborauftrag_template()?;
    let composition = kds_laborauftrag_composition(&index)?;
    assert_eq!(composition.template_id(), "KDS_Laborauftrag");
    Ok(())
}

/// `PutGet` over the request: the requester is a contained `Organization`
/// going in and a created one going out, so the request itself declares the
/// contained resource lost and the reference changed; the organisation is
/// compared on its own in the next case.
#[test]
fn putget_holds_on_the_kds_service_request() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_laborauftrag_program()?;
    let declared = laws::putget(&program, &index, &kds_service_request()?, &laws::untouched)?;
    insta::assert_json_snapshot!("kds_laborauftrag_putget", declared);
    Ok(())
}

/// Runs `PutGet` over the requester `Organization` and returns its declared
/// set.
///
/// # Errors
///
/// The errors of [`contained_putget`].
pub(crate) fn organization_putget() -> Result<serde_json::Value, Box<dyn Error>> {
    contained_putget("Organization", CONTAINED_ID, "/requester/reference")
}

/// Runs `PutGet` over the resource the request contains under `id` and
/// returns its declared set.
///
/// The request goes into openEHR with the contained resource and comes back
/// with a created resource of `resource_type` it references at `pointer`; the
/// law compares the contained resource with the created one, the `id` the
/// identity sink assigns aside.
///
/// # Errors
///
/// Returns an error when a leg refuses or the request does not reference the
/// resource the run created.
fn contained_putget(
    resource_type: &str,
    id: &str,
    pointer: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let (program, index) = kds_laborauftrag_program()?;
    let request = kds_service_request()?;
    let into = inbound(&program, &index, request.clone())?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        into.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let created = created_of(outbound.created(), resource_type)?;
    let back = json_of(outbound.value(), "ServiceRequest")?;
    let created_id = created
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the created {resource_type} carries no id"))?;
    let reference = back.pointer(pointer).and_then(serde_json::Value::as_str);
    if reference != Some(format!("{resource_type}/{created_id}").as_str()) {
        return Err(format!(
            "the request references {reference:?}, not the {resource_type} the run created"
        )
        .into());
    }
    let mut original = contained(&request, id)?;
    let mut answered = created.clone();
    for document in [&mut original, &mut answered] {
        if let Some(object) = document.as_object_mut() {
            object.remove("id");
        }
    }
    Ok(declared(
        &messages(into.warnings()),
        &messages(outbound.warnings()),
        &original,
        &answered,
    ))
}

/// Runs `GetPut` over the synthetic composition and returns its declared
/// set.
///
/// Going out the run creates the `Organization`, and going back in the
/// request's reference resolves to it through the reference source, as a
/// Bundle carrying both resolves it
/// (<https://hl7.org/fhir/R4/bundle.html#references>).
///
/// # Errors
///
/// Returns an error when a leg refuses or the run out of openEHR creates
/// another number of resources than the one `Organization`.
pub(crate) fn laborauftrag_getput() -> Result<serde_json::Value, Box<dyn Error>> {
    let (program, index) = kds_laborauftrag_program()?;
    let composition = kds_laborauftrag_composition(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    for resource_type in CREATED {
        created_of(outbound.created(), resource_type)?;
    }
    let entries: Vec<(Option<String>, Value)> = outbound
        .created()
        .iter()
        .map(|resource| (None, resource.clone()))
        .collect();
    let bundle = BundleReferences::new(entries, &NoReferences);
    let back = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        outbound.value(),
        &Seams::default().with_references(&bundle),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let input = serde_json::Value::Object(index.flatten(&composition)?);
    let output = serde_json::Value::Object(index.flatten(back.value())?);
    Ok(declared(
        &messages(outbound.warnings()),
        &messages(back.warnings()),
        &input,
        &output,
    ))
}

/// The requester `Organization` maps into `CLUSTER.organisation.v1` and comes
/// back as the resource the run creates, which the request references.
#[test]
fn putget_holds_on_the_requester_organization() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "kds_laborauftrag_organization_putget",
        organization_putget()?
    );
    Ok(())
}

/// The contained `Specimen` maps into `CLUSTER.specimen.v1` through the
/// published specimen model and comes back as the resource the run creates,
/// which the request references.
#[test]
fn putget_holds_on_the_contained_specimen() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "kds_laborauftrag_specimen_putget",
        contained_putget("Specimen", SPECIMEN_ID, "/specimen/0/reference")?
    );
    Ok(())
}

/// `GetPut` over the composition, the organisation and specimen clusters
/// included.
#[test]
fn getput_holds_on_the_kds_laborauftrag_composition() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!("kds_laborauftrag_getput", laborauftrag_getput()?);
    Ok(())
}

/// `INSTRUCTION.narrative` is a mandatory `DV_TEXT` (RM EHR IM §INSTRUCTION)
/// that no mapping of the request writes, so the FLAT builder fills it; the
/// run declares the fill once, as a warning and in the composition's
/// `FEEDER_AUDIT` (RM common IM, the `FEEDER_AUDIT` class).
#[test]
fn an_instruction_narrative_left_to_the_builder_is_declared() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_laborauftrag_program()?;
    let into = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(kds_service_request()?),
        &Seams::default(),
        &laws::defaults().with_origin(Origin::new("ferrobridge.test")),
        &CallContext::new(),
    )?;
    let narratives: Vec<&str> = into
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Defaulted { ref field } if field.ends_with("/narrative") => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        narratives,
        ["/content[openEHR-EHR-INSTRUCTION.service_request.v1,'Laborleistung']/narrative"],
        "the one instruction's narrative is declared once"
    );
    let audited: Vec<&str> = into
        .value()
        .value()
        .pointer("/feeder_audit/feeder_system_item_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or("the composition carries the defaulted fields in its FEEDER_AUDIT")?
        .iter()
        .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        audited.contains(&narratives[0]),
        "the FEEDER_AUDIT names the narrative: {audited:?}"
    );
    Ok(())
}

/// `KDS_Laborauftrag` constrains the `Einsender` organisation cluster to
/// occur at least once, so a requester the run cannot resolve leaves a
/// mandatory node empty and builds no composition the template admits; the
/// run refuses rather than commit a request with no sender.
#[test]
fn an_unresolved_requester_builds_no_kds_laborauftrag_composition() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_laborauftrag_program()?;
    let mut request = kds_service_request()?;
    if let Some(object) = request.as_object_mut() {
        object.remove("contained");
        object.insert(
            String::from("requester"),
            serde_json::json!({ "reference": "Organization/elsewhere" }),
        );
    }
    let Err(error) = inbound(&program, &index, request) else {
        return Err("an unresolved requester is expected to be refused by the template".into());
    };
    assert!(
        matches!(
            error,
            fhirconnect::engine::traverse::error::EngineError::Build { .. }
        ),
        "{error}"
    );
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("'Einsender']: mandatory node is missing"),
        "{rendered}"
    );
    Ok(())
}

/// A requester naming a contained resource of another type is no input of
/// the mapping, which names `Organization`
/// (`types-of-mappings/concept-type/Reference.adoc`, `resourceType`; our own
/// design on the silence, #1888), so the mandatory `Einsender` cluster stays
/// empty and the template admits no composition.
#[test]
fn a_requester_of_another_type_builds_no_kds_laborauftrag_composition() -> Result<(), Box<dyn Error>>
{
    let (program, index) = kds_laborauftrag_program()?;
    let mut request = kds_service_request()?;
    if let Some(contained) = request
        .get_mut("contained")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|contained| contained.first_mut())
        .and_then(serde_json::Value::as_object_mut)
    {
        contained.insert(
            String::from("resourceType"),
            serde_json::Value::String(String::from("Practitioner")),
        );
        contained.remove("identifier");
        contained.remove("name");
    }
    let Err(error) = inbound(&program, &index, request) else {
        return Err("a Practitioner requester leaves the sender empty".into());
    };
    assert!(
        matches!(
            error,
            fhirconnect::engine::traverse::error::EngineError::Build { .. }
        ),
        "{error}"
    );
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("'Einsender']: mandatory node is missing"),
        "{rendered}"
    );
    Ok(())
}

/// Compiling the chain as the library publishes it refuses exactly the rows
/// the snapshot pins:
///
/// - `service_request.v1.yml:34`: `provider` below the COMPOSITION, which the
///   reference model does not define.
/// - `request.v1.ServiceRequest.yml:41, 43, 45, 64, 66, 68`: `null_flavour`
///   below a `DV_DATE_TIME` and a `PARTY_PROXY`, which carry none.
/// - `KDS_laborauftrag.yml:17`: the composition model names the archetype
///   `openEHR-EHR-COMPOSITION.ServiceRequest.v1` and the template carries
///   `openEHR-EHR-COMPOSITION.request.v1`; lines `54, 83`: a string written
///   onto `TERMINOLOGY_ID`; line `108`: an overwrite of `status`, which the
///   model nests under `protocol0008Parent`.
/// - `specimen.v1.yml`: nodes the template's specimen cluster does not
///   carry, attributes `CLUSTER` does not have, and anchors with no child.
#[test]
fn the_kds_laborauftrag_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .chain(&["projects/org.highmed/KDS/laborauftrag/KDS_laborauftrag.context.yaml"])
        .map(|file| published(file))
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_laborauftrag.context")?,
        &kds_laborauftrag_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the laborauftrag chain as published compiles")?;
    insta::assert_json_snapshot!("kds_laborauftrag_chain_as_published", rows(&diagnostics));
    Ok(())
}
