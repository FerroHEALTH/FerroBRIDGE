// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console with its WebAssembly withheld: every page the navigation
//! leads to still draws its content, and a plain post of the playground and
//! of the editor draws its page with the answer and the posted fields
//! (FerroUI `docs/consoles.md`, An action that answers on its page).
//!
//! The browser is refused the bundle's WebAssembly, its glue and its
//! chunks, so nothing hydrates, as on a page whose WebAssembly has not
//! loaded or will not; the stylesheets under `/pkg/` still load. A refused
//! bundle file is the one browser error a page may log here.

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

/// The slug of the context the editor checks with the WebAssembly withheld.
const NO_WASM: &str = "nowasm";

/// The bundle files the browser is refused.
const WITHHELD: [&str; 3] = [
    "*/pkg/ferrobridge-console*",
    "*/pkg/__wasm_split*",
    "*/pkg/*.wasm",
];

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

/// Refuses the browser the bundle's files.
async fn withhold_the_webassembly(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    cdp.send_raw("Network.enable", json!({})).await?;
    cdp.send_raw("Network.setBlockedURLs", json!({ "urls": WITHHELD }))
        .await?;
    Ok(())
}

/// Fails on any browser error other than a refused bundle file, naming
/// `step`.
async fn only_the_bundle_refused(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    let others: Vec<String> = browser
        .console_errors()
        .await?
        .into_iter()
        .filter(|entry| !entry.contains("/pkg/"))
        .collect();
    if others.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the browser logged {} error(s) other than the withheld bundle during {step}:\n{}",
            others.len(),
            others.join("\n")
        )
        .into())
    }
}

/// Fails if the page hydrated: the user menu's button says whether its
/// menu is open only once it has.
async fn not_hydrated(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath("//button[@aria-controls = 'user-menu-menu']"))
        .await?;
    let hydrated = browser
        .driver()
        .find_all(By::XPath(
            "//button[@aria-controls = 'user-menu-menu' and @aria-expanded]",
        ))
        .await?;
    if hydrated.is_empty() {
        Ok(())
    } else {
        Err(format!("{step} hydrated with its WebAssembly withheld").into())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_and_a_post_work_with_the_webassembly_withheld() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        withhold_the_webassembly(browser).await?;
        browser.goto(&stack.url("/")).await?;
        not_hydrated(browser, "the dashboard").await?;
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
            not_hydrated(browser, words).await?;
            only_the_bundle_refused(browser, words).await?;
        }

        browser.goto(&stack.url("/playground/toopenehr")).await?;
        not_hydrated(browser, "the playground").await?;
        browser
            .element(By::Id("input"))
            .await?
            .send_keys(&bundle)
            .await?;
        browser.element(By::Id("pg-run")).await?.click().await?;
        browser.text("main", "Answered").await?;
        browser.at(&stack.url("/api/run")).await?;
        assert_eq!(
            bundle.trim(),
            posted(browser, "input").await?.trim(),
            "the playground's answer keeps the posted input"
        );
        not_hydrated(browser, "the playground's answer").await?;

        let draft = context(NO_WASM, MISSING_TEMPLATE);
        browser.goto(&stack.url("/mappings/new")).await?;
        not_hydrated(browser, "the editor").await?;
        browser
            .element(By::Id("editor-path"))
            .await?
            .send_keys(&context_path(NO_WASM))
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
        browser.at(&stack.url("/api/edit")).await?;
        assert_eq!(
            draft.trim(),
            posted(browser, "draft").await?.trim(),
            "the editor's answer keeps the posted draft"
        );
        not_hydrated(browser, "the editor's answer").await?;
        only_the_bundle_refused(browser, "a post").await
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
