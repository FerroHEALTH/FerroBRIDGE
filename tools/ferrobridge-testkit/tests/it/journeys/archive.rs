// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set as a ZIP: an archive imported from the import page is
//! checked and saved as a revision that does not run, and its page applies
//! it.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{Browser, with_browser};
use crate::journeys::editor::{TEMPLATE, context, context_path, press};
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

/// Writes a ZIP of one context `slug` names into `directory`, as an export
/// writes it, and returns its path.
pub(crate) fn archive(directory: &std::path::Path, slug: &str) -> Result<PathBuf, Box<dyn Error>> {
    let contents = BTreeMap::from([(context_path(slug), context(slug, TEMPLATE).into_bytes())]);
    let bytes = ferrobridge_server::operator::revisions::archive::export(&contents)?;
    let path = directory.join(format!("{slug}.zip"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Imports the ZIP at `path` from the import page with `reason`.
pub(crate) async fn import(
    browser: &Browser,
    stack: &Stack,
    path: &std::path::Path,
    reason: &str,
) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/mappings/import")).await?;
    loaded(browser).await?;
    browser
        .element(By::Id("import-archive"))
        .await?
        .send_keys(path.to_string_lossy().as_ref())
        .await?;
    browser
        .element(By::Id("import-reason"))
        .await?
        .send_keys(reason)
        .await?;
    press(browser, "import").await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_imported_archive_is_a_revision_its_page_applies() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    let files = tempfile::tempdir()?;
    let zipped = archive(files.path(), "imported")?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        import(
            browser,
            &stack,
            &zipped,
            "a context from the regional repository",
        )
        .await?;
        browser
            .titled(&format!("Import a mapping set · {PRODUCT}"))
            .await?;
        browser
            .element(By::XPath("//*[contains(., 'saved as revision 2')]"))
            .await?;
        browser
            .element(By::XPath(
                "//a[contains(., 'Open the revision to apply it')]",
            ))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/mappings/revisions/2")).await?;
        loaded(browser).await?;
        browser
            .element(By::Id("apply-reason"))
            .await?
            .send_keys("apply the regional context")
            .await?;
        press(browser, "apply").await?;
        browser
            .element(By::XPath(
                "//*[contains(., 'Applied: the set compiled and runs as revision 3')]",
            ))
            .await?;
        browser.goto(&stack.url("/mappings/files")).await?;
        loaded(browser).await?;
        if !browser
            .driver()
            .source()
            .await?
            .contains(&context_path("imported"))
        {
            return Err("the imported context is not listed after the apply".into());
        }
        browser.console_clean("the mapping set's import").await
    })
    .await
}
