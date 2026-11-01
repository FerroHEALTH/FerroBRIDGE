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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::dialog::{Explanation, info_dialog};
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::secret;
use ferrohealth_ui::view::secret::secret_state;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::template::settings::{
    How, Setting, ValueSource, file_line, setting_row, settings_panel,
};

use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
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
pub fn setting(l: &Locale, configuration: &Configuration, row: &Row) -> Setting {
    let entries: Vec<(Option<&Entry>, Shape)> = row
        .keys
        .iter()
        .map(|(key, shape)| (configuration.entry(key), *shape))
        .collect();
    let values = entries
        .iter()
        .enumerate()
        .map(|(at, (entry, shape))| {
            let joint = if at > 0 { t(" · ") } else { Html::empty() };
            let value = entry.map_or_else(|| unset(l), |entry| value(l, &entry.value, *shape));
            joint.then(value)
        })
        .collect::<Html>();
    Setting {
        label: tr(l, row.label),
        description: tr(l, row.description),
        value: values,
        source: origin(&entries),
        keys: row.keys.iter().map(|(key, _)| (*key).to_owned()).collect(),
        next_start: false,
    }
}

/// Where the row's values came from, when that is not the file: the
/// environment wins over it, and a default is named as one.
fn origin(entries: &[(Option<&Entry>, Shape)]) -> Option<ValueSource> {
    let sources = entries
        .iter()
        .map(|(entry, _)| entry.map_or(Source::Default, |entry| entry.source))
        .collect::<Vec<_>>();
    if sources.contains(&Source::Environment) {
        Some(ValueSource::Environment)
    } else if sources.iter().all(|source| *source == Source::Default) {
        Some(ValueSource::Default)
    } else {
        None
    }
}

/// Returns the quiet dash of a key the configuration leaves unset.
fn unset(l: &Locale) -> Html {
    muted(tr(l, Msg::ValueNotSet))
}

/// Returns `words` in the muted face.
fn muted(words: String) -> Html {
    h(format!(r#"<span class="fui-muted">{}</span>"#, t(words)))
}

/// Returns `held` drawn as `shape`.
fn value(l: &Locale, held: &Value, shape: Shape) -> Html {
    match (held, shape) {
        (Value::Secret(state), _) => secret_state(l, &shown(state)),
        (Value::Boolean(on), _) => switch(l, *on),
        (Value::Integer(millis), Shape::Millis) => t(duration(l, *millis)),
        (Value::Integer(bytes), Shape::Bytes) => t(size(l, *bytes)),
        (Value::String(text), Shape::Word | Shape::Code) if text.is_empty() => unset(l),
        (Value::String(text), Shape::Code) => code(text),
        (Value::String(text), Shape::Query) => h(format!(
            r#"<pre class="fui-code-panel">{}</pre>"#,
            code(text)
        )),
        (Value::List(items), Shape::Count) => count(l, items.len()),
        (Value::Table(items), Shape::Count) => count(l, items.len()),
        (Value::List(items), _) if items.is_empty() => muted(tr(l, Msg::ValueNone)),
        (Value::List(items), _) => items
            .iter()
            .enumerate()
            .map(|(at, item)| {
                let joint = if at > 0 { t(", ") } else { Html::empty() };
                joint.then(value(l, item, Shape::Code))
            })
            .collect::<Html>(),
        (Value::Table(_), _) => count(l, 1),
        (Value::String(text), _) => t(text),
        (Value::Integer(number), _) => t(number.to_string()),
        (Value::Float(number), _) => t(number.to_string()),
    }
}

/// Returns the state of a secret as the library draws it.
fn shown(state: &SecretState) -> secret::SecretState {
    match state {
        SecretState::Set => secret::SecretState::Redacted,
        SecretState::File { path } => secret::SecretState::Set(path.clone()),
        SecretState::Unset => secret::SecretState::Unset,
    }
}

/// Returns a switch read from the file: a state badge, never a raw on or
/// off.
fn switch(l: &Locale, on: bool) -> Html {
    if on {
        badge(Tone::Ok, tr(l, Msg::StateOn))
    } else {
        badge(Tone::Neutral, tr(l, Msg::StateOff))
    }
}

/// Returns how many entries a list or a table holds.
fn count(l: &Locale, entries: usize) -> Html {
    if entries == 0 {
        muted(tr(l, Msg::ValueNone))
    } else {
        t(tr_with(l, Msg::ValueEntries, &[("count", entries.into())]))
    }
}

/// Returns a duration of `millis` milliseconds, in seconds where it is
/// whole seconds.
#[must_use]
pub fn duration(l: &Locale, millis: i64) -> String {
    if millis != 0 && millis % 1000 == 0 {
        tr_with(
            l,
            Msg::ValueSeconds,
            &[("count", millis.div_euclid(1000).to_string().into())],
        )
    } else {
        tr_with(l, Msg::ValueMillis, &[("count", millis.to_string().into())])
    }
}

/// Returns a size of `bytes`, in the largest whole binary unit.
#[must_use]
pub fn size(l: &Locale, bytes: i64) -> String {
    const KIB: i64 = 1024;
    const MIB: i64 = 1024 * 1024;
    if bytes != 0 && bytes % MIB == 0 {
        tr_with(
            l,
            Msg::ValueMib,
            &[("count", bytes.div_euclid(MIB).to_string().into())],
        )
    } else if bytes != 0 && bytes % KIB == 0 {
        tr_with(
            l,
            Msg::ValueKib,
            &[("count", bytes.div_euclid(KIB).to_string().into())],
        )
    } else {
        tr_with(l, Msg::ValueBytes, &[("count", bytes.to_string().into())])
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
pub fn tab(l: &Locale, configuration: &Loaded<Configuration>, rows: &[Row], again: &str) -> Html {
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => configuration,
        Ok(Outcome::Refused(refusal)) => return refused_operator(l, refusal),
        Err(fault) => return faulted(l, fault, again),
    };
    let drawn = rows
        .iter()
        .map(|row| setting_row(l, setting(l, configuration, row), false))
        .collect();
    let line = file_line(
        l,
        &file_of(configuration),
        How::Dialog(Cow::Borrowed(HOW_DIALOG)),
    );
    settings_panel(line, None, drawn).then(how_dialog(l))
}

/// Returns the dialog "How configuration works": where the settings are
/// shown and how they change.
#[must_use]
pub fn how_dialog(l: &Locale) -> Html {
    let shown = bullet_list(vec![
        t(tr(l, Msg::HowHere)),
        t(tr(l, Msg::HowFaces)),
        t(tr(l, Msg::HowConnections)),
    ]);
    let body = h(format!(
        r#"<p>{intro}</p><div class="fui-ownership"><div><h3>{where_title}</h3><p class="fui-small fui-muted">{where_words}</p>{shown}</div><div><h3>{change_title}</h3><p class="fui-small fui-muted">{change}</p></div></div><p class="fui-small fui-muted">{mappings}</p>"#,
        intro = t(tr(l, Msg::HowIntro)),
        where_title = t(tr(l, Msg::HowWhereTitle)),
        where_words = t(tr(l, Msg::HowWhere)),
        change_title = t(tr(l, Msg::HowChangeTitle)),
        change = t(tr(l, Msg::HowChange)),
        mappings = t(tr(l, Msg::HowMappings)),
    ));
    info_dialog(
        l,
        Explanation {
            id: Cow::Borrowed(HOW_DIALOG),
            title: tr(l, Msg::HowTitle).into(),
        },
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::{duration, size};

    #[test]
    fn a_duration_reads_in_seconds_where_it_is_whole_seconds() {
        let l = &ferrohealth_ui::i18n::Locale::english();
        assert_eq!("30 s", duration(l, 30_000));
        assert_eq!("250 ms", duration(l, 250));
        assert_eq!("0 ms", duration(l, 0));
    }

    #[test]
    fn a_size_reads_in_its_largest_whole_binary_unit() {
        let l = &ferrohealth_ui::i18n::Locale::english();
        assert_eq!("1 MiB", size(l, 1024 * 1024));
        assert_eq!("64 KiB", size(l, 64 * 1024));
        assert_eq!("1000 bytes", size(l, 1000));
    }
}
