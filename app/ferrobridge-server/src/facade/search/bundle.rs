// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `searchset` Bundle a search answers with.
//!
//! Every entry is a match with a unique `fullUrl` and `search.mode = match`,
//! `total` counts the matches and never the page, `link[self]` names the
//! parameters the search used and `link[next]` the next page, and the Bundle
//! carries an `id` and `meta.lastUpdated`
//! (<https://hl7.org/fhir/R4/bundle.html>, `bdl-7` for the unique `fullUrl`;
//! <https://hl7.org/fhir/R4/search.html>, §3.1.1.5.2, §3.1.1.5.3, §3.1.1.6).
//! The Bundle is built from the generated R4 model, each resource read back
//! through its strict codec, so a rendered resource the model refuses is a
//! `500` and never a malformed entry.

use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::bundle::BundleEntry;
use fhir_types::r4::bundle::BundleEntrySearch;
use fhir_types::r4::bundle::BundleLink;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::resource::Resource;

use crate::facade::identity::FhirResourceId;
use crate::facade::outcome::Issue;
use crate::facade::search::criteria::CURSOR;

/// The `Bundle.type` a search answers.
const SEARCHSET: &str = "searchset";

/// Why the Bundle could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BundleError {
    /// A rendered resource does not read as the R4 model.
    #[error("the rendered {resource_type} does not read as an R4 resource")]
    Resource {
        /// The resource type.
        resource_type: String,
        /// What the codec reported.
        #[source]
        source: fhir_types::codec::DecodeError,
    },
    /// The Bundle could not be encoded.
    #[error("the searchset Bundle could not be encoded")]
    Encode(#[source] fhir_types::codec::EncodeError),
}

/// One match: its logical id and the resource.
#[derive(Debug, Clone)]
pub struct Match {
    /// The logical id.
    pub id: FhirResourceId,
    /// The rendered resource.
    pub resource: Object,
}

/// Everything one searchset carries besides its matches.
#[derive(Debug, Clone)]
pub struct Envelope<'a> {
    /// The `Bundle.id`.
    pub id: &'a str,
    /// The `meta.lastUpdated`, the instant the search ran.
    pub last_updated: &'a str,
    /// The FHIR service base.
    pub base: &'a str,
    /// The resource type searched.
    pub resource_type: &'a str,
    /// The number of matches, when it is known.
    pub total: Option<u64>,
    /// The `self` link.
    pub self_link: String,
    /// The `next` link, when another page follows.
    pub next_link: Option<String>,
    /// The issues an `OperationOutcome` entry reports, when there are any.
    pub outcome: Vec<Issue>,
}

/// Returns the URL of one page of a search.
///
/// The parameters are the ones the search used, in received order, then the
/// page size, then the cursor of the page, each form-encoded
/// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.2).
#[must_use]
pub fn page_url(
    base: &str,
    resource_type: &str,
    used: &[(String, String)],
    count: u32,
    cursor: Option<&str>,
) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in used {
        query.append_pair(name, value);
    }
    query.append_pair("_count", &count.to_string());
    if let Some(cursor) = cursor {
        query.append_pair(CURSOR, cursor);
    }
    format!(
        "{}/{resource_type}?{}",
        base.trim_end_matches('/'),
        query.finish()
    )
}

/// Returns the encoded searchset of `matches` inside `envelope`.
///
/// # Errors
///
/// Returns [`BundleError::Resource`] for a rendered resource the R4 model
/// refuses and [`BundleError::Encode`] when the Bundle does not encode.
pub fn searchset(envelope: &Envelope<'_>, matches: &[Match]) -> Result<Object, BundleError> {
    let base = envelope.base.trim_end_matches('/');
    let mut entry = Vec::with_capacity(matches.len().saturating_add(1));
    for found in matches {
        let resource =
            Resource::from_json(&found.resource, &mut Path::root(envelope.resource_type)).map_err(
                |source| BundleError::Resource {
                    resource_type: envelope.resource_type.to_owned(),
                    source,
                },
            )?;
        entry.push(BundleEntry {
            full_url: Some(
                format!("{base}/{}/{}", envelope.resource_type, found.id.as_str()).into(),
            ),
            resource: Some(resource),
            search: Some(BundleEntrySearch {
                mode: Some("match".into()),
                ..BundleEntrySearch::default()
            }),
            ..BundleEntry::default()
        });
    }
    if !envelope.outcome.is_empty() {
        entry.push(BundleEntry {
            resource: Some(Resource::OperationOutcome(Box::new(
                crate::facade::outcome::outcome(&envelope.outcome),
            ))),
            search: Some(BundleEntrySearch {
                mode: Some("outcome".into()),
                ..BundleEntrySearch::default()
            }),
            ..BundleEntry::default()
        });
    }
    let mut link = vec![BundleLink {
        relation: "self".into(),
        url: envelope.self_link.as_str().into(),
        ..BundleLink::default()
    }];
    if let Some(next) = envelope.next_link.as_deref() {
        link.push(BundleLink {
            relation: "next".into(),
            url: next.into(),
            ..BundleLink::default()
        });
    }
    let bundle = Bundle {
        id: Some(envelope.id.to_owned()),
        meta: Some(Meta {
            last_updated: Some(envelope.last_updated.into()),
            ..Meta::default()
        }),
        r#type: SEARCHSET.into(),
        total: envelope
            .total
            .map(|total| u32::try_from(total).unwrap_or(u32::MAX).into()),
        link,
        entry,
        ..Bundle::default()
    };
    bundle.to_json().map_err(BundleError::Encode)
}

#[cfg(test)]
mod tests {
    use super::page_url;

    #[test]
    fn a_page_url_carries_the_used_parameters_the_size_and_the_cursor() {
        let used = vec![
            (String::from("patient"), String::from("Patient/p 1")),
            (
                String::from("code"),
                String::from("http://snomed.info/sct|44054006"),
            ),
        ];
        assert_eq!(
            "http://ferrobridge.invalid/fhir/Condition?patient=Patient%2Fp+1&code=http%3A%2F%2Fsnomed.info%2Fsct%7C44054006&_count=10&_cursor=0.10.ab",
            page_url(
                "http://ferrobridge.invalid/fhir/",
                "Condition",
                &used,
                10,
                Some("0.10.ab")
            )
        );
    }
}
