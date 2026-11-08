// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `GET` form, the instance level, `CodeSystem/$validate-code`, the
//! inline `$expand`, and the answer elements beside the verdict.
//!
//! The `GET` form carries each primitive input as a query parameter of its
//! own name (<https://hl7.org/fhir/R4/operations.html#request>); the
//! instance level names the resource in the address
//! (<https://hl7.org/fhir/R4/operations.html#invoking>).

use crate::releases::{names, url};
use crate::support;
use ferrobridge_term::config::{Form, WireVersion};
use ferrobridge_term::error::Error;
use ferrobridge_term::invocation::{Invocation, answer};
use ferrobridge_term::outcome::{BatchOutcome, ExpandOutcome, ValidateOutcome};
use ferrobridge_term::request::{
    CodeSystemValidation, Coded, Expansion, Filter, Include, InlineValueSet, Request, Subsumption,
    Target, Validation, ValueSetSource,
};
use http::{Method, StatusCode};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ALL: [WireVersion; 5] = [
    WireVersion::Stu3,
    WireVersion::R4,
    WireVersion::R4B,
    WireVersion::R5,
    WireVersion::R6,
];

fn validation() -> Validation {
    let mut validation = Validation::new(
        "http://example.org/ValueSet/vs",
        Coded::Code {
            system: "http://example.org/cs".to_owned(),
            code: "a".to_owned(),
            version: None,
            display: None,
        },
    );
    validation.active_only = Some(true);
    validation
}

#[test]
fn the_get_form_carries_every_primitive_input_in_its_query() -> Result<(), Error> {
    let request = Request::ValidateCode(validation());
    let invocation = Invocation::new(WireVersion::R4B, Form::Get, &request)?;
    assert_eq!(&Method::GET, invocation.method());
    assert_eq!(None, invocation.body());
    let query: Vec<(&str, &str)> = invocation
        .query()
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    assert_eq!(
        vec![
            ("url", "http://example.org/ValueSet/vs"),
            ("code", "a"),
            ("system", "http://example.org/cs"),
            ("activeOnly", "true"),
        ],
        query
    );
    let address = invocation.url(&"http://tx.invalid/r4b".parse().expect("a URL"))?;
    assert_eq!(
        "http://tx.invalid/r4b/ValueSet/$validate-code?url=http%3A%2F%2Fexample.org%2FValueSet%2Fvs&code=a&system=http%3A%2F%2Fexample.org%2Fcs&activeOnly=true",
        address.as_str()
    );
    Ok(())
}

#[test]
fn a_complex_input_is_sent_as_a_post_in_the_get_form() -> Result<(), Error> {
    let mut validation = validation();
    validation.coded = Coded::Coding(ferrobridge_term::concept::Concept {
        system: Some("http://example.org/cs".to_owned()),
        code: Some("a".to_owned()),
        ..Default::default()
    });
    let invocation = Invocation::new(
        WireVersion::R4,
        Form::Get,
        &Request::ValidateCode(validation),
    )?;
    assert_eq!(&Method::POST, invocation.method());
    assert_eq!(invocation.query(), []);
    assert!(names(invocation.body().expect("a body")).contains(&"coding".to_owned()));
    Ok(())
}

#[test]
fn an_instance_level_validation_names_the_value_set_in_its_address_and_sends_no_url()
-> Result<(), Error> {
    let mut validation = validation();
    validation.value_set = Target::Instance("vs 1".to_owned());
    for version in [WireVersion::R4, WireVersion::R5, WireVersion::R6] {
        let invocation = Invocation::new(
            version,
            Form::Post,
            &Request::ValidateCode(validation.clone()),
        )?;
        assert_eq!(
            ["ValueSet", "vs 1", "$validate-code"],
            invocation.segments()
        );
        assert!(!names(invocation.body().expect("a body")).contains(&"url".to_owned()));
        let address = invocation.url(&"http://tx.invalid/r4".parse().expect("a URL"))?;
        assert_eq!("/r4/ValueSet/vs%201/$validate-code", address.path());
    }
    Ok(())
}

#[test]
fn an_instance_level_subsumption_sends_no_system() -> Result<(), Error> {
    let request = Request::Subsumes(
        Subsumption::on(Target::Instance("cs-1".to_owned()), "a", "b").with_version("1"),
    );
    for version in ALL {
        let invocation = Invocation::new(version, Form::Get, &request)?;
        assert_eq!(["CodeSystem", "cs-1", "$subsumes"], invocation.segments());
        let names: Vec<&str> = invocation
            .query()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(vec!["codeA", "codeB", "version"], names, "{version:?}");
    }
    Ok(())
}

#[test]
fn code_system_validate_code_is_written_in_every_release_that_defines_it() -> Result<(), Error> {
    let mut validation =
        CodeSystemValidation::new(Target::Canonical("http://example.org/cs".to_owned()), "a");
    validation.version = Some("1".to_owned());
    validation.display = Some("A".to_owned());
    validation.display_language = Some("nl".to_owned());
    let request = Request::CodeSystemValidateCode(validation);
    for version in [
        WireVersion::R4,
        WireVersion::R4B,
        WireVersion::R5,
        WireVersion::R6,
    ] {
        let invocation = Invocation::new(version, Form::Post, &request)?;
        assert_eq!(["CodeSystem", "$validate-code"], invocation.segments());
        assert_eq!(
            vec!["url", "code", "version", "display", "displayLanguage"],
            names(invocation.body().expect("a body")),
            "{version:?}"
        );
    }
    let refused = Invocation::new(WireVersion::Stu3, Form::Post, &request);
    assert!(
        matches!(refused, Err(Error::UnsupportedOperation { .. })),
        "STU3 defines no CodeSystem/$validate-code: {refused:?}"
    );
    Ok(())
}

#[test]
fn stu3_refuses_active_only_on_validate_code() {
    let refused = Invocation::new(
        WireVersion::Stu3,
        Form::Post,
        &Request::ValidateCode(validation()),
    );
    assert!(
        matches!(
            refused,
            Err(Error::UnsupportedParameter {
                parameter: "activeOnly",
                ..
            })
        ),
        "{refused:?}"
    );
}

#[test]
fn r6_writes_its_repeating_display_language() -> Result<(), Error> {
    let mut validation = validation();
    validation.display_language = Some("nl".to_owned());
    let invocation = Invocation::new(
        WireVersion::R6,
        Form::Post,
        &Request::ValidateCode(validation),
    )?;
    assert!(names(invocation.body().expect("a body")).contains(&"displayLanguage".to_owned()));
    Ok(())
}

#[test]
fn an_inline_value_set_travels_as_the_value_set_resource_in_every_release() -> Result<(), Error> {
    let mut expansion = Expansion::of(ValueSetSource::Inline(InlineValueSet {
        include: vec![Include {
            system: "http://example.org/cs".to_owned(),
            version: Some("1".to_owned()),
            filters: vec![Filter {
                property: "parent".to_owned(),
                op: "child-of".to_owned(),
                value: "a".to_owned(),
            }],
        }],
    }));
    expansion.exclude_nested = Some(true);
    expansion.count = Some(20);
    let request = Request::Expand(expansion);
    for version in ALL {
        let invocation = Invocation::new(version, Form::Get, &request)?;
        assert_eq!(&Method::POST, invocation.method(), "{version:?}");
        let body: serde_json::Value =
            serde_json::from_str(invocation.body().expect("a body")).expect("JSON");
        let value_set = body["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .find(|parameter| parameter["name"] == "valueSet")
            .expect("the valueSet input");
        let filter = &value_set["resource"]["compose"]["include"][0]["filter"][0];
        assert_eq!("child-of", filter["op"], "{version:?}");
        assert_eq!("a", filter["value"], "{version:?}");
        assert_eq!("active", value_set["resource"]["status"], "{version:?}");
    }
    Ok(())
}

#[test]
fn an_expansion_reads_its_parameters_and_its_unclosed_mark() -> Result<(), Error> {
    let body = r#"{"resourceType":"ValueSet","status":"active","expansion":{
        "extension":[
          {"url":"http://hl7.org/fhir/StructureDefinition/valueset-unclosed","valueBoolean":true},
          {"url":"http://hl7.org/fhir/StructureDefinition/valueset-unclosed-reason","valueString":"too many codes"}],
        "timestamp":"2026-11-08T00:00:00Z",
        "parameter":[{"name":"count","valueInteger":20}],
        "contains":[{"system":"http://example.org/cs","code":"a"}]}}"#;
    let request = Request::Expand(Expansion::new("http://example.org/ValueSet/vs"));
    for version in [WireVersion::R4, WireVersion::R5, WireVersion::R6] {
        let outcome = answer(version, &request, &url(), StatusCode::OK, body)?;
        let BatchOutcome::Expand(ExpandOutcome::Expanded(expanded)) = outcome else {
            panic!("{version:?}: an expansion, got {outcome:?}");
        };
        let parameter = expanded.parameters.first().expect("one parameter");
        assert_eq!("count", parameter.name);
        assert_eq!(
            Some("valueInteger"),
            parameter.value.as_ref().map(|value| value.member.as_str())
        );
        assert_eq!(
            vec!["too many codes".to_owned()],
            expanded.unclosed.expect("the unclosed mark").reasons
        );
    }
    Ok(())
}

#[test]
fn a_validation_reads_the_elements_beside_its_verdict() -> Result<(), Error> {
    let body = r#"{"resourceType":"Parameters","parameter":[
        {"name":"result","valueBoolean":false},
        {"name":"code","valueCode":"A"},
        {"name":"system","valueUri":"http://example.org/cs"},
        {"name":"issues","resource":{"resourceType":"OperationOutcome","issue":[{
          "severity":"error","code":"code-invalid",
          "details":{"coding":[{"system":"http://hl7.org/fhir/tools/CodeSystem/tx-issue-type","code":"invalid-code"}],"text":"Unknown code"},
          "expression":["code"]}]}},
        {"name":"normalized-code","valueCode":"a"},
        {"name":"x-unknown-system","valueCanonical":"http://example.org/other"},
        {"name":"inactive","valueBoolean":true}]}"#;
    let request = Request::ValidateCode(validation());
    for version in [
        WireVersion::R4,
        WireVersion::R4B,
        WireVersion::R5,
        WireVersion::R6,
    ] {
        let outcome = answer(version, &request, &url(), StatusCode::OK, body)?;
        let BatchOutcome::ValidateCode(ValidateOutcome::Invalid {
            outcome: Some(issues),
            details,
            ..
        }) = outcome
        else {
            panic!("{version:?}: an invalid code with issues, got {outcome:?}");
        };
        assert_eq!(Some("A"), details.code.as_deref());
        assert_eq!(Some("a"), details.normalized_code.as_deref());
        assert_eq!(Some(true), details.inactive);
        assert_eq!(
            vec!["http://example.org/other".to_owned()],
            details.unknown_systems
        );
        let issue = issues.issues.first().expect("one issue");
        assert_eq!(vec!["code".to_owned()], issue.expression);
        assert_eq!(
            Some("invalid-code"),
            issue
                .codings
                .first()
                .and_then(|coding| coding.code.as_deref())
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_client_in_the_get_form_sends_a_get() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r5/ValueSet/$validate-code"))
        .and(query_param("activeOnly", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":true}]}"#,
            "application/fhir+json",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let config = support::config(&server, WireVersion::R5)?.with_form(Form::Get);
    let client = ferrobridge_term::client::Client::new(config)?;
    let outcome = client.send(&Request::ValidateCode(validation())).await?;
    assert!(matches!(
        outcome,
        BatchOutcome::ValidateCode(ValidateOutcome::Valid { .. })
    ));
    Ok(())
}
