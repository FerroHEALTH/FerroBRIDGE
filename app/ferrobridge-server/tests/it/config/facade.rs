// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade]` section: off until present, its policy, its store, its
//! subject link, and its refusals.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::facade::subject::IdForm;
use ferrobridge_server::facade::subject::SubjectError;
use std::collections::BTreeMap;
use std::error::Error as StdError;

#[test]
fn the_facade_is_off_until_its_section_turns_it_on() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert!(
        settings.facade.is_none(),
        "the facade lane is off until [facade] enabled is set"
    );
    assert!(settings.mapping_directory.is_none());
    Ok(())
}

#[test]
fn an_enabled_facade_resolves_its_policy_and_its_store() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n",
        "[facade]\nenabled = true\n",
        "base_url = \"https://bridge.invalid/fhir\"\n",
        "ehr_policy = \"create_on_first_write\"\n",
        "identity_store = \"/var/lib/ferrobridge/identity.redb\"\n",
        "composition_language = \"en\"\ncomposition_territory = \"GB\"\n",
    );

    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!("https://bridge.invalid/fhir", facade.settings.base_url);
    assert_eq!(
        ferrobridge_server::facade::ehr::Policy::CreateOnFirstWrite,
        facade.settings.ehr_policy
    );
    assert_eq!("en", facade.settings.language);
    assert_eq!("GB", facade.settings.territory);
    assert_eq!(
        std::path::Path::new("/var/lib/ferrobridge/identity.redb"),
        facade.identity_store
    );
    assert_eq!(
        Some(std::path::Path::new("/srv/mappings")),
        settings.mapping_directory.as_deref()
    );
    Ok(())
}

#[test]
fn an_ehr_policy_the_facade_does_not_know_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[facade]\nenabled = true\nehr_policy = \"whatever\"\n",
        "composition_language = \"en\"\ncomposition_territory = \"GB\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
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
-> Result<(), Box<dyn StdError>> {
    let text = "[facade]\nenabled = true\ncomposition_territory = \"GB\"\n";

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("a clinical record has no default language");
    match &error {
        Error::Missing { key } => assert_eq!("facade.composition_language", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

/// The smallest enabled facade section.
const ENABLED: &str =
    "[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n";

#[test]
fn the_search_page_sizes_default_and_resolve_as_configured() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        ferrobridge_server::facade::search::Limits::default(),
        facade.search
    );
    let text = format!("{ENABLED}search_count = 5\nsearch_count_max = 50\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(5, facade.search.default_count);
    assert_eq!(50, facade.search.max_count);
    Ok(())
}

#[test]
fn a_search_page_size_no_page_can_hold_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
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
            .resolve()
            .expect_err("the page size is refused");
        match &error {
            Error::SearchCount { key, .. } => assert_eq!(refused, key, "{keys}"),
            other => return Err(format!("expected a search count error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn by_default_the_facade_fetches_no_absolute_reference() -> Result<(), Box<dyn StdError>> {
    // engine/references.adoc: "If not possible, the engine proceeds with the
    // mapping", so an empty allow-list is a deployment that fetches nothing.
    let settings = Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.references.fetch_bases().is_empty());
    assert_eq!(
        ferrobridge_server::facade::references::DEFAULT_MAX_DEPTH,
        facade.references.max_depth()
    );
    Ok(())
}

#[test]
fn the_reference_section_resolves_its_bases_and_its_depth() -> Result<(), Box<dyn StdError>> {
    let text = format!(
        "{ENABLED}\n[facade.references]\nfetch_bases = [\"https://sender.invalid/fhir\"]\nmax_depth = 1\ntimeout_ms = 2500\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
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
fn the_reference_bases_are_set_from_the_environment() -> Result<(), Box<dyn StdError>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FACADE__REFERENCES__FETCH_BASES"),
        String::from("[\"https://sender.invalid/fhir\"]"),
    )]);
    let settings = Config::from_sources(Some(ENABLED), &environment)?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(1, facade.references.fetch_bases().len());
    Ok(())
}

#[test]
fn a_fetch_base_that_is_no_http_base_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    for value in [
        "ftp://sender.invalid/fhir",
        "https://sender.invalid/fhir?x=1",
    ] {
        let text = format!("{ENABLED}\n[facade.references]\nfetch_bases = [\"{value}\"]\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
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
/// The `[facade]` section `extra` adds to, with the two required fields set.
fn facade_with(extra: &str) -> String {
    format!(
        "[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n{extra}"
    )
}

#[test]
fn the_subject_link_defaults_to_the_reference_form_and_a_person() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(&facade_with("")), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let link = &facade.settings.subject;
    assert_eq!("ferrobridge", link.namespace());
    assert_eq!(IdForm::Reference, link.form());
    assert_eq!("PERSON", link.party_type());
    Ok(())
}

#[test]
fn the_subject_link_takes_the_cdr_owners_namespace_form_and_type() -> Result<(), Box<dyn StdError>>
{
    let text = facade_with(concat!(
        "subject_namespace = \"fhir_facade\"\n",
        "subject_id_form = \"logical_id\"\n",
        "subject_type = \"PARTY\"\n",
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let link = &facade.settings.subject;
    assert_eq!("fhir_facade", link.namespace());
    assert_eq!(IdForm::LogicalId, link.form());
    assert_eq!("PARTY", link.party_type());
    let person = link.person_of_patient("p-1")?;
    assert_eq!(("fhir_facade", "p-1"), (person.namespace(), person.id()));
    Ok(())
}

#[test]
fn a_subject_type_outside_type_validity_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    // BASE base_types PARTY_REF §Invariants, Type_validity: PATIENT is no party type.
    let text = facade_with("subject_type = \"PATIENT\"\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()
        .expect_err("the type is one of the BASE party types");
    match &error {
        Error::Subject { key, source } => {
            assert_eq!("facade.subject_type", key);
            assert!(matches!(
                source.as_ref(),
                SubjectError::PartyType { value, .. } if value == "PATIENT"
            ));
        }
        other => return Err(format!("expected a subject link error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_subject_namespace_breaking_the_object_ref_rule_refuses_to_boot()
-> Result<(), Box<dyn StdError>> {
    let text = facade_with("subject_namespace = \"9-local\"\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()
        .expect_err("the namespace starts with a letter");
    match &error {
        Error::Subject { key, source } => {
            assert_eq!("facade.subject_namespace", key);
            assert!(matches!(source.as_ref(), SubjectError::Namespace { .. }));
        }
        other => return Err(format!("expected a subject link error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_subject_id_form_the_facade_does_not_know_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = facade_with("subject_id_form = \"pseudonym\"\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()
        .expect_err("the form is one of three values");
    match &error {
        Error::SubjectIdForm { key, value } => {
            assert_eq!("facade.subject_id_form", key);
            assert_eq!("pseudonym", value);
        }
        other => return Err(format!("expected a subject id form error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn the_subject_link_keys_are_set_from_the_environment() -> Result<(), Box<dyn StdError>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__FACADE__SUBJECT_ID_FORM"),
            String::from("uuid"),
        ),
        (
            String::from("FERROBRIDGE__FACADE__SUBJECT_TYPE"),
            String::from("GROUP"),
        ),
    ]);
    let settings = Config::from_sources(Some(&facade_with("")), &environment)?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(IdForm::Uuid, facade.settings.subject.form());
    assert_eq!("GROUP", facade.settings.subject.party_type());
    Ok(())
}
