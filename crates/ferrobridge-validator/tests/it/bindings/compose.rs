// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Value set membership read from the package set: listed concepts, whole
//! code systems with their hierarchy, filters, excludes, imports, version
//! pins and expansions (valueset.html §4.9.5 and §4.9.8).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::bindings::COLORS;
use crate::bindings::VS;
use crate::bindings::binding;
use crate::bindings::binding_listings;
use crate::bindings::bound;
use crate::bindings::check;
use crate::bindings::value_set;
use crate::support::TestResult;
use crate::support::locations;

/// Validates a Patient whose gender is `code` against a required binding to
/// the value set `set`, with `extra` beside it.
fn gender(set: &serde_json::Value, extra: &[serde_json::Value], code: &str) -> TestResult<Report> {
    let id = set["id"].as_str().unwrap_or_default();
    let definition = bound("Patient.gender", "code", "1", &binding("required", id));
    let mut resources = vec![set.clone()];
    resources.extend(extra.iter().cloned());
    check(
        &definition,
        &resources,
        &json!({"resourceType": "Patient", "gender": code}),
    )
}

/// Answers whether `code` is accepted by a required binding to `set`.
fn accepts(set: &serde_json::Value, code: &str) -> TestResult<bool> {
    let report = gender(set, &[], code)?;
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    Ok(locations(&report, MessageId::NotInValueSet).is_empty())
}

/// A value set over [`COLORS`] with one filter.
fn filtered(id: &str, property: &str, op: &str, value: &str) -> serde_json::Value {
    value_set(
        id,
        &json!({"include": [{"system": COLORS,
                             "filter": [{"property": property, "op": op, "value": value}]}]}),
    )
}

#[test]
fn a_whole_code_system_holds_its_nested_codes() -> TestResult {
    let every = value_set("every", &json!({"include": [{"system": COLORS}]}));
    assert!(accepts(&every, "navy")?);
    assert!(!accepts(&every, "purple")?);
    Ok(())
}

#[test]
fn an_is_a_filter_selects_the_concept_and_its_descendants() -> TestResult {
    let blues = filtered("blues", "concept", "is-a", "blue");
    assert!(accepts(&blues, "blue")?);
    assert!(accepts(&blues, "navy")?);
    assert!(!accepts(&blues, "red")?);
    Ok(())
}

#[test]
fn a_descendent_of_filter_leaves_the_concept_out() -> TestResult {
    let below = filtered("below-blue", "concept", "descendent-of", "blue");
    assert!(!accepts(&below, "blue")?);
    assert!(accepts(&below, "navy")?);
    Ok(())
}

#[test]
fn an_is_not_a_filter_selects_what_is_not_subsumed() -> TestResult {
    let not_blue = filtered("not-blue", "concept", "is-not-a", "blue");
    assert!(accepts(&not_blue, "red")?);
    assert!(!accepts(&not_blue, "navy")?);
    Ok(())
}

#[test]
fn property_filters_select_by_value() -> TestResult {
    assert!(accepts(&filtered("warm", "hue", "=", "warm"), "red")?);
    assert!(!accepts(&filtered("warm", "hue", "=", "warm"), "blue")?);
    assert!(accepts(&filtered("hues", "hue", "in", "hot, warm"), "red")?);
    assert!(!accepts(
        &filtered("not-cold", "hue", "not-in", "cold"),
        "navy"
    )?);
    Ok(())
}

#[test]
fn an_exclude_removes_codes_the_includes_hold() -> TestResult {
    let but_green = value_set(
        "but-green",
        &json!({"include": [{"system": COLORS}],
                "exclude": [{"system": COLORS, "concept": [{"code": "green"}]}]}),
    );
    assert!(accepts(&but_green, "red")?);
    assert!(!accepts(&but_green, "green")?);
    Ok(())
}

#[test]
fn imported_value_sets_intersect() -> TestResult {
    let imported = value_set(
        "imported",
        &json!({"include": [{"valueSet": [format!("{VS}listed"), format!("{VS}every-color")]}]}),
    );
    assert!(accepts(&imported, "red")?);
    assert!(
        !accepts(&imported, "navy")?,
        "navy is in every-color and not in listed; valueset.html §4.9.5 asks for all of them"
    );
    Ok(())
}

#[test]
fn a_subsumption_filter_over_an_undeclared_hierarchy_is_named() -> TestResult {
    let flat_url = "http://example.org/fhir/CodeSystem/flat";
    let flat = json!({
        "resourceType": "CodeSystem", "id": "flat", "url": flat_url, "status": "active",
        "content": "complete", "concept": [{"code": "a", "concept": [{"code": "a1"}]}]
    });
    let set = value_set(
        "flat-a",
        &json!({"include": [{"system": flat_url,
                             "filter": [{"property": "concept", "op": "is-a", "value": "a"}]}]}),
    );
    let report = gender(&set, &[flat], "a1")?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::HierarchyNotDeclared(flat_url.to_owned())]
    );
    assert!(report.is_valid());
    Ok(())
}

#[test]
fn a_regex_filter_is_named_and_decides_nothing() -> TestResult {
    let report = gender(&filtered("pattern", "hue", "regex", "w.*"), &[], "red")?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::FilterNotEvaluated("hue regex w.*".to_owned())]
    );
    Ok(())
}

#[test]
fn a_code_a_fragment_does_not_carry_is_undecided() -> TestResult {
    let part_url = "http://example.org/fhir/CodeSystem/part";
    let part = json!({
        "resourceType": "CodeSystem", "id": "part", "url": part_url, "status": "active",
        "content": "fragment", "concept": [{"code": "known"}]
    });
    let set = value_set("part", &json!({"include": [{"system": part_url}]}));
    let report = gender(&set, std::slice::from_ref(&part), "known")?;
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let report = gender(&set, &[part], "unknown")?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::CodeSystemIncomplete(part_url.to_owned())]
    );
    Ok(())
}

#[test]
fn a_pinned_version_is_held() -> TestResult {
    let pinned = value_set(
        "pinned",
        &json!({"include": [{"system": COLORS, "version": "1.0.0"}]}),
    );
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &binding("required", "pinned"),
    );
    let tag = |version: &str| {
        json!({"resourceType": "Patient",
               "meta": {"tag": [{"system": COLORS, "version": version, "code": "red"}]}})
    };
    let extra = [pinned];
    assert!(check(&definition, &extra, &tag("1.0.0"))?.is_valid());
    let report = check(&definition, &extra, &tag("2.0.0"))?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.meta.tag[0]"]
    );
    Ok(())
}

#[test]
fn a_pinned_version_the_set_does_not_carry_is_named() -> TestResult {
    let set = value_set(
        "missing-version",
        &json!({"include": [{"system": COLORS, "version": "9.9.9"}]}),
    );
    let report = gender(&set, &[], "red")?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::CodeSystemNotFound(format!("{COLORS}|9.9.9"))]
    );
    Ok(())
}

#[test]
fn an_expansion_carried_whole_answers_without_a_compose() -> TestResult {
    let expanded = json!({
        "resourceType": "ValueSet", "id": "expanded", "url": format!("{VS}expanded"),
        "status": "active",
        "expansion": {"timestamp": "2026-01-01T00:00:00Z", "total": 1,
                      "contains": [{"system": COLORS, "code": "red"}]}
    });
    assert!(accepts(&expanded, "red")?);
    assert!(!accepts(&expanded, "blue")?);
    Ok(())
}

#[test]
fn a_value_set_without_inactive_codes_leaves_a_retired_code_out() -> TestResult {
    let statuses_url = "http://example.org/fhir/CodeSystem/statuses";
    let statuses = json!({
        "resourceType": "CodeSystem", "id": "statuses", "url": statuses_url, "status": "active",
        "content": "complete",
        "property": [{"code": "status", "type": "code"}],
        "concept": [
            {"code": "current"},
            {"code": "old", "property": [{"code": "status", "valueCode": "retired"}]}
        ]
    });
    let set = value_set(
        "active-only",
        &json!({"inactive": false, "include": [{"system": statuses_url}]}),
    );
    let report = gender(&set, std::slice::from_ref(&statuses), "current")?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = gender(&set, &[statuses], "old")?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.gender"]
    );
    Ok(())
}
