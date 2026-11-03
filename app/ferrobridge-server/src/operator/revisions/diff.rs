// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The diff of one revision against the running one, file by file, each
//! changed file as a unified diff of its lines (`similar`). No specification
//! governs the diff: our own design.

use std::collections::BTreeSet;

use ferrobridge_operator_api::revisions::{FileChange, FileDiff, Revision, RevisionDiff};

use crate::operator::revisions::{Store, StoreError};

/// The lines of context a unified diff keeps around each change.
const CONTEXT: usize = 3;

/// Returns how `to` differs from `from`, the running revision, file by file.
///
/// # Errors
///
/// Returns what [`Store::blob`] returns for a file either revision holds.
pub fn diff(
    store: &Store,
    from: Option<&Revision>,
    to: &Revision,
) -> Result<RevisionDiff, StoreError> {
    let empty = std::collections::BTreeMap::new();
    let before = from.map_or(&empty, |revision| &revision.files);
    let paths = before
        .keys()
        .chain(to.files.keys())
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();
    for path in paths {
        let (old, new) = (before.get(path), to.files.get(path));
        let change = match (old, new) {
            (Some(old), Some(new)) if old == new => continue,
            (Some(_), Some(_)) => FileChange::Changed,
            (None, Some(_)) => FileChange::Added,
            (Some(_), None) => FileChange::Removed,
            (None, None) => continue,
        };
        let text = |digest: Option<&String>| -> Result<String, StoreError> {
            Ok(match digest {
                Some(digest) => String::from_utf8_lossy(&store.blob(digest)?).into_owned(),
                None => String::new(),
            })
        };
        let (old_text, new_text) = (text(old)?, text(new)?);
        let unified = similar::TextDiff::from_lines(&old_text, &new_text)
            .unified_diff()
            .context_radius(CONTEXT)
            .header(&format!("a/{path}"), &format!("b/{path}"))
            .to_string();
        files.push(FileDiff {
            path: path.clone(),
            change,
            from_sha256: old.cloned(),
            to_sha256: new.cloned(),
            unified,
        });
    }
    Ok(RevisionDiff {
        from: from.map(|revision| revision.id.clone()),
        to: to.id.clone(),
        files,
    })
}
