// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration against a stub bridge: the Configuration page
//! and its tabs, the Settings tab of each face and connection, the
//! Connections table's hosts and credentials, and the dialog every file
//! line opens.

use std::error::Error;

use http::StatusCode;
use wiremock::MockServer;

use crate::stub::{answer, bridge, info, readiness, signed_in};
use crate::support::{get_with, send};

#[tokio::test]
async fn the_configuration_page_draws_each_tab_read_only_with_its_keys()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, logging) = send(&service, get_with("/configuration", &cookie)?).await?;
    let logging = logging.replace("<!>", "");
    for words in [
        ">Configuration</h1>",
        "Logging",
        "Server and access",
        "telemetry.format",
        "/etc/ferrobridge/ferrobridge.toml",
        "From the environment",
    ] {
        assert!(logging.contains(words), "{words}: {logging}");
    }
    let (_response, core) = send(&service, get_with("/configuration/core", &cookie)?).await?;
    let core = core.replace("<!>", "");
    assert!(core.contains("0.0.0.0:8080"), "{core}");
    assert!(core.contains("__ferrobridge.operator"), "{core}");
    assert!(core.contains("30 s"), "{core}");
    assert!(core.contains("1 MiB"), "{core}");
    let (response, _body) = send(&service, get_with("/configuration/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn how_configuration_works_opens_as_a_dialog_from_the_file_line() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for route in [
        "/configuration",
        "/hl7v2/settings",
        "/connections/cdm/settings",
    ] {
        let (_response, page) = send(&service, get_with(route, &cookie)?).await?;
        let page = page.replace("<!>", "");
        assert!(
            page.contains(r#"commandfor="how-configuration""#),
            "{route}: {page}"
        );
        assert!(page.contains(r#"id="how-configuration""#), "{route}");
        assert!(page.contains("How configuration works"), "{route}");
        assert!(page.contains("restart the bridge"), "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn each_face_draws_its_settings_from_the_running_configuration() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, facade) = send(&service, get_with("/facade/settings", &cookie)?).await?;
    let facade = facade.replace("<!>", "");
    assert!(
        facade.contains("https://bridge.example.org/fhir"),
        "{facade}"
    );
    assert!(facade.contains("en · NL"), "{facade}");
    let (_response, hl7) = send(&service, get_with("/hl7v2/settings", &cookie)?).await?;
    let hl7 = hl7.replace("<!>", "");
    assert!(hl7.contains("300 s · 30 s"), "{hl7}");
    assert!(hl7.contains("From the environment"), "{hl7}");
    let (_response, omop) = send(&service, get_with("/omop/settings", &cookie)?).await?;
    let omop = omop.replace("<!>", "");
    assert!(omop.contains("SELECT e/ehr_id/value"), "{omop}");
    assert!(omop.contains("etl.visits.aql"), "{omop}");
    assert!(
        omop.contains("not set"),
        "a visit query the file leaves out: {omop}"
    );
    let (response, _body) = send(&service, get_with("/hl7v2/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn a_connection_draws_its_credentials_by_reference() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, cdr) = send(&service, get_with("/connections/cdr/settings", &cookie)?).await?;
    let cdr = cdr.replace("<!>", "");
    assert!(cdr.contains("cdr.credentials.oauth.client_secret"), "{cdr}");
    assert!(cdr.contains("/run/secrets/cdr.secret"), "{cdr}");
    assert!(
        !cdr.contains("cdr.credentials.password"),
        "a CDR on the client credentials grant shows no Basic rows: {cdr}"
    );
    let (_response, term) = send(
        &service,
        get_with("/connections/terminology/settings", &cookie)?,
    )
    .await?;
    let term = term.replace("<!>", "");
    assert!(term.contains("terminology.credentials.password"), "{term}");
    assert!(term.contains("fui-secret-now"), "{term}");
    let (_response, cdm) = send(&service, get_with("/connections/cdm/settings", &cookie)?).await?;
    let cdm = cdm.replace("<!>", "");
    assert!(cdm.contains("/run/secrets/cdm-url"), "{cdm}");
    Ok(())
}

#[tokio::test]
async fn the_connections_table_names_each_host_and_how_it_signs_in() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, list) = send(&service, get_with("/connections", &cookie)?).await?;
    let list = list.replace("<!>", "");
    for words in [
        "Host",
        "Credentials",
        "cdr.example.org",
        "term.example.org",
        "OAuth 2.0 client credentials",
        "HTTP Basic",
        "Password in the connection URL, from a file",
    ] {
        assert!(list.contains(words), "{words}: {list}");
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_configuration_is_drawn_where_the_settings_would_be() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    answer(&server, "/health/info", 200, &info()).await;
    answer(&server, "/health/readiness", 503, &readiness()).await;
    answer(
        &server,
        "/operator/configuration",
        403,
        &serde_json::json!({"error": "insufficient_scope", "message": "the operator scope is needed"}),
    )
    .await;
    let (service, cookie) = signed_in(&server).await?;
    let (response, page) = send(&service, get_with("/configuration", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains(">Configuration</h1>"), "{page}");
    assert!(page.contains("403"), "{page}");
    let (_response, list) = send(&service, get_with("/connections", &cookie)?).await?;
    let list = list.replace("<!>", "");
    assert!(list.contains("openEHR CDR"), "the list still draws: {list}");
    Ok(())
}
