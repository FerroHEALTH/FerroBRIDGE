// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The playground against the running bridge: a `$toopenehr` the bridge
//! answers, a `$tofhir` it refuses, and a bridge whose operations lane is
//! off. Each run is the playground's form, which its tab's own address
//! answers with the page; what the operator entered reaches no URL.

use std::error::Error;

use thirtyfour::By;

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, PROFILE, Stack};

/// The testkit's synthetic `Condition` claiming the provisioned profile, in
/// a `collection` Bundle.
pub(crate) fn bundle() -> Result<String, Box<dyn Error>> {
    let mut condition: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = condition
        .as_object_mut()
        .ok_or("the synthetic Condition is an object")?;
    object.insert(
        String::from("meta"),
        serde_json::json!({ "profile": [PROFILE] }),
    );
    Ok(serde_json::to_string(&serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{ "resource": condition }],
    }))?)
}

/// The id of the playground's run button.
const RUN: &str = "pg-run";

/// Waits for the page's script modules: the root element marked by
/// FerroUI's loader (`ferrohealth_ui::script::MARK`), which it writes before
/// any module arrives, and then every element that asks for a module
/// (`ferrohealth_ui::script::HOOK`) marked ready by it
/// (`ferrohealth_ui::script::READY`), so a journey drives no control before
/// its module listens.
pub(crate) async fn loaded(browser: &Browser) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Css(format!("html[{}]", ferrohealth_ui::script::MARK)))
        .await?;
    let waiting = format!(
        "[{}]:not([{}])",
        ferrohealth_ui::script::HOOK,
        ferrohealth_ui::script::READY
    );
    until(
        "every module of the page has enhanced its element",
        async || {
            let left = browser
                .driver()
                .execute(
                    "return document.querySelectorAll(arguments[0]).length;",
                    vec![serde_json::Value::from(waiting.as_str())],
                )
                .await?;
            Ok(left.json().as_u64() == Some(0))
        },
    )
    .await
}

/// Writes `text` into the playground's input and presses its run.
async fn run(browser: &Browser, text: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Id("input"))
        .await?
        .send_keys(text)
        .await?;
    browser.element(By::Id(RUN)).await?.click().await?;
    Ok(())
}

/// Runs `$toopenehr` over `bundle` from the playground's tab.
pub(crate) async fn to_openehr(browser: &Browser, bundle: &str) -> Result<(), Box<dyn Error>> {
    run(browser, bundle).await
}

/// Runs `$tofhir` over `composition` from the playground.
pub(crate) async fn to_fhir(browser: &Browser, composition: &str) -> Result<(), Box<dyn Error>> {
    run(browser, composition).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_transform_is_answered_and_a_refusal_keeps_its_status() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground/toopenehr")).await?;
        loaded(browser).await?;
        to_openehr(browser, &bundle).await?;
        browser.text("main", "Answered").await?;
        browser.text("main", "Synthetic problem one").await?;
        browser.at(&stack.url("/playground/toopenehr")).await?;
        browser.console_clean("an answered $toopenehr").await?;

        browser.goto(&stack.url("/playground")).await?;
        loaded(browser).await?;
        to_fhir(browser, "{}").await?;
        browser.text("main", "Refused").await?;
        browser
            .text("main", "The bridge refused the request")
            .await?;
        browser.at(&stack.url("/playground")).await?;
        browser.console_clean("a refused $tofhir").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bridge_whose_operations_are_off_disables_run() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithoutOperations).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground/toopenehr")).await?;
        loaded(browser).await?;
        browser
            .text("main", "Conversions are off on this bridge.")
            .await?;
        browser
            .element(By::XPath("//button[@id = 'pg-run' and @disabled]"))
            .await?;
        browser
            .console_clean("an operations lane that is off")
            .await
    })
    .await
}
