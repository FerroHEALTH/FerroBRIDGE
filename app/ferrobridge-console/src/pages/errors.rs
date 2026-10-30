// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (R20): a path the console does not serve (`404`) in the
//! shell, and a session store the console cannot read (`500`) in the
//! centred layout, since it cannot tell whether anyone is signed in.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};
use http::StatusCode;

use crate::app::{DASHBOARD, Drawn, MAPPINGS};
use crate::locale::messages::Msg;
use crate::markup::t;
use crate::visit::Visit;

/// The page for a path the console does not serve: `404`, what it may
/// mean, and the ways on.
#[must_use]
pub fn not_found(visit: &Visit) -> Drawn {
    let page = error_page(
        &visit.locale,
        ErrorPage {
            status: Status::NotFound,
            title: visit.tr(Msg::NotFoundTitle),
            body: t(visit.tr(Msg::NotFoundBody)),
            hint: None,
            ways: vec![
                Way {
                    label: visit.tr(Msg::BackToDashboard),
                    href: String::from(DASHBOARD),
                    glyph: Some(Glyph::ARROW_LEFT),
                },
                Way {
                    label: visit.tr(Msg::EveryFile),
                    href: format!("{MAPPINGS}/files"),
                    glyph: None,
                },
            ],
            detail: None,
        },
    );
    Drawn::shell(visit.tr(Msg::NotFoundTitle), page).with_status(StatusCode::NOT_FOUND)
}

/// The page a request reads when the console cannot read its session store.
///
/// It cannot tell whether anyone is signed in, so it offers neither the
/// sign-in nor the operator's pages, says who can fix it, and answers `500`.
#[must_use]
pub fn unreadable(visit: &Visit) -> Drawn {
    let page = error_page(
        &visit.locale,
        ErrorPage {
            status: Status::Fault,
            title: visit.tr(Msg::UnreadableTitle),
            body: t(visit.tr(Msg::UnreadableBody)),
            hint: None,
            ways: Vec::new(),
            detail: None,
        },
    );
    Drawn::centred(visit.tr(Msg::UnreadableTitle), page)
        .with_status(StatusCode::INTERNAL_SERVER_ERROR)
}
