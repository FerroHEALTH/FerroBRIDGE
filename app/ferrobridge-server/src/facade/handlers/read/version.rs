// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composition versions a read takes from the CDR: the latest, the one an
//! `ETag` or an update names, and the canonical form each is read in.

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::cdr::ids::entity_tag;
use crate::facade::Facade;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::write;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::status;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_base::v1_3::base_types::identification::version_tree_id::VersionTreeId;
use openehr_its::rest::generated::ehr::client::CompositionGetOutcome;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

/// Returns the `410` of a composition the CDR reports deleted.
fn reported_deleted() -> Refusal {
    write::refuse(&status::Answer::new(
        status::GONE,
        String::from("the CDR reports this composition deleted"),
    ))
}

/// Fetches the latest version of one composition.
pub(crate) async fn fetch(
    facade: &Facade,
    client: &CdrClient,
    ehr_id: &EhrId,
    container: &HierObjectId,
    template_id: &str,
) -> Result<(ObjectVersionId, CanonicalComposition), Refusal> {
    let (version, composition) = latest(client, ehr_id, container).await?;
    Ok((
        version,
        canonical(facade, ehr_id, &composition, template_id)?,
    ))
}

/// Returns the canonical form of one composition the CDR served from the EHR
/// `ehr_id`, which the access the request makes reached.
pub(crate) fn canonical(
    facade: &Facade,
    ehr_id: &EhrId,
    composition: &Composition,
    template_id: &str,
) -> Result<CanonicalComposition, Refusal> {
    crate::cdr::reach::touched(
        crate::cdr::reach::Touch::Read,
        Some(ehr_id),
        crate::cdr::reach::composition(composition, Some(template_id)),
    );
    let text = openehr_its::json::to_canonical_json(composition);
    let value = text.parse::<serde_json::Value>().map_err(|error| {
        reply::refusal(
            StatusCode::BAD_GATEWAY,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the composition the CDR served could not be read back: {error}"
            )),
        )
    })?;
    let generation = facade
        .programs()
        .loaded()
        .iter()
        .find(|entry| entry.index().template_id() == template_id)
        .map_or(openehr_mapping_core::template::Generation::Adl14, |entry| {
            entry.index().generation()
        });
    Ok(CanonicalComposition::new(value, template_id, generation))
}

/// Reads the latest version of the composition `container` holds, with the
/// version its `ETag` names.
pub(crate) async fn latest(
    client: &CdrClient,
    ehr_id: &EhrId,
    container: &HierObjectId,
) -> Result<(ObjectVersionId, Composition), Refusal> {
    let (version, body) =
        answered_version(client, ehr_id, &UidBasedId::HierObjectId(container.clone())).await?;
    let version = version.ok_or_else(|| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception)
                .diagnosing("the CDR answered a composition with no ETag"),
        )
    })?;
    Ok((version, body))
}

/// Reads the composition version `uid_based_id` names, with the version its
/// `ETag` names when the CDR sent one.
///
/// A `204` is the CDR reporting the version deleted, which is `410`
/// (`ehr-codegen.openapi.yaml`, `composition_get`).
pub(super) async fn answered_version(
    client: &CdrClient,
    ehr_id: &EhrId,
    uid_based_id: &UidBasedId,
) -> Result<(Option<ObjectVersionId>, Composition), Refusal> {
    let answered = client
        .composition(ehr_id, uid_based_id, None)
        .await
        .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
    match answered.outcome {
        CompositionGetOutcome::Ok { body, headers } => {
            let version =
                crate::cdr::optional_version_from_etag("composition_get", headers.etag.as_deref())
                    .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
            Ok((version, body))
        }
        CompositionGetOutcome::NoContent => Err(reported_deleted()),
        CompositionGetOutcome::NotFound { body } => Err(write::refuse(&status::Answer::new(
            status::NOT_FOUND,
            status::diagnostics(StatusCode::NOT_FOUND, &body),
        ))),
    }
}

/// Reads the composition version an update follows, which the CDR must hold.
///
/// A version the CDR does not hold is no version the write can follow, so it
/// is `412` as a stale `If-Match` is
/// (<https://hl7.org/fhir/R4/http.html#concurrency>).
///
/// # Errors
///
/// A `412` for a version the CDR does not hold, a `410` for one it reports
/// deleted, and every client failure through the status table.
pub(crate) async fn followed_version(
    client: &CdrClient,
    ehr_id: &EhrId,
    version: &ObjectVersionId,
) -> Result<Composition, Refusal> {
    let answered = client
        .composition(ehr_id, &UidBasedId::ObjectVersionId(version.clone()), None)
        .await
        .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
    match answered.outcome {
        CompositionGetOutcome::Ok { body, .. } => Ok(body),
        CompositionGetOutcome::NoContent => Err(reported_deleted()),
        CompositionGetOutcome::NotFound { body } => Err(write::refuse(&status::Answer::new(
            status::PRECONDITION_FAILED,
            format!(
                "If-Match names version {}, which the CDR does not hold: {}",
                version.value(),
                status::diagnostics(StatusCode::NOT_FOUND, &body)
            ),
        ))),
    }
}

/// Returns the openEHR version an `If-Match` names, checked against the latest
/// version `container` holds, with that version's composition when the check
/// read it.
///
/// R4 sends the `versionId` as a weak entity tag ("`If-Match: W/"2"`",
/// <https://hl7.org/fhir/R4/http.html#concurrency>), and ITS-REST takes the
/// whole `OBJECT_VERSION_ID`. A version tree id (`W/"N"`, `"N"` or `N`) that
/// names the latest version completes to it, as vread completes its `[vid]`;
/// one that names any other version is `412` carrying the current `ETag`. The
/// CDR's own `uid::system::N` of `container` passes through for the CDR to
/// check, and one of another container is `412`.
///
/// # Errors
///
/// A `400` for a value that names no version at all, a `412` for a stale or
/// foreign version, and the refusal the read of the latest version answers.
pub(crate) async fn version_of_etag(
    client: &CdrClient,
    ehr_id: &EhrId,
    container: &HierObjectId,
    value: &str,
) -> Result<(ObjectVersionId, Option<Composition>), Refusal> {
    let bare = entity_tag(value);
    if let Ok(full) = ObjectVersionId::new(bare) {
        if full.object_id().value() != container.value() {
            return Err(write::refuse(&status::Answer::new(
                status::PRECONDITION_FAILED,
                format!(
                    "If-Match carries `{bare}`, a version of another composition than {}",
                    container.value()
                ),
            )));
        }
        return Ok((full, None));
    }
    // NOTE: a parse failure IS the answer here: a value that is no
    // OBJECT_VERSION_ID is the R4 spelling, which names the version tree id
    // alone (<https://hl7.org/fhir/R4/http.html#concurrency>).
    let tree = VersionTreeId::new(bare).map_err(|error| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid).diagnosing(format!(
                "If-Match carries `{bare}`, which is neither a versionId nor an openEHR version id: {}",
                crate::cause::chain(&error)
            )),
        )
    })?;
    let (current, composition) = latest(client, ehr_id, container).await?;
    let current_tree = current.version_tree_id();
    if current_tree.value() == tree.value() {
        return Ok((current, Some(composition)));
    }
    Err(write::refuse(
        &status::Answer::new(
            status::PRECONDITION_FAILED,
            format!(
                "If-Match names version {}, and the current version is {}",
                tree.value(),
                current_tree.value()
            ),
        )
        .with_entity_tag(current_tree.value()),
    ))
}
