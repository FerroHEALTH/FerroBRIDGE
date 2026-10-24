// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration `GET /operator/configuration` answers: the tree
//! the bridge started on, every key with its value and where it came from,
//! each secret by reference alone.
//!
//! The tree is serialized with every secret already redacted
//! ([`crate::config::section::Secret`], and the redacted sections of
//! `ferrohealth-smart`), so no secret value reaches the document. A secret
//! and its `_file` sibling are drawn as one entry, keyed by the secret,
//! whose value is its state. No specification governs the document: our
//! own design.

use std::collections::BTreeMap;

use ferrobridge_operator_api::configuration::{Configuration, Entry, SecretState, Source, Value};

use crate::config::Config;
use crate::config::provenance::Provenance;
use crate::config::section::Secret;

/// The secrets the tree holds as a value or through a `_file` sibling, by
/// dotted key; a key under an array of tables names the array.
const SECRETS: [&str; 6] = [
    "cdm.url",
    "cdr.credentials.password",
    "cdr.credentials.oauth.client_secret",
    "terminology.credentials.password",
    "terminology.credentials.oauth.client_secret",
    "auth.basic.users.password_hash",
];

/// The keys that name the file of a private key, drawn as a secret read
/// from that file.
const KEY_FILES: [&str; 4] = [
    "cdr.credentials.oauth.private_key_file",
    "cdr.credentials.oauth.client_key_file",
    "terminology.credentials.oauth.private_key_file",
    "terminology.credentials.oauth.client_key_file",
];

/// Returns the running configuration of `config`, read at `read_at`.
///
/// # Errors
///
/// Returns the TOML writer's error when the tree cannot be written as a
/// TOML table.
pub fn document(
    config: &Config,
    read_at: jiff::Timestamp,
) -> Result<Configuration, toml::ser::Error> {
    let tree = toml::Table::try_from(config)?;
    let provenance = config.provenance();
    let mut entries = Vec::new();
    walk("", &tree, provenance, &mut entries);
    for entry in &mut entries {
        if entry.key == "auth.product_scope" && entry.value == Value::String(String::new()) {
            // NOTE: no specification governs this: our own design; an empty
            // product scope runs as the bridge's own, so the document answers it.
            entry.value = Value::String(String::from(crate::smart::OPERATOR_SCOPE));
        }
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(Configuration {
        file: provenance.file().map(|path| path.display().to_string()),
        read_at: read_at.to_string(),
        entries,
    })
}

/// Joins `prefix` and `name` into a dotted key.
fn dotted(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Adds an entry to `out` for every key under `table`, a table at `path`.
fn walk(path: &str, table: &toml::Table, provenance: &Provenance, out: &mut Vec<Entry>) {
    let folded = fold_secrets(path, table);
    for (name, value, set_by) in folded.secrets {
        let source = set_by.map_or(Source::Default, |key| provenance.source(&key));
        out.push(Entry {
            key: dotted(path, &name),
            value,
            source,
        });
    }
    for (name, value) in table {
        if folded.taken.contains(&name.as_str()) {
            continue;
        }
        let key = dotted(path, name);
        if let toml::Value::Table(inner) = value {
            walk(&key, inner, provenance, out);
        } else {
            out.push(Entry {
                value: convert(&key, value),
                source: provenance.source(&key),
                key,
            });
        }
    }
}

/// The secrets of one table, drawn by state, and the keys they take.
struct Folded<'a> {
    /// Each secret by its name in the table, with its state and the dotted
    /// key that set it, when one did.
    secrets: Vec<(String, Value, Option<String>)>,
    /// The names in the table the secrets stand for.
    taken: Vec<&'a str>,
}

/// Returns the secrets of `table`, a table at `path`, each by state.
fn fold_secrets<'a>(path: &str, table: &'a toml::Table) -> Folded<'a> {
    let mut folded = Folded {
        secrets: Vec::new(),
        taken: Vec::new(),
    };
    for secret in SECRETS {
        let Some(name) = child_name(path, secret) else {
            continue;
        };
        let file_name = format!("{name}_file");
        let file = table.get_key_value(file_name.as_str());
        let inline = table.get_key_value(name);
        let (state, set_by) = match (file, inline) {
            (Some((key, toml::Value::String(file))), _) => (
                SecretState::File { path: file.clone() },
                Some(dotted(path, key)),
            ),
            (_, Some((key, _))) => (SecretState::Set, Some(dotted(path, key))),
            _ => (SecretState::Unset, None),
        };
        for taken in [file, inline].into_iter().flatten() {
            folded.taken.push(taken.0.as_str());
        }
        folded
            .secrets
            .push((name.to_owned(), Value::Secret(state), set_by));
    }
    for key_file in KEY_FILES {
        let Some(name) = child_name(path, key_file) else {
            continue;
        };
        if let Some((key, toml::Value::String(file))) = table.get_key_value(name) {
            folded.taken.push(key.as_str());
            folded.secrets.push((
                name.to_owned(),
                Value::Secret(SecretState::File { path: file.clone() }),
                Some(dotted(path, key)),
            ));
        }
    }
    folded
}

/// Returns the name `key` gives a child of the table at `path`, when `key`
/// names one.
fn child_name<'k>(path: &str, key: &'k str) -> Option<&'k str> {
    let (parent, name) = key.rsplit_once('.')?;
    (parent == path).then_some(name)
}

/// Converts the value of `key` that is not a table.
fn convert(key: &str, value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) if text == Secret::REDACTED => Value::Secret(SecretState::Set),
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => Value::Integer(*number),
        toml::Value::Float(number) => Value::Float(*number),
        toml::Value::Boolean(on) => Value::Boolean(*on),
        toml::Value::Datetime(when) => Value::String(when.to_string()),
        toml::Value::Array(items) => {
            Value::List(items.iter().map(|item| convert(key, item)).collect())
        }
        toml::Value::Table(table) => Value::Table(item_table(key, table)),
    }
}

/// Converts `table`, an item of the array at `key`, with its secrets by
/// state.
fn item_table(key: &str, table: &toml::Table) -> BTreeMap<String, Value> {
    let folded = fold_secrets(key, table);
    let mut out: BTreeMap<String, Value> = folded
        .secrets
        .into_iter()
        .map(|(name, value, _)| (name, value))
        .collect();
    for (name, value) in table {
        if !folded.taken.contains(&name.as_str()) {
            out.insert(name.clone(), convert(&dotted(key, name), value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::child_name;

    #[test]
    fn a_secret_key_names_the_child_of_its_own_table_alone() {
        assert_eq!(child_name("cdm", "cdm.url"), Some("url"));
        assert_eq!(child_name("cdr", "cdr.credentials.password"), None);
        assert_eq!(child_name("", "cdm.url"), None);
    }
}
