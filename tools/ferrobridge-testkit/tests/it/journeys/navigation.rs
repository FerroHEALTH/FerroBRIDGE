// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Navigation in the page: once the console has hydrated, a followed link,
//! the search's `GET` form and a form post each draw their page in the same
//! document, and the browser's back and forward walk the pages it drew.
//!
//! Each journey marks the window before it acts and asks for the mark after:
//! a document the browser loaded anew has lost it.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::{bundle, hydrated, to_openehr};
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// The words the window's mark holds.
const MARK: &str = "drawn-in-this-document";

/// Marks the window, which a document loaded anew does not carry.
async fn mark(browser: &Browser) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .execute(
            &format!("window.ferrobridgeJourney = '{MARK}';"),
            Vec::new(),
        )
        .await?;
    Ok(())
}

/// Fails unless the window still carries its mark, after `step`.
async fn kept(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    let answer = browser
        .driver()
        .execute("return window.ferrobridgeJourney || null;", Vec::new())
        .await?;
    let held = answer.json().as_str().map(str::to_owned);
    if held.as_deref() == Some(MARK) {
        Ok(())
    } else {
        Err(format!("{step} loaded a new document").into())
    }
}

/// Follows the navigation entry whose words are `words`.
async fn entry(browser: &Browser, words: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::XPath(format!(
            "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), '{words}')]"
        )))
        .await?
        .click()
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_draws_its_page_in_the_same_document_and_back_and_forward_walk_it()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/")).await?;
        hydrated(browser).await?;
        mark(browser).await?;

        entry(browser, "Mapping set").await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.titled(&format!("Mapping set · {PRODUCT}")).await?;
        kept(browser, "following Mapping set").await?;

        entry(browser, "About").await?;
        browser.at(&stack.url("/about")).await?;
        browser.titled(&format!("About · {PRODUCT}")).await?;
        browser.text("main", "Manufacturer").await?;
        kept(browser, "following About").await?;

        browser.driver().back().await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.titled(&format!("Mapping set · {PRODUCT}")).await?;
        browser
            .element(By::XPath(
                "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), 'Mapping set') and @aria-current = 'page']",
            ))
            .await?;
        kept(browser, "going back").await?;

        browser.driver().forward().await?;
        browser.at(&stack.url("/about")).await?;
        browser.titled(&format!("About · {PRODUCT}")).await?;
        kept(browser, "going forward").await?;
        browser.console_clean("links, back and forward").await?;

        browser.goto(&stack.url("/mappings")).await?;
        hydrated(browser).await?;
        if kept(browser, "loading a page").await.is_ok() {
            return Err("a document loaded anew still carries the mark".into());
        }
        Ok(())
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_search_draws_its_results_in_the_same_document() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/")).await?;
        hydrated(browser).await?;
        mark(browser).await?;
        let field = browser.element(By::Id("find")).await?;
        field.send_keys("nothing-matches-this").await?;
        field.send_keys(thirtyfour::Key::Enter).await?;
        browser
            .at(&stack.url("/search?q=nothing-matches-this"))
            .await?;
        browser.text("main", "Nothing found").await?;
        kept(browser, "a search").await?;
        browser.console_clean("a search").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_form_post_draws_its_answer_in_the_same_document() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground/toopenehr")).await?;
        hydrated(browser).await?;
        mark(browser).await?;
        to_openehr(browser, &bundle).await?;
        browser.text("main", "Answered").await?;
        browser.at(&stack.url("/playground/toopenehr")).await?;
        kept(browser, "a run of the playground").await?;
        browser
            .element(By::Id("input"))
            .await?
            .prop("value")
            .await?
            .filter(|value| value.contains("Bundle"))
            .ok_or("the input still holds what the operator entered")?;
        browser.console_clean("a form post").await
    })
    .await
}
