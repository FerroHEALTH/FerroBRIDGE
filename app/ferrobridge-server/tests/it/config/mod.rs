// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration contract: the file, the environment over it, the `_file`
//! secrets, and every refusal.

mod audit;
mod auth;
mod change_feed;
mod consent;
mod environment;
mod etl;
mod events;
mod facade;
mod fhir_outbound;
mod file;
mod hl7v2;
mod oauth;
mod omop_incremental;
mod onward;
mod operator;
mod refusals;
mod secrets;
mod usage_report;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use std::collections::BTreeMap;

/// A file with every section set, so a test can override one key at a time.
const FULL: &str = r#"
[server]
listen = "0.0.0.0:9000"
request_timeout = "1s"
shutdown_timeout = "2s"
body_limit = "4KiB"

[telemetry]
format = "json"
filter = "debug"
logged_query_parameters = ["_count"]

[cdr]
endpoint.url = "http://cdr.invalid/v1"
endpoint.timeout = "5s"

[cdr.endpoint.retry]
max_attempts = 5
initial_backoff = "10ms"
max_backoff = "20ms"

[terminology]
endpoint.url = "http://tx.invalid/r4"
wire_version = "r4b"

[cdm]
url = "postgres://bridge@db.invalid/cdm?sslmode=disable"

[mappings]
directory = "/srv/ferrobridge/mappings"
templates = "/srv/ferrobridge/templates"

[operations]
enabled = false
device_reference = "Device/bridge-one"
composer = "A synthetic composer"
composition_language = "de"
composition_territory = "DE"
"#;

/// Returns the environment map a single override makes.
fn env(name: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_owned(), value.to_owned())])
}

/// Returns the refusal `text` resolves to.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("the configuration is refused"))
}
