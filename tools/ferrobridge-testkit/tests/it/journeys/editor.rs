// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping editor end to end against the running bridge: a draft that
//! does not compile is refused and leaves the running set unchanged, a save
//! writes the draft byte for byte and reloads the programs, a reload
//! compiles the files on disk again, and the bridge's audit line names the
//! actor and the file and nothing of the draft.

use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use thirtyfour::By;
use tracing_subscriber::fmt::MakeWriter;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{CONTEXT, Stack};

/// The slug of the context the journey writes.
const SECOND: &str = "second";

/// The template the bridge holds.
pub(crate) const TEMPLATE: &str = "ferrobridge.diagnose.v1";

/// A template the bridge lacks, so a context over it does not compile.
pub(crate) const MISSING_TEMPLATE: &str = "ferrobridge.unknown.v1";

/// The wire path of the context `slug` names.
pub(crate) fn context_path(slug: &str) -> String {
    format!("fhirconnect/ferrobridge_journey_{slug}.context.yml")
}

/// The name of the context `slug` names.
fn context_name(slug: &str) -> String {
    format!("ferrobridge_journey_{slug}.context")
}

/// The profile of the context `slug` names, which no log line may carry.
fn context_profile(slug: &str) -> String {
    format!("http://example.org/fhir/StructureDefinition/ferrobridge-journey-{slug}")
}

/// A further context `slug` names over the provisioned model, naming the
/// template `template`.
pub(crate) fn context(slug: &str, template: &str) -> String {
    let lines = [
        String::from("grammar: FHIRConnect/v1.0.0"),
        String::from("type: context"),
        String::from("metadata:"),
        format!("  name: {}", context_name(slug)),
        String::from("  version: 1.0.0"),
        String::from("spec:"),
        String::from("  system: FHIR"),
        String::from("  version: R4"),
        String::from("context:"),
        String::from("  profile:"),
        format!("    url: \"{}\"", context_profile(slug)),
        String::from("  template:"),
        format!("    id: \"{template}\""),
        String::from("  archetypes:"),
        String::from("    - \"ferrobridge_journey_diagnose\""),
        String::from("  start: \"ferrobridge_journey_diagnose\""),
    ];
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// The `tracing` lines the bridge and the console write during a journey.
#[derive(Debug, Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Everything written so far.
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

/// The writer [`Logs`] hands out.
#[derive(Debug)]
struct LogsWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogsWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = LogsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogsWriter(Arc::clone(&self.0))
    }
}

/// Opens a new file at `path` in the editor and writes `draft` into it.
pub(crate) async fn draft(
    browser: &Browser,
    stack: &Stack,
    path: &str,
    draft: &str,
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/mappings/new")).await?;
    loaded(browser).await?;
    browser
        .element(By::Id("editor-path"))
        .await?
        .send_keys(path)
        .await?;
    browser
        .element(By::Id("draft"))
        .await?
        .send_keys(draft)
        .await?;
    Ok(())
}

/// Clicks the editor's submit button that posts the decision `decision`.
pub(crate) async fn press(browser: &Browser, decision: &str) -> Result<(), Box<dyn Error>> {
    let button = By::XPath(format!(
        "//button[@name = 'decision' and @value = '{decision}']"
    ));
    browser.enabled(button.clone()).await?;
    browser.element(button).await?.click().await?;
    Ok(())
}

/// Saves the new file open in the editor with `reason`, and confirms the
/// save.
pub(crate) async fn save(browser: &Browser, reason: &str) -> Result<(), Box<dyn Error>> {
    browser
        .element(By::Id("editor-reason"))
        .await?
        .send_keys(reason)
        .await?;
    press(browser, "save").await?;
    press(browser, "confirm").await
}

/// Reloads the set from the Mapping set's More menu and its confirmation.
pub(crate) async fn reload(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/mappings")).await?;
    loaded(browser).await?;
    browser
        .element(By::XPath("//summary[contains(@aria-controls, 'set-more')]"))
        .await?
        .click()
        .await?;
    browser
        .element(By::XPath("//button[@commandfor = 'reload-set']"))
        .await?
        .click()
        .await?;
    let confirm = By::XPath("//dialog[@id = 'reload-set']//button[@type = 'submit']");
    browser.enabled(confirm.clone()).await?;
    browser.element(confirm).await?.click().await?;
    Ok(())
}

/// The names of the contexts the Mapping set lists, of the provisioned one
/// and the journey's.
async fn contexts(browser: &Browser, stack: &Stack) -> Result<Vec<String>, Box<dyn Error>> {
    browser.goto(&stack.url("/mappings")).await?;
    browser.text("h1", "Mapping set").await?;
    browser.text("tr", CONTEXT).await?;
    let mut names = Vec::new();
    for name in [CONTEXT.to_owned(), context_name(SECOND)] {
        let found = browser
            .driver()
            .find_all(By::XPath(format!("//tr[contains(., '{name}')]")))
            .await?;
        if !found.is_empty() {
            names.push(name);
        }
    }
    Ok(names)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_draft_changes_nothing_and_a_save_is_byte_identical_and_reloads()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let logs = Logs::default();
    tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .try_init()
        .map_err(|error| -> Box<dyn Error> { error })?;
    let stack = Stack::start().await?;
    let path = context_path(SECOND);
    let saved = stack.console_files().join(&path);
    let valid = context(SECOND, TEMPLATE);
    let invalid = context(SECOND, MISSING_TEMPLATE);
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        draft(browser, &stack, &path, &invalid).await?;
        press(browser, "check").await?;
        browser
            .text("main", "The set does not compile with this draft")
            .await?;
        browser
            .element(By::XPath(
                "//button[@name = 'decision' and @value = 'save' and @disabled]",
            ))
            .await?;
        browser
            .console_clean("a draft that does not compile")
            .await?;

        draft(browser, &stack, &path, &invalid).await?;
        save(browser, "a context over a template the bridge lacks").await?;
        browser
            .text(
                "main",
                "Not saved: the set does not compile with your draft",
            )
            .await?;
        browser.console_clean("a refused draft").await?;
        assert!(!saved.exists(), "a refused draft wrote {}", saved.display());
        assert_eq!(vec![CONTEXT.to_owned()], contexts(browser, &stack).await?);

        draft(browser, &stack, &path, &valid).await?;
        press(browser, "check").await?;
        browser
            .text("main", "The set compiles with this draft")
            .await?;
        save(browser, "a second profile over the diagnosis").await?;
        browser
            .text("main", "Saved, and the set reloaded with it.")
            .await?;
        browser.console_clean("a save").await?;
        assert_eq!(valid.as_bytes(), std::fs::read(&saved)?.as_slice());
        assert_eq!(
            vec![CONTEXT.to_owned(), context_name(SECOND)],
            contexts(browser, &stack).await?
        );

        reload(browser, &stack).await?;
        browser
            .text(
                "main",
                "Reloaded: the set compiled and replaced the running one",
            )
            .await?;
        browser.console_clean("a reload").await
    })
    .await?;

    let text = logs.text();
    let profile = context_profile(SECOND);
    let audit: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("action=\"save\""))
        .collect();
    assert_eq!(2, audit.len(), "one audit line per save:\n{text}");
    for line in &audit {
        assert!(line.contains("subject=\"synthetic-operator\""), "{line}");
        assert!(line.contains(&format!("file=\"{path}\"")), "{line}");
    }
    assert!(
        !text.contains(&profile),
        "the draft reached a log line:\n{text}"
    );
    Ok(())
}
