// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The message codes and trigger events a version's interactions send a
//! structure under, beyond the one `messages.json` gives each structure.

use std::collections::{BTreeMap, BTreeSet};

use crate::v2::legacy::lower::{LegacyError, LegacyMessage, invalid};
use crate::v2::legacy::source::VersionTables;
use crate::v2::lower::Structure;

/// Every `code^event` the interactions of `version` send one of its lowered
/// `structures` under, apart from the pairs its `messages.json` gives.
///
/// An interaction sends its sender structure and answers with its receiver
/// structure under its event, each under the message code `messages.json`
/// gives that structure: the event I14 sends `REF_I12` as `REF^I14` and
/// answers with `RRI_I12` as `RRI^I14`.
///
/// # Errors
///
/// Returns [`LegacyError::Invalid`] when two interactions send different
/// structures under one `code^event` that `messages.json` does not give.
pub(super) fn event_entries(
    version: &VersionTables,
    structures: &BTreeMap<String, Structure>,
) -> Result<Vec<LegacyMessage>, LegacyError> {
    let codes: BTreeMap<&str, &str> = version
        .messages
        .iter()
        .map(|message| (message.id.as_str(), message.msg_type_id.as_str()))
        .collect();
    let listed: BTreeSet<(&str, &str)> = version
        .messages
        .iter()
        .map(|message| (message.msg_type_id.as_str(), message.event_id.as_str()))
        .collect();
    let mut bound: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for interaction in &version.interactions {
        for sent in [&interaction.sender_msg, &interaction.receiver_msg] {
            // NOTE: no specification governs this: our own design; an interaction naming a
            // structure this version emits no tree for (`NUL`, `QRY` at 2.5) has nothing to index.
            if !structures.contains_key(sent.as_str()) {
                continue;
            }
            let Some(code) = codes.get(sent.as_str()).copied() else {
                return Err(invalid(
                    &version.file("messages"),
                    sent,
                    "a lowered structure the file does not list",
                ));
            };
            let key = (code, interaction.event_id.as_str());
            if listed.contains(&key) {
                continue;
            }
            match bound.insert(key, sent.as_str()) {
                Some(other) if other != sent.as_str() => {
                    return Err(invalid(
                        &version.file("interactions"),
                        &format!("{code}^{}", interaction.event_id),
                        format!("sends both {other} and {sent}"),
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(bound
        .into_iter()
        .map(|((code, event), structure)| LegacyMessage {
            code: code.to_owned(),
            event: event.to_owned(),
            version: version.version.clone(),
            structure: structure.to_owned(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::de::IgnoredAny;

    use super::event_entries;
    use crate::v2::legacy::lower::LegacyError;
    use crate::v2::legacy::source::{InteractionRow, MessageRow, VersionTables};
    use crate::v2::lower::Structure;

    fn message(id: &str, code: &str, event: &str) -> MessageRow {
        MessageRow {
            id: id.to_owned(),
            msg_type_id: code.to_owned(),
            event_id: event.to_owned(),
            section: IgnoredAny,
        }
    }

    fn interaction(event: &str, sender: &str, receiver: &str) -> InteractionRow {
        InteractionRow {
            event_id: event.to_owned(),
            sender_msg: sender.to_owned(),
            receiver_msg: receiver.to_owned(),
            number: IgnoredAny,
        }
    }

    fn tables(messages: Vec<MessageRow>, interactions: Vec<InteractionRow>) -> VersionTables {
        VersionTables {
            version: String::from("2.8"),
            messages,
            groups: Vec::new(),
            elements: Vec::new(),
            segments: Vec::new(),
            fields: Vec::new(),
            data_elements: Vec::new(),
            data_types: Vec::new(),
            interactions,
        }
    }

    fn lowered(ids: &[&str]) -> BTreeMap<String, Structure> {
        ids.iter()
            .map(|id| {
                let structure = Structure {
                    id: (*id).to_owned(),
                    url: None,
                    version: String::from("2.8"),
                    withdrawn_as_of: None,
                    nodes: Vec::new(),
                };
                ((*id).to_owned(), structure)
            })
            .collect()
    }

    #[test]
    fn an_interaction_sends_both_its_structures_under_its_event() {
        let version = tables(
            vec![
                message("REF_I12", "REF", "I12"),
                message("RRI_I12", "RRI", "I12"),
            ],
            vec![
                interaction("I12", "REF_I12", "RRI_I12"),
                interaction("I14", "REF_I12", "RRI_I12"),
                interaction("I14", "REF_I12", "NUL"),
            ],
        );
        let entries = event_entries(&version, &lowered(&["REF_I12", "RRI_I12"])).expect("lowers");
        let keys: Vec<(&str, &str, &str)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.code.as_str(),
                    entry.event.as_str(),
                    entry.structure.as_str(),
                )
            })
            .collect();
        assert_eq!(keys, [("REF", "I14", "REF_I12"), ("RRI", "I14", "RRI_I12")]);
    }

    #[test]
    fn two_structures_under_one_unlisted_code_and_event_are_refused() {
        let version = tables(
            vec![
                message("ORL_O22", "ORL", "O22"),
                message("ORL_O41", "ORL", "O41"),
            ],
            vec![
                interaction("O99", "OML_O21", "ORL_O22"),
                interaction("O99", "OML_O21", "ORL_O41"),
            ],
        );
        let refused = event_entries(&version, &lowered(&["ORL_O22", "ORL_O41"]));
        assert!(
            matches!(refused, Err(LegacyError::Invalid { ref row, .. }) if row == "ORL^O99"),
            "{refused:?}"
        );
    }
}
