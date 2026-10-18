// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Agreement with the HL7 FHIR Validator 6.10.4 on a shared corpus (#442):
//! the synthetic cases under `tests/agreement/cases/`, validated here and by
//! the reference, whose outcomes `scripts/vendor/hl7-validator-agreement.sh`
//! recorded under `tests/agreement/reference/`.
//!
//! The reference is prior art and never the oracle. Two outcomes agree when
//! they report the same findings of severity error or fatal, each read as its
//! location and its rule: the key of a failed invariant, the slice a slice
//! cardinality names, or no rule beyond the location. Warnings and
//! information are not compared, since the two validators word and grade them
//! in their own vocabularies. Every case on which they disagree is
//! adjudicated in [`ADJUDICATED`] against the specification text.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_testkit::conformance;
use ferrobridge_testkit::conformance::Case;
use ferrobridge_testkit::conformance::Corpus;
use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::validate::Validator;
use fhir_types::codec::Value;

use crate::eu::eu_set;
use crate::support::TestResult;

/// The cases on which the two validators disagree, each with the reading of
/// the specification that settles it.
const ADJUDICATED: [(&str, &str); 1] = [(
    "09-eps-bundle-without-its-patient-entry",
    "both find the Composition's subject outside the document Bundle (documents.html §3.3.1); the \
     reference places it at Bundle.entry[1] while the Composition is the first entry, and the \
     FHIRPath indexer is 0-based (FHIRPath N1 §5.3.1), so Bundle.entry[0] here is the location",
)];

/// One finding, as both sides are compared: the location and the rule.
type Finding = (String, String);

/// The directory of the corpus.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agreement")
}

/// The rule of a slice cardinality finding, read from the slice id the text
/// names, or `*`.
fn slice_rule(text: &str) -> Option<String> {
    let id = if let Some(rest) = text.strip_prefix("Slice '") {
        rest.split('\'').next()?
    } else {
        text.split(" occurs ")
            .next()
            .filter(|id| id.contains(':'))?
    };
    Some(format!("slice:{id}"))
}

/// The findings of this validator's `report`.
fn ours(report: &Report) -> BTreeSet<Finding> {
    report
        .issues()
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Error | Severity::Fatal))
        // NOTE: no specification governs this: our own design; a claimed profile's failure is
        // also stated as its own findings, which are what the reference reports.
        .filter(|issue| issue.rule != Rule::Message(MessageId::ClaimedProfileMismatch))
        .map(|issue| {
            let rule = match &issue.rule {
                Rule::Constraint { key, .. } => format!("constraint:{key}"),
                Rule::Message(MessageId::MinCardinality | MessageId::MaxCardinality) => {
                    slice_rule(&issue.message).unwrap_or_else(|| "*".to_owned())
                }
                Rule::Message(_) => "*".to_owned(),
            };
            (issue.expression.clone(), rule)
        })
        .collect()
}

/// The findings the reference recorded in `outcome`.
fn reference(outcome: &serde_json::Value) -> BTreeSet<Finding> {
    outcome["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|issue| {
            let location = issue["location"].as_str().unwrap_or_default().to_owned();
            let id = issue["rule"].as_str().unwrap_or_default();
            let text = issue["text"].as_str().unwrap_or_default();
            let rule = if id.starts_with("constraint:") {
                id.to_owned()
            } else if id.starts_with("Validation_VAL_Profile_M") && id.ends_with("_SLICE") {
                slice_rule(text).unwrap_or_else(|| "*".to_owned())
            } else {
                "*".to_owned()
            };
            (location, rule)
        })
        .collect()
}

#[test]
fn conformance_the_validator_agrees_with_the_hl7_validator_on_its_corpus() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let validator = Validator::new(&set);
    let mut cases = Vec::new();
    let mut unadjudicated = Vec::new();
    let mut stale = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(corpus().join("cases"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    files.sort();
    for path in &files {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let file = path.file_name().unwrap_or_default();
        let resource: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let outcome: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
            corpus().join("reference").join(file),
        )?)?;
        let found = ours(&validator.validate(&resource)?);
        let expected = reference(&outcome);
        let adjudication = ADJUDICATED
            .iter()
            .find(|(case, _)| *case == name)
            .map(|(_, reading)| *reading);
        if found == expected {
            if adjudication.is_some() {
                stale.push(name.clone());
            }
            cases.push(Case::pass(name));
            continue;
        }
        let only_ours: Vec<&Finding> = found.difference(&expected).collect();
        let only_reference: Vec<&Finding> = expected.difference(&found).collect();
        match adjudication {
            Some(reading) => cases.push(Case::fail(name, format!("adjudicated: {reading}"))),
            None => unadjudicated.push(format!(
                "{name}: only here {only_ours:?}, only in the reference {only_reference:?}"
            )),
        }
    }
    assert!(
        unadjudicated.is_empty(),
        "disagreements no adjudication settles:\n{}",
        unadjudicated.join("\n")
    );
    assert!(stale.is_empty(), "adjudicated cases that agree: {stale:?}");
    let outcome = conformance::record(Corpus::ValidatorAgreement, &cases)?;
    assert!(
        outcome.regressed.is_empty(),
        "cases the pass list records that fail: {:?}",
        outcome.regressed
    );
    Ok(())
}
