// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every signed-in page.
//!
//! It is the top bar with the bridge's status and version, the theme switch
//! and the operator's menu; the navigation grouped Overview, Mappings, Tools
//! and Bridge, a column beside the page from `md` and a drawer below it; and
//! the one-line footer every page carries.
//!
//! The drawer and the operator's menu are HTML popovers the top bar's
//! buttons open, so the browser opens them, closes them on Escape and
//! returns the focus to the button, before the page has hydrated and with
//! no script (HTML, the `popover` attribute). The browser half adds what
//! HTML leaves out: the focus moves into the drawer as it opens, and the
//! drawer closes when one of its links is followed. No specification
//! governs the console: our own design.

use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_router::components::A;

use crate::app::{ABOUT, MANUFACTURER, PRODUCT, SIGN_OUT, VERSION};
use crate::model::{BridgeView, Outcome, Refusal, ViewError};
use crate::styles;
use crate::theme::{SetTheme, Theme};

/// The id of the navigation, which the drawer button opens.
const SIDEBAR_ID: &str = "sidebar";

/// The id of the operator's menu, which the operator's button opens.
const MENU_ID: &str = "operator-menu";

/// What the top bar of a signed-in page shows.
#[derive(Debug, Clone, Copy)]
pub struct Bar {
    /// The theme in force, which the switch marks pressed.
    pub theme: Theme,
    /// The bridge's status, loaded once for the page.
    pub bridge: Resource<Result<Outcome<BridgeView>, ViewError>>,
}

/// One entry of the navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Where it leads.
    pub href: &'static str,
    /// What it says.
    pub label: &'static str,
}

/// One group of the navigation, under its heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    /// The id its heading carries, which names its list.
    pub id: &'static str,
    /// Its heading.
    pub title: &'static str,
    /// Its entries, in order; a group with none is not drawn.
    pub entries: &'static [Entry],
}

/// The navigation, in the order it is drawn.
// TODO(#605): the Tools group lists the transform playground and the validation dry run.
pub const NAVIGATION: [Group; 4] = [
    Group {
        id: "nav-overview",
        title: "Overview",
        entries: &[Entry {
            href: "/",
            label: "Dashboard",
        }],
    },
    Group {
        id: "nav-mappings",
        title: "Mappings",
        entries: &[
            Entry {
                href: crate::engine::MAPPINGS,
                label: "Overview",
            },
            Entry {
                href: crate::engine::FILES,
                label: "Files",
            },
        ],
    },
    Group {
        id: "nav-tools",
        title: "Tools",
        entries: &[],
    },
    Group {
        id: "nav-bridge",
        title: "Bridge",
        entries: &[
            Entry {
                href: crate::engine::CAPABILITY,
                label: "Capability",
            },
            Entry {
                href: ABOUT,
                label: "About",
            },
        ],
    },
];

/// The top bar of a signed-in page: the drawer button, the lockup, and at
/// the far end the bridge's status, the theme switch and the operator.
#[must_use]
pub fn topbar(operator: String, bar: Bar) -> AnyView {
    let bridge = bar.bridge;
    let status = view! {
        <Suspense fallback=|| {
            view! { <span class=styles::Tone::Neutral.pill()>"Bridge"</span> }
        }>{move || Suspend::new(async move { bridge_pill(&bridge.await) })}</Suspense>
    };
    view! {
        <header class=styles::TOPBAR>
            <div class=styles::TOPBAR_ROW>
                <button
                    type="button"
                    class=styles::DRAWER_BUTTON
                    popovertarget=SIDEBAR_ID
                    aria-controls=SIDEBAR_ID
                >
                    "Menu"
                </button>
                {lockup()}
                <div class=styles::TOPBAR_END>
                    {status} <div class="hidden sm:block">{theme_switch(bar.theme)}</div>
                    {operator_menu(operator, bar.theme)}
                </div>
            </div>
        </header>
    }
    .into_any()
}

/// The mark and the product name, which lead to the dashboard.
#[must_use]
pub fn lockup() -> impl IntoView {
    view! {
        <a href="/" class=styles::LOCKUP>
            // The brand mark is the stylesheet's, drawn for the theme; it is
            // decoration, and the product name stands beside it.
            <span class=styles::MARK aria-hidden="true"></span>
            <span class=styles::LOCKUP_NAME>
                "FerroBRIDGE " <span class=styles::LOCKUP_ACCENT>"Console"</span>
            </span>
        </a>
    }
}

/// Returns the tone and the words of the bridge's status.
///
/// They are `UP` with its version once the bridge answered, its status when
/// it refused, `DOWN` when the console could not reach it, and `UNKNOWN`
/// for any other fault.
#[must_use]
pub fn bridge_status(answer: &Result<Outcome<BridgeView>, ViewError>) -> (styles::Tone, String) {
    match answer {
        Ok(Outcome::Shown(view)) => {
            let version = view
                .version
                .as_deref()
                .map(|version| format!(" · v{version}"))
                .unwrap_or_default();
            (styles::Tone::Ok, format!("Bridge UP{version}"))
        }
        Ok(Outcome::Refused(Refusal::Bridge { status, .. })) => {
            (styles::Tone::Danger, format!("Bridge refused {status}"))
        }
        Ok(Outcome::Refused(Refusal::NotAuthenticated)) => {
            (styles::Tone::Danger, String::from("Bridge refused 401"))
        }
        Err(ViewError::Unreachable) => (styles::Tone::Danger, String::from("Bridge DOWN")),
        Ok(Outcome::Refused(Refusal::Invalid { .. })) | Err(_) => {
            (styles::Tone::Warn, String::from("Bridge UNKNOWN"))
        }
    }
}

/// The bridge's status as a pill: a word, never a colour alone.
fn bridge_pill(answer: &Result<Outcome<BridgeView>, ViewError>) -> AnyView {
    let (tone, words) = bridge_status(answer);
    view! { <span class=tone.pill()>{words}</span> }.into_any()
}

/// The theme switch: light, dark and system, the one in force pressed.
///
/// Each button posts its theme to the server function as a plain form,
/// before the page has hydrated and after, and the server keeps the choice
/// and sends the browser back to the page, which it then draws in that
/// theme. The browser half carries no code for it.
fn theme_switch(theme: Theme) -> AnyView {
    let buttons = Theme::ALL
        .into_iter()
        .map(|choice| {
            view! {
                <button
                    type="submit"
                    name="theme"
                    value=choice.key()
                    class=styles::SEGMENT
                    aria-pressed=(theme == choice).to_string()
                >
                    {choice.label()}
                </button>
            }
        })
        .collect_view();
    let location = leptos_router::hooks::use_location();
    view! {
        <form method="post" action=SetTheme::url() class="m-0 flex flex-col items-end">
            // The page the post returns to, which the server checks is one of
            // the console's own.
            <input type="hidden" name="back" value=move || location.pathname.get() />
            <div role="group" aria-label="Theme" class=styles::SEGMENTED>
                {buttons}
            </div>
        </form>
    }
    .into_any()
}

/// The operator's name, and the menu it opens with the sign-out, and below
/// `sm`, where the top bar has no room for it, the theme switch.
fn operator_menu(operator: String, theme: Theme) -> impl IntoView {
    let shown = operator.clone();
    view! {
        <button type="button" class=styles::MENU_BUTTON popovertarget=MENU_ID aria-controls=MENU_ID>
            <span class="max-w-28 truncate sm:max-w-40">{operator}</span>
        </button>
        <div id=MENU_ID popover="auto" class=styles::MENU>
            <p class="m-0 text-sm font-semibold">{shown}</p>
            <p class="mt-0.5 mb-3 text-xs text-muted">"Signed in to this console"</p>
            <div class="mb-3 sm:hidden">{theme_switch(theme)}</div>
            // The sign-out route is the server's: a plain form post it answers
            // with a redirect, which no client router takes.
            <form method="post" action=SIGN_OUT class="m-0">
                <button type="submit" class=format!("{} w-full", styles::BUTTON_SECONDARY)>
                    "Sign out"
                </button>
            </form>
        </div>
    }
}

/// The navigation entry `entry`, tinted while it is the page: the router's
/// link marks itself `aria-current="page"` there, the dashboard on `/`
/// alone.
fn entry(entry: Entry) -> impl IntoView {
    let exact = entry.href == "/";
    view! {
        <li>
            <A href=entry.href exact=exact attr:class=styles::NAV_ENTRY>
                {entry.label}
            </A>
        </li>
    }
}

/// The group `group`, its heading naming its list; nothing for a group with
/// no entry.
fn group(group: Group) -> Option<impl IntoView> {
    if group.entries.is_empty() {
        return None;
    }
    let entries = group.entries.iter().copied().map(entry).collect_view();
    Some(view! {
        <p id=group.id class=styles::NAV_GROUP>
            {group.title}
        </p>
        <ul class=styles::NAV_LIST aria-labelledby=group.id>
            {entries}
        </ul>
    })
}

/// The navigation a signed-in operator reads, grouped by what each page is
/// about.
#[must_use]
pub fn sidebar() -> AnyView {
    let nav = NodeRef::<leptos::html::Nav>::new();
    let groups = NAVIGATION.into_iter().map(group).collect_view();
    view! {
        <nav
            id=SIDEBAR_ID
            popover="auto"
            aria-label="Pages"
            class=styles::SIDEBAR
            node_ref=nav
            on:toggle=move |_| drawer::opened(nav)
            on:click=move |event| drawer::followed(nav, &event)
        >
            <button
                type="button"
                class=format!("{} mb-1 md:hidden", styles::BUTTON_GHOST)
                popovertarget=SIDEBAR_ID
                popovertargetaction="hide"
            >
                "Close"
            </button>
            {groups}
        </nav>
    }
    .into_any()
}

/// What the browser half adds to the drawer: the focus moves into it as it
/// opens, and it closes when one of its links is followed (WCAG 2.2 SC
/// 2.4.3). From `md` up the navigation is no popover the browser opens, so
/// neither applies.
mod drawer {
    use leptos::prelude::*;

    /// Moves the focus to the drawer's first link, once it is open.
    pub(super) fn opened(nav: NodeRef<leptos::html::Nav>) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast as _;
            let Some(nav) = nav.get_untracked() else {
                return;
            };
            if !nav.matches(":popover-open").unwrap_or(false) {
                return;
            }
            let first = nav
                .query_selector("a")
                .ok()
                .flatten()
                .and_then(|link| link.dyn_into::<leptos::web_sys::HtmlElement>().ok());
            if let Some(first) = first
                && first.focus().is_err()
            {
                leptos::logging::warn!("the browser refused the focus on the navigation");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _: NodeRef<leptos::html::Nav> = nav;
    }

    /// Closes the open drawer when `event` follows one of its links.
    pub(super) fn followed(nav: NodeRef<leptos::html::Nav>, event: &leptos::ev::MouseEvent) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast as _;
            let Some(nav) = nav.get_untracked() else {
                return;
            };
            let on_link = event
                .target()
                .and_then(|target| target.dyn_into::<leptos::web_sys::Element>().ok())
                .and_then(|target| target.closest("a").ok().flatten())
                .is_some();
            if on_link
                && nav.matches(":popover-open").unwrap_or(false)
                && nav.hide_popover().is_err()
            {
                leptos::logging::warn!("the browser refused to close the navigation");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _: (NodeRef<leptos::html::Nav>, &leptos::ev::MouseEvent) = (nav, event);
    }
}

/// The one line at the foot of every page: the product, its version, its
/// maker, and the About page.
#[must_use]
pub fn footer() -> impl IntoView {
    view! {
        <footer class=styles::FOOTER>
            <p class="m-0">
                {PRODUCT} " v" {VERSION} " · " {MANUFACTURER} " · "
                <a href=ABOUT>"About"</a>
            </p>
        </footer>
    }
}
