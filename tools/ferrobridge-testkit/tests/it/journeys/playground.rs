// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The transform playground against the running bridge: a `$toopenehr`
//! the bridge answers, a `$tofhir` it refuses, and a bridge whose
//! operations lane is off. What the operator entered reaches no URL.

use std::error::Error;

use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
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
    Ok(serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{ "resource": condition }],
    })
    .to_string())
}

/// A submit still disabled for want of the bundle: any form's, save the
/// editor's save, which waits on its confirmation too.
const HYDRATING: &str =
    "//form[not(.//input[@id = 'edit-confirm'])]//button[@type = 'submit' and @disabled]";

/// Waits for the page's forms to hydrate: no submit is disabled for want of
/// the bundle.
pub(crate) async fn hydrated(browser: &Browser) -> Result<(), Box<dyn Error>> {
    crate::journeys::browser::until("the forms have hydrated", async || {
        Ok(browser
            .driver()
            .find_all(By::XPath(HYDRATING))
            .await?
            .is_empty())
    })
    .await
}

/// Runs `$toopenehr` over `bundle` from the playground.
pub(crate) async fn to_openehr(browser: &Browser, bundle: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Id("toopenehr-bundle"))
        .await?
        .send_keys(bundle)
        .await?;
    browser
        .element(By::XPath(
            "//form//button[@type = 'submit' and normalize-space(.) = 'Run $toopenehr']",
        ))
        .await?
        .click()
        .await?;
    Ok(())
}

/// Runs `$tofhir` over `composition` from the playground.
pub(crate) async fn to_fhir(browser: &Browser, composition: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Id("tofhir-composition"))
        .await?
        .send_keys(composition)
        .await?;
    browser
        .element(By::XPath(
            "//form//button[@type = 'submit' and normalize-space(.) = 'Run $tofhir']",
        ))
        .await?
        .click()
        .await?;
    Ok(())
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
        browser.goto(&stack.url("/playground")).await?;
        hydrated(browser).await?;
        to_openehr(browser, &bundle).await?;
        browser.text("h2", "$toopenehr answered").await?;
        browser.text("pre", "Synthetic problem one").await?;
        browser.console_clean("an answered $toopenehr").await?;

        to_fhir(browser, "{}").await?;
        browser.text("h2", "$tofhir answered").await?;
        browser.text("p", "the bridge refused the request.").await?;
        browser.at(&stack.url("/playground")).await?;
        browser.console_clean("a refused $tofhir").await
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bridge_whose_operations_are_off_refuses_the_transform() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithoutOperations).await?;
    let bundle = bundle()?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/playground")).await?;
        hydrated(browser).await?;
        to_openehr(browser, &bundle).await?;
        browser.text("h2", "$toopenehr answered").await?;
        browser
            .text("p", "503: the bridge refused the request.")
            .await?;
        browser.text("li", "[operations] enabled is false").await?;
        browser
            .console_clean("an operations lane that is off")
            .await
    })
    .await
}
