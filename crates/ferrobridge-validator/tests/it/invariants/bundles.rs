// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Bundle invariants of R4 bundle.html (`bdl-7`, and `bdl-9` to `bdl-11`
//! on a document), each broken by a synthetic Bundle.

use serde_json::json;

use super::against_core;
use super::failed_at;
use crate::support::TestResult;

/// A narrative, so `dom-6` holds.
fn text() -> serde_json::Value {
    json!({"status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">synthetic</div>"})
}

/// A synthetic Patient.
fn patient() -> serde_json::Value {
    json!({"resourceType": "Patient", "text": text(), "name": [{"family": "Doe"}]})
}

#[test]
fn bdl_7_reports_a_repeated_full_url() -> TestResult {
    let entry = |id: &str| json!({"fullUrl": format!("urn:uuid:4a1e8f9e-0000-4000-8000-00000000004{id}"), "resource": patient()});
    let distinct =
        json!({"resourceType": "Bundle", "type": "collection", "entry": [entry("1"), entry("2")]});
    assert_eq!(
        failed_at(&against_core(&distinct)?, "bdl-7"),
        Vec::<String>::new()
    );
    let repeated =
        json!({"resourceType": "Bundle", "type": "collection", "entry": [entry("1"), entry("1")]});
    assert_eq!(failed_at(&against_core(&repeated)?, "bdl-7"), ["Bundle"]);
    Ok(())
}

/// The fullUrl of the document's Composition.
const COMPOSITION: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000050";

/// The fullUrl of the document's Patient.
const PATIENT: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000051";

/// A synthetic document Bundle: the Composition first unless `patient_first`,
/// with an identifier when `identifier` and a timestamp when `timestamp`.
fn document(identifier: bool, timestamp: bool, patient_first: bool) -> serde_json::Value {
    let composition = json!({
        "resourceType": "Composition",
        "text": text(),
        "status": "final",
        "type": {"text": "synthetic"},
        "subject": {"reference": PATIENT},
        "date": "2026-01-01",
        "author": [{"reference": PATIENT}],
        "title": "Synthetic",
        "section": [{"title": "Notes", "text": text()}]
    });
    let mut entries = vec![
        json!({"fullUrl": COMPOSITION, "resource": composition}),
        json!({"fullUrl": PATIENT, "resource": patient()}),
    ];
    if patient_first {
        entries.reverse();
    }
    let mut bundle = json!({"resourceType": "Bundle", "type": "document", "entry": entries});
    if identifier {
        bundle["identifier"] = json!({"system": "urn:ietf:rfc:3986", "value": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000052"});
    }
    if timestamp {
        bundle["timestamp"] = json!("2026-01-01T00:00:00Z");
    }
    bundle
}

#[test]
fn a_complete_document_meets_bdl_9_to_bdl_11() -> TestResult {
    let report = against_core(&document(true, true, false))?;
    for key in ["bdl-9", "bdl-10", "bdl-11"] {
        assert!(
            failed_at(&report, key).is_empty(),
            "{key}: {:#?}",
            report.issues()
        );
    }
    Ok(())
}

#[test]
fn bdl_9_reports_a_document_without_an_identifier() -> TestResult {
    let report = against_core(&document(false, true, false))?;
    assert_eq!(failed_at(&report, "bdl-9"), ["Bundle"]);
    Ok(())
}

#[test]
fn bdl_10_reports_a_document_without_a_timestamp() -> TestResult {
    let report = against_core(&document(true, false, false))?;
    assert_eq!(failed_at(&report, "bdl-10"), ["Bundle"]);
    Ok(())
}

#[test]
fn bdl_11_reports_a_document_whose_first_entry_is_no_composition() -> TestResult {
    let report = against_core(&document(true, true, true))?;
    assert_eq!(failed_at(&report, "bdl-11"), ["Bundle"]);
    Ok(())
}

#[test]
fn bdl_3_reads_the_bundle_through_percent_resource() -> TestResult {
    // bundle.html bdl-3: an entry of a transaction carries a request; %resource is the Bundle.
    let bundle = json!({"resourceType": "Bundle", "type": "transaction", "entry": [
        {"fullUrl": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000053", "resource": patient()}
    ]});
    assert_eq!(failed_at(&against_core(&bundle)?, "bdl-3"), ["Bundle"]);
    Ok(())
}
