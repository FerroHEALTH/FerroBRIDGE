// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The top bar's search over the running load: contexts by name or
//! resource type, mapping files by path and templates by id, never a
//! patient identifier or a value inside a composition.
//!
//! As you type, the panel under the field asks [`ANSWER`], which answers
//! with the [`SearchAnswer`] as JSON. Enter, or "See all results", goes to
//! the search page ([`SEARCH`]), which lists every hit in the page; with no
//! WebAssembly the form submits there as a plain `GET`.

#[cfg(not(target_arch = "wasm32"))]
use axum::extract::{Extension, Query};
#[cfg(not(target_arch = "wasm32"))]
use axum::response::{IntoResponse, Response};
use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::search::{
    Hit, HitGroup, Mark, SearchAnswer, SearchBox, Way, find_results,
};
use ferrohealth_ui::view::surface::{Header, page_heading};
#[cfg(not(target_arch = "wasm32"))]
use http::StatusCode;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::app::{FILES, TEMPLATES, page_title};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome};
use crate::pages::mappings::{program_href, template_id};
use crate::pages::parts::{faulted, file_href, refused_operator};

/// The search page, which lists every hit for the words in `q`.
pub const SEARCH: &str = "/search";

/// The route the panel under the field asks as you type.
pub const ANSWER: &str = "/search/answer";

/// Returns the top bar's search box holding `words`.
#[must_use]
pub fn search_box(words: &str) -> AnyView {
    let search = SearchBox {
        id: String::from(crate::shell::SEARCH_ID),
        label: tr(Msg::SearchLabel),
        placeholder: tr(Msg::SearchPlaceholder),
        action: String::from(SEARCH),
        value: words.to_owned(),
        suggest: Some(String::from(ANSWER)),
        ..SearchBox::default()
    };
    ferrohealth_ui::view::search::search_box(search)
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
            query: words.to_owned(),
            head: None,
            unfinished: None,
            groups,
            none: Some((
                tr_with(Msg::SearchNothing, &[("words", words.into())]),
                tr(Msg::SearchScope),
            )),
            more: Some(every),
        };
    }
    SearchAnswer {
        query: words.to_owned(),
        unfinished: None,
        head: Some(tr_with(
            Msg::SearchFound,
            &[("count", found.into()), ("words", words.into())],
        )),
        groups,
        none: None,
        more: Some(every),
    }
}

/// What the panel under the field asks: the words typed.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Deserialize)]
pub struct Asked {
    /// The words typed.
    #[serde(default)]
    q: String,
}

/// Serves [`ANSWER`]: the answer over the running load for the words typed,
/// as JSON; `401` without a live session and `502` when the bridge does not
/// answer, which the panel shows as a row.
#[cfg(not(target_arch = "wasm32"))]
pub async fn answer_route(
    Extension(state): Extension<crate::server::ConsoleState>,
    headers: http::HeaderMap,
    Query(asked): Query<Asked>,
) -> Response {
    let token = state
        .sessions()
        .cookie_in(&headers, crate::session::COOKIE)
        .and_then(|id| state.sessions().access_token(&id).ok().flatten());
    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let words = asked.q.trim();
    match crate::model::loaded(
        state
            .bridge()
            .operator_state(&crate::bridge::AccessToken::new(token))
            .await,
    ) {
        Ok(Outcome::Shown(running)) => axum::Json(answer(words, &running)).into_response(),
        Ok(Outcome::Refused(_)) => StatusCode::FORBIDDEN.into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// The search page: every context, file and template the words in `q` find.
#[must_use]
pub fn page() -> impl IntoView {
    let words = use_query_map()
        .with_untracked(|query| query.get("q"))
        .unwrap_or_default();
    let state = Resource::new(crate::load::changes, |_| crate::load::state_now());
    view! {
        {page_title(tr(Msg::SearchTitle))}
        <Suspense>
            {move || {
                let words = words.clone();
                Suspend::new(async move {
                    let state = state.await;
                    drawn(words.trim(), &state)
                })
            }}
        </Suspense>
    }
}

/// The search page for `words` over the running load `state`.
fn drawn(words: &str, state: &Loaded<EngineState>) -> AnyView {
    let heading = page_heading(
        Header {
            title: tr(Msg::SearchTitle).into(),
            subtitle: Some(tr(Msg::SearchScope).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let body = match state {
        _ if words.is_empty() => {
            view! { <p class="fui-muted">{tr(Msg::SearchEmpty)}</p> }.into_any()
        }
        Ok(Outcome::Shown(state)) => find_results(answer(words, state)),
        Ok(Outcome::Refused(refusal)) => refused_operator(refusal),
        Err(fault) => faulted(fault, SEARCH),
    };
    view! {
        {heading}
        <div class="fui-list-page">{body}</div>
    }
    .into_any()
}
