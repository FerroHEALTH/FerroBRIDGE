// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge's running configuration as read-only settings (FerroUI T3,
//! R27).
//!
//! This module draws the rows of a face's or a connection's Settings tab and
//! of the Configuration page, and the dialog that says how configuration
//! works.
//!
//! The bridge reads its configuration from its file once, at start, so
//! every row is a view of `GET /operator/configuration`: the label, what
//! the setting governs, the value it runs on with where that value came
//! from, and the key that sets it. A secret is drawn by reference alone, as
//! the document carries it. No specification governs the console: our own
//! design.

pub mod rows;

use std::borrow::Cow;

use ferrobridge_operator_api::configuration::{Configuration, Entry, SecretState, Source, Value};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::dialog::{Explanation, info_dialog};
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::secret::{SecretState as Shown, secret_state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::template::settings::{
    How, Setting, file_line, setting_row, settings_panel,
};
use leptos::prelude::*;

use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused_operator};

/// The id of the dialog "How configuration works", which every file line
/// opens.
pub const HOW_DIALOG: &str = "how-configuration";

/// The file the bridge reads, named when it read none.
const FILE: &str = "ferrobridge.toml";

/// How one row draws the values of its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A word, as it stands.
    Word,
    /// A path, an address or a URL, in the code face.
    Code,
    /// A switch, as a state badge.
    Switch,
    /// A duration in milliseconds, in seconds where it is whole seconds.
    Millis,
    /// A size in bytes, in the largest whole binary unit.
    Bytes,
    /// A number.
    Number,
    /// A list, each item in the code face.
    List,
    /// A list or a table, by how many entries it holds.
    Count,
    /// A query, in a code block.
    Query,
    /// A secret, by reference alone.
    Secret,
}

/// One row: its words, its keys and how it draws them.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    /// The row's label.
    pub label: Msg,
    /// The one line on what it governs.
    pub description: Msg,
    /// The keys that set it, the first the row's own, each with how it
    /// draws its value.
    pub keys: &'static [(&'static str, Shape)],
}

/// Returns the setting `row` draws over `configuration`.
#[must_use]
pub fn setting(configuration: &Configuration, row: &Row) -> Setting {
    let entries: Vec<(Option<&Entry>, Shape)> = row
        .keys
        .iter()
        .map(|(key, shape)| (configuration.entry(key), *shape))
        .collect();
    let values = entries
        .iter()
        .enumerate()
        .map(|(at, (entry, shape))| {
            let joint = (at > 0).then_some(view! { " · " });
            let value = entry.map_or_else(unset, |entry| value(&entry.value, *shape));
            view! { {joint} {value} }.into_any()
        })
        .collect_view();
    let source = origin(&entries);
    Setting {
        label: tr(row.label),
        description: tr(row.description),
        value: view! { {values} {source} }.into_any(),
        keys: row.keys.iter().map(|(key, _)| (*key).to_owned()).collect(),
        next_start: false,
    }
}

/// Returns where the row's values came from, when that is not the file: the
/// environment wins over it, and a default is named as one.
fn origin(entries: &[(Option<&Entry>, Shape)]) -> Option<AnyView> {
    let sources = entries
        .iter()
        .map(|(entry, _)| entry.map_or(Source::Default, |entry| entry.source))
        .collect::<Vec<_>>();
    let words = if sources.contains(&Source::Environment) {
        Msg::SourceEnvironment
    } else if sources.iter().all(|source| *source == Source::Default) {
        Msg::SourceDefault
    } else {
        return None;
    };
    Some(view! { " " <span class="fui-muted fui-small">{tr(words)}</span> }.into_any())
}

/// Returns the quiet dash of a key the configuration leaves unset.
fn unset() -> AnyView {
    view! { <span class="fui-muted">{tr(Msg::ValueNotSet)}</span> }.into_any()
}

/// Returns `held` drawn as `shape`.
fn value(held: &Value, shape: Shape) -> AnyView {
    match (held, shape) {
        (Value::Secret(state), _) => secret_state(&shown(state)),
        (Value::Boolean(on), _) => switch(*on),
        (Value::Integer(millis), Shape::Millis) => duration(*millis).into_any(),
        (Value::Integer(bytes), Shape::Bytes) => size(*bytes).into_any(),
        (Value::String(text), Shape::Word) if text.is_empty() => unset(),
        (Value::String(text), Shape::Code) if text.is_empty() => unset(),
        (Value::String(text), Shape::Code) => view! { <code>{text.clone()}</code> }.into_any(),
        (Value::String(text), Shape::Query) => {
            view! { <pre class="fui-code-panel"><code>{text.clone()}</code></pre> }.into_any()
        }
        (Value::List(items), Shape::Count) => count(items.len()),
        (Value::Table(items), Shape::Count) => count(items.len()),
        (Value::List(items), _) if items.is_empty() => {
            view! { <span class="fui-muted">{tr(Msg::ValueNone)}</span> }.into_any()
        }
        (Value::List(items), _) => items
            .iter()
            .enumerate()
            .map(|(at, item)| {
                let joint = (at > 0).then_some(view! { ", " });
                view! { {joint} {value(item, Shape::Code)} }
            })
            .collect_view()
            .into_any(),
        (Value::Table(_), _) => count(1),
        (Value::String(text), _) => text.clone().into_any(),
        (Value::Integer(number), _) => number.to_string().into_any(),
        (Value::Float(number), _) => number.to_string().into_any(),
    }
}

/// Returns the state of a secret as the library draws it.
fn shown(state: &SecretState) -> Shown {
    match state {
        SecretState::Set => Shown::Redacted,
        SecretState::File { path } => Shown::Set(path.clone()),
        SecretState::Unset => Shown::Unset,
    }
}

/// Returns a switch read from the file: a state badge, never a raw on or
/// off.
fn switch(on: bool) -> AnyView {
    if on {
        badge(Tone::Ok, tr(Msg::StateOn)).into_any()
    } else {
        badge(Tone::Neutral, tr(Msg::StateOff)).into_any()
    }
}

/// Returns how many entries a list or a table holds.
fn count(entries: usize) -> AnyView {
    if entries == 0 {
        view! { <span class="fui-muted">{tr(Msg::ValueNone)}</span> }.into_any()
    } else {
        tr_with(Msg::ValueEntries, &[("count", entries.into())]).into_any()
    }
}

/// Returns a duration of `millis` milliseconds, in seconds where it is
/// whole seconds.
#[must_use]
pub fn duration(millis: i64) -> String {
    if millis != 0 && millis % 1000 == 0 {
        tr_with(
            Msg::ValueSeconds,
            &[("count", millis.div_euclid(1000).to_string().into())],
        )
    } else {
        tr_with(Msg::ValueMillis, &[("count", millis.to_string().into())])
    }
}

/// Returns a size of `bytes`, in the largest whole binary unit.
#[must_use]
pub fn size(bytes: i64) -> String {
    const KIB: i64 = 1024;
    const MIB: i64 = 1024 * 1024;
    if bytes != 0 && bytes % MIB == 0 {
        tr_with(
            Msg::ValueMib,
            &[("count", bytes.div_euclid(MIB).to_string().into())],
        )
    } else if bytes != 0 && bytes % KIB == 0 {
        tr_with(
            Msg::ValueKib,
            &[("count", bytes.div_euclid(KIB).to_string().into())],
        )
    } else {
        tr_with(Msg::ValueBytes, &[("count", bytes.to_string().into())])
    }
}

/// Returns the file the bridge read, as the file line names it.
#[must_use]
pub fn file_of(configuration: &Configuration) -> String {
    configuration
        .file
        .clone()
        .unwrap_or_else(|| String::from(FILE))
}

/// Returns a Settings tab: the file line, then the rows over the running
/// configuration, or the refusal or fault in their place, and the dialog
/// the file line opens.
#[must_use]
pub fn tab(configuration: &Loaded<Configuration>, rows: &[Row], again: &str) -> AnyView {
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => configuration,
        Ok(Outcome::Refused(refusal)) => return refused_operator(refusal),
        Err(fault) => return faulted(fault, again),
    };
    let drawn = rows
        .iter()
        .map(|row| setting_row(setting(configuration, row), false))
        .collect();
    let line = file_line(
        file_of(configuration),
        How::Dialog(Cow::Borrowed(HOW_DIALOG)),
    );
    view! {
        {settings_panel(line, None, drawn)}
        {how_dialog()}
    }
    .into_any()
}

/// Returns the dialog "How configuration works": where the settings are
/// shown and how they change.
#[must_use]
pub fn how_dialog() -> AnyView {
    let shown = bullet_list(vec![
        tr(Msg::HowHere).into_any(),
        tr(Msg::HowFaces).into_any(),
        tr(Msg::HowConnections).into_any(),
    ]);
    let body = view! {
        <p>{tr(Msg::HowIntro)}</p>
        <div class="fui-ownership">
            <div>
                <h3>{tr(Msg::HowWhereTitle)}</h3>
                <p class="fui-small fui-muted">{tr(Msg::HowWhere)}</p>
                {shown}
            </div>
            <div>
                <h3>{tr(Msg::HowChangeTitle)}</h3>
                <p class="fui-small fui-muted">{tr(Msg::HowChange)}</p>
            </div>
        </div>
        <p class="fui-small fui-muted">{tr(Msg::HowMappings)}</p>
    };
    info_dialog(
        Explanation {
            id: Cow::Borrowed(HOW_DIALOG),
            title: tr(Msg::HowTitle).into(),
        },
        body,
    )
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::{duration, size};

    #[test]
    fn a_duration_reads_in_seconds_where_it_is_whole_seconds() {
        assert_eq!("30 s", duration(30_000));
        assert_eq!("250 ms", duration(250));
        assert_eq!("0 ms", duration(0));
    }

    #[test]
    fn a_size_reads_in_its_largest_whole_binary_unit() {
        assert_eq!("1 MiB", size(1024 * 1024));
        assert_eq!("64 KiB", size(64 * 1024));
        assert_eq!("1000 bytes", size(1000));
    }
}
