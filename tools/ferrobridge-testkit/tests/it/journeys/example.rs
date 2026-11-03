// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The playground's Example menu: a synthetic composition generated from a
//! template fills the input, and `$tofhir` answers it.

use std::error::Error;

use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// The address of the example of the journeys' template.
pub(crate) const EXAMPLE: &str = "/playground?example=ferrobridge.diagnose.v1";

#[tokio::test(flavor = "multi_thread")]
async fn an_example_composition_fills_the_playground_and_converts() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground")).await?;
        loaded(browser).await?;
        browser
            .element(By::XPath(format!("//a[@href = '{EXAMPLE}']")))
            .await?;
        browser.goto(&stack.url(EXAMPLE)).await?;
        loaded(browser).await?;
        browser
            .text("main", "openEHR-EHR-EVALUATION.problem_diagnosis.v1")
            .await?;
        browser.element(By::Id("pg-run")).await?.click().await?;
        browser.text("main", "Answered").await?;
        browser.console_clean("an example composition").await
    })
    .await
}
