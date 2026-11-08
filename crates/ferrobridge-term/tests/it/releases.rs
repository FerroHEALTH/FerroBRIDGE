// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! R5 and the R6 ballot, written and read per release.
//!
//! Every parameter name asserted here is the one the release's vendored
//! `OperationDefinition` declares, under
//! `tools/fhir-codegen/vendor/hl7.fhir.r5.core` 5.0.0 and
//! `hl7.fhir.r6.core` 6.0.0-ballot5.

use ferrobridge_term::concept::Relationship;
use ferrobridge_term::config::{Form, WireVersion};
use ferrobridge_term::error::Error;
use ferrobridge_term::invocation::{Invocation, answer};
use ferrobridge_term::outcome::{BatchOutcome, LookupOutcome, TranslateOutcome};
use ferrobridge_term::request::{Request, Translation};
use http::StatusCode;
use serde_json::Value;

/// Returns the names of the top-level parameters of a `Parameters` body.
pub(crate) fn names(body: &str) -> Vec<String> {
    let parsed: Value = serde_json::from_str(body).expect("a JSON body");
    parsed
        .get("parameter")
        .and_then(Value::as_array)
        .expect("a parameter array")
        .iter()
        .map(|parameter| {
            parameter
                .get("name")
                .and_then(Value::as_str)
                .expect("a name")
                .to_owned()
        })
        .collect()
}

/// Returns the URL an answer is read for in these tests.
pub(crate) fn url() -> url::Url {
    "http://tx.invalid/r5/ConceptMap/$translate"
        .parse()
        .expect("a valid URL")
}

fn translation() -> Request {
    Request::Translate(
        Translation::new("http://example.org/source", "a")
            .with_version("1")
            .with_concept_map("http://example.org/ConceptMap/m")
            .with_concept_map_version("2")
            .with_target_system("http://example.org/target"),
    )
}

#[test]
fn r4_and_r4b_translate_under_code_and_targetsystem() -> Result<(), Error> {
    for version in [WireVersion::R4, WireVersion::R4B] {
        let invocation = Invocation::new(version, Form::Post, &translation())?;
        assert_eq!(
            vec![
                "url",
                "conceptMapVersion",
                "code",
                "system",
                "version",
                "targetsystem"
            ],
            names(invocation.body().expect("a body"))
        );
    }
    Ok(())
}

#[test]
fn r5_translates_under_source_code_and_target_system() -> Result<(), Error> {
    let invocation = Invocation::new(WireVersion::R5, Form::Post, &translation())?;
    assert_eq!(
        vec![
            "url",
            "conceptMapVersion",
            "sourceCode",
            "system",
            "version",
            "targetSystem"
        ],
        names(invocation.body().expect("a body"))
    );
    Ok(())
}

#[test]
fn r6_translates_under_source_system_and_source_version() -> Result<(), Error> {
    let invocation = Invocation::new(WireVersion::R6, Form::Post, &translation())?;
    assert_eq!(
        vec![
            "url",
            "conceptMapVersion",
            "sourceCode",
            "sourceSystem",
            "sourceVersion",
            "targetSystem"
        ],
        names(invocation.body().expect("a body"))
    );
    Ok(())
}

#[test]
fn r5_and_r6_refuse_reverse_which_they_replaced() {
    let reverse =
        Request::Translate(Translation::new("http://example.org/source", "a").with_reverse(true));
    for version in [WireVersion::R5, WireVersion::R6] {
        let refused = Invocation::new(version, Form::Post, &reverse);
        assert!(
            matches!(
                refused,
                Err(Error::UnsupportedParameter {
                    parameter: "reverse",
                    ..
                })
            ),
            "{version:?}: {refused:?}"
        );
    }
}

#[test]
fn an_r5_and_r6_match_states_its_relationship_and_parts() -> Result<(), Error> {
    let body = r#"{"resourceType":"Parameters","parameter":[
        {"name":"result","valueBoolean":true},
        {"name":"match","part":[
          {"name":"relationship","valueCode":"source-is-narrower-than-target"},
          {"name":"concept","valueCoding":{"system":"http://example.org/target","code":"b"}},
          {"name":"product","part":[
            {"name":"attribute","valueUri":"http://example.org/attribute"},
            {"name":"value","valueCoding":{"code":"p"}}]},
          {"name":"originMap","valueCanonical":"http://example.org/ConceptMap/m|2"}]},
        {"name":"used-conceptmap","valueUri":"http://example.org/ConceptMap/m"}]}"#;
    for version in [WireVersion::R5, WireVersion::R6] {
        let outcome = answer(version, &translation(), &url(), StatusCode::OK, body)?;
        let BatchOutcome::Translate(TranslateOutcome::Translated {
            matches, used_maps, ..
        }) = outcome
        else {
            panic!("{version:?}: a translation, got {outcome:?}");
        };
        assert_eq!(
            vec!["http://example.org/ConceptMap/m".to_owned()],
            used_maps
        );
        let found = matches.first().expect("one match");
        assert_eq!(
            Some(Relationship::SourceIsNarrowerThanTarget),
            found.relationship
        );
        assert!(!found.accepted(), "a narrower match is no translation");
        assert_eq!(
            Some("http://example.org/ConceptMap/m|2"),
            found.origin_map.as_deref()
        );
        let product = found.products.first().expect("one product");
        assert_eq!(
            Some("http://example.org/attribute"),
            product.name.as_deref()
        );
        assert_eq!(
            Some("valueCoding"),
            product.value.as_ref().map(|value| value.member.as_str())
        );
    }
    Ok(())
}

#[test]
fn a_false_result_keeps_its_matches() -> Result<(), Error> {
    let body = r#"{"resourceType":"Parameters","parameter":[
        {"name":"result","valueBoolean":false},
        {"name":"match","part":[
          {"name":"relationship","valueCode":"not-related-to"},
          {"name":"noMap","valueBoolean":true}]}]}"#;
    let outcome = answer(
        WireVersion::R5,
        &translation(),
        &url(),
        StatusCode::OK,
        body,
    )?;
    let BatchOutcome::Translate(TranslateOutcome::NoMatch { matches, .. }) = &outcome else {
        panic!("no match, got {outcome:?}");
    };
    let found = matches.first().expect("the no-map match");
    assert_eq!(Some(true), found.no_map);
    assert_eq!(Some(Relationship::NotRelatedTo), found.relationship);
    Ok(())
}

#[test]
fn an_r5_lookup_reads_its_definition() -> Result<(), Error> {
    let lookup = Request::Lookup {
        system: "http://example.org/cs".to_owned(),
        code: "a".to_owned(),
        version: None,
        date: None,
        display_language: Some("nl".to_owned()),
    };
    let invocation = Invocation::new(WireVersion::R5, Form::Post, &lookup)?;
    assert_eq!(
        vec!["code", "system", "displayLanguage"],
        names(invocation.body().expect("a body"))
    );
    let body = r#"{"resourceType":"Parameters","parameter":[
        {"name":"name","valueString":"Example"},
        {"name":"display","valueString":"A"},
        {"name":"definition","valueString":"The first code."}]}"#;
    let outcome = answer(WireVersion::R5, &lookup, &url(), StatusCode::OK, body)?;
    let BatchOutcome::Lookup(LookupOutcome::Found { definition, .. }) = outcome else {
        panic!("a found code, got {outcome:?}");
    };
    assert_eq!(Some("The first code.".to_owned()), definition);
    Ok(())
}
