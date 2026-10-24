// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where each key of the configuration came from: the file, an environment
//! variable, or neither. No specification governs this: our own design.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ferrobridge_operator_api::configuration::Source;

/// The keys the file and the environment set, as the load read them.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// The file the load read, when it read one.
    file: Option<PathBuf>,
    /// Every dotted key the file holds a value that is not a table for.
    file_keys: BTreeSet<String>,
    /// Every dotted key an environment variable set.
    environment_keys: BTreeSet<String>,
}

impl Provenance {
    /// Returns the provenance of a load that read `file` as TOML and set
    /// `environment_keys` from the environment.
    pub(super) fn new(file: Option<&toml::Table>, environment_keys: BTreeSet<String>) -> Self {
        let mut file_keys = BTreeSet::new();
        if let Some(table) = file {
            leaves("", table, &mut file_keys);
        }
        Self {
            file: None,
            file_keys,
            environment_keys,
        }
    }

    /// Records the path of the file the load read.
    pub(super) fn read_from(&mut self, path: PathBuf) {
        self.file = Some(path);
    }

    /// Returns the file the load read, when it read one.
    #[must_use]
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Returns where the value of the dotted key `key` came from.
    ///
    /// The environment wins over the file, as the load applies it; a key
    /// counts as set when it or a table above it was set whole.
    #[must_use]
    pub fn source(&self, key: &str) -> Source {
        if covers(&self.environment_keys, key) {
            Source::Environment
        } else if covers(&self.file_keys, key) {
            Source::File
        } else {
            Source::Default
        }
    }
}

/// Returns whether `keys` holds `key` or a key above it.
fn covers(keys: &BTreeSet<String>, key: &str) -> bool {
    keys.contains(key)
        || key
            .rmatch_indices('.')
            .filter_map(|(at, _)| key.get(..at))
            .any(|above| keys.contains(above))
}

/// Adds to `out` the dotted key of every value under `table` that is not a
/// table, below `prefix`.
fn leaves(prefix: &str, table: &toml::Table, out: &mut BTreeSet<String>) {
    for (name, value) in table {
        let key = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            toml::Value::Table(inner) => leaves(&key, inner, out),
            _ => {
                out.insert(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferrobridge_operator_api::configuration::Source;

    use super::Provenance;

    #[test]
    fn the_environment_wins_over_the_file_and_a_whole_table_counts_for_its_keys() {
        let file: toml::Table = toml::from_str(
            "[server]\nlisten = \"0.0.0.0:8080\"\nbody_limit_bytes = 10\n[cdr.retry]\nmax_attempts = 2\n",
        )
        .expect("the file parses");
        let environment = BTreeSet::from([
            String::from("server.listen"),
            String::from("terminology.retry"),
        ]);
        let provenance = Provenance::new(Some(&file), environment);
        assert_eq!(provenance.source("server.listen"), Source::Environment);
        assert_eq!(provenance.source("server.body_limit_bytes"), Source::File);
        assert_eq!(
            provenance.source("server.request_timeout_ms"),
            Source::Default
        );
        assert_eq!(provenance.source("cdr.retry.max_attempts"), Source::File);
        assert_eq!(
            provenance.source("cdr.retry.initial_backoff_ms"),
            Source::Default
        );
        assert_eq!(
            provenance.source("terminology.retry.max_attempts"),
            Source::Environment
        );
    }
}
