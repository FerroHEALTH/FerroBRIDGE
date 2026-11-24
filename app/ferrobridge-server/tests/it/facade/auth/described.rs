// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer the SMART discovery document and the `CapabilityStatement`
//! name, chosen by `ferrohealth-smart`'s `describe`: `[auth]
//! described_issuer`, else the one SMART issuer; never a `federation` issuer
//! unnamed, and never by the order of the `[auth.issuer.<name>]` tables.
//! SMART App Launch 2.2.0, Conformance, §Metadata; which of several issuers
//! is described is our own design.

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

/// The resource-server settings of `issuer` as another party's `federation`
/// issuer, mapping the persons of `https://as.example.org`.
fn federation_of(issuer: &Issuer) -> Settings {
    let mut settings = of_kind(issuer, IssuerKind::Federation);
    settings.role_claims = Vec::new();
    settings.federation = Some(
        toml::from_str(concat!(
            "contact_point = false\n",
            "mapping = \"iua\"\n",
            "countries = [\"NL\"]\n",
            "issuing_authorities = [\"CIBG\"]\n",
            "provider_identifier_prefixes = [\"urn:oid:2.16.528.1.1007.3.3.\"]\n",
            "exchanges_from = [\"https://as.example.org\"]\n",
            "person_mapping = \"hp_identifier\"\n",
            "fields = { \"professional.country_code\" = [{ constant = \"NL\" }], ",
            "\"professional.issuing_authority_name\" = [{ constant = \"CIBG\" }] }\n",
        ))
        .expect("the federation table reads"),
    );
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
async fn a_federation_issuer_written_first_is_never_described() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let federation = Issuer::start(&[&key]).await;
    let persons = Issuer::start(&[&key]).await;
    let written = vec![
        federation_of(&federation),
        of_kind(&persons, IssuerKind::Smart),
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
async fn several_smart_issuers_without_the_key_refuse_the_start() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let persons = Issuer::start(&[&key]).await;
    let patients = Issuer::start(&[&key]).await;
    let written = vec![
        of_kind(&persons, IssuerKind::Smart),
        of_kind(&patients, IssuerKind::Smart),
    ];
    let mut reversed = written.clone();
    reversed.reverse();
    let mut sorted = vec![persons.uri(), patients.uri()];
    sorted.sort();
    for issuers in [written, reversed] {
        let DescribeError::SeveralEligible { issuers } = refusal(trusting(issuers, None)).await?
        else {
            return Err("several SMART issuers are refused as such".into());
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
        of_kind(&persons, IssuerKind::Smart),
        of_kind(&patients, IssuerKind::Smart),
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
async fn a_named_unknown_issuer_refuses_the_start() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let persons = Issuer::start(&[&key]).await;
    let elsewhere = Issuer::start(&[&key]).await;
    let refused = refusal(trusting(
        vec![of_kind(&persons, IssuerKind::Smart)],
        Some(&elsewhere),
    ))
    .await?;
    assert!(
        matches!(&refused, DescribeError::UnknownIssuer { issuer } if issuer.expose() == elsewhere.uri()),
        "{refused:?}"
    );
    Ok(())
}
