// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console with its script modules refused: every page the navigation
//! leads to still draws its content, and a plain post of the playground and
//! of the editor draws its page with the answer and the posted fields
//! (FerroUI `docs/consoles.md`, An action that answers on its page).
//!
//! The browser is refused every script module of FerroUI
//! (`/ferrohealth-ui/<version>/scripts/`), the loader among them, as on a
//! page whose modules have not loaded or will not; the stylesheets still
//! load. A refused module is the one browser error a page may log here.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use serde_json::json;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::editor::{MISSING_TEMPLATE, context, context_path, press};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::bundle;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// The slug of the context the editor checks with the modules withheld.
const NO_SCRIPT: &str = "noscript";

/// The address pattern of FerroUI's script modules.
const WITHHELD: &str = "*/ferrohealth-ui/*/scripts/*";

/// Every navigation entry: its words, the page it leads to, and the words
/// that page draws in its main region.
const ENTRIES: [(&str, &str, &str); 8] = [
    ("Mapping set", "/mappings", "Mapping set"),
    ("Playground", "/playground", "Playground"),
    ("FHIR facade", "/facade", "FHIR facade"),
    ("HL7 v2", "/hl7v2", "HL7 v2"),
    ("OMOP ETL", "/omop", "OMOP ETL"),
    ("Connections", "/connections", "Connections"),
    ("Configuration", "/configuration", "Configuration"),
    ("About", "/about", "About"),
];

/// Refuses the browser every script module.
async fn withhold_the_modules(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    cdp.send_raw("Network.enable", json!({})).await?;
    cdp.send_raw("Network.setBlockedURLs", json!({ "urls": [WITHHELD] }))
        .await?;
    Ok(())
}

/// Fails on any browser error other than a refused module, naming `step`.
async fn only_the_modules_refused(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    let others: Vec<String> = browser
        .console_errors()
        .await?
        .into_iter()
        .filter(|entry| !entry.contains("/scripts/"))
        .collect();
    if others.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the browser logged {} error(s) other than the withheld modules during {step}:\n{}",
            others.len(),
            others.join("\n")
        )
        .into())
    }
}

/// Fails if a module ran: FerroUI's loader marks the root element as it
/// starts.
async fn no_module_ran(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath("//button[@aria-controls = 'user-menu-menu']"))
        .await?;
    let ran = browser
        .driver()
        .find_all(By::Css(format!("html[{}]", ferrohealth_ui::script::MARK)))
        .await?;
    if ran.is_empty() {
        Ok(())
    } else {
        Err(format!("{step} ran a module with the modules withheld").into())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_and_a_post_work_with_the_modules_withheld() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        withhold_the_modules(browser).await?;
        browser.goto(&stack.url("/")).await?;
        no_module_ran(browser, "the dashboard").await?;
        for (words, path, heading) in ENTRIES {
            browser
                .element(By::XPath(format!(
                    "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), '{words}')]"
                )))
                .await?
                .click()
                .await?;
            browser.at(&stack.url(path)).await?;
            browser.titled(&format!("{heading} · {PRODUCT}")).await?;
            browser.text("main", heading).await?;
            no_module_ran(browser, words).await?;
            only_the_modules_refused(browser, words).await?;
        }

        browser.goto(&stack.url("/playground/toopenehr")).await?;
        no_module_ran(browser, "the playground").await?;
        browser
            .element(By::Id("input"))
            .await?
            .send_keys(&bundle)
            .await?;
        browser.element(By::Id("pg-run")).await?.click().await?;
        browser.text("main", "Answered").await?;
        browser.at(&stack.url("/playground/toopenehr")).await?;
        assert_eq!(
            bundle.trim(),
            posted(browser, "input").await?.trim(),
            "the playground's answer keeps the posted input"
        );
        no_module_ran(browser, "the playground's answer").await?;

        let draft = context(NO_SCRIPT, MISSING_TEMPLATE);
        browser.goto(&stack.url("/mappings/new")).await?;
        no_module_ran(browser, "the editor").await?;
        browser
            .element(By::Id("editor-path"))
            .await?
            .send_keys(&context_path(NO_SCRIPT))
            .await?;
        browser
            .element(By::Id("draft"))
            .await?
            .send_keys(&draft)
            .await?;
        press(browser, "check").await?;
        browser
            .text("main", "The set does not compile with this draft")
            .await?;
        browser.at(&stack.url("/mappings/new")).await?;
        assert_eq!(
            draft.trim(),
            posted(browser, "draft").await?.trim(),
            "the editor's answer keeps the posted draft"
        );
        no_module_ran(browser, "the editor's answer").await?;
        only_the_modules_refused(browser, "a post").await
    })
    .await
}

/// The value of the field `id` on the page drawn after a plain post, with
/// the CRLF line breaks a form submits read back as LF.
async fn posted(browser: &Browser, id: &str) -> Result<String, Box<dyn Error>> {
    let value = browser
        .element(By::Id(id))
        .await?
        .prop("value")
        .await?
        .ok_or("a text field has a value")?;
    Ok(value.replace("\r\n", "\n"))
}
