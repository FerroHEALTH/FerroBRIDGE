// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set's revisions: a save records one, the Revisions tab lists
//! it, and the rollback from the baseline's page puts the console's files
//! back as they were, with the reason the operator gave.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::editor::{TEMPLATE, context, context_path, draft, press, save};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

#[tokio::test(flavor = "multi_thread")]
async fn a_save_is_a_revision_and_the_baseline_rolls_it_back() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        let path = context_path("revised");
        draft(browser, &stack, &path, &context("revised", TEMPLATE)).await?;
        save(browser, "a context the rollback takes back").await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'Saved, and the set reloaded with it.')]",
            ))
            .await?;
        browser.goto(&stack.url("/mappings/revisions")).await?;
        browser.titled(&format!("Mapping set · {PRODUCT}")).await?;
        loaded(browser).await?;
        browser
            .element(By::XPath(
                "//table//tr[contains(., 'a context the rollback takes back') and contains(., 'Running')]",
            ))
            .await?;
        browser
            .element(By::XPath("//table//a[contains(@href, '/mappings/revisions/1')]"))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/mappings/revisions/1")).await?;
        loaded(browser).await?;
        browser
            .element(By::XPath(format!("//*[contains(@class, 'fui-diff-removed') and contains(., 'type: context')]//ancestor::section[contains(., '{path}')]")))
            .await?;
        browser
            .element(By::Id("apply-reason"))
            .await?
            .send_keys("back to the provisioned set")
            .await?;
        press(browser, "apply").await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'Applied: the set compiled and runs as revision 3')]",
            ))
            .await?;
        browser.goto(&stack.url("/mappings/files")).await?;
        loaded(browser).await?;
        let listed = browser.driver().source().await?;
        if listed.contains(&path) {
            return Err(format!("{path} is still listed after the rollback").into());
        }
        browser.console_clean("the mapping set's revisions").await
    })
    .await
}
