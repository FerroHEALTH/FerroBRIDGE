// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals the service answers with for what the CDR, the EHR
//! resolution and the identity map refuse.

use fhirconnect::engine::traverse::error::EngineError;
use http::StatusCode;

use crate::cdr::error::CdrError;
use crate::cdr::ids::ContributionUid;
use crate::facade::ehr;
use crate::facade::engine;
use crate::facade::identity::store::StoreError;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;
use crate::facade::status;

use super::Mapped;
use super::Refused;

/// Returns the refusal a CDR call that reached no usable documented answer
/// renders as, through the status table.
pub(super) fn cdr_refusal(error: &CdrError) -> Refused {
    Refused::of_answer(&status::of_client_error(error))
}

/// Returns the refusal a documented CDR `answered` refusal with `body` decides
/// through `row`.
pub(super) fn upstream_refusal(
    row: status::Row,
    answered: StatusCode,
    body: &openehr_its::rest::client::ErrorBody,
) -> Refused {
    Refused::of_answer(&status::Answer::new(
        row,
        status::diagnostics(answered, body),
    ))
}

/// Returns the refusal a rejected contribution renders as.
///
/// Nothing was committed, so the answer names every entry the Bundle mapped:
/// a caller cannot tell from a partial list which entries still stand.
pub(super) fn refuse_all(mapped: &[Mapped], row: &status::Row, detail: &str) -> Refused {
    let mut issues = vec![
        Issue::error(row.issue())
            .diagnosing(detail.to_owned())
            .detailing("the CDR refused the contribution, so no entry of this Bundle is stored"),
    ];
    for entry in mapped {
        issues.push(
            Issue::error(row.issue())
                .diagnosing(detail.to_owned())
                .at(entry.full_url.clone()),
        );
    }
    Refused::of(row.status(), issues)
}

/// Returns the refusal an EHR resolution of a single write renders as.
pub(crate) fn ehr_refusal(error: &ehr::EhrError) -> Refused {
    match *error {
        ehr::EhrError::Absent { .. } => Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::NotFound).diagnosing(chain(error)),
        ),
        ehr::EhrError::Refused { .. } | ehr::EhrError::Subject { .. } => Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::Processing).diagnosing(chain(error)),
        ),
        ehr::EhrError::Client { ref source } => {
            Refused::of_answer(&status::of_client_error(source))
        }
        _ => Refused::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(chain(error)),
        ),
    }
}

/// Returns the refusal a malformed stored openEHR identifier renders as.
pub(super) fn stored_identifier(error: &impl std::fmt::Display) -> Refused {
    Refused::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception).diagnosing(format!(
            "the identity map holds an openEHR identifier this version cannot read: {error}"
        )),
    )
}

/// The refusal a composition whose entries cannot be named renders as: a
/// `500`, since the template the program was compiled against cannot answer
/// for its own start archetype (no specification governs this: our own
/// design).
impl From<engine::entry::EntryError> for Refused {
    fn from(error: engine::entry::EntryError) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(chain(&error)),
        )
    }
}

/// Returns the refusal of a committed contribution whose answer cannot bind
/// its entries, naming the contribution so it can be reconciled.
pub(super) fn unbound(contribution: &ContributionUid, why: &str) -> Refused {
    Refused::of_answer(&status::Answer::new(
        status::UNDOCUMENTED,
        format!(
            "the CDR committed contribution {contribution} and {why}, so its entries cannot be bound to their compositions"
        ),
    ))
}

/// Returns the refusal one engine error renders as.
///
/// An element the program cannot map refuses the unit (no specification
/// governs this: our own design), so the answer is a `422` whose diagnostics
/// name the mapping and the element the engine refused at.
///
/// A run refused because a referenced resource could not be fetched
/// ([`crate::facade::references::Unfetched`]) failed upstream, so it answers
/// the [`crate::facade::status::UPSTREAM`] row, a `502` with `transient`, and
/// so does a run refused because the terminology server failed
/// ([`crate::terminology::failed_upstream`]); the diagnostics carry the
/// upstream status the failure names.
#[must_use]
pub(crate) fn engine_refusal(error: &EngineError) -> Refused {
    let row = status::UPSTREAM;
    if crate::facade::references::unfetched(error).is_some() {
        return Refused::new(
            row.status(),
            Issue::error(row.issue())
                .diagnosing(chain(error))
                .detailing("a resource the mapping reads could not be fetched"),
        );
    }
    if crate::terminology::failed_upstream(error) {
        return Refused::new(
            row.status(),
            Issue::error(row.issue())
                .diagnosing(chain(error))
                .detailing("the terminology server failed while the mapping ran"),
        );
    }
    Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        Issue::error(IssueType::Processing)
            .diagnosing(chain(error))
            .detailing("the mapping refused this resource"),
    )
}

/// Returns the refusal an identity-store failure renders as.
#[must_use]
pub(crate) fn store_refusal(error: &StoreError) -> Refused {
    Refused::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception).diagnosing(chain(&error)),
    )
}
