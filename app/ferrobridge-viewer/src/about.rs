// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The About page: the console, its version, its maker and its licence, and
//! for a signed-in operator the bridge it is a client of.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::{MANUFACTURER, PRODUCT, VERSION};
use crate::locale::{Msg, tr, tr_with};

/// The page that says what the console is and who makes it.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn AboutPage(
    /// The bridge's base URL, shown to a signed-in operator; empty for a
    /// visitor.
    bridge: String,
) -> impl IntoView {
    let bridge = (!bridge.is_empty()).then(|| {
        view! {
            <dt>{tr(Msg::Bridge)}</dt>
            <dd>
                <code>{bridge}</code>
            </dd>
        }
    });
    view! {
        <Title text=format!("{} · {PRODUCT}", tr(Msg::About)) />
        {ferrohealth_ui::view::surface::page_header(
            tr(Msg::About),
            Some(tr_with(Msg::AboutBody, &[("product", PRODUCT.into())])),
            (),
        )}
        <section class="fui-card">
        <h2 class="fui-card-title">{PRODUCT}</h2>
        <dl class="fui-facts">
            <dt>{tr(Msg::Version)}</dt>
            <dd>{VERSION}</dd>
            <dt>{tr(Msg::MadeBy)}</dt>
            <dd>
                {MANUFACTURER} " ("
                <a href="https://www.cadasto.com/contact/">"www.cadasto.com/contact"</a> ")"
            </dd>
            <dt>{tr(Msg::Licence)}</dt>
            <dd>{tr(Msg::BusinessSourceLicense11)}</dd>
            {bridge}
        </dl>
        </section>
    }
}
