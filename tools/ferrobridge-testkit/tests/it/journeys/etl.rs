// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL's run history: its Runs tab lists the runs the bridge schema
//! keeps, and a run's link opens its report with the record it refused.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::{Lanes, RUN_ID, Stack};

#[tokio::test(flavor = "multi_thread")]
async fn the_etl_lists_its_runs_and_opens_the_report_of_one() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start_with(Lanes::WithEtl).await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/omop")).await?;
        browser.titled(&format!("OMOP ETL · {PRODUCT}")).await?;
        loaded(browser).await?;
        browser
            .element(By::XPath(
                "//nav[contains(@class, 'fui-tabs')]//a[normalize-space(.) = 'Runs']",
            ))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/omop/runs")).await?;
        browser
            .element(By::XPath(format!("//table//a[contains(., '{RUN_ID}')]")))
            .await?
            .click()
            .await?;
        browser
            .at(&stack.url(&format!("/omop/runs/{RUN_ID}")))
            .await?;
        browser
            .element(By::XPath(
                "//table//*[contains(., 'the composition names a template the CDR does not hold')]",
            ))
            .await?;
        browser
            .element(By::XPath(
                "//*[contains(., '7d44b88c-4199-4bad-97dc-d78268e01398')]",
            ))
            .await?;
        browser.console_clean("the OMOP ETL's run history").await
    })
    .await
}
