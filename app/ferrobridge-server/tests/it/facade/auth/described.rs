// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer the SMART discovery document and the `CapabilityStatement`
//! name, chosen by `ferrohealth-smart`'s `describe`: `[auth]
//! described_issuer`, else the one issuer a user launch starts at, else, with
//! only `machines` issuers trusted, the one `machines` issuer for SMART
//! Backend Services; never by the order of the `[[auth.issuer]]` tables.
//! SMART App Launch 2.2.0, Conformance, §Metadata, and Backend Services,
//! §Retrieve `.well-known/smart-configuration`; which of several issuers is
//! described is our own design.

use std::error::Error;

use ferrobridge_server::smart::Smart;
use ferrobridge_server::smart::SmartError;
use ferrobridge_server::smart::SmartSettings;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::resource::Settings;
use ferrohealth_smart::resource::described::DescribeError;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::discovery::read_open;
use super::ec_key;
use super::secured;
use crate::facade::harness;

/// The resource-server settings of `issuer` as an issuer of `kind`.
fn of_kind(issuer: &Issuer, kind: IssuerKind) -> Settings {
    let mut settings = issuer.issuer_settings();
    settings.kind = kind;
    settings
}

/// The settings of a resource server trusting `issuers` in this order,
/// describing `named` when set.
fn trusting(issuers: Vec<Settings>, named: Option<&Issuer>) -> SmartSettings {
    let mut issuers = issuers.into_iter();
    let first = issuers.next().expect("at least one issuer");
    let mut settings = SmartSettings::new(first);
    settings.issuers.extend(issuers);
    settings.described_issuer = named.map(|issuer| SecretUrl::new(issuer.uri()));
    settings
}

/// Returns the discovery document and the `oauth-uris` members of the
/// `CapabilityStatement` the resource server `settings` describe serves.
async fn documents(
    settings: SmartSettings,
) -> Result<(serde_json::Value, Vec<(String, String)>), Box<dyn Error>> {
    let harness = harness().await;
    let smart = authority(settings).await;
    let (_, _, document) = read_open(
        secured(&harness, &smart),
        "/fhir/.well-known/smart-configuration",
    )
    .await?;
    let (_, _, statement) = read_open(secured(&harness, &smart), "/fhir/metadata").await?;
    let members = statement["rest"][0]["security"]["extension"][0]["extension"]
        .as_array()
        .ok_or("the oauth-uris members")?
        .iter()
        .filter_map(|member| {
            Some((
                member["url"].as_str()?.to_owned(),
                member["valueUri"].as_str()?.to_owned(),
            ))
        })
        .collect();
    Ok((document, members))
}

/// Returns why `settings` refuse the start.
async fn refusal(settings: SmartSettings) -> Result<DescribeError, Box<dyn Error>> {
    match Smart::discover(settings).await {
        Err(SmartError::Described { source }) => Ok(source),
        Err(other) => Err(format!("refused for another reason: {other:?}").into()),
        Ok(_) => Err("the start is refused".into()),
    }
}

#[tokio::test]
async fn a_machines_issuer_written_first_is_never_described() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let machines = Issuer::start(&[&key]).await;
    let persons = Issuer::start(&[&key]).await;
    let written = vec![
        of_kind(&machines, IssuerKind::Machines),
        of_kind(&persons, IssuerKind::Persons),
    ];
    let mut reversed = written.clone();
    reversed.reverse();
    for issuers in [written, reversed] {
        let (document, members) = documents(trusting(issuers, None)).await?;
        let uri = persons.uri();
        assert_eq!(
            Some(format!("{uri}/authorize").as_str()),
            document["authorization_endpoint"].as_str(),
            "{document}"
        );
        assert_eq!(
            Some(format!("{uri}/token").as_str()),
            document["token_endpoint"].as_str()
        );
        assert!(
            members.contains(&(String::from("authorize"), format!("{uri}/authorize"))),
            "{members:?}"
        );
        assert!(
            members.contains(&(String::from("token"), format!("{uri}/token"))),
            "{members:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn several_user_issuers_without_the_key_refuse_the_start() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let persons = Issuer::start(&[&key]).await;
    let patients = Issuer::start(&[&key]).await;
    let written = vec![
        of_kind(&persons, IssuerKind::Persons),
        of_kind(&patients, IssuerKind::Patients),
    ];
    let mut reversed = written.clone();
    reversed.reverse();
    let mut sorted = vec![persons.uri(), patients.uri()];
    sorted.sort();
    for issuers in [written, reversed] {
        let DescribeError::SeveralEligible { issuers } = refusal(trusting(issuers, None)).await?
        else {
            return Err("several user issuers are refused as such".into());
        };
        let named: Vec<&str> = issuers.iter().map(SecretUrl::expose).collect();
        assert_eq!(named, sorted, "the table order never shows");
    }
    Ok(())
}

#[tokio::test]
async fn the_named_issuer_is_described_among_several() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let persons = Issuer::start(&[&key]).await;
    let patients = Issuer::start(&[&key]).await;
    let issuers = vec![
        of_kind(&persons, IssuerKind::Persons),
        of_kind(&patients, IssuerKind::Patients),
    ];
    for named in [&persons, &patients] {
        let (document, members) = documents(trusting(issuers.clone(), Some(named))).await?;
        let uri = named.uri();
        assert_eq!(
            Some(format!("{uri}/token").as_str()),
            document["token_endpoint"].as_str(),
            "{document}"
        );
        assert!(
            members.contains(&(String::from("authorize"), format!("{uri}/authorize"))),
            "{members:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_machines_only_bridge_is_described_for_backend_services() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let machines = Issuer::start(&[&key]).await;
    let mut settings = trusting(vec![of_kind(&machines, IssuerKind::Machines)], None);
    settings.options.scopes_supported = vec![
        String::from("system/Condition.rs"),
        String::from("user/Condition.rs"),
    ];
    settings.options.capabilities = vec![String::from("launch-ehr")];
    let (document, members) = documents(settings).await?;
    let uri = machines.uri();
    // NOTE: SMART App Launch 2.2.0, Backend Services, Retrieve
    // `.well-known/smart-configuration`: a backend client reads the token endpoint here.
    assert_eq!(
        Some(format!("{uri}/token").as_str()),
        document["token_endpoint"].as_str(),
        "{document}"
    );
    assert_eq!(
        serde_json::json!(["client_credentials"]),
        document["grant_types_supported"]
    );
    assert_eq!(
        serde_json::json!(["private_key_jwt"]),
        document["token_endpoint_auth_methods_supported"]
    );
    assert_eq!(
        serde_json::json!(["client-confidential-asymmetric", "permission-v1"]),
        document["capabilities"]
    );
    assert_eq!(
        serde_json::json!(["system/Condition.rs"]),
        document["scopes_supported"]
    );
    assert!(
        document.get("authorization_endpoint").is_none(),
        "{document}"
    );
    assert!(
        members.contains(&(String::from("token"), format!("{uri}/token"))),
        "{members:?}"
    );
    assert!(
        !members.iter().any(|(name, _)| name == "authorize"),
        "no user launch starts at a machines issuer: {members:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_named_machines_issuer_beside_a_user_issuer_refuses_the_start()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let machines = Issuer::start(&[&key]).await;
    let persons = Issuer::start(&[&key]).await;
    let issuers = vec![
        of_kind(&persons, IssuerKind::Persons),
        of_kind(&machines, IssuerKind::Machines),
    ];
    let DescribeError::MachinesIssuer { issuer } =
        refusal(trusting(issuers, Some(&machines))).await?
    else {
        return Err("a named machines issuer beside a user issuer is refused".into());
    };
    assert_eq!(issuer.expose(), machines.uri());
    Ok(())
}

#[tokio::test]
async fn a_named_unknown_issuer_refuses_the_start() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let persons = Issuer::start(&[&key]).await;
    let elsewhere = Issuer::start(&[&key]).await;
    let refused = refusal(trusting(
        vec![of_kind(&persons, IssuerKind::Persons)],
        Some(&elsewhere),
    ))
    .await?;
    assert!(
        matches!(&refused, DescribeError::UnknownIssuer { issuer } if issuer.expose() == elsewhere.uri()),
        "{refused:?}"
    );
    Ok(())
}
