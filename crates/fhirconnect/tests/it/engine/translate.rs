// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The translation of a code through the concept map its mapping names
//! (`types-of-mappings/concept-type/manual.adoc`, §`ConceptMaps`), over
//! `ConceptMap/$translate`
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).

use core::cell::RefCell;
use core::error::Error;
use std::collections::BTreeMap;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponseMatch;
use fhir_types::r4::primitives::Boolean;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::terminology::Terminology;
use fhirconnect::engine::terminology::TerminologyError;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::defaults;
use crate::support::compiled;
use crate::support::template;

/// The concept map the certainty mappings name.
const CERTAINTY_MAP: &str = "http://example.org/fhir/ConceptMap/ferrobridge-certainty";

/// The concept map the problem name mapping names.
const PROBLEM_MAP: &str = "http://example.org/fhir/ConceptMap/ferrobridge-problems";

/// The system the synthetic `Condition` writes its verification status in.
const VER_STATUS: &str = "http://terminology.hl7.org/CodeSystem/condition-ver-status";

/// The system the synthetic `Condition` writes its problem code in.
const PROBLEMS: &str = "http://example.org/fhir/CodeSystem/synthetic-problems";

/// The path of the diagnostic certainty value in the synthetic template.
const CERTAINTY: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0073]/value";

/// The path of the problem name value in the synthetic template.
const PROBLEM_NAME: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// One request the stub received: the concept map, the system and the code.
type Received = (Option<String>, Option<String>, Option<String>);

/// A terminology stub over a table from concept map, system and code to the
/// matches it answers, recording every request in order.
#[derive(Debug, Default)]
struct StubTerminology {
    answers: BTreeMap<(String, String, String), Vec<(String, Coding)>>,
    received: RefCell<Vec<Received>>,
}

impl StubTerminology {
    /// Returns this stub answering `system` and `code` through `map` with one
    /// more match of `equivalence` to `concept`.
    fn answering(
        mut self,
        map: &str,
        (system, code): (&str, &str),
        equivalence: &str,
        concept: Coding,
    ) -> Self {
        self.answers
            .entry((String::from(map), String::from(system), String::from(code)))
            .or_default()
            .push((String::from(equivalence), concept));
        self
    }

    /// Returns the requests the stub received, in order.
    fn received(&self) -> Vec<Received> {
        self.received.borrow().clone()
    }
}

impl Terminology for StubTerminology {
    fn translate(
        &self,
        request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        let url = request.url.as_ref().and_then(|uri| uri.value.clone());
        let system = request.system.as_ref().and_then(|uri| uri.value.clone());
        let code = request.code.as_ref().and_then(|code| code.value.clone());
        self.received
            .borrow_mut()
            .push((url.clone(), system.clone(), code.clone()));
        let key = (
            url.unwrap_or_default(),
            system.unwrap_or_default(),
            code.unwrap_or_default(),
        );
        let matches: Vec<ConceptMapTranslateResponseMatch> = self
            .answers
            .get(&key)
            .into_iter()
            .flatten()
            .map(|(equivalence, concept)| ConceptMapTranslateResponseMatch {
                equivalence: Some(Code::from(equivalence.as_str())),
                concept: Some(concept.clone()),
                ..ConceptMapTranslateResponseMatch::default()
            })
            .collect();
        Ok(ConceptMapTranslateResponse {
            result: Boolean {
                value: Some(!matches.is_empty()),
                ..Boolean::default()
            },
            r#match: matches,
            ..ConceptMapTranslateResponse::default()
        })
    }
}

/// Returns a coding of `system`, `code` and `display`.
fn coding(system: &str, code: &str, display: Option<&str>) -> Coding {
    Coding {
        system: Some(Uri::from(system)),
        code: Some(Code::from(code)),
        display: display.map(FhirString::from),
        ..Coding::default()
    }
}

/// Returns the local certainty concept a translation writes.
fn local_confirmed() -> Coding {
    coding("local", "at0074", Some("Confirmed by the map"))
}

/// Returns the synthetic `Condition` of the testkit, with `code` removed when
/// `with_code` is false.
fn condition(with_code: bool) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    if !with_code && let Some(object) = parsed.as_object_mut() {
        object.remove("code");
    }
    Ok(Value::from_serde_json(parsed))
}

/// Returns the certainty value a composition holds, when it holds one.
fn certainty_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Option<serde_json::Value>, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(CERTAINTY))?;
    Ok(index.read(composition, node, &[])?)
}

/// Returns the skipped mappings an outcome declares, in order.
fn skipped<T>(outcome: &Outcome<T>) -> Vec<&Loss> {
    outcome
        .warnings()
        .iter()
        .map(Warning::loss)
        .filter(|loss| matches!(loss, Loss::Skipped { .. }))
        .collect()
}

/// Returns the loss of a mapping whose code `conceptmap` did not translate.
fn untranslated(mapping: &str, conceptmap: &str) -> Loss {
    Loss::Skipped {
        mapping: String::from(mapping),
        reason: SkipReason::Untranslated {
            conceptmap: String::from(conceptmap),
        },
    }
}

/// Returns a stub that translates the synthetic problem code equivalently.
fn problems_translated() -> StubTerminology {
    StubTerminology::default().answering(
        PROBLEM_MAP,
        (PROBLEMS, "SYN-001"),
        "equivalent",
        coding(
            "http://example.org/fhir/CodeSystem/target-problems",
            "TGT-1",
            Some("Target problem"),
        ),
    )
}

#[test]
fn a_translate_with_no_terminology_server_is_a_typed_error() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("no server fails closed, never a pass-through");
    match refused {
        EngineError::Terminology {
            ref mapping,
            ref conceptmap,
            ref source,
        } => {
            assert_eq!(mapping, "certainty");
            assert_eq!(conceptmap, CERTAINTY_MAP);
            assert!(
                matches!(**source, TerminologyError::NotConfigured),
                "the cause is the missing server: {source:?}"
            );
        }
        ref other => return Err(format!("expected a terminology refusal, got {other}").into()),
    }
    Ok(())
}

#[test]
fn a_translated_code_writes_the_concept_and_its_display_wins() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = problems_translated().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equal",
        local_confirmed(),
    );
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )?;
    let written = certainty_of(&index, inbound.value())?.ok_or("the certainty was written")?;
    assert_eq!(
        written
            .pointer("/defining_code/code_string")
            .and_then(serde_json::Value::as_str),
        Some("at0074"),
        "the translated code is the defining code: {written}"
    );
    assert_eq!(
        written
            .pointer("/defining_code/terminology_id/value")
            .and_then(serde_json::Value::as_str),
        Some("local")
    );
    assert_eq!(
        written.get("value").and_then(serde_json::Value::as_str),
        Some("Confirmed by the map"),
        "the translated concept's display wins over the source display"
    );
    assert_eq!(
        skipped(&inbound),
        Vec::<&Loss>::new(),
        "a translated run skips nothing"
    );
    Ok(())
}

#[test]
fn translate_is_called_with_the_resources_own_system_and_code() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = problems_translated().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equivalent",
        local_confirmed(),
    );
    to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(
        stub.received(),
        vec![
            (
                Some(String::from(CERTAINTY_MAP)),
                Some(String::from(VER_STATUS)),
                Some(String::from("confirmed"))
            ),
            (
                Some(String::from(PROBLEM_MAP)),
                Some(String::from(PROBLEMS)),
                Some(String::from("SYN-001"))
            ),
        ],
        "one call per code, in program order, with the codes the resource carries"
    );
    Ok(())
}

#[test]
fn translate_is_called_once_before_a_refusal_later_in_the_unit() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate_late")?;
    let index = template()?;
    let stub = StubTerminology::default().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equivalent",
        local_confirmed(),
    );
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(false)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("the required problem name is missing");
    assert!(
        matches!(
            refused,
            EngineError::MissingRequired { ref mapping, .. } if mapping == "diagnosis.problemName"
        ),
        "the unit refuses after the translation: {refused}"
    );
    assert_eq!(
        stub.received(),
        vec![(
            Some(String::from(CERTAINTY_MAP)),
            Some(String::from(VER_STATUS)),
            Some(String::from("confirmed"))
        )],
        "exactly one $translate, made before the refusal"
    );
    Ok(())
}

#[test]
fn a_code_repeated_in_one_concept_is_translated_once() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    if let Some(object) = parsed.as_object_mut() {
        let confirmed = serde_json::json!({"system": VER_STATUS, "code": "confirmed"});
        object.insert(
            String::from("verificationStatus"),
            serde_json::json!({"coding": [confirmed.clone(), confirmed]}),
        );
    }
    let stub = problems_translated().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equivalent",
        local_confirmed(),
    );
    to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(parsed),
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(
        stub.received().len(),
        2,
        "the certainty code and the problem code, one call each"
    );
    Ok(())
}

#[test]
fn an_untranslatable_optional_code_writes_no_element() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = problems_translated();
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(
        certainty_of(&index, inbound.value())?,
        None,
        "the source code is not written under the target"
    );
    assert_eq!(
        skipped(&inbound),
        vec![&untranslated("certainty", CERTAINTY_MAP)]
    );
    let location = inbound
        .warnings()
        .iter()
        .find(|warning| matches!(warning.loss(), Loss::Skipped { .. }))
        .map(|warning| warning.location().clone())
        .ok_or("one skipped mapping")?;
    assert_eq!(location.fhir(), Some("Condition.verificationStatus"));
    assert_eq!(location.openehr(), Some(CERTAINTY));
    Ok(())
}

#[test]
fn an_untranslatable_required_code_refuses_the_unit() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = StubTerminology::default().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equivalent",
        local_confirmed(),
    );
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("the problem name is required");
    match refused {
        EngineError::Untranslated {
            ref mapping,
            ref conceptmap,
            ref element,
        } => {
            assert_eq!(mapping, "problemName");
            assert_eq!(conceptmap, PROBLEM_MAP);
            assert_eq!(element, PROBLEM_NAME);
        }
        ref other => return Err(format!("expected an untranslated refusal, got {other}").into()),
    }
    Ok(())
}

#[test]
fn only_equivalent_and_equal_answers_count() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    for equivalence in [
        "relatedto",
        "wider",
        "subsumes",
        "narrower",
        "specializes",
        "inexact",
        "unmatched",
        "disjoint",
    ] {
        let stub = problems_translated().answering(
            CERTAINTY_MAP,
            (VER_STATUS, "confirmed"),
            equivalence,
            local_confirmed(),
        );
        let inbound = to_openehr(
            &program,
            &SCHEMAS,
            &index,
            &condition(true)?,
            &Seams::default().with_terminology(&stub),
            &defaults(),
            &CallContext::new(),
        )?;
        assert_eq!(
            certainty_of(&index, inbound.value())?,
            None,
            "a {equivalence} match is no translation"
        );
        assert_eq!(
            skipped(&inbound),
            vec![&untranslated("certainty", CERTAINTY_MAP)],
            "a {equivalence} match is a declared loss"
        );
    }
    Ok(())
}

#[test]
fn two_equivalent_concepts_refuse_as_ambiguous() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = problems_translated()
        .answering(
            CERTAINTY_MAP,
            (VER_STATUS, "confirmed"),
            "equivalent",
            local_confirmed(),
        )
        .answering(
            CERTAINTY_MAP,
            (VER_STATUS, "confirmed"),
            "equal",
            coding("local", "at0075", Some("Another")),
        );
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&stub),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("two concepts are no single translation");
    assert!(
        matches!(
            refused,
            EngineError::Terminology { ref source, .. }
                if matches!(**source, TerminologyError::Ambiguous { concepts: 2 })
        ),
        "the refusal names the ambiguity: {refused}"
    );
    Ok(())
}

#[test]
fn coming_out_of_openehr_the_read_code_is_translated() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let stub = problems_translated()
        .answering(
            CERTAINTY_MAP,
            (VER_STATUS, "confirmed"),
            "equivalent",
            local_confirmed(),
        )
        .answering(
            CERTAINTY_MAP,
            ("local", "at0074"),
            "equivalent",
            coding(VER_STATUS, "confirmed", Some("Confirmed")),
        );
    let seams = Seams::default().with_terminology(&stub);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &seams,
        &defaults(),
        &CallContext::new(),
    )?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &seams,
        &CallContext::new(),
    )?;
    let status = outbound
        .value()
        .get("verificationStatus")
        .ok_or("the certainty was written")?
        .to_serde_json(&mut Path::root("CodeableConcept"))?;
    assert_eq!(
        status,
        serde_json::json!({
            "coding": [{"system": VER_STATUS, "code": "confirmed", "display": "Confirmed"}],
            "text": "Confirmed"
        }),
        "the openEHR code went through the map, never out under its own system"
    );
    Ok(())
}

#[test]
fn coming_out_of_openehr_an_untranslatable_optional_code_writes_nothing()
-> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_translate")?;
    let index = template()?;
    let inbound_stub = problems_translated().answering(
        CERTAINTY_MAP,
        (VER_STATUS, "confirmed"),
        "equivalent",
        local_confirmed(),
    );
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(true)?,
        &Seams::default().with_terminology(&inbound_stub),
        &defaults(),
        &CallContext::new(),
    )?;
    let outbound_stub = StubTerminology::default();
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default().with_terminology(&outbound_stub),
        &CallContext::new(),
    )?;
    assert!(
        outbound.value().get("verificationStatus").is_none(),
        "an untranslated certainty writes no element"
    );
    assert!(
        outbound.value().get("code").is_none(),
        "an untranslated problem name writes no element"
    );
    assert_eq!(
        skipped(&outbound),
        vec![
            &untranslated("certainty", CERTAINTY_MAP),
            &untranslated("problemName", PROBLEM_MAP),
        ]
    );
    assert_eq!(
        outbound_stub.received().first(),
        Some(&(
            Some(String::from(CERTAINTY_MAP)),
            Some(String::from("local")),
            Some(String::from("at0074"))
        )),
        "the code the composition holds is the one sent"
    );
    Ok(())
}
