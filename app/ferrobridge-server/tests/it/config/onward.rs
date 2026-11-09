// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr.credentials.onward]` table and the `[[cdr.onward]]` tables
//! beside it: the onward lane the facade reaches the CDR under its caller
//! through, one issuer per kind, and every refusal of it.

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
    "[cdr.credentials.oauth]\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "client_id = \"ferrobridge\"\n",
    "client_secret = \"synthetic-secret\"\n",
    "resource = \"https://cdr.example/openehr\"\n\n",
);

/// A complete onward table: a `persons` issuer exchanging under RFC 8693.
const ONWARD: &str = concat!(
    "[cdr.credentials.onward]\n",
    "issuer = \"https://as.example.org\"\n",
    "kind = \"persons\"\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "indicators = [\"https://cdr.example/openehr\"]\n",
    "[cdr.credentials.onward.keys]\n",
    "max_onward_lifetime = 300\n",
);

/// A `[[cdr.onward]]` table of the deployment's `patients` issuer, a realm
/// of its own.
const PATIENTS: &str = concat!(
    "[[cdr.onward]]\n",
    "issuer = \"https://as.example.org/patients\"\n",
    "kind = \"patients\"\n",
    "token_endpoint = \"https://as.example.org/patients/token\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "indicators = [\"https://cdr.example/openehr\"]\n",
    "[cdr.onward.keys]\n",
    "max_onward_lifetime = 300\n",
);

/// A `[[cdr.onward]]` table of another party's `federation` issuer, which
/// maps the persons of `https://as.example.org` to its own professionals.
const FEDERATION: &str = concat!(
    "[[cdr.onward]]\n",
    "issuer = \"https://sts.partner.example.org\"\n",
    "kind = \"federation\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "[cdr.onward.keys]\n",
    "max_onward_lifetime = 300\n",
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
    format!("[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n{tables}")
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
    assert_eq!(vec![IssuerKind::Persons], onward.kinds());
    assert_eq!(
        ["https://cdr.example/openehr"],
        onward
            .target(IssuerKind::Persons)
            .ok_or("the persons issuer")?
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
    assert_eq!(Some(true), onward.caches(IssuerKind::Persons));
    Ok(())
}

#[test]
fn an_onward_table_with_cache_off_holds_no_token() -> Result<(), Box<dyn std::error::Error>> {
    let table = ONWARD.replace(
        "[cdr.credentials.onward.keys]\n",
        "cache = false\n[cdr.credentials.onward.keys]\n",
    );
    let settings = Config::from_sources(Some(&cdr(&format!("{OAUTH}{table}"))), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let onward = settings
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(Some(false), onward.caches(IssuerKind::Persons));
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
                if section == "cdr.credentials" && matches!(source.as_ref(), ConfigError::Missing { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_without_a_kind_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let table = ONWARD.replace("kind = \"persons\"\n", "");
    let error = refusal(&cdr(&format!("{OAUTH}{table}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Credentials { source, .. }
                if matches!(source.as_ref(), ConfigError::Missing { key } if *key == "credentials.onward.kind")
        ),
        "{error:?}"
    );
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
                if key == "cdr.credentials.onward" && needs == "auth.enabled"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_on_the_terminology_lane_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[terminology]\nbase_url = \"http://tx.invalid/r4\"\nwire_version = \"r4\"\n\n{}{}",
        OAUTH.replace("[cdr.", "[terminology."),
        ONWARD.replace("[cdr.", "[terminology."),
    );
    let error = refusal(&text, Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::OnwardLane { section } if section == "terminology.credentials"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_lane_whose_own_grant_names_no_cdr_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    // RFC 8707 §2: a machine's commit goes under the bridge's own token, which
    // names the CDR by `resource`, or by `scope` under the Entra profile.
    let own = OAUTH.replace("resource = \"https://cdr.example/openehr\"\n", "");
    let error = refusal(&cdr(&format!("{own}{ONWARD}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.credentials.onward" && needs.contains("resource")
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
fn a_persons_a_federation_and_a_patients_issuer_resolve_side_by_side()
-> Result<(), Box<dyn std::error::Error>> {
    let text = cdr(&format!("{OAUTH}{FEDERATION}{PATIENTS}{ONWARD}"));
    let onward = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)?
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(
        vec![
            IssuerKind::Persons,
            IssuerKind::Federation,
            IssuerKind::Patients
        ],
        onward.kinds(),
        "the credentials table first, then the array in order"
    );
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::Person));
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::Patient));
    assert_eq!(Route::Own, onward.route(&CallerKind::System));
    assert_eq!(
        Route::Refused,
        onward.route(&CallerKind::Guest(Box::new(CallerKind::Person)))
    );
    Ok(())
}

#[test]
fn a_patients_issuer_alone_carries_patients_and_refuses_persons()
-> Result<(), Box<dyn std::error::Error>> {
    let text = cdr(&format!("{OAUTH}{PATIENTS}"));
    let onward = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)?
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(vec![IssuerKind::Patients], onward.kinds());
    assert_eq!(Route::Exchanged, onward.route(&CallerKind::Patient));
    assert_eq!(Route::Refused, onward.route(&CallerKind::Person));
    Ok(())
}

#[test]
fn two_issuers_of_one_kind_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let twice = ONWARD
        .replace("[cdr.credentials.onward]", "[[cdr.onward]]")
        .replace("[cdr.credentials.onward.keys]", "[cdr.onward.keys]");
    let error = refusal(&cdr(&format!("{OAUTH}{ONWARD}{twice}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Duplicate { key, value } if key == "cdr.onward.kind" && value == "persons"
        ),
        "{error:?}"
    );
    let error = refusal(&cdr(&format!("{OAUTH}{PATIENTS}{PATIENTS}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Duplicate { key, value } if key == "cdr.onward.kind" && value == "patients"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_array_without_the_oauth_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&cdr(PATIENTS), Process::Job)?;
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
        cdr(&format!("{OAUTH}{PATIENTS}"))
    );
    let error = refusal(&text, Process::Server)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.credentials.onward" && needs == "auth.enabled"
        ),
        "{error:?}"
    );
    Ok(())
}
