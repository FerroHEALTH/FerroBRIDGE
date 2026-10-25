// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (R20): a path the console does not serve (`404`) in the
//! shell, and a session store the console cannot read (`500`) in the
//! centred layout, since it cannot tell whether anyone is signed in.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};
use ferrohealth_ui::view::shell::centred;
use leptos::prelude::*;

use crate::app::{DASHBOARD, MAPPINGS, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;

/// Sets the answer's status to `status`, while the server draws it.
fn answer_with(status: http::StatusCode) {
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(status);
    }
}

/// The page for a path the console does not serve: `404`, what it may
/// mean, and the ways on.
#[must_use]
pub fn not_found() -> impl IntoView {
    answer_with(http::StatusCode::NOT_FOUND);
    let page = error_page(ErrorPage {
        status: Status::NotFound,
        title: tr(Msg::NotFoundTitle),
        body: view! { <p>{tr(Msg::NotFoundBody)}</p> }.into_any(),
        hint: None,
        ways: vec![
            Way {
                label: tr(Msg::BackToDashboard),
                href: String::from(DASHBOARD),
                glyph: Some(Glyph::ARROW_LEFT),
            },
            Way {
                label: tr(Msg::EveryFile),
                href: format!("{MAPPINGS}/files"),
                glyph: None,
            },
        ],
        detail: None,
    });
    view! {
        {page_title(tr(Msg::NotFoundTitle))}
        {page}
    }
}

/// The page a request reads when the console cannot read its session store.
///
/// It cannot tell whether anyone is signed in, so it offers neither the
/// sign-in nor the operator's pages, says who can fix it, and answers `500`.
#[must_use]
pub fn unreadable() -> impl IntoView {
    answer_with(http::StatusCode::INTERNAL_SERVER_ERROR);
    let page = error_page(ErrorPage {
        status: Status::Fault,
        title: tr(Msg::UnreadableTitle),
        body: view! { <p>{tr(Msg::UnreadableBody)}</p> }.into_any(),
        hint: None,
        ways: Vec::new(),
        detail: None,
    });
    view! {
        {page_title(tr(Msg::UnreadableTitle))}
        {centred(page, crate::shell::footer_names())}
    }
}
