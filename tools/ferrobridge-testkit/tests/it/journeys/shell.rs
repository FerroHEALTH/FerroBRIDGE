// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell: every navigation entry leads to its page and marks itself the
//! current one, the theme switch redraws the page in the chosen theme, and
//! the sign-out ends the session at the provider and lands on the sign-in.

use std::error::Error;

use ferrobridge_viewer::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::{sign_in, signed_out_shell};
use crate::journeys::stack::Stack;

/// Every navigation entry: its words, the page it leads to, and the title
/// that page carries.
const ENTRIES: [(&str, &str, &str); 8] = [
    ("Overview", "/mappings", "Mappings"),
    ("Files", "/files", "Mapping files"),
    ("New file", "/edit", "New mapping file"),
    ("Playground", "/playground", "Transform playground"),
    ("Validation dry run", "/validate", "Validation dry run"),
    ("Capability", "/capability", "Capability"),
    ("About", "/about", "About"),
    ("Dashboard", "/", "Dashboard"),
];

#[tokio::test(flavor = "multi_thread")]
async fn every_entry_leads_to_its_page_and_marks_itself_current() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        for (words, path, title) in ENTRIES {
            let link = format!("//nav[@aria-label = 'Pages']//a[normalize-space(.) = '{words}']");
            browser
                .element(By::XPath(link.clone()))
                .await?
                .click()
                .await?;
            browser.at(&stack.url(path)).await?;
            browser.titled(&format!("{title} · {PRODUCT}")).await?;
            browser
                .element(By::XPath(format!("{link}[@aria-current = 'page']")))
                .await?;
            browser.console_clean(words).await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_theme_toggle_swaps_in_place_keeps_the_choice_and_the_sign_out_ends_the_session()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser
            .driver()
            .cdp()
            .send_raw(
                "Emulation.setEmulatedMedia",
                serde_json::json!({ "features": [{ "name": "prefers-color-scheme", "value": "light" }] }),
            )
            .await?;
        browser.goto(&stack.url("/files")).await?;
        until("the toggle swaps the theme in place", async || {
            let to_dark = browser.element(By::Id("theme-to-dark")).await?;
            if to_dark.is_displayed().await? {
                to_dark.click().await?;
            }
            Ok(!browser
                .driver()
                .find_all(By::Css(r#"html[data-theme="dark"]"#))
                .await?
                .is_empty())
        })
        .await?;
        browser.at(&stack.url("/files")).await?;
        browser.element(By::Id("theme-to-light")).await?;
        browser.goto(&stack.url("/files")).await?;
        browser.element(By::Css(r#"html[data-theme="dark"]"#)).await?;
        browser.console_clean("the theme toggle").await?;

        browser
            .element(By::XPath("//header//button[@popovertarget]//span"))
            .await?;
        let menu = browser
            .driver()
            .find_all(By::XPath("//header//button[@aria-controls = 'operator-menu']"))
            .await?;
        let operator = menu.first().ok_or("the operator menu button")?;
        operator.click().await?;
        browser
            .element(By::XPath("//form//button[normalize-space(.) = 'Sign out']"))
            .await?
            .click()
            .await?;
        until("the session cookie is gone", async || {
            Ok(browser.cookie(stack.cookie()).await?.is_none())
        })
        .await?;
        browser.at(&stack.url("/")).await?;
        browser.titled(PRODUCT).await?;
        signed_out_shell(browser).await?;
        browser.console_clean("the sign-out").await
    })
    .await
}
