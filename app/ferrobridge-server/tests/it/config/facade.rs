// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade]` section: off until present, its policy, its store, the
//! identity source its subject link asks, and its refusals.

use crate::support::IDENTITY_SOURCE;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use std::collections::BTreeMap;

#[test]
fn the_facade_is_off_until_its_section_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve(Process::Server)?;
    assert!(
        settings.facade.is_none(),
        "the facade lane is off until [facade] enabled is set"
    );
    assert!(settings.mapping_directory.is_none());
    Ok(())
}

#[test]
fn an_enabled_facade_resolves_its_policy() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{IDENTITY_SOURCE}{}",
        concat!(
            "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n",
            "[auth]\nenabled = false\n\n[facade]\nenabled = true\n",
            "base_url = \"https://bridge.invalid/fhir\"\n",
            "ehr_policy = \"create_on_first_write\"\n",
            "composition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n",
        )
    );

    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!("https://bridge.invalid/fhir", facade.settings.base_url);
    assert_eq!(
        ferrobridge_server::facade::ehr::Policy::CreateOnFirstWrite,
        facade.settings.ehr_policy
    );
    assert_eq!("en", facade.settings.language);
    assert_eq!("GB", facade.settings.territory);
    assert_eq!(
        Some(std::path::Path::new("/srv/mappings")),
        settings.mapping_directory.as_deref()
    );
    Ok(())
}

#[test]
fn an_ehr_policy_the_facade_does_not_know_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let text = format!(
        "{IDENTITY_SOURCE}{}",
        concat!(
            "[auth]\nenabled = false\n\n[facade]\nenabled = true\nehr_policy = \"whatever\"\n",
            "composition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n",
        )
    );

    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("the policy is one of two values");
    match &error {
        Error::EhrPolicy { key, value } => {
            assert_eq!("facade.ehr_policy", key);
            assert_eq!("whatever", value);
        }
        other => return Err(format!("expected an ehr policy error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_enabled_facade_without_a_composition_language_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{IDENTITY_SOURCE}[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n"
    );

    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("a clinical record has no default language");
    match &error {
        Error::Missing { key } => assert_eq!("facade.composition_language", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_enabled_facade_without_each_custodian_key_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    for (dropped, key) in [
        ("custodian.name", "facade.custodian.name"),
        (
            "custodian.identifier_system",
            "facade.custodian.identifier_system",
        ),
        (
            "custodian.identifier_value",
            "facade.custodian.identifier_value",
        ),
    ] {
        let text: String = ENABLED
            .lines()
            .filter(|line| !line.starts_with(dropped))
            .flat_map(|line| [line, "\n"])
            .collect();
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("every document the bridge writes names its custodian");
        match &error {
            Error::Missing { key: missing } => assert_eq!(key, missing, "{dropped}"),
            other => return Err(format!("expected a missing key, got {other:?}").into()),
        }
    }
    Ok(())
}

/// The smallest enabled facade section.
pub(super) const ENABLED: &str = "[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\n\n[identity_source.endpoint]\nurl = \"http://127.0.0.1:9/fhir\"\n\n[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n";

#[test]
fn the_search_page_sizes_default_and_resolve_as_configured()
-> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        ferrobridge_server::facade::search::Limits::default(),
        facade.search
    );
    let text = format!("{ENABLED}search_count = 5\nsearch_count_max = 50\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(5, facade.search.default_count);
    assert_eq!(50, facade.search.max_count);
    Ok(())
}

#[test]
fn a_search_page_size_no_page_can_hold_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    for (keys, refused) in [
        ("search_count_max = 0\n", "facade.search_count_max"),
        ("search_count = 0\n", "facade.search_count"),
        (
            "search_count = 60\nsearch_count_max = 50\n",
            "facade.search_count",
        ),
    ] {
        let text = format!("{ENABLED}{keys}");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("the page size is refused");
        match &error {
            Error::SearchCount { key, .. } => assert_eq!(refused, key, "{keys}"),
            other => return Err(format!("expected a search count error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn by_default_the_facade_fetches_no_absolute_reference() -> Result<(), Box<dyn std::error::Error>> {
    // engine/references.adoc: "If not possible, the engine proceeds with the
    // mapping", so an empty allow-list is a deployment that fetches nothing.
    let settings =
        Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(facade.references.fetch_bases(), []);
    assert_eq!(
        ferrobridge_server::facade::references::DEFAULT_MAX_DEPTH,
        facade.references.max_depth()
    );
    Ok(())
}

#[test]
fn the_reference_section_resolves_its_bases_and_its_depth() -> Result<(), Box<dyn std::error::Error>>
{
    let text = format!(
        "{ENABLED}\n[facade.references]\nfetch_bases = [\"https://sender.invalid/fhir\"]\nmax_depth = 1\ntimeout = \"2500ms\"\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let bases: Vec<&str> = facade
        .references
        .fetch_bases()
        .iter()
        .map(url::Url::as_str)
        .collect();
    assert_eq!(vec!["https://sender.invalid/fhir"], bases);
    assert_eq!(1, facade.references.max_depth());
    Ok(())
}

#[test]
fn the_reference_bases_are_set_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FACADE__REFERENCES__FETCH_BASES"),
        String::from("[\"https://sender.invalid/fhir\"]"),
    )]);
    let settings = Config::from_sources(Some(ENABLED), &environment)?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(1, facade.references.fetch_bases().len());
    Ok(())
}

#[test]
fn a_fetch_base_that_is_no_http_base_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    for value in [
        "ftp://sender.invalid/fhir",
        "https://sender.invalid/fhir?x=1",
    ] {
        let text = format!("{ENABLED}\n[facade.references]\nfetch_bases = [\"{value}\"]\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("only an http or https base is fetched from");
        match &error {
            Error::FetchBase { key, value: held } => {
                assert_eq!("facade.references.fetch_bases", key);
                assert_eq!(value, held);
            }
            other => return Err(format!("expected a fetch base error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn an_unknown_reference_key_refuses_to_boot() {
    let text = format!("{ENABLED}\n[facade.references]\nfetch_everything = true\n");
    assert!(Config::from_sources(Some(&text), &BTreeMap::new()).is_err());
}

#[test]
fn the_identifier_search_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.identifier.is_none());
    Ok(())
}

#[test]
fn the_identifier_search_takes_the_observed_query_when_on() -> Result<(), Box<dyn std::error::Error>>
{
    let text = format!("{ENABLED}\n[facade.identifier_search]\nenabled = true\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let identifier = settings
        .facade
        .ok_or("the facade lane is on")?
        .identifier
        .ok_or("the identifier search is on")?;
    assert!(!identifier.resolves_systems());
    Ok(())
}

#[test]
fn the_identifier_search_is_set_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__FACADE__IDENTIFIER_SEARCH__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__FACADE__IDENTIFIER_SEARCH__SYSTEM_QUERY"),
            String::from(
                "SELECT p/uid/value FROM PERSON p WHERE p/a = $identifier AND p/b = $system",
            ),
        ),
    ]);
    let settings = Config::from_sources(Some(ENABLED), &environment)?.resolve(Process::Server)?;
    let identifier = settings
        .facade
        .ok_or("the facade lane is on")?
        .identifier
        .ok_or("the identifier search is on")?;
    assert!(identifier.resolves_systems());
    Ok(())
}

#[test]
fn an_identifier_query_binding_no_parameter_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    for (extra, refused) in [
        (
            "query = \"SELECT p/uid/value FROM PERSON p\"\n",
            "facade.identifier_search.query",
        ),
        (
            "system_query = \"SELECT p/uid/value FROM PERSON p WHERE p/a = $identifier\"\n",
            "facade.identifier_search.system_query",
        ),
    ] {
        let text = format!("{ENABLED}\n[facade.identifier_search]\nenabled = true\n{extra}");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("a query must bind what the facade hands it");
        match &error {
            Error::IdentifierQuery { key, .. } => assert_eq!(refused, key, "{extra}"),
            other => {
                return Err(format!("expected an identifier query error, got {other:?}").into());
            }
        }
    }
    Ok(())
}

#[test]
fn an_unknown_identifier_search_key_refuses_to_boot() {
    let text =
        format!("{ENABLED}\n[facade.identifier_search]\nenabled = true\nname_query = \"x\"\n");
    assert!(Config::from_sources(Some(&text), &BTreeMap::new()).is_err());
}

/// A party identifier query in the dialect's documented shape.
const PARTY_QUERY: &str =
    "SELECT p/uid/value FROM PERSON p WHERE p/details[at0001]/items[at0002]/value/id = $identifier";

#[test]
fn the_party_search_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.party_search.is_none());
    Ok(())
}

#[test]
fn the_party_search_takes_one_query_per_type() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{ENABLED}\n[facade.party_search]\nenabled = true\n[facade.party_search.types.Patient]\nquery = \"{PARTY_QUERY}\"\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let search = settings
        .facade
        .ok_or("the facade lane is on")?
        .party_search
        .ok_or("the party search is on")?;
    assert_eq!(search.types().collect::<Vec<_>>(), ["Patient"]);
    assert!(
        !search
            .of_type("Patient")
            .and_then(|queries| queries.identifier())
            .ok_or("Patient is searched by identifier")?
            .resolves_systems()
    );
    Ok(())
}

#[test]
fn a_party_search_with_no_type_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!("{ENABLED}\n[facade.party_search]\nenabled = true\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("a party search needs a type");
    assert!(
        matches!(&error, Error::Missing { key } if key == "facade.party_search.types"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_party_query_binding_no_parameter_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    for (extra, refused) in [
        (
            "query = \"SELECT p/uid/value FROM PERSON p\"\n".to_owned(),
            "facade.party_search.types.Patient.query",
        ),
        (
            format!("query = \"{PARTY_QUERY}\"\nsystem_query = \"{PARTY_QUERY}\"\n"),
            "facade.party_search.types.Patient.system_query",
        ),
    ] {
        let text = format!(
            "{ENABLED}\n[facade.party_search]\nenabled = true\n[facade.party_search.types.Patient]\n{extra}"
        );
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("a query must bind what the facade hands it");
        match &error {
            Error::IdentifierQuery { key, .. } => assert_eq!(refused, key, "{extra}"),
            other => {
                return Err(format!("expected an identifier query error, got {other:?}").into());
            }
        }
    }
    Ok(())
}

#[test]
fn a_party_type_takes_a_name_query_alone() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{ENABLED}\n[facade.party_search]\nenabled = true\n[facade.party_search.types.Practitioner]\nname_query = \"SELECT p/uid/value FROM PERSON p WHERE n LIKE $name\"\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let search = settings
        .facade
        .ok_or("the facade lane is on")?
        .party_search
        .ok_or("the party search is on")?;
    let queries = search
        .of_type("Practitioner")
        .ok_or("Practitioner is searched")?;
    assert_eq!(queries.parameters(), ["_id", "name"]);
    Ok(())
}

#[test]
fn a_name_query_binding_no_name_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{ENABLED}\n[facade.party_search]\nenabled = true\n[facade.party_search.types.Patient]\nname_query = \"SELECT p/uid/value FROM PERSON p WHERE n LIKE $name_lower\"\n"
    );
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("a name query must bind $name");
    assert!(
        matches!(&error, Error::IdentifierQuery { key, .. } if key == "facade.party_search.types.Patient.name_query"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_party_search_key_refuses_to_boot() {
    let text = format!(
        "{ENABLED}\n[facade.party_search]\nenabled = true\n[facade.party_search.types.Patient]\nquery = \"{PARTY_QUERY}\"\nbirthdate_query = \"x\"\n"
    );
    assert!(Config::from_sources(Some(&text), &BTreeMap::new()).is_err());
}
/// The `[facade]` section `extra` adds to, with the two required fields set.
fn facade_with(extra: &str) -> String {
    format!(
        "{IDENTITY_SOURCE}[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n{extra}"
    )
}

/// Returns the facade with the `[identity_source]` table `identity`.
fn facade_under(identity: &str) -> String {
    format!(
        "{identity}[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n"
    )
}

/// Returns the `[identity_source]` table with `domains` as its body.
fn identity(domains: &str) -> String {
    format!(
        "[identity_source]\n{domains}\n[identity_source.endpoint]\nurl = \"http://127.0.0.1:9/fhir\"\n\n"
    )
}

#[test]
fn an_enabled_facade_without_an_identity_source_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(Some(&facade_under("")), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("every EHR subject is a pseudonym the identity source holds");
    match &error {
        Error::Needs { key, needs } => {
            assert_eq!("facade.enabled", key);
            assert_eq!("[identity_source]", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn the_subject_link_names_the_pseudonym_domain_and_the_source_domains_in_order()
-> Result<(), Box<dyn std::error::Error>> {
    let text = facade_under(&identity(concat!(
        "pseudonym_domain = \"urn:oid:2.999.9\"\n",
        "source_domains = [\"urn:oid:2.999.2\", \"urn:oid:2.999.1\"]\n",
    )));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let link = &facade.settings.subject;
    assert_eq!("urn:oid:2.999.9", link.namespace());
    assert_eq!(
        ["urn:oid:2.999.2", "urn:oid:2.999.1"],
        link.identity().source_domains()
    );
    let identity = settings
        .identity_source
        .ok_or("the identity source is resolved")?;
    assert_eq!("urn:oid:2.999.9", identity.pseudonym_domain());
    Ok(())
}

#[test]
fn an_identity_source_without_its_domains_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    for (body, key) in [
        (
            "source_domains = [\"urn:oid:2.999.1\"]\n",
            "identity_source.pseudonym_domain",
        ),
        (
            "pseudonym_domain = \"urn:oid:2.999.9\"\n",
            "identity_source.source_domains",
        ),
    ] {
        let error = Config::from_sources(Some(&facade_under(&identity(body))), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("both domains are required");
        match &error {
            Error::Missing { key: missing } => assert_eq!(key, missing),
            other => return Err(format!("expected a missing key, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn an_identity_domain_that_is_no_absolute_uri_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let text = facade_under(&identity(concat!(
        "pseudonym_domain = \"urn:oid:2.999.9\"\n",
        "source_domains = [\"local-mrn\"]\n",
    )));
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("an identifier system is an absolute URI");
    match &error {
        Error::IdentityDomain { key, value } => {
            assert_eq!("identity_source.source_domains", key);
            assert_eq!("local-mrn", value);
        }
        other => return Err(format!("expected an identity domain error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_source_domain_named_twice_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = facade_under(&identity(concat!(
        "pseudonym_domain = \"urn:oid:2.999.9\"\n",
        "source_domains = [\"urn:oid:2.999.1\", \"urn:oid:2.999.1\"]\n",
    )));
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("the order of the domains is the choice, so each is named once");
    match &error {
        Error::Duplicate { key, value } => {
            assert_eq!("identity_source.source_domains", key);
            assert_eq!("urn:oid:2.999.1", value);
        }
        other => return Err(format!("expected a duplicate error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn the_removed_subject_link_keys_refuse_to_boot() {
    for key in ["subject_namespace", "subject_id_form", "subject_type"] {
        let text = facade_with(&format!("{key} = \"x\"\n"));
        assert!(
            Config::from_sources(Some(&text), &BTreeMap::new()).is_err(),
            "facade.{key} is no key"
        );
    }
}

#[test]
fn the_pseudonym_domain_is_set_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__IDENTITY_SOURCE__PSEUDONYM_DOMAIN"),
        String::from("urn:oid:2.999.7"),
    )]);
    let settings =
        Config::from_sources(Some(&facade_with("")), &environment)?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!("urn:oid:2.999.7", facade.settings.subject.namespace());
    Ok(())
}

#[test]
fn the_ehr_status_identifier_system_is_unset_until_a_deployment_names_one()
-> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&facade_with("")), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.settings.ehr_status_system);
    let text = facade_with("ehr_status_identifier_system = \"urn:oid:2.999.1\"\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        Some("urn:oid:2.999.1"),
        facade.settings.ehr_status_system.as_deref()
    );
    Ok(())
}

#[test]
fn the_binding_identifier_issuers_are_empty_until_a_deployment_names_them()
-> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&facade_with("")), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(Vec::<String>::new(), facade.binding_issuers);
    let text = facade_with("binding_identifier_issuers = [\"https://login.example/persons\"]\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        ["https://login.example/persons"],
        facade.binding_issuers.as_slice()
    );
    Ok(())
}
