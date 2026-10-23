// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell: every navigation entry leads to its page and marks itself the
//! current one, the user menu's theme redraws the page in the chosen theme
//! and keeps it, and its sign-out ends the session at the provider and lands on
//! the sign-in.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::hydrated;
use crate::journeys::sign_in::{sign_in, signed_out_shell};
use crate::journeys::stack::Stack;

/// Every navigation entry: its words, the page it leads to, and the title
/// that page carries.
const ENTRIES: [(&str, &str, &str); 8] = [
    ("Mapping set", "/mappings", "Mapping set"),
    ("Playground", "/playground", "Playground"),
    ("FHIR facade", "/facade", "FHIR facade"),
    ("HL7 v2", "/hl7v2", "HL7 v2"),
    ("OMOP ETL", "/omop", "OMOP ETL"),
    ("Connections", "/connections", "Connections"),
    ("About", "/about", "About"),
    ("Dashboard", "/", "Dashboard"),
];

/// Opens the user menu once its island runs.
pub(crate) async fn open_user_menu(browser: &Browser) -> Result<(), Box<dyn Error>> {
    hydrated(browser).await?;
    browser
        .element(By::XPath(
            "//header//button[@aria-controls = 'user-menu-menu']",
        ))
        .await?
        .click()
        .await?;
    browser.element(By::Css("#user-menu:popover-open")).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_entry_leads_to_its_page_and_marks_itself_current() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        for (words, path, title) in ENTRIES {
            let link = format!(
                "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), '{words}')]"
            );
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
async fn the_user_menu_sets_the_theme_and_its_sign_out_ends_the_session()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/mappings")).await?;
        browser
            .element(By::Css(r#"html[data-theme="system"]"#))
            .await?;
        open_user_menu(browser).await?;
        browser
            .element(By::XPath(
                "//button[@role = 'menuitemradio' and @name = 'theme' and @value = 'dark']",
            ))
            .await?
            .click()
            .await?;
        browser
            .element(By::Css(r#"html[data-theme="dark"]"#))
            .await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.goto(&stack.url("/playground")).await?;
        browser
            .element(By::Css(r#"html[data-theme="dark"]"#))
            .await?;
        if !browser
            .driver()
            .find_all(By::Css(".fui-theme-toggle"))
            .await?
            .is_empty()
        {
            return Err("the top bar draws a theme toggle".into());
        }
        browser
            .console_clean("the theme from the user menu")
            .await?;

        open_user_menu(browser).await?;
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
        browser.titled(&format!("Sign in · {PRODUCT}")).await?;
        signed_out_shell(browser).await?;
        browser.console_clean("the sign-out").await
    })
    .await
}
