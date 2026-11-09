// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A whole authored `ValueSet` sent as the `valueSet` input of
//! `ValueSet/$expand`, "the value set is provided directly as part of the
//! request" (<https://hl7.org/fhir/R4/valueset-operation-expand.html>), and
//! an expansion read without the request that asked for it.

use ferrobridge_term::config::{Form, WireVersion};
use ferrobridge_term::error::Error;
use ferrobridge_term::invocation::{Invocation, expand_answer};
use ferrobridge_term::outcome::ExpandOutcome;
use ferrobridge_term::request::{AuthoredValueSet, Expansion, Request, ValueSetSource};
use http::{Method, StatusCode};
use serde_json::{Value, json};

const ALL: [WireVersion; 5] = [
    WireVersion::Stu3,
    WireVersion::R4,
    WireVersion::R4B,
    WireVersion::R5,
    WireVersion::R6,
];

/// A draft written over a stored value set: enumerated concepts with their
/// displays, an imported value set, an exclude and `compose.inactive`.
fn draft() -> Value {
    json!({
        "resourceType": "ValueSet",
        "id": "vs-draft",
        "url": "http://example.org/ValueSet/draft",
        "version": "2",
        "name": "Draft",
        "status": "draft",
        "compose": {
            "inactive": true,
            "include": [
                {
                    "system": "http://example.org/cs",
                    "concept": [
                        {"code": "a", "display": "Alpha"},
                        {"code": "b", "display": "Beta"}
                    ]
                },
                {"valueSet": ["http://example.org/ValueSet/imported"]}
            ],
            "exclude": [
                {"system": "http://example.org/cs", "concept": [{"code": "c"}]}
            ]
        }
    })
}

/// Returns the `valueSet` resource the body of `invocation` carries.
fn sent_value_set(invocation: &Invocation) -> Value {
    let body: Value = serde_json::from_str(invocation.body().expect("a body")).expect("JSON");
    body.get("parameter")
        .and_then(Value::as_array)
        .expect("parameters")
        .iter()
        .find(|parameter| parameter.get("name") == Some(&json!("valueSet")))
        .and_then(|parameter| parameter.get("resource"))
        .expect("the valueSet input")
        .clone()
}

#[test]
fn an_authored_value_set_travels_whole_in_every_release() -> Result<(), Error> {
    let mut expansion = Expansion::of(ValueSetSource::Authored(AuthoredValueSet::from_json(
        draft().to_string(),
    )));
    expansion.count = Some(10);
    let request = Request::Expand(expansion);
    for version in ALL {
        let invocation = Invocation::new(version, Form::Get, &request)?;
        assert_eq!(&Method::POST, invocation.method(), "{version:?}");
        assert_eq!(
            ["ValueSet", "$expand"],
            invocation.segments(),
            "{version:?}: an authored value set is invoked at the type level"
        );
        assert_eq!(draft(), sent_value_set(&invocation), "{version:?}");
        let body: Value = serde_json::from_str(invocation.body().expect("a body")).expect("JSON");
        let names: Vec<&str> = body["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .filter_map(|parameter| parameter["name"].as_str())
            .collect();
        assert!(!names.contains(&"url"), "{version:?}: no canonical is sent");
        assert!(names.contains(&"count"), "{version:?}");
    }
    Ok(())
}

#[test]
fn a_resource_that_is_no_value_set_is_refused_before_sending() {
    for json in [
        json!({"resourceType": "CodeSystem", "status": "draft", "content": "complete"}).to_string(),
        "{ not json".to_owned(),
    ] {
        let request = Request::Expand(Expansion::of(ValueSetSource::Authored(
            AuthoredValueSet::from_json(json),
        )));
        for version in ALL {
            let refused = Invocation::new(version, Form::Post, &request);
            assert!(
                matches!(refused, Err(Error::AuthoredValueSet { wire_version, .. }) if wire_version == version),
                "{version:?}: {refused:?}"
            );
        }
    }
}

#[test]
fn an_expansion_is_read_without_the_request() -> Result<(), Error> {
    let url = "http://tx.invalid/r4/ValueSet/$expand"
        .parse()
        .expect("a URL");
    let body = r#"{"resourceType":"ValueSet","status":"draft","expansion":{
        "identifier":"urn:uuid:4f1e0c6e-8d2a-4b5e-9f3c-1a2b3c4d5e6f","timestamp":"2026-11-09T00:00:00Z",
        "contains":[{"system":"http://example.org/cs","code":"a","display":"Alpha"}]}}"#;
    for version in ALL {
        let ExpandOutcome::Expanded(expanded) = expand_answer(version, &url, StatusCode::OK, body)?
        else {
            panic!("{version:?}: an expansion");
        };
        let codes: Vec<_> = expanded
            .contains
            .iter()
            .map(|contains| contains.code.as_deref())
            .collect();
        assert_eq!(vec![Some("a")], codes, "{version:?}");
    }
    let unknown = r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found",
        "details":{"coding":[{"system":"http://hl7.org/fhir/tools/CodeSystem/tx-issue-type","code":"not-found"}]}}]}"#;
    let outcome = expand_answer(WireVersion::R4, &url, StatusCode::NOT_FOUND, unknown)?;
    assert!(
        matches!(outcome, ExpandOutcome::NotFound { .. }),
        "{outcome:?}"
    );
    let refused = expand_answer(
        WireVersion::R4,
        &url,
        StatusCode::UNPROCESSABLE_ENTITY,
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"too-costly"}]}"#,
    );
    assert!(matches!(refused, Err(Error::Refused { .. })), "{refused:?}");
    Ok(())
}
