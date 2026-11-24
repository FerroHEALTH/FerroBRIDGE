// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Sources pages: an operator creates an MLLP source from the form,
//! finds it on the list, disables it with a change under the `ETag` the page
//! showed, sees both versions, and deletes it.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, Stack};

/// The document of the synthetic MLLP source.
const DOCUMENT: &str = r#"{"routes": [{"sending_application": "MONITOR", "sending_facility": "WARD-1", "source_cidrs": ["127.0.0.0/8"]}]}"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_source_is_created_disabled_and_deleted_from_the_pages() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithSources).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/sources/new")).await?;
        browser.titled(&format!("New source · {PRODUCT}")).await?;
        loaded(browser).await?;
        browser
            .element(By::Id("source-name"))
            .await?
            .send_keys("journey-device")
            .await?;
        browser
            .element(By::XPath(
                "//select[@id = 'source-kind']/option[@value = 'mllp']",
            ))
            .await?
            .click()
            .await?;
        let document = browser.element(By::Id("document")).await?;
        document.clear().await?;
        document.send_keys(DOCUMENT).await?;
        browser
            .element(By::XPath(
                "//button[@name = 'decision' and @value = 'save']",
            ))
            .await?
            .click()
            .await?;
        browser
            .titled(&format!("journey-device · {PRODUCT}"))
            .await?;

        browser
            .element(By::Id("source-enabled"))
            .await?
            .click()
            .await?;
        browser
            .element(By::XPath(
                "//button[@name = 'decision' and @value = 'save']",
            ))
            .await?
            .click()
            .await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'Saved: every replica serves this version')]",
            ))
            .await?;
        browser
            .element(By::XPath("//*[normalize-space(.) = 'Disabled']"))
            .await?;

        browser.goto(&stack.url("/sources")).await?;
        browser
            .element(By::XPath("//a[contains(., 'journey-device')]"))
            .await?
            .click()
            .await?;
        browser
            .element(By::XPath(
                "//button[@name = 'decision' and @value = 'delete']",
            ))
            .await?
            .click()
            .await?;
        browser.titled(&format!("Sources · {PRODUCT}")).await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'No inbound source is configured.')]",
            ))
            .await?;
        Ok(())
    })
    .await?;
    stack.stop().await
}
