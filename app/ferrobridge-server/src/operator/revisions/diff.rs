// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The diff of one revision against the running one, file by file, each
//! changed file as a unified diff of its lines (`similar`). No specification
//! governs the diff: our own design.

use std::collections::BTreeSet;
use std::sync::Arc;

use ferrobridge_operator_api::revisions::{FileChange, FileDiff, Revision, RevisionDiff};

use crate::mappings::files::sha256;
use crate::mappings::revisions::Files;

/// The lines of context a unified diff keeps around each change.
const CONTEXT: usize = 3;

/// Returns how `to` differs from `from`, the running revision, file by file,
/// each revision beside its files.
#[must_use]
pub fn diff(from: Option<(&Revision, &Files)>, to: (&Revision, &Files)) -> RevisionDiff {
    let (to, to_files) = to;
    let empty = Files::new();
    let before = from.map_or(&empty, |(_revision, files)| files);
    let paths = before
        .keys()
        .chain(to_files.keys())
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    for path in paths {
        let (old, new) = (before.get(path), to_files.get(path));
        let (from_sha256, to_sha256) = (
            old.map(|bytes| sha256(bytes)),
            new.map(|bytes| sha256(bytes)),
        );
        let change = match (&from_sha256, &to_sha256) {
            (Some(old), Some(new)) if old == new => continue,
            (Some(_), Some(_)) => FileChange::Changed,
            (None, Some(_)) => FileChange::Added,
            (Some(_), None) => FileChange::Removed,
            (None, None) => continue,
        };
        let (old_text, new_text) = (text(old), text(new));
        let unified = similar::TextDiff::from_lines(&old_text, &new_text)
            .unified_diff()
            .context_radius(CONTEXT)
            .header(&format!("a/{path}"), &format!("b/{path}"))
            .to_string();
        files.push(FileDiff {
            path: path.clone(),
            change,
            from_sha256,
            to_sha256,
            unified,
        });
    }
    RevisionDiff {
        from: from.map(|(revision, _files)| revision.id.clone()),
        to: to.id.clone(),
        files,
    }
}

/// Returns the lines a diff reads from `bytes`, none for a file that is not
/// there.
fn text(bytes: Option<&Arc<[u8]>>) -> String {
    bytes.map_or_else(String::new, |bytes| {
        String::from_utf8_lossy(bytes).into_owned()
    })
}
