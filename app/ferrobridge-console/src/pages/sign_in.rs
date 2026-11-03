// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in (R19): one card in the centred layout, the provider's button
//! and the notice of the state the sign-in is in.
//!
//! The console signs an operator in at their OpenID Provider alone (OpenID
//! Connect Core 1.0 §3.1), so the card offers the provider's button and no
//! password form. The state comes from where the visitor arrived: a
//! sign-out, a session that ended, a console with no `[oidc]`, or a callback
//! the console refused, which hands its state to the page it draws. The
//! provider's button signs in and comes back to the page the visitor asked
//! for, or to the one FerroUI's session watch left as its return address.

use ferrohealth_session::return_path::ReturnPath;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::session::{RETURN, session_ended, sign_in_address};
use ferrohealth_ui::view::shell::{brand_mark, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, SignIn, State, sign_in};

use crate::app::{Drawn, ENDED, MARK_DARK, MARK_LIGHT, SIGN_IN, SIGNED_OUT};
use crate::locale::messages::Msg;
use crate::visit::Visit;

/// The state a sign-in route hands the page it draws.
#[derive(Debug, Clone)]
pub struct Arrived(pub State);

/// The state of a visitor's sign-in: what the console's configuration and
/// the request's cookies say.
fn arrived(visit: &Visit) -> State {
    if visit.sign_in {
        if visit.ended || visit.param(ENDED) == Some("ended") {
            State::Expired(Notice::new(visit.tr(Msg::SignInExpired)))
        } else {
            State::Ready
        }
    } else {
        State::Unavailable(Notice {
            text: visit.tr(Msg::SignInUnavailable),
            detail: Some(visit.tr(Msg::SignInUnavailableDetail)),
            retry: None,
        })
    }
}

/// Where the provider's button sends a visitor: the sign-in, with the page
/// to come back to when the visitor asked for one of the console's pages.
fn sign_in_href(visit: &Visit) -> String {
    let asked = if visit.path == SIGNED_OUT {
        None
    } else if visit.param(ENDED).is_some() {
        visit.param(RETURN).map(str::to_owned)
    } else {
        Some(visit.here())
    };
    // NOTE: no specification governs this: our own design; `ferrohealth-session` takes only
    // a return address on the console's own origin, so the button names no other.
    asked
        .and_then(|back| ReturnPath::parse(&back, SIGNED_OUT))
        .filter(|back| back.as_str() != "/")
        .map_or_else(
            || String::from(SIGN_IN),
            |back| sign_in_address(SIGN_IN, back.as_str()),
        )
}

/// The sign-in card in `state`.
fn card(visit: &Visit, state: State) -> Drawn {
    card_with(visit, state, Html::empty())
}

/// The sign-in card in `state`, followed by `mark`.
fn card_with(visit: &Visit, state: State, mark: Html) -> Drawn {
    let locale = &visit.locale;
    let lockup = lockup_plain(
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroBRIDGE",
        visit.tr(Msg::Console),
    );
    let card = sign_in(
        locale,
        SignIn {
            lockup,
            title: Some(visit.tr(Msg::SignInTitle)),
            state,
            provider: Some(Provider {
                href: sign_in_href(visit),
                label: visit.tr(Msg::SignInWithProvider),
                busy: visit.tr(Msg::SignInRedirecting),
                glyph: Glyph::LOG_IN,
            }),
            basic: None,
            line: None,
            note: Some(visit.tr(Msg::SignInNote)),
            links: vec![(
                visit.tr(Msg::Documentation),
                String::from("https://ferrobridge.eu/docs/"),
            )],
            caption: Some(visit.tr(Msg::SignInCaption)),
        },
    );
    Drawn::centred(visit.tr(Msg::SignIn), card.then(mark))
}

/// The landing page of a visitor: the sign-in in the state a sign-in route
/// handed over, or the one they arrived in.
#[must_use]
pub fn sign_in_page(visit: &Visit, handed: Option<Arrived>) -> Drawn {
    let state = handed.map_or_else(|| arrived(visit), |Arrived(state)| state);
    card(visit, state)
}

/// The page a signed-out operator lands on: the sign-in, saying so, with
/// the mark by which FerroUI's session watch ends every other tab.
#[must_use]
pub fn signed_out_page(visit: &Visit) -> Drawn {
    card_with(
        visit,
        State::SignedOut(Notice::new(visit.tr(Msg::SignedOut))),
        session_ended(),
    )
}
