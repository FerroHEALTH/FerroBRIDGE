// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a capturing log writer, test settings, a test state and a
//! terminology translator.

use ferrobridge_server::config::ServerSettings;
use ferrobridge_server::health::Registry;
use ferrobridge_server::state::AppState;
use ferrobridge_server::terminology::Translator;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use std::error::Error as StdError;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;
use wiremock::MockServer;

/// Returns the subject link in `namespace` with the reference id form and the
/// `PERSON` type, the facade's defaults.
pub(crate) fn subject_link(namespace: &str) -> ferrobridge_server::facade::subject::SubjectLink {
    ferrobridge_server::facade::subject::SubjectLink::new(
        namespace,
        ferrobridge_server::facade::subject::IdForm::Reference,
        "PERSON",
    )
    .expect("a legal subject link")
}

/// A `tracing` writer that keeps every line in memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Returns everything written so far.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The writer [`Logs`] hands out.
#[derive(Debug)]
pub(crate) struct LogsWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = LogsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogsWriter(Arc::clone(&self.0))
    }
}

/// Returns server settings a test drives the middleware with.
pub(crate) fn settings() -> ServerSettings {
    ServerSettings {
        listen: "127.0.0.1:0".parse().expect("a socket address"),
        request_timeout: Duration::from_secs(5),
        shutdown_timeout: Duration::from_secs(5),
        body_limit: 1024,
    }
}

/// Returns a state with no indicator and no logged query parameter.
pub(crate) fn state() -> Arc<AppState> {
    Arc::new(AppState::with_health(Registry::default()))
}

/// Returns a translator over the R4 terminology server `server` mocks.
pub(crate) fn translator(server: &MockServer) -> Result<Translator, Box<dyn StdError>> {
    let config = Config::new(format!("{}/r4", server.uri()).parse()?, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Translator::new(Some(Client::new(config)?)))
}
