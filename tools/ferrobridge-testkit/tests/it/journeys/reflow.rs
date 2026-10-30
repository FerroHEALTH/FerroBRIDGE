// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every console page at 320 CSS pixels, with FerroUI's script modules
//! loaded and with them refused: the page reads whole with no sideways
//! scroll (WCAG 2.2 SC 1.4.10, Reflow).

use std::error::Error;

use serde_json::json;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Every page the reflow is held on.
const PAGES: [&str; 24] = [
    "/",
    "/search?q=diagnose",
    "/mappings",
    "/mappings/omocl",
    "/mappings/templates",
    "/mappings/files",
    "/mappings/diagnostics",
    "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.context.yml",
    // TODO(#1356): the Program tab, once FerroUI wraps a code in a stacked cell.
    "/mappings/templates/ferrobridge.diagnose.v1",
    "/mappings/new",
    "/mappings/edit/fhirconnect/ferrobridge_journey_diagnose.yml",
    "/playground",
    "/playground/toopenehr",
    "/playground/validate",
    "/facade",
    "/facade/capability",
    "/facade/security",
    "/hl7v2",
    "/omop",
    "/connections",
    "/connections/cdr",
    "/configuration",
    "/preferences",
    "/about",
];

/// The address pattern of FerroUI's script modules.
const MODULES: &str = "*/ferrohealth-ui/*/scripts/*";

/// The part of `Page.getLayoutMetrics` the reflow check reads.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metrics {
    /// The size of the page's content.
    css_content_size: Size,
    /// The layout viewport.
    css_layout_viewport: Viewport,
}

/// A width.
#[derive(Debug, serde::Deserialize)]
struct Size {
    /// The width, in CSS pixels.
    width: f64,
}

/// The layout viewport.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Viewport {
    /// Its width, in CSS pixels.
    client_width: f64,
}

/// Lays the page out in a window `width` by `height` CSS pixels.
async fn window(browser: &Browser, width: u32, height: u32) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// Fails when a page of [`PAGES`] scrolls sideways at 320 CSS pixels.
async fn every_page_reads_whole(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    window(browser, 320, 800).await?;
    for page in PAGES {
        browser.goto(&stack.url(page)).await?;
        browser.element(By::Css("main h1")).await?;
        let metrics: Metrics = serde_json::from_value(
            browser
                .driver()
                .cdp()
                .send_raw("Page.getLayoutMetrics", json!({}))
                .await?,
        )?;
        if metrics.css_content_size.width > metrics.css_layout_viewport.client_width + 0.5 {
            return Err(format!("{page} scrolls sideways at 320 px: {metrics:?}").into());
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_page_reflows_at_320_pixels_with_the_modules_loaded_and_refused()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        every_page_reads_whole(browser, &stack).await?;
        browser.console_clean("the reflow").await?;
        let cdp = browser.driver().cdp();
        cdp.send_raw("Network.enable", json!({})).await?;
        cdp.send_raw("Network.setBlockedURLs", json!({ "urls": [MODULES] }))
            .await?;
        every_page_reads_whole(browser, &stack).await?;
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
                "the reflow with the modules refused logged:\n{}",
                others.join("\n")
            )
            .into())
        }
    })
    .await
}
