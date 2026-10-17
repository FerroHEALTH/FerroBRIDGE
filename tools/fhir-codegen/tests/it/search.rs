// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The emitted search parameter registry against the vendored
//! `SearchParameter` resources it comes from, and the lookup by resource type
//! and code (<https://hl7.org/fhir/R4/searchparameter.html>).
//!
//! Every expectation is read from the package JSON with `serde_json`, so the
//! registry is compared with the definitions rather than with the lowering.

use std::fs;

use fhir_codegen::package::without_byte_order_mark;
use fhir_codegen::search::TOLERATED_AMBIGUITIES;
use fhir_types::search::SearchParameters;
use serde_json::Value;

use crate::packages;

/// The emitted registry of each version, in the order [`packages`] lists them.
fn registries() -> [(&'static str, &'static SearchParameters); 5] {
    [
        (
            "stu3",
            &fhir_types::stu3::search_parameters::SEARCH_PARAMETERS,
        ),
        ("r4", &fhir_types::r4::search_parameters::SEARCH_PARAMETERS),
        (
            "r4b",
            &fhir_types::r4b::search_parameters::SEARCH_PARAMETERS,
        ),
        ("r5", &fhir_types::r5::search_parameters::SEARCH_PARAMETERS),
        ("r6", &fhir_types::r6::search_parameters::SEARCH_PARAMETERS),
    ]
}

/// Every `SearchParameter` file of the package at `dir`, parsed.
fn search_parameters(dir: &std::path::Path) -> Vec<Value> {
    let content = dir.join("package");
    let mut files: Vec<_> = fs::read_dir(&content)
        .expect("the package directory should read")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("SearchParameter-"))
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path).expect("the file should read");
            serde_json::from_str(without_byte_order_mark(&text)).expect("the file is JSON")
        })
        .collect()
}

/// Whether `resource` is one of the specification's example parameters.
fn is_example(resource: &Value) -> bool {
    resource["id"]
        .as_str()
        .is_some_and(|id| id == "example" || id.starts_with("example-"))
}

#[test]
fn every_definition_the_package_holds_is_in_the_registry_as_written() {
    for ((module, dir, _), (registry_module, registry)) in packages().iter().zip(registries()) {
        assert_eq!(*module, registry_module);
        let definitions: Vec<Value> = search_parameters(dir)
            .into_iter()
            .filter(|resource| !is_example(resource))
            .collect();
        assert_eq!(
            definitions.len(),
            registry.parameters.len(),
            "{module}: one registry entry per definition"
        );
        for definition in &definitions {
            let url = definition["url"].as_str().expect("a url");
            let entry = registry
                .by_url(url)
                .unwrap_or_else(|| panic!("{module}: {url} is in the registry"));
            assert_eq!(definition["code"].as_str(), Some(entry.code), "{url}");
            assert_eq!(definition["name"].as_str(), Some(entry.name), "{url}");
            assert_eq!(
                definition["type"].as_str(),
                Some(entry.r#type.code()),
                "{url}"
            );
            assert_eq!(definition["expression"].as_str(), entry.expression, "{url}");
            let base: Vec<&str> = definition["base"]
                .as_array()
                .map(|base| base.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            assert_eq!(base, entry.base, "{url}");
            assert_eq!(
                definition["experimental"].as_bool().unwrap_or(false),
                entry.experimental,
                "{url}"
            );
        }
    }
}

#[test]
fn the_examples_are_left_out() {
    for (module, registry) in registries() {
        assert!(
            registry
                .by_url("http://hl7.org/fhir/SearchParameter/example")
                .is_none(),
            "{module}"
        );
    }
}

#[test]
fn a_type_answers_its_own_parameters_and_those_of_its_bases() {
    let r4 = &fhir_types::r4::search_parameters::SEARCH_PARAMETERS;
    let code = r4.of("Condition", "code").expect("Condition answers code");
    assert_eq!(
        "http://hl7.org/fhir/SearchParameter/clinical-code",
        code.url
    );
    assert_eq!("token", code.r#type.code());
    let id = r4.of("Condition", "_id").expect("Condition answers _id");
    assert_eq!("http://hl7.org/fhir/SearchParameter/Resource-id", id.url);
    let text = r4
        .of("Condition", "_text")
        .expect("a DomainResource answers _text");
    assert_eq!(
        "http://hl7.org/fhir/SearchParameter/DomainResource-text",
        text.url
    );
    assert!(
        r4.of("Bundle", "_text").is_none(),
        "Bundle specializes Resource, never DomainResource"
    );
    assert!(r4.of("Condition", "diagnosis-name").is_none());
    assert!(
        r4.of_type("Condition")
            .any(|parameter| parameter.code == "onset-date"),
        "the parameters of one type list its own"
    );
}

#[test]
fn a_type_definition_stands_over_its_base_definition_of_one_code() {
    // R5 defines `_text` on both `Resource` and `DomainResource`.
    let r5 = &fhir_types::r5::search_parameters::SEARCH_PARAMETERS;
    let text = r5.of("Account", "_text").expect("Account answers _text");
    assert_eq!(
        "http://hl7.org/fhir/SearchParameter/DomainResource-text",
        text.url
    );
    let bundle = r5
        .of("Bundle", "_text")
        .expect("Bundle answers Resource _text");
    assert_eq!(
        "http://hl7.org/fhir/SearchParameter/Resource-text",
        bundle.url
    );
}

#[test]
fn each_tolerated_ambiguity_is_still_in_its_package_and_names_neither() {
    for (package, base, code) in TOLERATED_AMBIGUITIES {
        let (module, dir, _) = packages()
            .into_iter()
            .find(|(_, _, loaded)| loaded.manifest().name == package)
            .unwrap_or_else(|| panic!("{package} is vendored"));
        let twice = search_parameters(&dir)
            .iter()
            .filter(|resource| !is_example(resource))
            .filter(|resource| resource["code"].as_str() == Some(code))
            .filter(|resource| {
                resource["base"]
                    .as_array()
                    .is_some_and(|bases| bases.iter().any(|b| b.as_str() == Some(base)))
            })
            .count();
        assert_eq!(2, twice, "{package} still defines {code} twice on {base}");
        let (_, registry) = registries()
            .into_iter()
            .find(|(name, _)| *name == module)
            .expect("a registry per version");
        assert!(registry.of("Account", code).is_none(), "{package}: {code}");
    }
}
