// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/sources`, one source with its versions, and the body a
//! create or a change of a source sends.
//!
//! A source's `document` is the kind's own JSON object, which the bridge
//! validates on every write; it names its secrets (`{"secret": "<name>"}`)
//! and never holds a secret's value.

use serde::{Deserialize, Serialize};

/// The kind of an inbound source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// An HTTPS webhook, such as Nedap Ons.
    Webhook,
    /// A FHIR R4 Subscription notification channel.
    FhirSubscription,
    /// An HL7 v2 sender over MLLP.
    Mllp,
}

impl Kind {
    /// Returns the kind as the store and the API write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::FhirSubscription => "fhir-subscription",
            Self::Mllp => "mllp",
        }
    }
}

/// One source at its head version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The source's id.
    pub id: String,
    /// The source's name, unique.
    pub name: String,
    /// The kind.
    pub kind: Kind,
    /// Whether the source is served.
    pub enabled: bool,
    /// The head version, which the source's `ETag` names.
    pub version: u64,
    /// The kind's document.
    pub document: serde_json::Value,
    /// Who wrote the head version: the subject of the operator's token.
    pub updated_by: String,
    /// When, RFC 3339 UTC.
    pub updated_at: String,
}

/// Every source, by name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    /// The sources.
    pub sources: Vec<Source>,
}

/// One version of a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersion {
    /// The version.
    pub version: u64,
    /// The name it carried.
    pub name: String,
    /// Whether it was served.
    pub enabled: bool,
    /// The document, absent on the version that records the deletion.
    pub document: Option<serde_json::Value>,
    /// Whether this version deleted the source.
    pub deleted: bool,
    /// Who committed it.
    pub committer: String,
    /// When, RFC 3339 UTC.
    pub committed_at: String,
}

/// Every version of one source, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Versions {
    /// The versions.
    pub versions: Vec<SourceVersion>,
}

/// The body of `POST /operator/sources` and `PUT /operator/sources/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    /// The name, unique.
    pub name: String,
    /// The kind, which a change keeps.
    pub kind: Kind,
    /// Whether the source is served; a change to `false` disables it.
    pub enabled: bool,
    /// The kind's document.
    pub document: serde_json::Value,
}
