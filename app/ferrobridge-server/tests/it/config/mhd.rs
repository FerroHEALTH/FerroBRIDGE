// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.mhd]`: off by default, and on only beside the identifier search
//! a chained `patient.identifier` runs.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;

/// An enabled facade with `tables` after its `[facade]` table.
fn with(tables: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n{tables}"
    )
}

#[test]
fn the_responder_is_off_until_a_deployment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with("")), &BTreeMap::new())?.resolve(Process::Job)?;
    assert!(!settings.facade.ok_or("the facade lane is on")?.mhd);
    Ok(())
}

#[test]
fn the_responder_needs_the_identifier_search() -> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(
        Some(&with("[facade.mhd]\nenabled = true\n")),
        &BTreeMap::new(),
    )?
    .resolve(Process::Job)
    .err()
    .ok_or("the responder without an identifier search is refused")?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "facade.mhd.enabled" && needs == "facade.identifier_search.enabled"),
        "{error:?}"
    );
    let settings = Config::from_sources(
        Some(&with(
            "[facade.mhd]\nenabled = true\n\n[facade.identifier_search]\nenabled = true\n",
        )),
        &BTreeMap::new(),
    )?
    .resolve(Process::Job)?;
    assert!(settings.facade.ok_or("the facade lane is on")?.mhd);
    Ok(())
}
