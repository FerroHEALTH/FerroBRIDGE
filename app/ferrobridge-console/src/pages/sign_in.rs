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
use ferrohealth_ui::view::shell::{brand_mark, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, SignIn, State, sign_in};

use crate::app::{Drawn, MARK_DARK, MARK_LIGHT, SIGN_IN};
use crate::locale::messages::Msg;
use crate::visit::Visit;

/// The state a sign-in route hands the page it draws.
#[derive(Debug, Clone)]
pub struct Arrived(pub State);

/// The state of a visitor's sign-in: what the console's configuration and
/// the request's cookies say.
fn arrived(visit: &Visit) -> State {
    if visit.sign_in {
        if visit.ended {
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

/// The sign-in card in `state`.
fn card(visit: &Visit, state: State) -> Drawn {
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
                href: String::from(SIGN_IN),
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
    Drawn::centred(visit.tr(Msg::SignIn), card)
}

/// The landing page of a visitor: the sign-in in the state a sign-in route
/// handed over, or the one they arrived in.
#[must_use]
pub fn sign_in_page(visit: &Visit, handed: Option<Arrived>) -> Drawn {
    let state = handed.map_or_else(|| arrived(visit), |Arrived(state)| state);
    card(visit, state)
}

/// The page a signed-out operator lands on: the sign-in, saying so.
#[must_use]
pub fn signed_out_page(visit: &Visit) -> Drawn {
    card(
        visit,
        State::SignedOut(Notice::new(visit.tr(Msg::SignedOut))),
    )
}
