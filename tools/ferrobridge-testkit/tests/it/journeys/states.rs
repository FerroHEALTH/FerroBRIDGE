// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's interaction states, captured behind the
//! `FERROBRIDGE_SCREENSHOTS` gate for the visual review: the pointer on a
//! navigation entry and on a table row, the keyboard focus on a navigation
//! entry, the user menu's button and a form's button, the user menu open,
//! the drawer open on a phone, and the top bar with a long operator name on
//! a phone and at 1440 pixels, each in the light and the dark theme.
//!
//! The images go to the ignored `target/console-screenshots/states/`, as
//! `<state>.png` and `<state>-dark.png`; the book shows the pages, not the
//! states. No specification governs the screenshots: our own design.

use std::error::Error;
use std::path::{Path, PathBuf};

use ferrobridge_console::session::PREFERENCE_PREFIX;
use serde_json::json;
use thirtyfour::{By, Cookie, WebElement};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, Stack};

/// The environment variable that opts in to the capture.
const SCREENSHOTS_GATE: &str = "FERROBRIDGE_SCREENSHOTS";

/// The loopback port of the bridge the states are captured from, fixed so
/// the address a state draws is the same every run.
const BRIDGE_PORT: u16 = 47182;

/// The ignored folder of the state images, from the workspace root.
const STATES: &str = "target/console-screenshots/states";

/// The file of `state` in `theme`.
fn file(state: &str, theme: &str) -> PathBuf {
    let suffix = if theme == "dark" { "-dark" } else { "" };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(STATES)
        .join(format!("{state}{suffix}.png"))
}

/// Lays the page out `width` by `height` CSS pixels, reporting `theme` as
/// the system's scheme and asking for reduced motion, so a state is drawn at
/// rest.
async fn layout(
    browser: &Browser,
    width: u32,
    height: u32,
    theme: &str,
) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    cdp.send_raw(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
    )
    .await?;
    cdp.send_raw(
        "Emulation.setEmulatedMedia",
        json!({ "features": [
            { "name": "prefers-color-scheme", "value": theme },
            { "name": "prefers-reduced-motion", "value": "reduce" }
        ] }),
    )
    .await?;
    Ok(())
}

/// Moves the pointer onto `element`.
async fn hover(browser: &Browser, element: &WebElement) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .action_chain()
        .move_to_element_center(element)
        .perform()
        .await?;
    Ok(())
}

/// Gives `element` the keyboard focus.
async fn focus(browser: &Browser, element: &WebElement) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .execute("arguments[0].focus();", vec![element.to_json()?])
        .await?;
    Ok(())
}

/// Writes the screenshot of `element` as `state` in `theme`.
async fn shot_of(
    browser: &Browser,
    element: &WebElement,
    state: &str,
    theme: &str,
) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .execute(
            "arguments[0].scrollIntoView({block: 'center'});",
            vec![element.to_json()?],
        )
        .await?;
    std::fs::write(file(state, theme), element.screenshot_as_png().await?)?;
    Ok(())
}

/// Writes the screenshot of the viewport as `state` in `theme`.
async fn shot_of_view(browser: &Browser, state: &str, theme: &str) -> Result<(), Box<dyn Error>> {
    std::fs::write(
        file(state, theme),
        browser.driver().screenshot_as_png().await?,
    )?;
    Ok(())
}

/// The user menu's button in the top bar.
const USER_MENU: &str = "//header//button[@aria-controls = 'user-menu-menu']";

/// Captures every state in `theme` on `stack`.
async fn every_state(browser: &Browser, stack: &Stack, theme: &str) -> Result<(), Box<dyn Error>> {
    layout(browser, 1440, 900, theme).await?;
    browser.driver().delete_all_cookies().await?;
    browser.goto(&stack.url("/")).await?;
    let mut cookie = Cookie::new(format!("{PREFERENCE_PREFIX}theme"), theme);
    cookie.set_path("/");
    browser.driver().add_cookie(cookie).await?;
    sign_in(browser, stack).await?;
    browser.goto(&stack.url("/mappings/files")).await?;
    browser.element(By::XPath("//table//tbody//tr")).await?;
    loaded(browser).await?;
    let sidebar = browser.element(By::Id("sidebar")).await?;
    let entry = browser
        .element(By::XPath(
            "//nav[@aria-label = 'Console']//a[starts-with(normalize-space(.), 'Playground')]",
        ))
        .await?;
    hover(browser, &entry).await?;
    shot_of(browser, &sidebar, "hover-nav", theme).await?;
    focus(browser, &entry).await?;
    shot_of(browser, &sidebar, "focus-nav", theme).await?;
    let row = browser.element(By::XPath("//table//tbody//tr")).await?;
    hover(browser, &row).await?;
    let table = browser.element(By::XPath("//table")).await?;
    shot_of(browser, &table, "hover-row", theme).await?;
    let bar = browser.element(By::Css(".fui-topbar")).await?;
    let menu = browser.element(By::XPath(USER_MENU)).await?;
    focus(browser, &menu).await?;
    shot_of(browser, &bar, "focus-user-menu", theme).await?;
    menu.click().await?;
    browser.element(By::Css("#user-menu:popover-open")).await?;
    shot_of_view(browser, "user-menu", theme).await?;
    browser.goto(&stack.url("/playground")).await?;
    loaded(browser).await?;
    let run = browser.element(By::Id("pg-run")).await?;
    focus(browser, &run).await?;
    let form = browser.element(By::XPath("//main//form")).await?;
    shot_of(browser, &form, "focus-run", theme).await?;
    layout(browser, 390, 900, theme).await?;
    browser.goto(&stack.url("/mappings/files")).await?;
    loaded(browser).await?;
    browser
        .element(By::XPath("//button[@commandfor = 'sidebar']"))
        .await?
        .click()
        .await?;
    browser.element(By::Css("#sidebar[open]")).await?;
    shot_of_view(browser, "drawer-open", theme).await?;
    long_name(browser, stack, theme).await
}

/// A long operator name, as a directory can hand one out.
const LONG_NAME: &str = "annemarie.van-der-berg-hoogstraten@ziekenhuis-noordwest.example.org";

/// Captures the top bar with [`LONG_NAME`] in place of the operator's name,
/// at 390 and at 1440 CSS pixels: the test provider issues one name, so the
/// longest words of the user menu's button are swapped once it is drawn.
async fn long_name(browser: &Browser, stack: &Stack, theme: &str) -> Result<(), Box<dyn Error>> {
    for (width, state) in [(390, "topbar-long-name-narrow"), (1440, "topbar-long-name")] {
        layout(browser, width, 900, theme).await?;
        browser.goto(&stack.url("/mappings/files")).await?;
        let button = browser.element(By::XPath(USER_MENU)).await?;
        browser
            .driver()
            .execute(
                "const walker = document.createTreeWalker(arguments[0], NodeFilter.SHOW_TEXT); \
                 let longest = null; while (walker.nextNode()) { const node = walker.currentNode; \
                 if (!longest || node.textContent.trim().length > longest.textContent.trim().length) longest = node; } \
                 if (longest) longest.textContent = arguments[1];",
                vec![button.to_json()?, json!(LONG_NAME)],
            )
            .await?;
        shot_of_view(browser, state, theme).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_console_state_is_captured_in_light_and_dark() -> Result<(), Box<dyn Error>> {
    if !std::env::var(SCREENSHOTS_GATE).is_ok_and(|value| value == "1") {
        return Ok(());
    }
    std::fs::create_dir_all(file("x", "light").parent().ok_or("a parent")?)?;
    for theme in ["light", "dark"] {
        let stack = Stack::capture(Lanes::All, BRIDGE_PORT).await?;
        with_browser(async |browser| every_state(browser, &stack, theme).await).await?;
        stack.stop().await?;
    }
    Ok(())
}
