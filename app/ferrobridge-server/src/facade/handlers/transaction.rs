// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]`: a `transaction` Bundle, all or nothing, or a `batch`
//! Bundle, which [`super::batch`] answers entry by entry.
//!
//! A transaction Bundle that cannot be mapped in full is refused with one
//! `OperationOutcome` naming every failing entry and nothing is committed
//! (the refusal shape is our own design). So every entry is mapped first, and only a
//! Bundle that mapped in full reaches the CDR, as one CONTRIBUTION, which is
//! what makes the commit itself atomic (`ehr-codegen.openapi.yaml`,
//! `contribution_create`). The mapping and the commit are
//! [`crate::facade::ingest`]'s, run with [`UnmappedEntries::Refuse`] so an
//! entry no program maps refuses the Bundle. A committed Bundle answers a
//! `transaction-response` Bundle naming each entry's resource id and version,
//! and a Bundle an earlier delivery already committed answers the same ids
//! without a second commit. An entry carrying a new `meta.versionId` of a
//! resource the identity map knows commits a later version of its
//! composition inside the same CONTRIBUTION.

use fhir_types::codec::Json as _;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::bundle::BundleEntry;
use fhir_types::r4::bundle::BundleEntryResponse;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;

use crate::auth::Access;
use crate::auth::compartment;
use crate::auth::scope::Permission;
use crate::facade::Facade;
use crate::facade::commit::Change;
use crate::facade::handlers::Body;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::read;
use crate::facade::ingest::Committed;
use crate::facade::ingest::EntryOutcome;
use crate::facade::ingest::Ingested;
use crate::facade::ingest::Provenance;
use crate::facade::ingest::UnmappedEntries;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::request::Inbound;

/// The `Bundle.type` this route commits.
const TRANSACTION: &str = "transaction";

/// The `Bundle.type` [`super::batch`] answers.
const BATCH: &str = "batch";

/// The `Bundle.type` a committed transaction answers with.
const RESPONSE: &str = "transaction-response";

/// `POST [base]`.
pub(crate) async fn transaction(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Present)?;
    let bundle = read::parse(body, "Bundle")?;
    let kind = bundle
        .document()
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind == BATCH {
        return super::batch::batch(facade, access, headers, bundle.document()).await;
    }
    if kind != TRANSACTION {
        return Err(reply::refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::NotSupported)
                .diagnosing(format!(
                    "Bundle.type is `{kind}`; this route answers a transaction or a batch Bundle"
                ))
                .at("Bundle.type"),
        ));
    }
    admit_entries(facade, access, bundle.document())?;
    let client = facade.client_for(headers);
    let resolved =
        crate::facade::references::resolve_bundle(facade, &client, bundle.document()).await;
    let bundled = fhirconnect::engine::seam::BundleReferences::new(
        crate::facade::references::entries_of(bundle.document()),
        &resolved,
    );
    let ingested = facade
        .ingest(client)
        .with_references(&bundled)
        .ingest_bundle(
            bundle.document(),
            UnmappedEntries::Refuse,
            &Provenance::EachResource,
        )
        .await?;
    let response = response_bundle(&facade.settings().base_url, &ingested);
    let body = response.to_json().map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the transaction-response Bundle could not be encoded: {error}"
            )),
        )
    })?;
    Ok(reply::resource(StatusCode::OK, &body, &[]))
}

/// Checks every entry of `bundle` against `access`, before anything is
/// mapped or committed.
///
/// A transaction "SHALL be processed as a single unit" and fails whole when
/// any entry fails (<https://hl7.org/fhir/R4/http.html#transaction>), so one
/// entry the token does not cover refuses the Bundle with the guard's `403`,
/// which names neither the entry nor the scope. Each entry needs the
/// permission of the method its `request` names, `create` for a `POST` and a
/// request-less entry, and a `patient/` grant reaches only an entry whose
/// subject is the patient in the launch context.
fn admit_entries(facade: &Facade, access: &Access, bundle: &Value) -> Result<(), Refusal> {
    let entries = bundle
        .get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default();
    for entry in entries {
        let method = entry
            .get("request")
            .and_then(|request| request.get("method"))
            .and_then(Value::as_str)
            .unwrap_or("POST");
        let permission = match method {
            "GET" | "HEAD" => Permission::Read,
            "PUT" | "PATCH" => Permission::Update,
            "DELETE" => Permission::Delete,
            _ => Permission::Create,
        };
        // NOTE: an entry with no readable resource refuses the whole Bundle
        // in the mapping and commits nothing (R4 http.html#transaction), so it
        // is legitimately skipped here rather than authorised.
        let Some(Ok(inbound)) = entry
            .get("resource")
            .map(|resource| Inbound::of(resource.clone(), ""))
        else {
            continue;
        };
        compartment::admit_inbound(facade, access, permission, &inbound)?;
    }
    Ok(())
}

/// Returns the `transaction-response` Bundle of one ingested transaction.
///
/// "the server SHALL return a Bundle with type set to … `transaction-response`
/// that contains one entry for each entry in the request, in the same order",
/// each with a `response` carrying the status, `location` and `ETag`
/// (<https://hl7.org/fhir/R4/http.html#transaction-response>). The location and
/// the tag are the ones a single create answers. An entry this delivery
/// committed as a first version answers `201 Created`; one it committed as a
/// later version of a known resource answers `200 OK`, as a single create of
/// that resource does, and so does one an earlier delivery consumed. An entry
/// another entry's mapping consumed through a `slotContext` is content of that
/// entry's composition (`engine/bundles.adoc`, §Resources consumed by a
/// sub-context), so it answers `200 OK` with no location of its own: no
/// specification governs this: our own design.
fn response_bundle(base_url: &str, ingested: &Ingested) -> Bundle {
    let base = base_url.trim_end_matches('/');
    Bundle {
        r#type: RESPONSE.into(),
        entry: ingested
            .entries()
            .iter()
            .filter_map(|outcome| match outcome {
                EntryOutcome::Committed(entry) => Some(committed_entry(base, entry)),
                EntryOutcome::Consumed(_) => Some(BundleEntry {
                    response: Some(BundleEntryResponse {
                        status: "200 OK".into(),
                        ..BundleEntryResponse::default()
                    }),
                    ..BundleEntry::default()
                }),
                EntryOutcome::Skipped(_) => None,
            })
            .collect(),
        ..Bundle::default()
    }
}

/// Returns the response entry of one committed entry.
fn committed_entry(base: &str, entry: &Committed) -> BundleEntry {
    BundleEntry {
        response: Some(BundleEntryResponse {
            status: match entry.change {
                Some(Change::Creation) => "201 Created",
                Some(Change::Modification) | None => "200 OK",
            }
            .into(),
            location: Some(
                format!(
                    "{base}/{}/{}/_history/{}",
                    entry.resource_type,
                    entry.id,
                    entry.version.version_tree_id().value()
                )
                .into(),
            ),
            etag: Some(format!("W/\"{}\"", entry.version.version_tree_id().value()).into()),
            ..BundleEntryResponse::default()
        }),
        ..BundleEntry::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{BATCH, TRANSACTION};

    #[test]
    fn the_two_bundle_types_this_route_distinguishes_are_the_r4_codes() {
        assert_eq!("transaction", TRANSACTION);
        assert_eq!("batch", BATCH);
    }
}
