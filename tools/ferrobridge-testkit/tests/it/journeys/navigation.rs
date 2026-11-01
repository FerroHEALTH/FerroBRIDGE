// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Navigation in the server-rendered console (FerroUI #510): a followed
//! link, the search's `GET` form and a form post each load their page as one
//! document the server drew whole, the browser's back and forward restore
//! the pages, and a dialog opens in the page with no document loaded.
//!
//! Each journey marks the window before it acts and asks for the mark after:
//! a document the browser loaded anew has lost it.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::{bundle, loaded, to_openehr};
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

/// Whether the window still carries its mark.
async fn marked(browser: &Browser) -> Result<bool, Box<dyn Error>> {
    let answer = browser
        .driver()
        .execute("return window.ferrobridgeJourney || null;", Vec::new())
        .await?;
    Ok(answer.json().as_str() == Some(MARK))
}

/// Fails unless the window still carries its mark, after `step`.
async fn kept(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    if marked(browser).await? {
        Ok(())
    } else {
        Err(format!("{step} loaded a new document").into())
    }
}

/// Fails unless `step` loaded a new document, which carries no mark; then
/// marks the new one.
async fn loaded_anew(browser: &Browser, step: &str) -> Result<(), Box<dyn Error>> {
    loaded(browser).await?;
    if marked(browser).await? {
        return Err(format!("{step} drew its page in the old document").into());
    }
    mark(browser).await
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
async fn a_link_loads_its_page_as_one_document_and_back_and_forward_restore_it()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/")).await?;
        loaded(browser).await?;
        mark(browser).await?;

        entry(browser, "Mapping set").await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.titled(&format!("Mapping set · {PRODUCT}")).await?;
        loaded_anew(browser, "following Mapping set").await?;

        entry(browser, "About").await?;
        browser.at(&stack.url("/about")).await?;
        browser.titled(&format!("About · {PRODUCT}")).await?;
        browser.text("main", "Manufacturer").await?;
        loaded_anew(browser, "following About").await?;

        browser.driver().back().await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.titled(&format!("Mapping set · {PRODUCT}")).await?;
        browser
            .element(By::XPath(
                "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), 'Mapping set') and @aria-current = 'page']",
            ))
            .await?;

        browser.driver().forward().await?;
        browser.at(&stack.url("/about")).await?;
        browser.titled(&format!("About · {PRODUCT}")).await?;
        browser.console_clean("links, back and forward").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_search_loads_its_results_as_one_document() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/")).await?;
        loaded(browser).await?;
        mark(browser).await?;
        let field = browser.element(By::Id("find")).await?;
        field.send_keys("nothing-matches-this").await?;
        field.send_keys(thirtyfour::Key::Enter).await?;
        browser
            .at(&stack.url("/search?q=nothing-matches-this"))
            .await?;
        browser.text("main", "Nothing found").await?;
        loaded_anew(browser, "a search").await?;
        browser.console_clean("a search").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_form_post_draws_its_answer_on_its_page_with_what_was_entered()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground/toopenehr")).await?;
        loaded(browser).await?;
        mark(browser).await?;
        to_openehr(browser, &bundle).await?;
        browser.text("main", "Answered").await?;
        browser.at(&stack.url("/playground/toopenehr")).await?;
        loaded_anew(browser, "a run of the playground").await?;
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

#[tokio::test(flavor = "multi_thread")]
async fn a_dialog_opens_in_the_page_and_loads_no_document() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/mappings")).await?;
        loaded(browser).await?;
        mark(browser).await?;
        browser
            .element(By::Css("[aria-controls='set-more']"))
            .await?
            .click()
            .await?;
        browser
            .element(By::Css("button[commandfor='reload-set']"))
            .await?
            .click()
            .await?;
        browser.element(By::Css("dialog#reload-set[open]")).await?;
        kept(browser, "opening the reload's dialog").await?;
        browser.at(&stack.url("/mappings")).await?;
        browser.console_clean("a dialog in the page").await
    })
    .await
}
