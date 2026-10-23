// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The top bar's search over the running load: contexts by name or
//! resource type, mapping files by path and templates by id, never a
//! patient identifier or a value inside a composition.
//!
//! It is a plain `GET` form to the page in view, so a search works with no
//! WebAssembly: the server draws the same page with the answer open under
//! the field, each hit a link, and the way to every matching file.

use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::search::{Hit, HitGroup, Mark, SearchAnswer, SearchBox, Way};
use leptos::prelude::*;

use crate::app::{FILES, TEMPLATES};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::pages::mappings::{program_href, template_id};
use crate::pages::parts::file_href;

/// The most hits a group of the answer shows.
const SHOWN: usize = 5;

/// Returns the search box of the page at `path` holding `words`, with the
/// answer over the running load `state` when the request searched.
#[must_use]
pub fn search_box(path: &str, words: &str, state: Option<&EngineState>) -> AnyView {
    let search = SearchBox {
        id: String::from(crate::shell::SEARCH_ID),
        label: tr(Msg::SearchLabel),
        placeholder: tr(Msg::SearchPlaceholder),
        action: path.to_owned(),
        value: words.to_owned(),
        ..SearchBox::default()
    };
    let answer = state
        .filter(|_| !words.trim().is_empty())
        .map(|state| answer(words.trim(), state));
    ferrohealth_ui::view::search::search_box(search, answer)
}

/// Whether `text` holds `words`, ignoring case.
fn holds(text: &str, words: &str) -> bool {
    text.to_lowercase().contains(&words.to_lowercase())
}

/// The answer for `words` over the running load `state`.
#[must_use]
pub fn answer(words: &str, state: &EngineState) -> SearchAnswer {
    let contexts = state
        .contexts
        .iter()
        .filter(|program| holds(&program.context, words) || holds(&program.resource_type, words))
        .take(SHOWN)
        .filter_map(|program| {
            Some(Hit {
                href: program_href(program)?,
                name: program.context.clone(),
                line: format!("{} · {}", program.resource_type, program.template),
                mark: Some(Mark::Glyph(Glyph::LAYERS)),
                tag: None,
            })
        })
        .collect::<Vec<_>>();
    let mut paths = state
        .contexts
        .iter()
        .flat_map(|program| {
            program.file.iter().chain(
                program
                    .models
                    .iter()
                    .filter_map(|model| model.file.as_ref()),
            )
        })
        .chain(state.omocl.files.iter())
        .filter(|path| holds(path, words))
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let files = paths
        .into_iter()
        .take(SHOWN)
        .map(|path| Hit {
            href: file_href(&path),
            line: tr(Msg::SearchFileLine),
            name: path,
            mark: Some(Mark::Glyph(Glyph::FILE_CODE)),
            tag: None,
        })
        .collect::<Vec<_>>();
    let templates = state
        .templates
        .iter()
        .map(template_id)
        .filter(|id| holds(id, words))
        .take(SHOWN)
        .map(|id| Hit {
            href: format!("{TEMPLATES}/{id}"),
            name: id.to_owned(),
            line: tr(Msg::SearchTemplateLine),
            mark: Some(Mark::Glyph(Glyph::FILE_TEXT)),
            tag: None,
        })
        .collect::<Vec<_>>();
    let found = contexts.len() + files.len() + templates.len();
    let groups = [
        (Msg::TabContexts, contexts),
        (Msg::SearchFiles, files),
        (Msg::TabTemplates, templates),
    ]
    .into_iter()
    .filter(|(_, hits)| !hits.is_empty())
    .map(|(kind, hits)| HitGroup {
        kind: tr(kind),
        hits,
    })
    .collect::<Vec<_>>();
    let every = Way {
        href: format!(
            "{FILES}?q={}",
            url::form_urlencoded::byte_serialize(words.as_bytes()).collect::<String>()
        ),
        words: tr(Msg::SearchEveryFile),
    };
    if groups.is_empty() {
        return SearchAnswer {
            head: None,
            groups,
            none: Some((
                tr_with(Msg::SearchNothing, &[("words", words.into())]),
                tr(Msg::SearchScope),
            )),
            more: Some(every),
        };
    }
    SearchAnswer {
        head: Some(tr_with(
            Msg::SearchFound,
            &[("count", found.into()), ("words", words.into())],
        )),
        groups,
        none: None,
        more: Some(every),
    }
}
