// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Turning a stored composition into the resource the facade answers with.
//!
//! The engine maps the composition back to FHIR, and this module adds the
//! facts the engine has no way to know: the logical id the identity map
//! holds, the `meta.versionId` the CDR's version tree names, the
//! `meta.source` that points at the composition version the content came
//! from, and the subject the identity map binds to the composition's EHR when
//! the mapping wrote none (no specification governs the identity: our own
//! design). Every resource the run creates beside it is bound under an id of
//! its own, and [`render_created`] answers one of them. The answer is then
//! checked against the required elements of the element table, so no
//! interaction answers an invalid instance.

use crate::cdr::ids::EhrId;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhirconnect::engine::demographics::Demographics;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::DerivedIdentity;
use fhirconnect::resolve::program::Program;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use url::Url;

use crate::facade::Facade;
use crate::facade::engine;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::complete;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::created::CreatedIdentities;
use crate::facade::identity::store::Store;
use crate::facade::ingest::Refused;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::status;
use crate::terminology::{Direction, Session};
use crate::usage_report::window::Window;

/// What one rendered resource carries beside its body.
#[derive(Debug, Clone)]
pub(crate) struct Rendered {
    /// The resource, ready for the wire.
    pub(crate) body: Object,
    /// The losses the run declared, as issues a caller may report.
    pub(crate) warnings: Vec<Warning>,
    /// The element paths the facade filled from the identity binding.
    pub(crate) filled: Vec<&'static str>,
}

/// Returns the subject the identity map binds to `ehr_id`, as the reference
/// a rendered resource names it by, when the map knows one.
///
/// # Errors
///
/// The refusal an identity-store failure renders as.
pub(crate) fn subject_of(facade: &Facade, ehr_id: &EhrId) -> Result<Option<Value>, Refusal> {
    let person = facade
        .store()
        .person_of(ehr_id)
        .map_err(|error| crate::facade::handlers::write::store_refusal(&error))?;
    Ok(person.map(|person| complete::reference_to(&person, &facade.settings().subject)))
}

/// Where one rendered resource comes from: the composition version, the
/// logical id the map holds, and the subject the map binds to its EHR.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Origin<'a> {
    /// The logical id the identity map holds for the entry's own resource.
    pub(crate) id: &'a FhirResourceId,
    /// The composition version the content came from.
    pub(crate) version: &'a ObjectVersionId,
    /// The ITS-REST URL of that version.
    pub(crate) source: &'a Url,
    /// The subject reference [`subject_of`] answered, when the map knows one.
    pub(crate) subject: Option<&'a Value>,
    /// The identity map every resource the run creates is bound in.
    pub(crate) store: &'a dyn Store,
    /// The usage report's window an engine refusal of the run is counted in.
    pub(crate) usage: &'a Window,
    /// The demographics endpoint every party reference resolves through.
    pub(crate) demographics: &'a dyn Demographics,
}

/// Returns the resource `composition` maps to, decorated with its identity
/// and checked against the required elements R4 gives its type.
///
/// Every resource the run creates beside it is bound in the identity map
/// under an id of its own and stays out of the body: the reference the
/// mapping wrote names it as `<Type>/<id>`, which a read answers
/// ([`render_created`]). A contained resource "does not have an independent
/// existence apart from the resource that contains it"
/// (<https://hl7.org/fhir/R4/references.html#contained>), so none is
/// contained.
///
/// # Errors
///
/// The engine's refusal, and the `500` of [`complete::refusal`] naming every
/// required element neither the mapping nor the binding supplies.
pub(crate) async fn render(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    origin: Origin<'_>,
    session: &Session<'_>,
) -> Result<Rendered, Refusal> {
    let resource_type = program.resource().as_str();
    let (value, _created, warnings) =
        produce(program, index, composition, &origin, session).await?;
    let body = resource_object(value, resource_type)?;
    decorate(
        body,
        resource_type,
        origin.id,
        &origin,
        Some(program.profile().url().as_str()),
        warnings,
    )
}

/// Returns the resource of type `resource_type` and id `id` the outbound run
/// over `composition` creates, decorated as [`render`] decorates the entry's
/// own resource.
///
/// The created resource claims no profile, because the context maps the
/// parent's (no specification governs this: our own design).
///
/// # Errors
///
/// A `404` when the run no longer creates that resource, the engine's
/// refusal, and the `500` of [`complete::refusal`].
pub(crate) async fn render_created(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    origin: Origin<'_>,
    target: (&str, &FhirResourceId),
    session: &Session<'_>,
) -> Result<Rendered, Refusal> {
    let (resource_type, id) = target;
    let (_value, created, warnings) =
        produce(program, index, composition, &origin, session).await?;
    let found = created.into_iter().find(|resource| {
        resource.get("resourceType").and_then(Value::as_str) == Some(resource_type)
            && resource.get("id").and_then(Value::as_str) == Some(id.as_str())
    });
    let Some(Value::Object(body)) = found else {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!(
                "{resource_type}/{id} names a resource the composition version no longer creates"
            )),
        ));
    };
    decorate(body, resource_type, id, &origin, None, warnings)
}

/// Returns every resource of the program's own type the outbound run over
/// `composition` answers: the entry's own resource first, then each one the
/// run creates beside it (a further `hierarchy.split` occurrence), in the
/// order the run creates them, each decorated as [`render`] decorates the
/// entry's own.
///
/// A search lists all of them, because each is a resource of the searched
/// type this server serves (no specification governs how a created resource
/// is reached from a row: our own design). The declared losses of the run
/// travel with the entry's own resource alone.
///
/// # Errors
///
/// The engine's refusal, the `500` of a created resource whose id is outside
/// the R4 grammar, and the `500` of [`complete::refusal`].
pub(crate) async fn render_units(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    origin: Origin<'_>,
    session: &Session<'_>,
) -> Result<Vec<(FhirResourceId, Rendered)>, Refusal> {
    let resource_type = program.resource().as_str();
    let (value, created, warnings) = produce(program, index, composition, &origin, session).await?;
    let body = resource_object(value, resource_type)?;
    let own = decorate(
        body,
        resource_type,
        origin.id,
        &origin,
        Some(program.profile().url().as_str()),
        warnings,
    )?;
    let mut units = vec![(origin.id.clone(), own)];
    for resource in created {
        if resource.get("resourceType").and_then(Value::as_str) != Some(resource_type) {
            continue;
        }
        let Value::Object(body) = resource else {
            continue;
        };
        let text = body.get("id").and_then(Value::as_str).unwrap_or_default();
        let id = FhirResourceId::new(text).map_err(|error| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "a {resource_type} the mapping created carries an id outside the R4 grammar: {error}"
                )),
            )
        })?;
        let rendered = decorate(body, resource_type, &id, &origin, None, Vec::new())?;
        units.push((id, rendered));
    }
    Ok(units)
}

/// Returns the resource `composition` maps to and every resource the
/// outbound run creates beside it, of any type, in the order the run creates
/// them, each decorated as [`render`] and [`render_created`] decorate them:
/// the parts of a `Composition/$document` answer ([`crate::facade::document`]).
///
/// # Errors
///
/// The engine's refusal, the `500` of a created resource whose id is outside
/// the R4 grammar, and the `500` of [`complete::refusal`].
pub(crate) async fn render_document(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    origin: Origin<'_>,
    session: &Session<'_>,
) -> Result<(Rendered, Vec<Rendered>), Refusal> {
    let resource_type = program.resource().as_str();
    let (value, created, warnings) = produce(program, index, composition, &origin, session).await?;
    let body = resource_object(value, resource_type)?;
    let own = decorate(
        body,
        resource_type,
        origin.id,
        &origin,
        Some(program.profile().url().as_str()),
        warnings,
    )?;
    let mut parts = Vec::with_capacity(created.len());
    for resource in created {
        let Value::Object(body) = resource else {
            continue;
        };
        let kind = body
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let text = body.get("id").and_then(Value::as_str).unwrap_or_default();
        let id = FhirResourceId::new(text).map_err(|error| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "a {kind} the mapping created carries an id outside the R4 grammar: {error}"
                )),
            )
        })?;
        parts.push(decorate(body, &kind, &id, &origin, None, Vec::new())?);
    }
    Ok((own, parts))
}

/// Runs the outbound mapping, binding every resource it creates under the
/// entry's own resource.
///
/// The `conceptmap` codes are resolved by probe runs that bind nothing
/// ([`DerivedIdentity`]), so only the one final run writes to the identity
/// map.
async fn produce(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    origin: &Origin<'_>,
    session: &Session<'_>,
) -> Result<(Value, Vec<Value>, Vec<Warning>), Refusal> {
    let resolved = session
        .resolve(Direction::ToFhir, |terminology| {
            engine::outbound(
                program,
                index,
                composition,
                &DerivedIdentity,
                terminology,
                origin.demographics,
            )
        })
        .await;
    let identities = CreatedIdentities::new(origin.store, program.resource().as_str(), origin.id);
    let produced = engine::outbound(
        program,
        index,
        composition,
        &identities,
        &resolved,
        origin.demographics,
    )
    .map_err(|error| {
        origin.usage.record_mapping_failure();
        Refusal::from(Refused::of_answer(&status::of_outbound_refusal(&error)))
    })?;
    let warnings = produced.warnings().to_vec();
    let (value, created) = produced.into_parts();
    Ok((value, created, warnings))
}

/// Writes the identity of one rendered resource onto `body` and checks it
/// against the required elements R4 gives `resource_type`.
fn decorate(
    mut body: Object,
    resource_type: &str,
    id: &FhirResourceId,
    origin: &Origin<'_>,
    profile: Option<&str>,
    warnings: Vec<Warning>,
) -> Result<Rendered, Refusal> {
    let filled: Vec<&'static str> =
        complete::fill_subject(&mut body, resource_type, origin.subject)
            .into_iter()
            .collect();
    body.insert(String::from("id"), Value::String(String::from(id.as_str())));
    let mut meta = body
        .get("meta")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    meta.insert(
        String::from("versionId"),
        Value::String(String::from(origin.version.version_tree_id().value())),
    );
    // NOTE: `Meta.source` is "a uri that identifies the source system"
    // (<https://hl7.org/fhir/R4/resource.html#Meta>), so the openEHR version
    // uid travels as the ITS-REST URL of that exact version.
    meta.insert(
        String::from("source"),
        Value::String(origin.source.to_string()),
    );
    if let Some(profile) = profile {
        meta.insert(
            String::from("profile"),
            Value::Array(vec![Value::String(String::from(profile))]),
        );
    }
    body.insert(String::from("meta"), Value::Object(meta));
    let absent = complete::absent_required(&body, resource_type);
    if !absent.is_empty() {
        tracing::error!(
            resource_type,
            absent = absent.join(", "),
            "the rendered resource lacks elements R4 requires"
        );
        return Err(Refusal::from(complete::refusal(resource_type, &absent)));
    }
    Ok(Rendered {
        body,
        warnings,
        filled,
    })
}

/// Returns the resource object an outbound run produced.
///
/// The value is mapped clinical content, so a refusal names only its shape:
/// the resource type the program maps and the JSON kind the engine produced.
/// The status is the table's [`status::INTERNAL`] row, because the engine is
/// the bridge's own.
fn resource_object(value: Value, resource_type: &str) -> Result<Object, Refusal> {
    let kind = match value {
        Value::Object(object) => return Ok(object),
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
    };
    Err(Refusal::from(Refused::of_answer(&status::Answer::new(
        status::INTERNAL,
        format!("the engine produced {kind} where a {resource_type} resource object belongs"),
    ))))
}
/// Records what the facade filled on one rendered resource and the declared
/// set of losses its outbound run took.
///
/// A fill is counted with the element paths it wrote. A warning is a loss the
/// specification itself declares, never a swallowed failure, and is logged as
/// its [`loss_kind`] and its location, never its display text, which can
/// hold a reference literal. Neither line holds clinical content.
pub(crate) fn log_outcome(rendered: &Rendered) {
    if !rendered.filled.is_empty() {
        tracing::info!(
            filled = rendered.filled.len(),
            elements = rendered.filled.join(", "),
            "the facade filled required elements from the identity binding"
        );
    }
    if rendered.warnings.is_empty() {
        return;
    }
    tracing::debug!(
        declared = rendered.warnings.len(),
        losses = rendered
            .warnings
            .iter()
            .map(located)
            .collect::<Vec<String>>()
            .join("; "),
        "the outbound mapping declared losses"
    );
}

/// Returns the value-free label of one declared loss.
///
/// The label names the variant of [`Loss`], and of [`SkipReason`] for a
/// skipped mapping, and carries none of the strings either holds. No
/// specification governs the labels: our own design.
pub(crate) const fn loss_kind(loss: &Loss) -> &'static str {
    match *loss {
        Loss::Skipped { ref reason, .. } => match *reason {
            SkipReason::Unidirectional => "skipped-unidirectional",
            SkipReason::PreprocessorGate { .. } => "skipped-preprocessor-gate",
            SkipReason::UnresolvedReference { .. } => "skipped-unresolved-reference",
            SkipReason::EmptyReference => "skipped-empty-reference",
            SkipReason::UnmappedLink { .. } => "skipped-unmapped-link",
            SkipReason::TermMappingMatch { .. } => "skipped-term-mapping-match",
            SkipReason::TermMappingUncarried { .. } => "skipped-term-mapping-uncarried",
            SkipReason::DefiningCodeUncarried { .. } => "skipped-defining-code-uncarried",
            SkipReason::Untranslated { .. } => "skipped-untranslated",
            SkipReason::EhrStatusSubject => "skipped-ehr-status-subject",
            SkipReason::EventFunctionUncarried { .. } => "skipped-event-function-uncarried",
            SkipReason::NullReasonUncarried => "skipped-null-reason-uncarried",
        },
        Loss::Defaulted { .. } => "defaulted",
        Loss::LastOfMany { .. } => "last-of-many",
        Loss::OneWayFallback { .. } => "one-way-fallback",
    }
}

/// Returns one warning as its kind and the paths of its location, for the
/// loss log.
fn located(warning: &Warning) -> String {
    let location = warning.location();
    format!(
        "{} fhir={} openehr={}",
        loss_kind(warning.loss()),
        location.fhir().unwrap_or("-"),
        location.openehr().unwrap_or("-")
    )
}

/// Returns the ITS-REST URL of one composition version.
pub(crate) fn composition_url(
    base: &Url,
    ehr_id: &EhrId,
    version: &ObjectVersionId,
) -> Result<Url, Refusal> {
    base.join(&format!(
        "ehr/{}/composition/{}",
        ehr_id.as_str(),
        version.value()
    ))
    .map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the composition URL could not be built from the configured CDR base: {error}"
            )),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{Rendered, composition_url, log_outcome, resource_object};
    use crate::cdr::ids::EhrId;
    use fhir_types::codec::Value;
    use fhirconnect::engine::outcome::Location;
    use fhirconnect::engine::outcome::Loss;
    use fhirconnect::engine::outcome::SkipReason;
    use fhirconnect::engine::outcome::Warning;
    use http::StatusCode;
    use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

    /// A synthetic value no refusal may carry onto the wire.
    const MARKER: &str = "ferrobridge-synthetic-clinical-marker";

    #[tokio::test]
    async fn an_output_that_is_no_object_is_refused_by_its_shape_alone() {
        let produced = Value::Array(vec![Value::String(String::from(MARKER))]);
        let refusal =
            resource_object(produced, "Condition").expect_err("an array is no resource object");
        let response = refusal.into_response();
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, response.status());
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("the refusal body reads");
        let text = String::from_utf8(bytes.to_vec()).expect("the body is UTF-8");
        assert!(
            text.contains("the engine produced an array where a Condition resource object belongs"),
            "{text}"
        );
        assert!(
            !text.contains(MARKER),
            "the produced value reached the wire: {text}"
        );
    }

    #[test]
    fn the_loss_log_carries_kinds_and_locations_and_no_reference_literal() {
        let warnings = vec![
            Warning::new(
                Loss::Skipped {
                    mapping: String::from("asserter"),
                    reason: SkipReason::UnresolvedReference {
                        reference: String::from("Patient/123"),
                    },
                },
                Location::default()
                    .with_fhir("Condition.asserter")
                    .with_openehr("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"),
            ),
            Warning::new(
                Loss::Skipped {
                    mapping: String::from("recorder"),
                    reason: SkipReason::UnresolvedReference {
                        reference: String::from("Practitioner/Synthetic-Marker"),
                    },
                },
                Location::default().with_fhir("Condition.recorder"),
            ),
        ];
        let rendered = Rendered {
            body: fhir_types::codec::Object::default(),
            warnings,
            filled: Vec::new(),
        };
        let logs = Capture::default();
        let subscriber = crate::telemetry::subscriber(
            crate::telemetry::Rendering::Json,
            "debug",
            false,
            logs.clone(),
        );
        tracing::subscriber::with_default(subscriber, || log_outcome(&rendered));
        let text = logs.text();
        assert!(
            text.contains("declared losses"),
            "the loss line is written: {text}"
        );
        assert!(
            text.contains("skipped-unresolved-reference fhir=Condition.asserter"),
            "{text}"
        );
        assert!(
            text.contains("skipped-unresolved-reference fhir=Condition.recorder"),
            "{text}"
        );
        assert!(
            !text.contains("Patient/123"),
            "a reference literal reached the log: {text}"
        );
        assert!(
            !text.contains("Synthetic-Marker"),
            "a display name reached the log: {text}"
        );
    }

    /// A `tracing` writer that keeps every line in memory.
    #[derive(Debug, Clone, Default)]
    struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("the capture lock holds")).into_owned()
        }
    }

    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("the capture lock holds")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn an_object_output_passes_through() {
        let Value::Object(object) = Value::from_serde_json(serde_json::json!({
            "resourceType": "Condition"
        })) else {
            panic!("the literal is an object");
        };
        let passed = resource_object(Value::Object(object.clone()), "Condition")
            .expect("an object is the resource");
        assert_eq!(object, passed);
    }

    #[test]
    fn the_source_url_names_the_ehr_and_the_exact_version() {
        let base: url::Url = "http://cdr.invalid/openehr/v1/"
            .parse()
            .expect("a legal base URL");
        let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11").expect("a legal ehr_id");
        let version = ObjectVersionId::new("8849182c-82ad-4088-a07f-48ead4180515::ferroehr::2")
            .expect("a legal version id");
        let built = composition_url(&base, &ehr, &version).expect("the URL builds");
        assert_eq!(
            "http://cdr.invalid/openehr/v1/ehr/bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11/composition/8849182c-82ad-4088-a07f-48ead4180515::ferroehr::2",
            built.as_str()
        );
    }
}
