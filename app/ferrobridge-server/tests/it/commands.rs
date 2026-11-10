// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The family's `config check` and `healthcheck`, run as the binary.

use std::error::Error;
use std::process::{Command, Output};
use std::sync::Arc;

use ferrohealth_health::registry::Registry;
use ferrohealth_server::run::EXIT_CONFIG;

use crate::support;

/// Runs the binary with `args` and `environment`, under no configuration
/// file and no usage report.
fn binary(args: &[&str], environment: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(args)
        .env_remove("FERROBRIDGE_CONFIG")
        .env_remove("RUST_LOG")
        .env("FERROBRIDGE__USAGE_REPORT__ENABLED", "false")
        .envs(environment.iter().copied())
        .output()
        .expect("the binary runs")
}

#[test]
fn config_check_prints_the_running_tree_with_every_secret_redacted() {
    let output = binary(
        &["config", "check"],
        &[(
            "FERROBRIDGE__CDM__URL",
            "postgres://bridge:synthetic-s3cret@db.invalid/cdm?sslmode=disable",
        )],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(Some(0), output.status.code(), "{stdout}");
    assert!(stdout.contains("server.listen"), "{stdout}");
    assert!(stdout.contains("cdm.url"), "{stdout}");
    assert!(stdout.contains("***"), "the secret is redacted: {stdout}");
    assert!(
        !stdout.contains("synthetic-s3cret"),
        "no secret reaches the report: {stdout}"
    );
}

/// A configured issuer identifier with a password in its userinfo reaches
/// neither stream of `config check`, rendered or refused (RFC 3986 §3.2.1:
/// the password is never rendered in clear text): the inbound issuer, the
/// CDR grant's issuer and the onward issuer.
#[test]
fn config_check_never_renders_an_issuers_userinfo() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    for (name, text) in [
        (
            "auth",
            "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = \"https://u:SYNTH_AUTH_PW@as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://bridge.example.org/fhir\"]\n",
        ),
        (
            "oauth",
            "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[cdr.endpoint.credentials.oauth]\nissuer = \"https://u:SYNTH_OAUTH_PW@as.example.org/realms/ferro\"\nclient_id = \"ferrobridge\"\nclient_secret = \"synthetic-client-secret\"\n",
        ),
        (
            "onward",
            "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[cdr.endpoint.credentials.onward]\nissuer = \"https://u:SYNTH_ONWARD_PW@as.example.org/realms/ferro\"\nkind = \"persons\"\n",
        ),
    ] {
        let path = dir.path().join(format!("{name}.toml"));
        std::fs::write(&path, text)?;
        let file = path.to_str().ok_or("a UTF-8 path")?;
        let output = binary(&["config", "check"], &[("FERROBRIDGE_CONFIG", file)]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stdout.contains("as.example.org") || stderr.contains("as.example.org"),
            "{name}: the issuer is named: {stdout}{stderr}"
        );
        for shown in [&stdout, &stderr] {
            assert!(
                !shown.contains("_PW@") && !shown.contains("SYNTH_"),
                "{name}: no issuer password reaches config check: {shown}"
            );
        }
    }
    Ok(())
}

#[test]
fn config_check_names_a_refused_key_and_exits_seventy_eight() {
    let output = binary(
        &["config", "check"],
        &[("FERROBRIDGE__SERVER__LISTEN_PORT", "8080")],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        Some(i32::from(EXIT_CONFIG.get())),
        output.status.code(),
        "{stderr}"
    );
    assert!(stderr.contains("server.listen_port"), "{stderr}");
}

#[test]
fn config_check_reports_a_refusal_the_resolution_finds() {
    let output = binary(
        &["config", "check"],
        &[
            ("FERROBRIDGE__SERVER__CALLER_RATE__REQUESTS_PER_SECOND", "5"),
            ("FERROBRIDGE__SERVER__CALLER_RATE__BURST", "10"),
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        Some(i32::from(EXIT_CONFIG.get())),
        output.status.code(),
        "{stderr}"
    );
    assert!(stderr.contains("server.caller_rate"), "{stderr}");
}

#[test]
fn healthcheck_fails_when_no_server_answers() -> Result<(), Box<dyn Error>> {
    let address = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let output = binary(
        &["healthcheck"],
        &[("FERROBRIDGE__SERVER__LISTEN", &address.to_string())],
    );
    assert_eq!(Some(1), output.status.code());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("ferrobridge healthcheck"),
        "the reason is on stderr"
    );
    Ok(())
}

#[test]
fn healthcheck_succeeds_once_the_server_is_ready() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
    let address = listener.local_addr()?;
    let state = Arc::new(ferrobridge_server::state::AppState::with_health(
        Registry::default(),
    ));
    let lifecycle = state.lifecycle().clone();
    let settings = support::settings();
    let app = ferrobridge_server::router(state, &settings);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = runtime.spawn(async move {
        ferrobridge_server::serve_until(listener, app, &settings, lifecycle, async move {
            let _closed = stopped.await.is_err();
        })
        .await
    });
    let output = binary(
        &["healthcheck"],
        &[("FERROBRIDGE__SERVER__LISTEN", &address.to_string())],
    );
    assert_eq!(
        Some(0),
        output.status.code(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stop.send(()).map_err(|()| "the server is gone")?;
    runtime.block_on(serving)??;
    Ok(())
}
