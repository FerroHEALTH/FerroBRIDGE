// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face: its tabs lead to the `ConceptMaps` the face runs, each
//! with what it maps from and to and the messages it served.

use std::error::Error;

use ferrobridge_console::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::with_browser;
use crate::journeys::journeys_enabled;
use crate::journeys::playground::loaded;
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

#[tokio::test(flavor = "multi_thread")]
async fn the_hl7v2_face_lists_the_concept_maps_it_runs() -> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/hl7v2")).await?;
        browser.titled(&format!("HL7 v2 · {PRODUCT}")).await?;
        loaded(browser).await?;
        browser
            .element(By::XPath(
                "//nav[contains(@class, 'fui-tabs')]//a[normalize-space(.) = 'ConceptMaps']",
            ))
            .await?
            .click()
            .await?;
        browser.at(&stack.url("/hl7v2/concept-maps")).await?;
        for text in [
            "Segment PID to Patient Map",
            "segment-pid-to-patient",
            "table-hl70001-to-administrative-gender",
            "http://hl7.org/fhir/administrative-gender",
        ] {
            browser
                .element(By::XPath(format!("//table//*[contains(., '{text}')]")))
                .await?;
        }
        browser
            .element(By::XPath(
                "//tr[.//*[contains(., 'segment-pid-to-patient')]]/td[last()][normalize-space(.) = '0']",
            ))
            .await?;
        browser.console_clean("the HL7 v2 face's ConceptMaps").await
    })
    .await
}
