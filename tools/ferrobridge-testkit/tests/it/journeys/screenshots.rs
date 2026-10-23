// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's screenshots of the operator console, taken behind the
//! `FERROBRIDGE_SCREENSHOTS` gate, and the guard that every console route
//! has one.
//!
//! With `FERROBRIDGE_SCREENSHOTS=1` the capture drives the journeys' stack
//! and browser through every shot of [`shots`], in the light and the dark
//! theme, at 1440 CSS pixels wide and the full height of the page, and
//! writes each as `<image>.png` and `<image>-dark.png` under [`IMAGES`]; it
//! takes each again at 390 CSS pixels, a phone, under the ignored
//! [`NARROW_IMAGES`], for the visual review the book does not show.
//! Without the gate the capture returns at once, so no CI job takes a
//! screenshot.
//!
//! The capture then writes the release it ran at to [`CAPTURED`] beside the
//! images.
//!
//! The guard runs in the ordinary suite: every route the console's router
//! declares is shown by a shot, every shot has both images on disk, every
//! image is a shot's, every light image is shown by a page of the book, and
//! [`CAPTURED`] names the workspace version, so a release bump fails until the
//! images are taken again.
//! No specification governs the screenshots: our own design.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;
use thirtyfour::{By, Cookie};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::editor::{MISSING_TEMPLATE, TEMPLATE, context, context_path, draft, save};
use crate::journeys::playground::{bundle, hydrated, to_fhir, to_openehr};
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, Stack};

/// The environment variable that opts in to the capture.
const SCREENSHOTS_GATE: &str = "FERROBRIDGE_SCREENSHOTS";

/// The book's folder of console screenshots, from the workspace root.
const IMAGES: &str = "website/book/src/operate/img/console";

/// The file beside the images that names the release they show.
const CAPTURED: &str = "captured-version.txt";

/// The ignored folder of the phone-width screenshots, from the workspace
/// root.
const NARROW_IMAGES: &str = "target/console-screenshots/narrow";

/// A width a screenshot is taken at.
#[derive(Debug, Clone, Copy)]
enum Width {
    /// 1440 CSS pixels, the book's.
    Wide,
    /// 390 CSS pixels, a phone.
    Narrow,
}

impl Width {
    /// The width in CSS pixels.
    const fn pixels(self) -> u32 {
        match self {
            Self::Wide => 1440,
            Self::Narrow => 390,
        }
    }
}

/// The height of the window a page is laid out in before its full height is
/// measured.
const HEIGHT: u32 = 900;

/// The tallest screenshot, so a page that grows without bound stays a file
/// the book can hold.
const TALLEST: u32 = 8000;

/// One screenshot of a console page.
#[derive(Debug)]
struct Shot {
    /// The image's name, without the theme suffix and the extension.
    image: &'static str,
    /// The route of the console's router the page answers, as `path!` names
    /// it, or `*` for the fallback.
    route: &'static str,
    /// The path the browser loads.
    url: &'static str,
    /// The `XPath` of an element the page shows once it has loaded.
    ready: &'static str,
}

/// The sign-in card a visitor reads.
const SIGN_IN: Shot = Shot {
    image: "sign-in",
    route: "/",
    url: "/",
    ready: "//a[normalize-space(.) = 'Sign in']",
};

/// The pages a signed-in operator loads and reads as they are.
const PAGES: [Shot; 11] = [
    Shot {
        image: "dashboard",
        route: "/",
        url: "/",
        ready: "//h2[contains(., 'Readiness')]",
    },
    Shot {
        image: "mappings",
        route: "/mappings",
        url: "/mappings",
        ready: "//h2[contains(., 'Contexts')]",
    },
    Shot {
        image: "files",
        route: "/files",
        url: "/files",
        ready: "//table//code",
    },
    Shot {
        image: "file-context",
        route: "/files/*path",
        url: "/files/fhirconnect/ferrobridge_journey_diagnose.context.yml",
        ready: "//*[@id = 'L1']",
    },
    Shot {
        image: "file-model",
        route: "/files/*path",
        url: "/files/fhirconnect/ferrobridge_journey_diagnose.yml",
        ready: "//*[@id = 'L1']",
    },
    Shot {
        image: "edit-new",
        route: "/edit",
        url: "/edit",
        ready: "//*[@id = 'edit-draft']",
    },
    Shot {
        image: "edit-provisioned",
        route: "/edit/*path",
        url: "/edit/fhirconnect/ferrobridge_journey_diagnose.yml",
        ready: "//p[@role = 'note']",
    },
    Shot {
        image: "playground",
        route: "/playground",
        url: "/playground",
        ready: "//*[@id = 'toopenehr-bundle']",
    },
    Shot {
        image: "validate",
        route: "/validate",
        url: "/validate",
        ready: "//h1[contains(., 'Validation dry run')]",
    },
    Shot {
        image: "capability",
        route: "/capability",
        url: "/capability",
        ready: "//h2[contains(., 'CapabilityStatement')]",
    },
    Shot {
        image: "about",
        route: "/about",
        url: "/about",
        ready: "//h1",
    },
];

/// The page no route answers.
const NOT_FOUND: Shot = Shot {
    image: "not-found",
    route: "*",
    url: "/no/such/page",
    ready: "//h1",
};

/// A `$toopenehr` the bridge answered.
const PLAYGROUND_ANSWERED: Shot = Shot {
    image: "playground-answered",
    route: "/playground",
    url: "/playground",
    ready: "//h2[contains(., '$toopenehr answered')]",
};

/// A `$tofhir` the bridge refused.
const PLAYGROUND_REFUSED: Shot = Shot {
    image: "playground-refused",
    route: "/playground",
    url: "/playground",
    ready: "//p[contains(., 'the bridge refused the request.')]",
};

/// A bridge whose operations lane is off.
const PLAYGROUND_OFF: Shot = Shot {
    image: "playground-off",
    route: "/playground",
    url: "/playground",
    ready: "//li[contains(., '[operations] enabled is false')]",
};

/// A draft the bridge refused to save.
const EDIT_REFUSED: Shot = Shot {
    image: "edit-refused",
    route: "/edit",
    url: "/edit",
    ready: "//h2[contains(., 'Not saved')]",
};

/// A draft the bridge saved and reloaded.
const EDIT_SAVED: Shot = Shot {
    image: "edit-saved",
    route: "/edit",
    url: "/edit",
    ready: "//div[contains(., 'Saved, and the set reloaded with it.')]",
};

/// Every shot the capture takes.
fn shots() -> impl Iterator<Item = &'static Shot> {
    [&SIGN_IN].into_iter().chain(PAGES.iter()).chain([
        &PLAYGROUND_ANSWERED,
        &PLAYGROUND_REFUSED,
        &PLAYGROUND_OFF,
        &EDIT_REFUSED,
        &EDIT_SAVED,
        &NOT_FOUND,
    ])
}

/// The workspace root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The file of `image` in the theme `theme`.
fn file(image: &str, theme: Theme) -> PathBuf {
    root()
        .join(IMAGES)
        .join(format!("{image}{}.png", theme.suffix()))
}

/// A theme of the console.
#[derive(Debug, Clone, Copy)]
enum Theme {
    /// The light theme.
    Light,
    /// The dark theme.
    Dark,
}

impl Theme {
    /// The word the theme cookie holds.
    fn word(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// What the image's name carries for the theme.
    fn suffix(self) -> &'static str {
        match self {
            Self::Light => "",
            Self::Dark => "-dark",
        }
    }
}

/// The part of the Chrome `DevTools` Protocol's `Page.getLayoutMetrics`
/// answer the capture reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutMetrics {
    /// The size of the page's content, in CSS pixels.
    css_content_size: ContentSize,
}

/// A size in CSS pixels.
#[derive(Debug, Deserialize)]
struct ContentSize {
    /// The height.
    height: f64,
}

/// Lays the page out in a window `height` CSS pixels tall at `width`.
async fn window(browser: &Browser, width: Width, height: u32) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width.pixels(), "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )
        .await?;
    Ok(())
}

/// Where the image of `image` in `theme` at `width` is written: the book's
/// folder for the wide one, the ignored folder for the narrow one.
fn output(image: &str, theme: Theme, width: Width) -> PathBuf {
    match width {
        Width::Wide => file(image, theme),
        Width::Narrow => root()
            .join(NARROW_IMAGES)
            .join(format!("{image}{}.png", theme.suffix())),
    }
}

/// Has the browser report `theme` as the system's colour scheme, so the
/// form controls the browser draws match the page.
async fn scheme(browser: &Browser, theme: Theme) -> Result<(), Box<dyn Error>> {
    browser
        .driver()
        .cdp()
        .send_raw(
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-color-scheme", "value": theme.word() }] }),
        )
        .await?;
    Ok(())
}

/// The height of the page's content, in whole CSS pixels from [`HEIGHT`] to
/// [`TALLEST`].
async fn content_height(browser: &Browser) -> Result<u32, Box<dyn Error>> {
    let metrics = browser
        .driver()
        .cdp()
        .send_raw("Page.getLayoutMetrics", json!({}))
        .await?;
    let metrics: LayoutMetrics = serde_json::from_value(metrics)?;
    let content = metrics.css_content_size.height;
    Ok((HEIGHT..=TALLEST)
        .find(|tall| f64::from(*tall) >= content)
        .unwrap_or(TALLEST))
}

/// The part of a Chrome `DevTools` Protocol answer that names a DOM node.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    /// The node, `0` when none matched.
    node_id: u64,
}

/// The document a `DOM.getDocument` answer names.
#[derive(Debug, Deserialize)]
struct Document {
    /// The document's root node.
    root: Found,
}

/// Scrolls the page back to its top, where the lockup or the sign-in card
/// sits: an answer takes the focus and scrolls the page to it.
async fn to_top(browser: &Browser) -> Result<(), Box<dyn Error>> {
    let cdp = browser.driver().cdp();
    let document: Document =
        serde_json::from_value(cdp.send_raw("DOM.getDocument", json!({})).await?)?;
    let top: Found = serde_json::from_value(
        cdp.send_raw(
            "DOM.querySelector",
            json!({ "nodeId": document.root.node_id, "selector": "body > div" }),
        )
        .await?,
    )?;
    if top.node_id != 0 {
        cdp.send_raw(
            "DOM.scrollIntoViewIfNeeded",
            json!({ "nodeId": top.node_id, "rect": { "x": 0, "y": 0, "width": 1, "height": 1 } }),
        )
        .await?;
    }
    Ok(())
}

/// Waits for `shot` to show in `theme`, and writes it at the page's full
/// height.
async fn capture(
    browser: &Browser,
    shot: &Shot,
    theme: Theme,
    width: Width,
) -> Result<(), Box<dyn Error>> {
    browser.element(By::XPath(shot.ready)).await?;
    browser
        .element(By::Css(format!(r#"html[data-theme="{}"]"#, theme.word())))
        .await?;
    hydrated(browser).await?;
    let height = content_height(browser).await?;
    window(browser, width, height).await?;
    to_top(browser).await?;
    let png = browser.driver().screenshot_as_png().await?;
    window(browser, width, HEIGHT).await?;
    std::fs::write(output(shot.image, theme, width), png)?;
    if shot.route != NOT_FOUND.route {
        return browser.console_clean(shot.image).await;
    }
    // The fallback answers 404, which the browser logs as an error: that
    // answer is the page shown, so it is the one entry allowed.
    let errors = browser.console_errors().await?;
    match errors.as_slice() {
        [error] if error.contains(shot.url) && error.contains("404") => Ok(()),
        _ => Err(format!("the page of an unknown path logged {errors:?}, not its one 404").into()),
    }
}

/// Loads `shot` and captures it in `theme`.
async fn visit(
    browser: &Browser,
    stack: &Stack,
    shot: &Shot,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url(shot.url)).await?;
    capture(browser, shot, theme, width).await
}

/// Puts the theme cookie for `theme` in the browser, which must be at the
/// console.
async fn choose(browser: &Browser, theme: Theme) -> Result<(), Box<dyn Error>> {
    let mut cookie = Cookie::new(ferrobridge_console::session::THEME_COOKIE, theme.word());
    cookie.set_path("/");
    browser.driver().add_cookie(cookie).await?;
    scheme(browser, theme).await
}

/// Signs in to `stack` afresh in `theme`.
async fn enter(browser: &Browser, stack: &Stack, theme: Theme) -> Result<(), Box<dyn Error>> {
    browser.driver().delete_all_cookies().await?;
    browser.goto(&stack.url("/")).await?;
    choose(browser, theme).await?;
    browser.goto(&stack.url("/")).await
}

/// Runs the playground's answers and the editor's two outcomes, and
/// captures each in `theme`.
async fn answers(
    browser: &Browser,
    stack: &Stack,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/playground")).await?;
    hydrated(browser).await?;
    to_openehr(browser, &bundle()?).await?;
    capture(browser, &PLAYGROUND_ANSWERED, theme, width).await?;
    browser.goto(&stack.url("/playground")).await?;
    hydrated(browser).await?;
    to_fhir(browser, "{}").await?;
    capture(browser, &PLAYGROUND_REFUSED, theme, width).await?;

    let slug = "second";
    let path = context_path(slug);
    draft(browser, stack, &path, &context(slug, MISSING_TEMPLATE)).await?;
    save(browser, "a context over a template the bridge lacks").await?;
    capture(browser, &EDIT_REFUSED, theme, width).await?;
    draft(browser, stack, &path, &context(slug, TEMPLATE)).await?;
    save(browser, "a second profile over the diagnosis").await?;
    capture(browser, &EDIT_SAVED, theme, width).await
}

/// Captures every shot of the full stack in `theme`.
async fn every_page(
    browser: &Browser,
    stack: &Stack,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    enter(browser, stack, theme).await?;
    visit(browser, stack, &SIGN_IN, (theme, width)).await?;
    sign_in(browser, stack).await?;
    for shot in &PAGES {
        visit(browser, stack, shot, (theme, width)).await?;
    }
    answers(browser, stack, (theme, width)).await?;
    visit(browser, stack, &NOT_FOUND, (theme, width)).await
}

/// Captures the playground of a bridge whose operations lane is off in
/// `theme`.
async fn lane_off(
    browser: &Browser,
    stack: &Stack,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    enter(browser, stack, theme).await?;
    sign_in(browser, stack).await?;
    browser.goto(&stack.url("/playground")).await?;
    hydrated(browser).await?;
    to_openehr(browser, &bundle()?).await?;
    capture(browser, &PLAYGROUND_OFF, theme, width).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_book_shows_every_console_page_in_light_and_dark() -> Result<(), Box<dyn Error>> {
    if !std::env::var(SCREENSHOTS_GATE).is_ok_and(|value| value == "1") {
        return Ok(());
    }
    std::fs::create_dir_all(root().join(IMAGES))?;
    std::fs::create_dir_all(root().join(NARROW_IMAGES))?;
    // Each theme and width runs on a stack of its own, so a save one pass
    // made is not on another pass's pages.
    for width in [Width::Wide, Width::Narrow] {
        for theme in [Theme::Light, Theme::Dark] {
            let stack = Stack::start().await?;
            let off = Stack::start_with(Lanes::WithoutOperations).await?;
            with_browser(async |browser| {
                window(browser, width, HEIGHT).await?;
                every_page(browser, &stack, (theme, width)).await?;
                lane_off(browser, &off, (theme, width)).await
            })
            .await?;
        }
    }
    std::fs::write(
        root().join(IMAGES).join(CAPTURED),
        format!("{}\n", env!("CARGO_PKG_VERSION")),
    )?;
    Ok(())
}

/// The routes the console's router declares, read from its source.
fn routes() -> Result<BTreeSet<String>, Box<dyn Error>> {
    // NOTE: no specification governs this: our own design; the testkit's
    // dev-dependency on the console already reaches it, so the path is joined
    // from parts and scripts/checks/affected.sh reads no second edge from it.
    let source = std::fs::read_to_string(
        root()
            .join("app")
            .join("ferrobridge-console")
            .join("src")
            .join("app.rs"),
    )?;
    let mut routes: BTreeSet<String> = source
        .split("path!(\"")
        .skip(1)
        .filter_map(|rest| rest.split_once('"').map(|(route, _)| route.to_owned()))
        .collect();
    if routes.is_empty() {
        return Err("the console's router declares no path!(…) route".into());
    }
    if source.contains("fallback=") {
        routes.insert(String::from("*"));
    }
    Ok(routes)
}

/// The text of every page of the book.
fn book() -> Result<String, Box<dyn Error>> {
    let mut text = String::new();
    let mut folders = vec![root().join("website/book/src")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?.path();
            if entry.is_dir() {
                folders.push(entry);
            } else if entry.extension().is_some_and(|extension| extension == "md") {
                text.push_str(&std::fs::read_to_string(&entry)?);
            }
        }
    }
    Ok(text)
}

#[test]
fn every_console_route_has_a_screenshot_in_light_and_dark() -> Result<(), Box<dyn Error>> {
    let shown: BTreeSet<&str> = shots().map(|shot| shot.route).collect();
    for route in routes()? {
        assert!(
            shown.contains(route.as_str()),
            "the console route {route} has no screenshot: add a shot for it to journeys::screenshots and run the capture"
        );
    }
    let mut images = BTreeSet::new();
    for shot in shots() {
        for theme in [Theme::Light, Theme::Dark] {
            let image = file(shot.image, theme);
            assert!(
                image.is_file(),
                "{} is missing: run the capture with {SCREENSHOTS_GATE}=1",
                image.display()
            );
            images.insert(image);
        }
    }
    let captured = root().join(IMAGES).join(CAPTURED);
    images.insert(captured.clone());
    let version = std::fs::read_to_string(&captured).map_err(|error| {
        format!(
            "{} cannot be read ({error}): run the capture with {SCREENSHOTS_GATE}=1",
            captured.display()
        )
    })?;
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        version.trim_end(),
        "the console screenshots were taken at another release: run the capture with {SCREENSHOTS_GATE}=1"
    );
    for entry in std::fs::read_dir(root().join(IMAGES))? {
        let entry = root().join(IMAGES).join(entry?.file_name());
        assert!(
            images.contains(&entry),
            "{} is no shot's image: remove it",
            entry.display()
        );
    }
    let book = book()?;
    for shot in shots() {
        let light = format!("img/console/{}.png", shot.image);
        let dark = format!("img/console/{}-dark.png", shot.image);
        assert!(book.contains(&light), "no page of the book shows {light}");
        assert!(book.contains(&dark), "no page of the book links {dark}");
    }
    Ok(())
}
