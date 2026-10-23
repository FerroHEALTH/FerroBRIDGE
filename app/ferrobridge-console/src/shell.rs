// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every signed-in page, drawn with the `FerroHEALTH`
//! components of `ferrohealth_ui`.
//!
//! It is the top bar with the bridge's status and version, the theme toggle
//! and the operator's menu; the navigation grouped Overview, Mappings, Tools
//! and Bridge, a column beside the page from `md` and a drawer below it; and
//! the one-line footer every page carries. The drawer and the operator's
//! menu are HTML popovers, which the browser opens and closes with no
//! script; the library's navigation adds the drawer's focus handling. No
//! specification governs the console: our own design.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::theme::Theme;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::menu::{OperatorMenu, operator_menu};
use ferrohealth_ui::view::shell::{Footer, drawer_button, mark};
use ferrohealth_ui::view::status::status;
use ferrohealth_ui::widget::nav::{Entry, Group};
use ferrohealth_ui::widget::theme::theme_toggle;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::app::{ABOUT, PRODUCT, SIGN_OUT, VERSION};
use crate::locale::{Msg, tr, tr_with};
use crate::model::{BridgeView, Outcome, Refusal, ViewError};
use crate::theme::SetTheme;

/// The id of the navigation, which the drawer button opens.
const SIDEBAR_ID: &str = "sidebar";

/// The id of the operator's menu, which the operator's button opens.
const MENU_ID: &str = "operator-menu";

/// What the footer of every page names.
#[must_use]
pub fn footer_names() -> Footer {
    Footer {
        product: PRODUCT.into(),
        version: VERSION,
        about: ABOUT,
    }
}

/// What the top bar of a signed-in page shows.
#[derive(Debug, Clone, Copy)]
pub struct Bar {
    /// The theme in force, which the server wrote on `<html>`.
    pub theme: Theme,
    /// The bridge's status, loaded once for the page.
    pub bridge: Resource<Result<Outcome<BridgeView>, ViewError>>,
}

/// The navigation, in the order it is drawn.
#[must_use]
pub fn navigation() -> [Group; 4] {
    [
        Group {
            id: "nav-overview",
            title: tr(Msg::Overview).into(),
            entries: vec![Entry {
                href: "/",
                label: tr(Msg::Dashboard).into(),
                glyph: Glyph::LAYOUT_DASHBOARD,
                exact: true,
            }],
        },
        Group {
            id: "nav-mappings",
            title: tr(Msg::Mappings).into(),
            entries: vec![
                Entry {
                    href: crate::engine::MAPPINGS,
                    label: tr(Msg::Overview).into(),
                    glyph: Glyph::LAYERS,
                    exact: true,
                },
                Entry {
                    href: crate::engine::FILES,
                    label: tr(Msg::Files).into(),
                    glyph: Glyph::FILES,
                    exact: false,
                },
                Entry {
                    href: crate::editor::EDIT,
                    label: tr(Msg::NewFile).into(),
                    glyph: Glyph::FILE_PLUS,
                    exact: true,
                },
            ],
        },
        Group {
            id: "nav-tools",
            title: tr(Msg::Tools).into(),
            entries: vec![
                Entry {
                    href: crate::tools::PLAYGROUND,
                    label: tr(Msg::Playground).into(),
                    glyph: Glyph::FLASK_CONICAL,
                    exact: true,
                },
                Entry {
                    href: crate::tools::VALIDATE,
                    label: tr(Msg::ValidationDryRun).into(),
                    glyph: Glyph::LIST_CHECKS,
                    exact: true,
                },
            ],
        },
        Group {
            id: "nav-bridge",
            title: tr(Msg::Bridge).into(),
            entries: vec![
                Entry {
                    href: crate::engine::CAPABILITY,
                    label: tr(Msg::Capability).into(),
                    glyph: Glyph::SHIELD_CHECK,
                    exact: true,
                },
                Entry {
                    href: ABOUT,
                    label: tr(Msg::About).into(),
                    glyph: Glyph::INFO,
                    exact: true,
                },
            ],
        },
    ]
}
/// The mark and the product name, which lead to the dashboard.
#[must_use]
pub fn lockup() -> impl IntoView {
    ferrohealth_ui::view::shell::lockup("/", mark(Glyph::LAYERS), "FerroBRIDGE", tr(Msg::Console))
}

/// The top bar of a signed-in page: the drawer button, the lockup, and at
/// the far end the bridge's status, the operator and the theme toggle.
#[must_use]
pub fn topbar(operator: String, bar: Bar) -> AnyView {
    let bridge = bar.bridge;
    let status = view! {
        <Suspense fallback=|| {
            status(Tone::Neutral, Glyph::SERVER, &tr(Msg::Bridge), None)
        }>{move || Suspend::new(async move { bridge_pill(&bridge.await) })}</Suspense>
    };
    let location = leptos_router::hooks::use_location();
    // NOTE: no specification governs this: our own design; the toggle swaps the
    // theme in place and the server function keeps it in the session's cookie.
    let keep = ServerAction::<SetTheme>::new();
    let chosen = Callback::new(move |theme: Theme| {
        keep.dispatch(SetTheme {
            theme: theme.key().to_owned(),
            back: None,
        });
    });
    let switch = move || theme_toggle(SetTheme::url().to_owned(), location.pathname.get(), chosen);
    let start = view! {
        {drawer_button(SIDEBAR_ID)}
        {lockup()}
    };
    let end = view! {
        <span class="fui-roomy-only">{status}</span>
        {operator_menu(
            OperatorMenu {
                id: MENU_ID,
                name: operator,
                subtitle: tr(Msg::SignedConsole).into(),
                sign_out: Some(String::from(SIGN_OUT)),
            },
            (),
        )}
        {switch}
    };
    ferrohealth_ui::view::shell::topbar(start, end).into_any()
}

/// Returns the tone and the words of the bridge's status.
///
/// They are `UP` with its version once the bridge answered, its status when
/// it refused, `DOWN` when the console could not reach it, and `UNKNOWN`
/// for any other fault.
#[must_use]
pub fn bridge_status(answer: &Result<Outcome<BridgeView>, ViewError>) -> (Tone, String) {
    match answer {
        Ok(Outcome::Shown(view)) => {
            let words = view.version.as_deref().map_or_else(
                || tr(Msg::BridgeUp),
                |version| tr_with(Msg::BridgeUpVersion, &[("version", version.into())]),
            );
            (Tone::Ok, words)
        }
        Ok(Outcome::Refused(Refusal::Bridge { status, .. })) => (
            Tone::Bad,
            tr_with(
                Msg::BridgeRefusedCode,
                &[("status", status.to_string().into())],
            ),
        ),
        Ok(Outcome::Refused(Refusal::NotAuthenticated)) => (Tone::Bad, tr(Msg::BridgeRefused401)),
        Err(ViewError::Unreachable) => (Tone::Bad, tr(Msg::BridgeDown)),
        Ok(Outcome::Refused(Refusal::Invalid { .. })) | Err(_) => {
            (Tone::Warn, tr(Msg::BridgeUnknown))
        }
    }
}

/// The bridge's status: an icon and a word, never a colour alone.
fn bridge_pill(answer: &Result<Outcome<BridgeView>, ViewError>) -> AnyView {
    let (tone, words) = bridge_status(answer);
    let glyph = match tone {
        Tone::Ok => Glyph::CIRCLE_CHECK,
        Tone::Bad => Glyph::SERVER_CRASH,
        _ => Glyph::TRIANGLE_ALERT,
    };
    status(tone, glyph, &words, None).into_any()
}

/// The navigation a signed-in operator reads, grouped by what each page is
/// about.
#[must_use]
pub fn sidebar() -> AnyView {
    ferrohealth_ui::widget::nav::sidebar(SIDEBAR_ID, tr(Msg::Pages), navigation().into())
}

/// The one line at the foot of every page: the product, its version, its
/// maker, and the About page.
#[must_use]
pub fn footer() -> impl IntoView {
    ferrohealth_ui::view::shell::footer(footer_names())
}
