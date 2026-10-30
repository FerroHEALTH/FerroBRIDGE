// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration: the Configuration page's tabs lead to their
//! sections, the file line opens "How configuration works" and its Close
//! shuts it, and a connection's Settings tab names its keys.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

#[tokio::test(flavor = "multi_thread")]
async fn the_configuration_opens_how_it_works_and_each_tab_reads_its_keys()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/configuration")).await?;
        browser
            .titled(&format!("Configuration · {PRODUCT}"))
            .await?;
        browser
            .element(By::XPath("//*[contains(., 'telemetry.format')]"))
            .await?;
        loaded(browser).await?;
        browser
            .element(By::XPath("//button[@commandfor = 'how-configuration']"))
            .await?
            .click()
            .await?;
        browser
            .element(By::Css("dialog#how-configuration[open]"))
            .await?;
        browser
            .element(By::XPath(
                "//dialog[@id = 'how-configuration']//button[@command = 'close']",
            ))
            .await?
            .click()
            .await?;
        until("the dialog closes", async || {
            Ok(browser
                .driver()
                .find_all(By::Css("dialog#how-configuration[open]"))
                .await?
                .is_empty())
        })
        .await?;
        browser
            .element(By::XPath(
                "//nav[contains(@class, 'fui-tabs')]//a[normalize-space(.) = 'Server and access']",
            ))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/configuration/core")).await?;
        browser
            .element(By::XPath("//*[contains(., 'server.listen')]"))
            .await?;
        browser
            .goto(&stack.url("/connections/cdr/settings"))
            .await?;
        browser
            .element(By::XPath("//*[contains(., 'cdr.base_url')]"))
            .await?;
        browser.console_clean("the running configuration").await
    })
    .await
}
