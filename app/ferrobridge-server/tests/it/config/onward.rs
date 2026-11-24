// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr.endpoint.credentials.onward]` table and the `[[cdr.onward]]` tables
//! beside it: the onward lane the facade reaches the CDR under its caller
//! through, one SMART and one `federation` issuer at most, and every refusal
//! of it.

use std::collections::BTreeMap;

use ferrobridge_server::cdr::onward::Route;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::kind::CallerKind;
use ferrohealth_smart::kind::IssuerKind;

/// The client credentials grant every case's lane authenticates with.
const OAUTH: &str = concat!(
    "[cdr.endpoint.credentials.oauth]\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "client_id = \"ferrobridge\"\n",
    "client_secret = \"synthetic-secret\"\n",
    "resource = \"https://cdr.example/openehr\"\n\n",
);

/// A complete onward table: a SMART issuer exchanging under RFC 8693.
const ONWARD: &str = concat!(
    "[cdr.endpoint.credentials.onward]\n",
    "issuer = \"https://as.example.org\"\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "indicators = [\"https://cdr.example/openehr\"]\n",
    "[cdr.endpoint.credentials.onward.profile]\n",
    "max_onward_lifetime = \"5min\"\n",
);

/// A `[[cdr.onward]]` table of another party's `federation` issuer, which
/// maps the persons of `https://as.example.org` to its own professionals.
const FEDERATION: &str = concat!(
    "[[cdr.onward]]\n",
    "issuer = \"https://sts.partner.example.org\"\n",
    "kind = \"federation\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "[cdr.onward.profile]\n",
    "max_onward_lifetime = \"5min\"\n",
    "[cdr.onward.federation]\n",
    "contact_point = false\n",
    "mapping = \"iua\"\n",
    "countries = [\"NL\"]\n",
    "issuing_authorities = [\"CIBG\"]\n",
    "provider_identifier_prefixes = [\"urn:oid:2.16.528.1.1007.3.3.\"]\n",
    "exchanges_from = [\"https://as.example.org\"]\n",
    "person_mapping = \"hp_identifier\"\n",
    "fields = { \"professional.country_code\" = [{ constant = \"NL\" }], ",
    "\"professional.issuing_authority_name\" = [{ constant = \"CIBG\" }] }\n",
);

/// Returns the text of a CDR lane carrying `tables` under its credentials.
fn cdr(tables: &str) -> String {
    format!("[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n{tables}")
}

/// Returns the refusal `text` resolves to as `process`.
fn refusal(text: &str, process: Process) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve(process)
            .err()
            .ok_or("the configuration should be refused")?),
    }
}

#[test]
fn an_onward_table_resolves_into_the_cdr_lane() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(&cdr(&format!("{OAUTH}{ONWARD}"))), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let onward = settings
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(vec![IssuerKind::Smart], onward.kinds());
    assert_eq!(
        ["https://cdr.example/openehr"],
        onward
            .target(IssuerKind::Smart)
            .ok_or("the SMART issuer")?
            .audiences(),
        "the target names the CDR alone"
    );
    Ok(())
}

#[test]
fn an_onward_table_caches_its_tokens_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(&cdr(&format!("{OAUTH}{ONWARD}"))), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let onward = settings
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(Some(true), onward.caches(IssuerKind::Smart));
    Ok(())
}

#[test]
fn an_onward_table_with_cache_off_holds_no_token() -> Result<(), Box<dyn std::error::Error>> {
    let table = ONWARD.replace(
        "[cdr.endpoint.credentials.onward.profile]\n",
        "cache = false\n[cdr.endpoint.credentials.onward.profile]\n",
    );
    let settings = Config::from_sources(Some(&cdr(&format!("{OAUTH}{table}"))), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let onward = settings
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(Some(false), onward.caches(IssuerKind::Smart));
    Ok(())
}

#[test]
fn a_cdr_lane_without_an_onward_table_has_no_onward_lane() -> Result<(), Box<dyn std::error::Error>>
{
    let settings =
        Config::from_sources(Some(&cdr(OAUTH)), &BTreeMap::new())?.resolve(Process::Job)?;
    assert!(settings.cdr.ok_or("the CDR lane is on")?.onward.is_none());
    Ok(())
}

#[test]
fn an_onward_table_without_its_oauth_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&cdr(ONWARD), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Credentials { section, source }
                if section == "cdr.endpoint.credentials" && matches!(source.as_ref(), ConfigError::Missing { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_declares_no_kind_but_federation() -> Result<(), Box<dyn std::error::Error>> {
    for kind in ["persons", "patients", "smart"] {
        let table = ONWARD.replace(
            "issuer = \"https://as.example.org\"\n",
            &format!("issuer = \"https://as.example.org\"\nkind = \"{kind}\"\n"),
        );
        let error = refusal(&cdr(&format!("{OAUTH}{table}")), Process::Job)?;
        assert!(matches!(error, Error::Parse { .. }), "{kind}: {error:?}");
    }
    Ok(())
}

#[test]
fn an_onward_table_on_a_server_without_auth_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[auth]\nenabled = false\n\n{}",
        cdr(&format!("{OAUTH}{ONWARD}"))
    );
    let error = refusal(&text, Process::Server)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.endpoint.credentials.onward" && needs == "auth.enabled"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_on_the_terminology_lane_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[terminology]\nendpoint.url = \"http://tx.invalid/r4\"\nwire_version = \"r4\"\n\n{}{}",
        OAUTH.replace("[cdr.", "[terminology."),
        ONWARD.replace("[cdr.", "[terminology."),
    );
    let error = refusal(&text, Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::OnwardLane { section } if section == "terminology.endpoint.credentials"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_lane_whose_own_grant_names_no_cdr_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    // RFC 8707 §2: a machine's commit goes under the bridge's own token, which
    // names the CDR by `resource`, or by `scope`.
    let own = OAUTH.replace("resource = \"https://cdr.example/openehr\"\n", "");
    let error = refusal(&cdr(&format!("{own}{ONWARD}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.endpoint.credentials.onward" && needs.contains("resource")
        ),
        "{error:?}"
    );
    let scoped = own.replace(
        "client_secret = \"synthetic-secret\"\n",
        "client_secret = \"synthetic-secret\"\nscope = \"api://cdr/.default\"\n",
    );
    let settings =
        Config::from_sources(Some(&cdr(&format!("{scoped}{ONWARD}"))), &BTreeMap::new())?
            .resolve(Process::Job)?;
    assert!(settings.cdr.ok_or("the CDR lane is on")?.onward.is_some());
    Ok(())
}

#[test]
fn a_smart_and_a_federation_issuer_resolve_side_by_side() -> Result<(), Box<dyn std::error::Error>>
{
    let text = cdr(&format!("{OAUTH}{FEDERATION}{ONWARD}"));
    let onward = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)?
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(
        vec![IssuerKind::Smart, IssuerKind::Federation],
        onward.kinds(),
        "the credentials table first, then the array in order"
    );
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::Practitioner));
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::Patient));
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::User));
    assert_eq!(Route::Own, onward.route(&CallerKind::System));
    Ok(())
}

#[test]
fn a_federation_issuer_alone_refuses_this_organisation_s_users()
-> Result<(), Box<dyn std::error::Error>> {
    let text = cdr(&format!("{OAUTH}{FEDERATION}"));
    let onward = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)?
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(vec![IssuerKind::Federation], onward.kinds());
    assert_eq!(Route::Refused, onward.route(&CallerKind::Patient));
    assert_eq!(Route::Refused, onward.route(&CallerKind::Practitioner));
    assert_eq!(Route::Own, onward.route(&CallerKind::System));
    Ok(())
}

#[test]
fn two_issuers_of_one_kind_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let twice = ONWARD
        .replace("[cdr.endpoint.credentials.onward]", "[[cdr.onward]]")
        .replace(
            "[cdr.endpoint.credentials.onward.profile]",
            "[cdr.onward.profile]",
        );
    let error = refusal(&cdr(&format!("{OAUTH}{ONWARD}{twice}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Duplicate { key, value } if key == "cdr.onward.kind" && value == "smart"
        ),
        "{error:?}"
    );
    let error = refusal(&cdr(&format!("{OAUTH}{FEDERATION}{FEDERATION}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Duplicate { key, value } if key == "cdr.onward.kind" && value == "federation"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_array_without_the_oauth_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&cdr(FEDERATION), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Credentials { section, source }
                if section == "cdr.onward" && matches!(source.as_ref(), ConfigError::Missing { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_array_on_a_server_without_auth_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[auth]\nenabled = false\n\n{}",
        cdr(&format!("{OAUTH}{FEDERATION}"))
    );
    let error = refusal(&text, Process::Server)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.endpoint.credentials.onward" && needs == "auth.enabled"
        ),
        "{error:?}"
    );
    Ok(())
}
