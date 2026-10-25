// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in (R19): one card in the centred layout, the provider's button
//! and the notice of the state the sign-in is in.
//!
//! The console signs an operator in at their OpenID Provider alone (OpenID
//! Connect Core 1.0 §3.1), so the card offers the provider's button and no
//! password form. The state comes from where the visitor arrived: a
//! sign-out, a session that ended, a console with no `[oidc]`, or a callback
//! the console refused, which hands its state to the page it draws.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::shell::{brand_mark, centred, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, SignIn, State, sign_in};
use leptos::prelude::*;

use crate::app::{MARK_DARK, MARK_LIGHT, SIGN_IN, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;

/// The state a sign-in route hands the page it draws.
#[derive(Debug, Clone)]
pub struct Arrived(pub State);

/// The state of a visitor's sign-in: the one a route handed over, or what
/// the console's configuration and the request's cookies say.
fn arrived() -> State {
    if let Some(Arrived(state)) = use_context::<Arrived>() {
        return state;
    }
    if crate::session::request::configured_sign_in() {
        if crate::session::request::ended() {
            State::Expired(Notice::new(tr(Msg::SignInExpired)))
        } else {
            State::Ready
        }
    } else {
        State::Unavailable(Notice {
            text: tr(Msg::SignInUnavailable),
            detail: Some(tr(Msg::SignInUnavailableDetail)),
            retry: None,
        })
    }
}

/// The sign-in card in `state`.
fn card(state: State) -> AnyView {
    let lockup = lockup_plain(
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroBRIDGE",
        tr(Msg::Console),
    )
    .into_any();
    let card = sign_in(SignIn {
        lockup,
        title: Some(tr(Msg::SignInTitle)),
        state,
        provider: Some(Provider {
            href: String::from(SIGN_IN),
            label: tr(Msg::SignInWithProvider),
            busy: tr(Msg::SignInRedirecting),
            glyph: Glyph::LOG_IN,
        }),
        basic: None,
        line: None,
        note: Some(tr(Msg::SignInNote)),
        links: vec![(
            tr(Msg::Documentation),
            String::from("https://ferrobridge.eu/docs/"),
        )],
        caption: Some(tr(Msg::SignInCaption)),
    });
    view! {
        {page_title(tr(Msg::SignIn))}
        {centred(card.into_any(), crate::shell::footer_names())}
    }
    .into_any()
}

/// The landing page of a visitor: the sign-in in the state they arrived in.
#[must_use]
pub fn sign_in_page() -> impl IntoView {
    card(arrived())
}

/// The page a signed-out operator lands on: the sign-in, saying so.
#[must_use]
pub fn signed_out_page() -> impl IntoView {
    card(State::SignedOut(Notice::new(tr(Msg::SignedOut))))
}
