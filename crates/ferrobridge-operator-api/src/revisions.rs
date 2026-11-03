// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set revisions `GET /operator/revisions`,
//! `GET /operator/revisions/{id}` and `GET /operator/revisions/{id}/diff`
//! answer.
//!
//! A revision is the whole set of mapping files the console owns under
//! `[operator] mapping_dir`, each by its wire path and SHA-256 digest, with
//! who recorded it, when and why. Every save, reload and apply that changes
//! the running console files records one; the provisioned files belong to the
//! deployment and are in no revision. No specification governs these
//! documents: our own design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What `GET /operator/revisions` answers with: the newest revision first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revisions {
    /// The id of the revision the bridge runs, when one is recorded. An
    /// apply names it in `If-Match`.
    pub running: Option<String>,
    /// Every revision, newest first.
    pub revisions: Vec<RevisionSummary>,
}

/// One revision as the listing names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionSummary {
    /// The revision's id, a decimal number that grows by one per revision.
    pub id: String,
    /// The revision that ran when this one was recorded.
    pub parent: Option<String>,
    /// The subject of the operator who recorded it, `-` for the bridge.
    pub author: String,
    /// When it was recorded, RFC 3339 UTC.
    pub time: String,
    /// The reason the operator gave.
    pub reason: String,
    /// What recorded it.
    pub origin: Origin,
    /// Whether it ran: an imported revision is checked and waits for an
    /// apply.
    pub applied: bool,
    /// How many console files it holds.
    pub files: u64,
}

/// What `GET /operator/revisions/{id}` answers with: one revision whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    /// The revision's id, a decimal number that grows by one per revision.
    pub id: String,
    /// The revision that ran when this one was recorded.
    pub parent: Option<String>,
    /// The subject of the operator who recorded it, `-` for the bridge.
    pub author: String,
    /// When it was recorded, RFC 3339 UTC.
    pub time: String,
    /// The reason the operator gave.
    pub reason: String,
    /// What recorded it.
    pub origin: Origin,
    /// Whether it ran.
    pub applied: bool,
    /// The revision an apply restored, for [`Origin::Apply`].
    pub source: Option<String>,
    /// Every console file, by wire path, with the SHA-256 digest of its
    /// bytes in lowercase hexadecimal.
    pub files: BTreeMap<String, String>,
}

impl Revision {
    /// Returns this revision as the listing names it.
    #[must_use]
    pub fn summary(&self) -> RevisionSummary {
        RevisionSummary {
            id: self.id.clone(),
            parent: self.parent.clone(),
            author: self.author.clone(),
            time: self.time.clone(),
            reason: self.reason.clone(),
            origin: self.origin,
            applied: self.applied,
            files: u64::try_from(self.files.len()).unwrap_or(u64::MAX),
        }
    }
}

/// What recorded a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The console files the bridge found before it recorded its first
    /// revision.
    Baseline,
    /// A save of one file through the operator API.
    Save,
    /// A reload that found the console files changed on disk.
    Reload,
    /// An apply of an earlier or an imported revision.
    Apply,
    /// An import of a mapping-set archive, checked and not yet applied.
    Import,
}

/// What `GET /operator/revisions/{id}/diff` answers with: how the revision
/// differs from the running one, file by file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionDiff {
    /// The running revision the diff reads from.
    pub from: Option<String>,
    /// The revision the diff reads to.
    pub to: String,
    /// Every file that differs, by wire path; an unchanged file is left out.
    pub files: Vec<FileDiff>,
}

/// How one file differs between two revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDiff {
    /// The file's wire path.
    pub path: String,
    /// What changed.
    pub change: FileChange,
    /// Its digest in the running revision, when it is there.
    pub from_sha256: Option<String>,
    /// Its digest in the other revision, when it is there.
    pub to_sha256: Option<String>,
    /// The unified diff of its lines, three lines of context around each
    /// change.
    pub unified: String,
}

/// What changed in one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    /// The other revision holds a file the running one does not.
    Added,
    /// The running revision holds a file the other one does not.
    Removed,
    /// Both hold the file with different bytes.
    Changed,
}

#[cfg(test)]
mod tests {
    use super::{FileChange, FileDiff, Origin, Revision, RevisionDiff};

    #[test]
    fn a_revision_round_trips_through_its_json() {
        let revision = Revision {
            id: String::from("3"),
            parent: Some(String::from("2")),
            author: String::from("operator-1"),
            time: String::from("2026-11-03T09:00:00Z"),
            reason: String::from("synthetic reason"),
            origin: Origin::Apply,
            applied: true,
            source: Some(String::from("1")),
            files: [(
                String::from("fhirconnect/a.yml"),
                String::from("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            )]
            .into(),
        };
        let json = serde_json::to_value(&revision).expect("the revision serializes");
        assert_eq!(json["origin"], "apply");
        let back: Revision = serde_json::from_value(json).expect("the revision reads back");
        assert_eq!(back, revision);
        assert_eq!(1, back.summary().files);
    }

    #[test]
    fn a_diff_names_each_change_in_snake_case() {
        let diff = RevisionDiff {
            from: Some(String::from("2")),
            to: String::from("1"),
            files: vec![FileDiff {
                path: String::from("fhirconnect/a.yml"),
                change: FileChange::Added,
                from_sha256: None,
                to_sha256: Some(String::from("00")),
                unified: String::from("+a\n"),
            }],
        };
        let json = serde_json::to_value(&diff).expect("the diff serializes");
        assert_eq!(json["files"][0]["change"], "added");
    }
}
