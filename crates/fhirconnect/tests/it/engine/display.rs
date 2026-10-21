// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display of a code that carries none, resolved through the
//! terminology seam going into openEHR (`DV_CODED_TEXT.adoc`: "We recommend
//! to resolve the code display using a terminology server when mapping to
//! openEHR, since the value is mandatory in openEHR").

use core::error::Error;
use std::sync::Mutex;

use fhir_types::codec::Value;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupRequest;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupResponse;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::terminology::Terminology;
use fhirconnect::engine::terminology::TerminologyError;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::defaults;
use crate::support::compiled;
use crate::support::template;

/// The diagnostic certainty in the synthetic template, a `DV_CODED_TEXT`.
const CERTAINTY: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0073]/value";

/// The system of the synthetic code, example.org being reserved for
/// examples (RFC 6761 section 6.5).
const SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrobridge-problems";

/// One question the stub was asked: the system, the code and the version.
type Asked = (Option<String>, Option<String>, Option<String>);

/// What the stub answers a `$lookup` with.
#[derive(Debug)]
enum Answer {
    /// The server knows the code and names this display.
    Found(&'static str),
    /// The server does not know the code.
    Unknown,
    /// The server failed.
    Failed,
}

/// A terminology server stub that answers every `$lookup` alike and records
/// what it was asked.
#[derive(Debug)]
struct Stub {
    answer: Answer,
    fallback: bool,
    asked: Mutex<Vec<Asked>>,
}

impl Stub {
    /// Creates a stub answering `answer`, with the code fallback chosen or
    /// not.
    fn new(answer: Answer, fallback: bool) -> Self {
        Self {
            answer,
            fallback,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Returns every system, code and version the stub was asked for.
    fn asked(&self) -> Vec<Asked> {
        self.asked
            .lock()
            .map(|asked| asked.clone())
            .unwrap_or_default()
    }
}

/// The failure the stub reports for [`Answer::Failed`].
#[derive(Debug, thiserror::Error)]
#[error("the synthetic terminology server answered 503")]
struct Unavailable;

impl Terminology for Stub {
    fn translate(
        &self,
        _request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        Err(TerminologyError::NotConfigured)
    }

    fn lookup(
        &self,
        request: &CodeSystemLookupRequest,
    ) -> Result<Option<CodeSystemLookupResponse>, TerminologyError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push((
                request.system.as_ref().and_then(|uri| uri.value.clone()),
                request.code.as_ref().and_then(|code| code.value.clone()),
                request.version.as_ref().and_then(|text| text.value.clone()),
            ));
        }
        match self.answer {
            Answer::Found(display) => Ok(Some(CodeSystemLookupResponse {
                name: FhirString::from("ferrobridge problems"),
                display: FhirString::from(display),
                ..CodeSystemLookupResponse::default()
            })),
            Answer::Unknown => Ok(None),
            Answer::Failed => Err(TerminologyError::Upstream {
                source: Box::new(Unavailable),
            }),
        }
    }

    fn display_fallback(&self) -> bool {
        self.fallback
    }
}

/// Returns a `Condition` whose verification status carries `coding`.
fn condition(coding: &serde_json::Value) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Condition",
        "code": {"text": "Synthetic condition"},
        "verificationStatus": {"coding": [coding]}
    }))
}

/// Returns the undisplayed coding of the synthetic code, at `version` when
/// one is given.
fn undisplayed(version: Option<&str>) -> serde_json::Value {
    let mut coding = serde_json::json!({"system": SYSTEM, "code": "P-17"});
    if let (Some(version), Some(object)) = (version, coding.as_object_mut()) {
        object.insert(String::from("version"), serde_json::json!(version));
    }
    coding
}

/// Maps `document` into openEHR through the display fixture over `terminology`.
fn mapped(
    document: &Value,
    terminology: &dyn Terminology,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    let program = compiled("ferrobridge_display").map_err(|error| EngineError::NotApplicable {
        context: error.to_string(),
    })?;
    let index = template().map_err(|error| EngineError::NotApplicable {
        context: error.to_string(),
    })?;
    to_openehr(
        &program,
        &SCHEMAS,
        &index,
        document,
        &Seams::default().with_terminology(terminology),
        &defaults(),
        &CallContext::new(),
    )
}

/// Returns the `value` of the diagnostic certainty a composition holds.
fn certainty_value(composition: &CanonicalComposition) -> Result<String, Box<dyn Error>> {
    let index = template()?;
    let node = index.node(&AqlPath::new(CERTAINTY))?;
    Ok(index
        .read(composition, node, &[])?
        .and_then(|value| {
            value
                .get("value")
                .and_then(|text| text.as_str().map(String::from))
        })
        .ok_or("the run wrote the certainty")?)
}

/// Returns whether the outcome declares the coded text's code fallback.
fn fell_back(outcome: &Outcome<CanonicalComposition>) -> bool {
    outcome.warnings().iter().any(|warning| {
        matches!(warning.loss(), Loss::OneWayFallback { field, .. } if field == "DV_TEXT.value")
    })
}

#[test]
fn a_display_the_server_knows_becomes_the_value() -> Result<(), Box<dyn Error>> {
    let stub = Stub::new(Answer::Found("Synthetic problem"), false);
    let outcome = mapped(&condition(&undisplayed(Some("2026"))), &stub)?;
    assert_eq!(certainty_value(outcome.value())?, "Synthetic problem");
    assert!(!fell_back(&outcome), "{:?}", outcome.warnings());
    assert_eq!(
        stub.asked(),
        vec![(
            Some(String::from(SYSTEM)),
            Some(String::from("P-17")),
            Some(String::from("2026"))
        )]
    );
    Ok(())
}

#[test]
fn a_code_the_server_does_not_know_refuses_the_unit_by_default() {
    let stub = Stub::new(Answer::Unknown, false);
    let error = mapped(&condition(&undisplayed(None)), &stub)
        .err()
        .map(|error| format!("{error:?}"));
    assert!(
        error
            .as_deref()
            .is_some_and(|error| error.starts_with("UnknownDisplay")),
        "{error:?}"
    );
}

#[test]
fn a_code_the_server_does_not_know_takes_the_code_when_the_fallback_is_chosen()
-> Result<(), Box<dyn Error>> {
    // DV_CODED_TEXT.adoc: "Alternatively the coding.code can be used".
    let stub = Stub::new(Answer::Unknown, true);
    let outcome = mapped(&condition(&undisplayed(None)), &stub)?;
    assert_eq!(certainty_value(outcome.value())?, "P-17");
    assert!(fell_back(&outcome), "the code fallback is declared");
    Ok(())
}

#[test]
fn a_server_that_fails_refuses_the_unit_carrying_its_failure() {
    let stub = Stub::new(Answer::Failed, true);
    match mapped(&condition(&undisplayed(None)), &stub) {
        Err(EngineError::DisplayLookup { source, code, .. }) => {
            assert_eq!(code, "P-17");
            assert!(
                matches!(*source, TerminologyError::Upstream { .. }),
                "{source:?}"
            );
        }
        other => panic!("a failed lookup refuses the unit: {other:?}"),
    }
}

#[test]
fn with_no_server_the_code_stands_as_the_display() -> Result<(), Box<dyn Error>> {
    // DV_CODED_TEXT.adoc: the code "can be used if a term server is not an
    // option".
    let program = compiled("ferrobridge_display")?;
    let index = template()?;
    let outcome = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(&undisplayed(None)),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(certainty_value(outcome.value())?, "P-17");
    assert!(fell_back(&outcome));
    Ok(())
}

#[test]
fn a_coding_that_carries_a_display_asks_the_server_nothing() -> Result<(), Box<dyn Error>> {
    let stub = Stub::new(Answer::Failed, false);
    let outcome = mapped(
        &condition(&serde_json::json!({"system": SYSTEM, "code": "P-17", "display": "Shown"})),
        &stub,
    )?;
    assert_eq!(certainty_value(outcome.value())?, "Shown");
    assert_eq!(
        stub.asked(),
        Vec::<(Option<String>, Option<String>, Option<String>)>::new()
    );
    Ok(())
}
