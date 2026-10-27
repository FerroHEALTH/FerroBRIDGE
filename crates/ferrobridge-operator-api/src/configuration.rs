// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration `GET /operator/configuration` answers: every
//! key of the configuration as the bridge read it at start, with the value
//! it runs on and where that value came from.
//!
//! A secret is carried by reference alone ([`SecretState`]): whether it is
//! set, and the file it is read from when it is read from one, never its
//! value. No specification governs this document: our own design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What `GET /operator/configuration` answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    /// The configuration file the bridge read, as it was named at start;
    /// `None` when it read no file and runs on the environment and the
    /// defaults.
    pub file: Option<String>,
    /// When the bridge read its configuration, RFC 3339 UTC.
    pub read_at: String,
    /// Every key the configuration holds a value for, in key order.
    pub entries: Vec<Entry>,
}

impl Configuration {
    /// Returns the entry of the dotted key `key`, such as `server.listen`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrobridge_operator_api::configuration::{Configuration, Entry, Source, Value};
    ///
    /// let configuration = Configuration {
    ///     file: Some(String::from("/etc/ferrobridge/ferrobridge.toml")),
    ///     read_at: String::from("2026-10-24T06:00:00Z"),
    ///     entries: vec![Entry {
    ///         key: String::from("server.listen"),
    ///         value: Value::String(String::from("0.0.0.0:8080")),
    ///         source: Source::File,
    ///     }],
    /// };
    /// let listen = configuration.entry("server.listen").expect("a key the file sets");
    /// assert_eq!(listen.source, Source::File);
    /// assert!(configuration.entry("server.port").is_none());
    /// ```
    #[must_use]
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

/// One key with the value the bridge runs on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The dotted key, such as `cdr.credentials.oauth.client_id`; a key
    /// under an array of tables is a key of the table the array's value
    /// holds.
    pub key: String,
    /// The value the bridge runs on.
    pub value: Value,
    /// Where the value came from.
    pub source: Source,
}

/// Where a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The configuration file.
    File,
    /// An environment variable, which wins over the file.
    Environment,
    /// A `--set key=value` argument, which wins over the environment.
    Set,
    /// None of them: the bridge's own default.
    Default,
}

/// A value as the bridge runs on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    /// A string, a path or an address among them.
    String(String),
    /// An integer.
    Integer(i64),
    /// A floating-point number.
    Float(f64),
    /// A switch.
    Boolean(bool),
    /// A list, in the order the configuration gives it.
    List(Vec<Value>),
    /// A table, an entry of an array of tables among them, by key.
    Table(BTreeMap<String, Value>),
    /// A secret, by reference alone.
    Secret(SecretState),
}

/// What is set for a secret, never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretState {
    /// Set as a value, in the file or the environment, which the document
    /// never names.
    Set,
    /// Read from the file at this path.
    File {
        /// The path, as the configuration names it.
        path: String,
    },
    /// Not set.
    Unset,
}

#[cfg(test)]
mod tests {
    use super::{Configuration, Entry, SecretState, Source, Value};

    #[test]
    fn a_secret_is_written_as_its_state_and_reads_back_the_same() {
        let document = Configuration {
            file: None,
            read_at: String::from("2026-10-24T06:00:00Z"),
            entries: vec![
                Entry {
                    key: String::from("cdm.url"),
                    value: Value::Secret(SecretState::File {
                        path: String::from("/run/secrets/cdm-url"),
                    }),
                    source: Source::Environment,
                },
                Entry {
                    key: String::from("cdr.credentials.password"),
                    value: Value::Secret(SecretState::Unset),
                    source: Source::Default,
                },
            ],
        };
        let json = serde_json::to_value(&document).expect("the document serializes");
        assert_eq!(
            json["entries"][0]["value"],
            serde_json::json!({ "kind": "secret", "value": { "state": "file", "path": "/run/secrets/cdm-url" } })
        );
        assert_eq!(json["entries"][0]["source"], "environment");
        assert_eq!(
            json["entries"][1]["value"]["value"],
            serde_json::json!({ "state": "unset" })
        );
        let back: Configuration = serde_json::from_value(json).expect("the document reads back");
        assert_eq!(back, document);
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let json = serde_json::json!({
            "file": null,
            "read_at": "2026-10-24T06:00:00Z",
            "entries": [],
            "secrets": [],
        });
        assert!(serde_json::from_value::<Configuration>(json).is_err());
    }
}
