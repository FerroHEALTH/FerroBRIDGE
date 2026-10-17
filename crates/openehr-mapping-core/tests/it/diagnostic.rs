// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The serde form of a diagnostic: its wire shape, and the refusals reading it
//! back runs (no specification governs the shape: our own design).

use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::DiagnosticCode;
use openehr_mapping_core::diagnostic::LanguageCode;
use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::position::Position;

type TestResult = Result<(), Box<dyn core::error::Error>>;

#[test]
fn a_full_diagnostic_has_the_flat_shape_and_reads_back_equal() -> TestResult {
    let diagnostic = Diagnostic::error(
        "model/observation.yml",
        DiagnosticCode::Language(LanguageCode::new("unknown-type")?),
        "the type `Foo` is not a FHIR R4 type",
    )
    .with_position(Position::new(12, 7))
    .with_mapping_name(MappingName::new("OBSERVATION.body_weight.v2")?)
    .with_model_path(ModelPath::root().field("mappings").index(3).field("with"));
    let json = serde_json::to_string(&diagnostic)?;
    assert_eq!(
        json,
        r#"{"file":"model/observation.yml","line":12,"column":7,"severity":"error","code":"unknown-type","message":"the type `Foo` is not a FHIR R4 type","mapping":"OBSERVATION.body_weight.v2","model_path":["mappings",3,"with"]}"#
    );
    assert_eq!(serde_json::from_str::<Diagnostic>(&json)?, diagnostic);
    Ok(())
}

#[test]
fn an_unplaced_diagnostic_writes_nulls_and_an_empty_path() -> TestResult {
    let diagnostic = Diagnostic::warning("context.yml", DiagnosticCode::Unreadable, "no such file");
    let json = serde_json::to_string(&diagnostic)?;
    assert_eq!(
        json,
        r#"{"file":"context.yml","line":null,"column":null,"severity":"warning","code":"unreadable","message":"no such file","mapping":null,"model_path":[]}"#
    );
    assert_eq!(serde_json::from_str::<Diagnostic>(&json)?, diagnostic);
    Ok(())
}

#[test]
fn a_built_in_spelling_reads_as_the_built_in_code() -> TestResult {
    let json = r#"{"file":"a.yml","line":null,"column":null,"severity":"error","code":"missing-key","message":"m","mapping":null,"model_path":[]}"#;
    let diagnostic: Diagnostic = serde_json::from_str(json)?;
    assert_eq!(diagnostic.code(), &DiagnosticCode::MissingKey);
    Ok(())
}

fn refused(json: &str) -> bool {
    serde_json::from_str::<Diagnostic>(json).is_err()
}

#[test]
fn an_invalid_code_is_refused() {
    assert!(refused(
        r#"{"file":"a.yml","line":null,"column":null,"severity":"error","code":"Not A Code","message":"m","mapping":null,"model_path":[]}"#
    ));
}

#[test]
fn an_invalid_mapping_name_is_refused() {
    assert!(refused(
        r#"{"file":"a.yml","line":null,"column":null,"severity":"error","code":"unreadable","message":"m","mapping":" padded","model_path":[]}"#
    ));
}

#[test]
fn a_line_without_a_column_is_refused() {
    assert!(refused(
        r#"{"file":"a.yml","line":3,"column":null,"severity":"error","code":"unreadable","message":"m","mapping":null,"model_path":[]}"#
    ));
}

#[test]
fn an_unknown_severity_or_key_is_refused() {
    assert!(refused(
        r#"{"file":"a.yml","line":null,"column":null,"severity":"fatal","code":"unreadable","message":"m","mapping":null,"model_path":[]}"#
    ));
    assert!(refused(
        r#"{"file":"a.yml","line":null,"column":null,"severity":"error","code":"unreadable","message":"m","mapping":null,"model_path":[],"extra":1}"#
    ));
}
