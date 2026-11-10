// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Events page: on a bridge whose events lane cannot reach its broker,
//! the consumer reads degraded with its cause and the configured topology,
//! and the event it parked is replayed from the page; on a bridge with no
//! consumer, the page says so.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, PARKED_ID, Stack};

#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_consumer_reads_degraded_with_its_cause() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithEvents).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/events")).await?;
        browser.titled(&format!("Events · {PRODUCT}")).await?;
        browser
            .element(By::XPath("//*[contains(., 'FerroEHR outbox feed')]"))
            .await?;
        browser
            .element(By::XPath("//code[. = 'ferroehr.events.analytics']"))
            .await?;
        // The consumer stalls on its first pass, once the broker refuses it.
        until("the consumer stalls on the broker", async || {
            browser.goto(&stack.url("/events")).await?;
            Ok(!browser
                .driver()
                .find_all(By::XPath("//*[contains(., 'could not be reached')]"))
                .await?
                .is_empty())
        })
        .await?;
        browser
            .element(By::XPath("//*[normalize-space(.) = 'Degraded']"))
            .await?;
        Ok(())
    })
    .await?;
    stack.stop().await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parked_event_is_replayed_from_the_page() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithEvents).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/events")).await?;
        browser
            .element(By::XPath(
                "//code[. = '0d4a3c52-5b51-4b4b-9e0e-000000000017']",
            ))
            .await?;
        browser
            .element(By::XPath("//*[contains(., 'the event names no template')]"))
            .await?;
        browser
            .element(By::XPath(format!(
                "//button[@aria-label = 'Replay parked event {PARKED_ID} of events']"
            )))
            .await?
            .click()
            .await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'Replay requested: the consumer offers the event again before its next page.')]",
            ))
            .await?;
        // The broker is down, so every pass's replay of the parked event
        // meets a transient failure and the replay stays requested.
        browser.goto(&stack.url("/events")).await?;
        browser
            .element(By::XPath("//*[normalize-space(.) = 'Replay requested']"))
            .await?;
        Ok(())
    })
    .await?;
    stack.stop().await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bridge_with_no_events_lane_says_so() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/events")).await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'No commit event is published to a broker.')]",
            ))
            .await?;
        Ok(())
    })
    .await?;
    stack.stop().await
}
