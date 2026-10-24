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

use ferrobridge_console::session::PREFERENCE_PREFIX;
use serde::Deserialize;
use serde_json::json;
use thirtyfour::{By, Cookie};

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::editor::{
    MISSING_TEMPLATE, TEMPLATE, context, context_path, draft, press, reload, save,
};
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
    /// Whether the page is drawn in the shell, whose islands the capture
    /// waits for.
    shell: bool,
}

impl Shot {
    /// A page in the shell, loaded at `url` for `route`.
    const fn page(
        image: &'static str,
        route: &'static str,
        url: &'static str,
        ready: &'static str,
    ) -> Self {
        Self {
            image,
            route,
            url,
            ready,
            shell: true,
        }
    }
}

/// The sign-in card a visitor reads.
const SIGN_IN: Shot = Shot {
    image: "sign-in",
    route: "/",
    url: "/",
    ready: "//a[normalize-space(.) = 'Sign in with your identity provider']",
    shell: false,
};

/// The card a signed-out operator reads.
const SIGNED_OUT: Shot = Shot {
    image: "signed-out",
    route: "/signed-out",
    url: "/signed-out",
    ready: "//*[contains(., 'You are signed out of the console.')]",
    shell: false,
};

/// The wire path of the provisioned context mapping.
const CONTEXT_FILE: &str = "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.context.yml";

/// The pages a signed-in operator loads and reads as they are.
const PAGES: [Shot; 32] = [
    Shot::page("dashboard", "/", "/", "//h1[. = 'Dashboard']"),
    Shot::page(
        "mappings",
        "/mappings",
        "/mappings",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "mappings-omocl",
        "/mappings/:tab",
        "/mappings/omocl",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "mappings-templates",
        "/mappings/:tab",
        "/mappings/templates",
        "//tr[contains(., 'ferrobridge.diagnose.v1')]",
    ),
    Shot::page(
        "mappings-files",
        "/mappings/:tab",
        "/mappings/files",
        "//table//tbody//tr",
    ),
    Shot::page(
        "mappings-diagnostics",
        "/mappings/:tab",
        "/mappings/diagnostics",
        "//h1[. = 'Mapping set']",
    ),
    Shot::page(
        "file-context",
        "/mappings/files/*path",
        CONTEXT_FILE,
        "//*[@id = 'L1']",
    ),
    Shot::page(
        "file-program",
        "/mappings/files/*path",
        "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.context.yml?tab=program",
        "//h1",
    ),
    Shot::page(
        "file-model",
        "/mappings/files/*path",
        "/mappings/files/fhirconnect/ferrobridge_journey_diagnose.yml",
        "//*[@id = 'L1']",
    ),
    Shot::page(
        "template",
        "/mappings/templates/:id",
        "/mappings/templates/ferrobridge.diagnose.v1",
        "//h1",
    ),
    Shot::page(
        "edit-new",
        "/mappings/new",
        "/mappings/new",
        "//*[@id = 'draft']",
    ),
    Shot::page(
        "edit-provisioned",
        "/mappings/edit/*path",
        "/mappings/edit/fhirconnect/ferrobridge_journey_diagnose.yml",
        "//*[@id = 'draft']",
    ),
    Shot::page(
        "playground",
        "/playground",
        "/playground",
        "//*[@id = 'pg-run']",
    ),
    Shot::page(
        "playground-toopenehr",
        "/playground/:tab",
        "/playground/toopenehr",
        "//*[@id = 'pg-run']",
    ),
    Shot::page(
        "playground-validate",
        "/playground/:tab",
        "/playground/validate",
        "//h1[. = 'Playground']",
    ),
    Shot::page("facade", "/facade", "/facade", "//h1[. = 'FHIR facade']"),
    Shot::page(
        "facade-capability",
        "/facade/:tab",
        "/facade/capability",
        "//*[contains(., 'From the CapabilityStatement')]",
    ),
    Shot::page(
        "facade-security",
        "/facade/:tab",
        "/facade/security",
        "//h1[. = 'FHIR facade']",
    ),
    Shot::page(
        "facade-settings",
        "/facade/:tab",
        "/facade/settings",
        "//*[contains(., 'facade.base_url')]",
    ),
    Shot::page("hl7v2", "/hl7v2", "/hl7v2", "//h1[. = 'HL7 v2']"),
    Shot::page(
        "hl7v2-settings",
        "/hl7v2/:tab",
        "/hl7v2/settings",
        "//h1[. = 'HL7 v2']",
    ),
    Shot::page("omop", "/omop", "/omop", "//h1[. = 'OMOP ETL']"),
    Shot::page(
        "omop-settings",
        "/omop/:tab",
        "/omop/settings",
        "//h1[. = 'OMOP ETL']",
    ),
    Shot::page(
        "connections",
        "/connections",
        "/connections",
        "//h1[. = 'Connections']",
    ),
    Shot::page(
        "connection-cdr",
        "/connections/:name",
        "/connections/cdr",
        "//h1",
    ),
    Shot::page(
        "connection-cdr-settings",
        "/connections/:name/:tab",
        "/connections/cdr/settings",
        "//*[contains(., 'cdr.base_url')]",
    ),
    Shot::page(
        "configuration",
        "/configuration",
        "/configuration",
        "//h1[. = 'Configuration']",
    ),
    Shot::page(
        "configuration-mappings",
        "/configuration/:tab",
        "/configuration/mappings",
        "//*[contains(., 'mappings.directory')]",
    ),
    Shot::page(
        "configuration-core",
        "/configuration/:tab",
        "/configuration/core",
        "//*[contains(., 'server.listen')]",
    ),
    Shot::page(
        "preferences",
        "/preferences",
        "/preferences",
        "//h1[. = 'Your preferences']",
    ),
    Shot::page(
        "preferences-language",
        "/preferences/:section",
        "/preferences/language",
        "//h1[. = 'Your preferences']",
    ),
    Shot::page("about", "/about", "/about", "//h1"),
];

/// The page no route answers.
const NOT_FOUND: Shot = Shot::page("not-found", "*", "/no/such/page", "//h1");

/// A `$toopenehr` the bridge answered.
const PLAYGROUND_ANSWERED: Shot = Shot::page(
    "playground-answered",
    "/playground/:tab",
    "/playground/toopenehr",
    "//*[contains(., 'Synthetic problem one')]",
);

/// A `$tofhir` the bridge refused.
const PLAYGROUND_REFUSED: Shot = Shot::page(
    "playground-refused",
    "/playground",
    "/playground",
    "//*[contains(., 'The bridge refused the request')]",
);

/// A bridge whose operations lane is off.
const PLAYGROUND_OFF: Shot = Shot::page(
    "playground-off",
    "/playground/:tab",
    "/playground/toopenehr",
    "//*[contains(., 'Conversions are off on this bridge.')]",
);

/// A draft whose check found the set does not compile.
const EDIT_CHECKED: Shot = Shot::page(
    "edit-checked",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'The set does not compile with this draft')]",
);

/// The confirmation a save asks for.
const EDIT_CONFIRM: Shot = Shot::page(
    "edit-confirm",
    "/mappings/new",
    "/mappings/new",
    "//button[@name = 'decision' and @value = 'confirm']",
);

/// A draft the bridge refused to save.
const EDIT_REFUSED: Shot = Shot::page(
    "edit-refused",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'Not saved: the set does not compile with your draft')]",
);

/// A draft the bridge saved and reloaded.
const EDIT_SAVED: Shot = Shot::page(
    "edit-saved",
    "/mappings/new",
    "/mappings/new",
    "//*[contains(., 'Saved, and the set reloaded with it.')]",
);

/// A reload of the set from the Mapping set.
const MAPPINGS_RELOADED: Shot = Shot::page(
    "mappings-reloaded",
    "/mappings",
    "/mappings",
    "//*[contains(., 'Reloaded: the set compiled and replaced the running one')]",
);

/// Every shot the capture takes.
fn shots() -> impl Iterator<Item = &'static Shot> {
    [&SIGN_IN].into_iter().chain(PAGES.iter()).chain([
        &PLAYGROUND_ANSWERED,
        &PLAYGROUND_REFUSED,
        &PLAYGROUND_OFF,
        &EDIT_CHECKED,
        &EDIT_CONFIRM,
        &EDIT_REFUSED,
        &EDIT_SAVED,
        &MAPPINGS_RELOADED,
        &NOT_FOUND,
        &SIGNED_OUT,
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
    if shot.shell {
        hydrated(browser).await?;
    }
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
    let mut cookie = Cookie::new(format!("{PREFERENCE_PREFIX}theme"), theme.word());
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

/// Runs the playground's answers, the editor's outcomes and a reload, and
/// captures each in `theme`.
async fn answers(
    browser: &Browser,
    stack: &Stack,
    (theme, width): (Theme, Width),
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/playground/toopenehr")).await?;
    hydrated(browser).await?;
    to_openehr(browser, &bundle()?).await?;
    capture(browser, &PLAYGROUND_ANSWERED, theme, width).await?;
    browser.goto(&stack.url("/playground")).await?;
    hydrated(browser).await?;
    to_fhir(browser, "{}").await?;
    capture(browser, &PLAYGROUND_REFUSED, theme, width).await?;

    let slug = "second";
    let path = context_path(slug);
    let invalid = context(slug, MISSING_TEMPLATE);
    draft(browser, stack, &path, &invalid).await?;
    press(browser, "check").await?;
    capture(browser, &EDIT_CHECKED, theme, width).await?;
    draft(browser, stack, &path, &invalid).await?;
    browser
        .element(By::Id("editor-reason"))
        .await?
        .send_keys("a context over a template the bridge lacks")
        .await?;
    press(browser, "save").await?;
    capture(browser, &EDIT_CONFIRM, theme, width).await?;
    press(browser, "confirm").await?;
    capture(browser, &EDIT_REFUSED, theme, width).await?;
    draft(browser, stack, &path, &context(slug, TEMPLATE)).await?;
    save(browser, "a second profile over the diagnosis").await?;
    capture(browser, &EDIT_SAVED, theme, width).await?;
    reload(browser, stack).await?;
    capture(browser, &MAPPINGS_RELOADED, theme, width).await
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
    visit(browser, stack, &NOT_FOUND, (theme, width)).await?;
    visit(browser, stack, &SIGNED_OUT, (theme, width)).await
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
    visit(browser, stack, &PLAYGROUND_OFF, (theme, width)).await
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
