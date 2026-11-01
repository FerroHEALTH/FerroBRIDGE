// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator console's browser journeys, behind the
//! `FERROBRIDGE_JOURNEYS` gate (#602).
//!
//! Each journey drives headless Chrome over `WebDriver`, through a
//! chromedriver of its own, against a running bridge (`ferrobridge serve`'s
//! run path over a `wiremock` CDR and a synthetic mapping tree) and a running
//! console that draws every page, whose behaviour FerroUI's script modules
//! add in the browser. The operator signs in at the test OpenID
//! Provider of `ferrobridge_testkit::oidc`, whose access token the bridge
//! validates. Each journey fails on any `SEVERE` entry the browser logs, and
//! waits on elements explicitly, never on a timer.
//!
//! Without `FERROBRIDGE_JOURNEYS=1` each journey returns at once, so the
//! ordinary suite needs no browser. The `journeys (browser)` job of
//! `.github/workflows/ci.yml` sets it and installs the pinned Chrome for
//! Testing. The book's screenshots are taken on the same stack
//! behind a gate of their own, `FERROBRIDGE_SCREENSHOTS` (`screenshots`),
//! whose guard runs in the ordinary suite. No specification governs the
//! journeys: our own design.

mod browser;
mod configuration;
mod editor;
mod engine;
mod navigation;
mod no_script;
mod playground;
mod reflow;
mod screenshots;
mod shell;
mod sign_in;
mod stack;
mod states;

/// The environment variable that opts in to the browser journeys.
const JOURNEYS_GATE: &str = "FERROBRIDGE_JOURNEYS";

/// Whether the browser journeys run: only when [`JOURNEYS_GATE`] is `1`.
fn journeys_enabled() -> bool {
    std::env::var(JOURNEYS_GATE).is_ok_and(|value| value == "1")
}
