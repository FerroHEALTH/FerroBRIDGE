// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The origin a run records in the composition's `FEEDER_AUDIT`.

use core::error::Error;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::origin::Origin;
use fhirconnect::engine::origin::SourceItem;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::defaults::Defaults;
use fhirconnect::engine::traverse::to_openehr;

use crate::support::compiled;
use crate::support::template;

use crate::engine::condition_document;
use crate::engine::defaults;

/// Returns the defaults of a run whose origin is the synthetic sender.
fn audited(source: SourceItem) -> Defaults {
    defaults().with_origin(Origin::new("ferrobridge.test").with_source(source))
}

/// Returns the `id` of every entry of one `FEEDER_AUDIT` identifier list.
fn audit_ids<'value>(audit: &'value serde_json::Value, list: &str) -> Vec<&'value str> {
    audit
        .get(list)
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_run_with_an_origin_records_it_in_the_feeder_audit() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 common.html §FEEDER_AUDIT: the audit "describes the origin of
    // data that have been transformed into openEHR form", and
    // FEEDER_AUDIT_DETAILS.system_id names the system that handled it.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &audited(
            SourceItem::new("Condition")
                .with_id("sender-1")
                .with_version_id("3"),
        ),
        &CallContext::new(),
    )?;
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(
        audit
            .pointer("/originating_system_audit/system_id")
            .and_then(serde_json::Value::as_str),
        Some("ferrobridge.test")
    );
    assert_eq!(
        audit
            .pointer("/originating_system_audit/version_id")
            .and_then(serde_json::Value::as_str),
        Some("3"),
        "the source meta.versionId travels as the originating version"
    );
    assert_eq!(
        audit_ids(audit, "originating_system_item_ids"),
        ["sender-1"],
        "the source resource id travels as the one originating item: {audit}"
    );
    assert_eq!(
        audit
            .pointer("/originating_system_item_ids/0/type")
            .and_then(serde_json::Value::as_str),
        Some("Condition")
    );
    assert_eq!(
        audit
            .pointer("/feeder_system_audit/system_id")
            .and_then(serde_json::Value::as_str),
        Some("ferrobridge.test")
    );
    Ok(())
}

#[test]
fn the_defaulted_warnings_and_the_feeder_audit_agree() -> Result<(), Box<dyn Error>> {
    // defaults-for-fields.adoc names the fields the engine fills; each one is
    // a declared Loss::Defaulted and one feeder_system_item_ids entry, in
    // the same order.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &audited(SourceItem::new("Condition").with_id("sender-1")),
        &CallContext::new(),
    )?;
    let warned: Vec<&str> = inbound
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Defaulted { ref field } => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        !warned.is_empty(),
        "the minimal chain leaves fields to default"
    );
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(
        audit_ids(audit, "feeder_system_item_ids"),
        warned,
        "the audit lists exactly the defaulted fields, in order: {audit}"
    );
    let types: Vec<Option<&str>> = audit
        .get("feeder_system_item_ids")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| entry.get("type").and_then(serde_json::Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        types.iter().all(|kind| *kind == Some("defaulted")),
        "every feeder item is typed as a default: {types:?}"
    );
    // The builder validated the composition against its template before it
    // answered; reading it back through the same validation shows the audit
    // is admitted content.
    index.accept(inbound.value().value().clone())?;
    Ok(())
}

#[test]
fn a_source_with_no_id_is_recorded_as_unknown() -> Result<(), Box<dyn Error>> {
    // An absent identifier is recorded, never invented.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &audited(SourceItem::new("Condition")),
        &CallContext::new(),
    )?;
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(audit_ids(audit, "originating_system_item_ids"), ["unknown"]);
    assert_eq!(
        audit.pointer("/originating_system_audit/version_id"),
        None,
        "a source with no version records none"
    );
    Ok(())
}

#[test]
fn a_run_without_an_origin_writes_no_feeder_audit() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(inbound.value().value().get("feeder_audit"), None);
    Ok(())
}

#[test]
fn an_originating_system_is_named_apart_from_the_feeder() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 common.html §FEEDER_AUDIT: `originating_system_audit` is "Any
    // audit information for the information item from the originating system",
    // and `feeder_system_audit` that of the feeder system, "if different from
    // the originating system".
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let origin = Origin::new("ferrobridge.test")
        .with_originating_system("synthetic-machine-client")
        .with_source(SourceItem::new("Condition").with_id("sender-1"));
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults().with_origin(origin),
        &CallContext::new(),
    )?;
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(
        audit
            .pointer("/originating_system_audit/system_id")
            .and_then(serde_json::Value::as_str),
        Some("synthetic-machine-client")
    );
    assert_eq!(
        audit
            .pointer("/feeder_system_audit/system_id")
            .and_then(serde_json::Value::as_str),
        Some("ferrobridge.test"),
        "the bridge is the feeder"
    );
    Ok(())
}

#[test]
fn named_item_ids_replace_the_source_item_and_lead_the_feeder_ids() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 common.html §FEEDER_AUDIT: `originating_system_item_ids` are
    // "Identifiers used for the item in the originating system, e.g. filler
    // and placer ids", and `feeder_system_item_ids` those of the feeder.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let placer = fhirconnect::engine::origin::ItemId {
        id: String::from("PLC-1"),
        kind: String::from("PLAC"),
        issuer: Some(String::from("NORTHLAB")),
    };
    let control = fhirconnect::engine::origin::ItemId {
        id: String::from("MSG-1"),
        kind: String::from("ORU^R01"),
        issuer: None,
    };
    let origin = Origin::new("ferrobridge.test")
        .with_source(SourceItem::new("Condition").with_id("sender-1"))
        .with_originating_items(vec![placer])
        .with_feeder_items(vec![control]);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults().with_origin(origin),
        &CallContext::new(),
    )?;
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(
        vec!["PLC-1"],
        audit_ids(audit, "originating_system_item_ids")
    );
    assert_eq!(
        Some("NORTHLAB"),
        audit
            .pointer("/originating_system_item_ids/0/issuer")
            .and_then(serde_json::Value::as_str)
    );
    assert_eq!(
        Some(&"MSG-1"),
        audit_ids(audit, "feeder_system_item_ids").first(),
        "the named feeder id comes before the defaulted fields"
    );
    Ok(())
}

/// Returns the synthetic facility party the facility cases name.
fn north_ward() -> fhirconnect::engine::origin::Party {
    fhirconnect::engine::origin::Party {
        name: Some(String::from("North ward")),
        identifiers: vec![fhirconnect::engine::origin::ItemId {
            id: String::from("NORTHWARD"),
            kind: String::from("HD"),
            issuer: None,
        }],
    }
}

#[test]
fn a_default_facility_is_the_context_s_health_care_facility() -> Result<(), Box<dyn Error>> {
    // RM EHR IM, EVENT_CONTEXT.health_care_facility: "The health care facility
    // under whose care the event took place".
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults()
            .with_facility(north_ward())
            .with_origin(Origin::new("ferrobridge.test")),
        &CallContext::new(),
    )?;
    let composition = inbound.value().value();
    let facility = composition
        .pointer("/context/health_care_facility")
        .ok_or("the context names a facility")?;
    assert_eq!(Some("North ward"), facility["name"].as_str(), "{facility}");
    assert_eq!(
        Some("NORTHWARD"),
        facility
            .pointer("/identifiers/0/id")
            .and_then(serde_json::Value::as_str),
        "{facility}"
    );
    assert!(
        composition
            .pointer("/feeder_audit/originating_system_audit/location")
            .is_none(),
        "a placed facility is not the location as well"
    );
    Ok(())
}

#[test]
fn an_origin_provider_is_the_originating_system_s_provider() -> Result<(), Box<dyn Error>> {
    // RM Common IM, FEEDER_AUDIT_DETAILS.provider: "provider(s) who created,
    // committed, forwarded or otherwise handled the item".
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let origin =
        Origin::new("ferrobridge.test").with_provider(fhirconnect::engine::origin::Party {
            name: Some(String::from("Synthetic performing laboratory")),
            identifiers: Vec::new(),
        });
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults().with_origin(origin),
        &CallContext::new(),
    )?;
    let audit = inbound
        .value()
        .value()
        .get("feeder_audit")
        .ok_or("the composition carries a FEEDER_AUDIT")?;
    assert_eq!(
        Some("Synthetic performing laboratory"),
        audit
            .pointer("/originating_system_audit/provider/name")
            .and_then(serde_json::Value::as_str),
        "{audit}"
    );
    Ok(())
}

#[test]
fn a_default_composer_carries_its_identifier() -> Result<(), Box<dyn Error>> {
    // Simplified Formats master06 §Composer: `ctx/composer_name` is the
    // PARTY_IDENTIFIED name and `ctx/composer_id` its `external_ref`.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults().with_composer("Dana Doctor").with_composer_id(
            fhirconnect::engine::traverse::defaults::ComposerId {
                id: String::from("DR-1"),
                namespace: String::from("NORTHLAB"),
                scheme: String::from("XCN"),
            },
        ),
        &CallContext::new(),
    )?;
    let composer = inbound
        .value()
        .value()
        .get("composer")
        .ok_or("the composition names a composer")?;
    assert_eq!(Some("Dana Doctor"), composer["name"].as_str(), "{composer}");
    assert_eq!(
        Some("DR-1"),
        composer
            .pointer("/external_ref/id/value")
            .and_then(serde_json::Value::as_str),
        "{composer}"
    );
    assert_eq!(
        Some("NORTHLAB"),
        composer
            .pointer("/external_ref/namespace")
            .and_then(serde_json::Value::as_str),
        "{composer}"
    );
    Ok(())
}

#[test]
fn a_default_composer_carries_its_identifiers_beside_its_reference() -> Result<(), Box<dyn Error>> {
    // RM Common IM, `PARTY_IDENTIFIED.identifiers`: "One or more formal
    // identifiers"; the composer's `ctx/` keys carry none (master06
    // §Composer), so the composer travels whole as `composer|raw`.
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_document()?,
        &Seams::default(),
        &defaults()
            .with_composer("Dana Doctor")
            .with_composer_id(fhirconnect::engine::traverse::defaults::ComposerId {
                id: String::from("DR-1"),
                namespace: String::from("NORTHLAB"),
                scheme: String::from("XCN"),
            })
            .with_composer_identifiers(vec![fhirconnect::engine::origin::ItemId {
                id: String::from("DR-1"),
                kind: String::from("XCN"),
                issuer: Some(String::from("NORTHLAB")),
            }]),
        &CallContext::new(),
    )?;
    let composer = inbound
        .value()
        .value()
        .get("composer")
        .ok_or("the composition names a composer")?;
    assert_eq!(Some("Dana Doctor"), composer["name"].as_str(), "{composer}");
    assert_eq!(
        Some("DR-1"),
        composer
            .pointer("/external_ref/id/value")
            .and_then(serde_json::Value::as_str),
        "{composer}"
    );
    assert_eq!(
        serde_json::json!([{
            "_type": "DV_IDENTIFIER", "issuer": "NORTHLAB", "id": "DR-1", "type": "XCN"
        }]),
        composer["identifiers"],
        "{composer}"
    );
    assert!(
        inbound.warnings().iter().any(|warning| matches!(
            warning.loss(),
            Loss::Defaulted { field } if field == "/composer"
        )),
        "the composer is recorded as defaulted"
    );
    Ok(())
}
