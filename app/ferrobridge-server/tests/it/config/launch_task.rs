// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.launch_task]`: off by default, its store file named by the
//! deployment, and settable from the environment.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;

/// An enabled facade with `launch_task` as its `[facade.launch_task]` table.
fn with_launch_task(launch_task: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.launch_task]\n{launch_task}"
    )
}

#[test]
fn launch_tasks_are_off_until_a_deployment_turns_them_on() -> Result<(), Box<dyn std::error::Error>>
{
    let settings = Config::from_sources(Some(&with_launch_task("")), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.launch_task);
    Ok(())
}

#[test]
fn an_enabled_endpoint_keeps_its_tasks_in_the_default_file()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(
        Some(&with_launch_task("enabled = true\n")),
        &BTreeMap::new(),
    )?
    .resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(Some(PathBuf::from("launch-tasks.redb")), facade.launch_task);
    Ok(())
}

#[test]
fn the_environment_turns_it_on_and_names_the_file() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__FACADE__LAUNCH_TASK__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__FACADE__LAUNCH_TASK__STORE"),
            String::from("/var/lib/ferrobridge/launch-tasks.redb"),
        ),
    ]);
    let settings =
        Config::from_sources(Some(&with_launch_task("")), &environment)?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        Some(PathBuf::from("/var/lib/ferrobridge/launch-tasks.redb")),
        facade.launch_task
    );
    Ok(())
}

#[test]
fn an_unknown_key_refuses_to_boot() {
    assert!(
        Config::from_sources(Some(&with_launch_task("enable = true\n")), &BTreeMap::new()).is_err()
    );
}
