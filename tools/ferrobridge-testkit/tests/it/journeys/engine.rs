// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine's state as the bridge reports it: the dashboard, the Mapping
//! set, the facade's capability, the file list, and the page of each
//! mapping file, each drawn from what the running bridge answered.

use std::error::Error;

use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{CONTEXT, MODEL_PATH, PROFILE, Stack};

/// The wire path of the provisioned context mapping.
const CONTEXT_PATH: &str = "fhirconnect/ferrobridge_journey_diagnose.context.yml";

#[tokio::test(flavor = "multi_thread")]
async fn the_dashboard_mapping_set_and_capability_draw_the_running_bridge()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.text("h1", "Dashboard").await?;
        browser.text("header", "Bridge up").await?;
        browser.text("main", "Contexts loaded").await?;
        browser.text("main", "Upstreams reachable").await?;
        browser.console_clean("the dashboard").await?;

        browser.goto(&stack.url("/mappings")).await?;
        browser.text("h1", "Mapping set").await?;
        browser.text("tr", CONTEXT).await?;
        browser.console_clean("the Mapping set").await?;

        browser.goto(&stack.url("/mappings/templates")).await?;
        browser.text("tr", "ferrobridge.diagnose.v1").await?;
        browser.console_clean("the templates").await?;

        browser.goto(&stack.url("/facade/capability")).await?;
        browser.text("main", "From the CapabilityStatement").await?;
        browser.text("main", "Condition").await?;
        browser.console_clean("the capability").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn each_mapping_file_is_listed_and_shown_with_its_lines() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/mappings/files")).await?;
        for path in [CONTEXT_PATH, MODEL_PATH] {
            browser
                .element(By::XPath(format!("//table//a[contains(@href, '{path}')]")))
                .await?;
        }
        browser.console_clean("the file list").await?;

        browser
            .element(By::XPath(format!(
                "//table//a[contains(@href, '{CONTEXT_PATH}')]"
            )))
            .await?
            .click()
            .await?;
        browser
            .at(&stack.url(&format!("/mappings/files/{CONTEXT_PATH}")))
            .await?;
        browser.text("main", PROFILE).await?;
        browser.element(By::Id("L1")).await?;
        browser.console_clean("the context file").await?;

        browser
            .goto(&stack.url(&format!("/mappings/files/{MODEL_PATH}")))
            .await?;
        browser.text("main", "problemDiagnose").await?;
        browser.element(By::Id("L1")).await?;
        browser.console_clean("the model file").await
    })
    .await
}
