// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SMART App Launch 2.x scopes as the facade reads them: the
//! `ferrohealth-smart` grammar over the R4 resource table, and what one
//! caller's grants reach.
//!
//! The grammar is SMART App Launch 2.2.0, Scopes and Launch Context, §Scopes
//! for requesting FHIR Resources: `<context>/<resource>.<permissions>`, the
//! permissions "a subset of the in-order string `.cruds`", and a request
//! with "undefined or out of order interactions" ignored.

use ferrohealth_smart::resource::Caller;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use ferrohealth_smart::scope::Reach;
use ferrohealth_smart::scope::Scopes;
use fhir_types::r4::schema::SCHEMAS;

/// Returns the clinical scopes `scope` grants over the R4 resource table.
fn parse(scope: &str) -> Scopes {
    Scopes::parse_known(scope, |resource_type| SCHEMAS.is_resource(resource_type))
}

#[test]
fn a_v2_scope_grants_exactly_its_letters() {
    let scopes = parse("system/Condition.crs");
    assert_eq!(
        Some(Reach::Any),
        scopes.reach(Permission::Create, "Condition")
    );
    assert_eq!(
        Some(Reach::Any),
        scopes.reach(Permission::Read, "Condition")
    );
    assert_eq!(
        Some(Reach::Any),
        scopes.reach(Permission::Search, "Condition")
    );
    assert_eq!(None, scopes.reach(Permission::Update, "Condition"));
    assert_eq!(None, scopes.reach(Permission::Delete, "Condition"));
    assert_eq!(None, scopes.reach(Permission::Read, "Observation"));
}

#[test]
fn the_wildcard_names_every_type() {
    let scopes = parse("system/*.r");
    assert_eq!(
        Some(Reach::Any),
        scopes.reach(Permission::Read, "Observation")
    );
    assert_eq!(None, scopes.reach(Permission::Create, "Observation"));
}

#[test]
fn letters_out_of_order_or_repeated_grant_nothing() {
    for token in [
        "system/Condition.rc",
        "system/Condition.rr",
        "system/Condition.",
        "system/Condition.x",
    ] {
        assert!(parse(token).is_empty(), "{token}");
    }
}

#[test]
fn the_v1_suffixes_read_as_their_v2_letters() {
    let read = parse("user/Condition.read");
    assert!(read.reach(Permission::Read, "Condition").is_some());
    assert!(read.reach(Permission::Search, "Condition").is_some());
    assert!(read.reach(Permission::Create, "Condition").is_none());
    let write = parse("user/Condition.write");
    assert!(write.reach(Permission::Create, "Condition").is_some());
    assert!(write.reach(Permission::Update, "Condition").is_some());
    assert!(write.reach(Permission::Delete, "Condition").is_some());
    assert!(write.reach(Permission::Read, "Condition").is_none());
    let all = parse("user/*.*");
    assert!(all.reach(Permission::Delete, "Condition").is_some());
}

#[test]
fn a_patient_scope_reaches_the_compartment_and_a_system_scope_wins() {
    let patient = parse("patient/Condition.rs");
    assert_eq!(
        Some(Reach::Patient),
        patient.reach(Permission::Read, "Condition")
    );
    let both = parse("patient/Condition.rs system/Condition.r");
    assert_eq!(Some(Reach::Any), both.reach(Permission::Read, "Condition"));
}

#[test]
fn other_tokens_and_types_r4_does_not_define_grant_nothing() {
    let scopes = parse(
        "openid fhirUser launch/patient offline_access __ferrobridge.operator system/NotAType.r",
    );
    assert!(scopes.is_empty());
}

#[test]
fn a_constrained_scope_grants_nothing_without_an_evaluator() {
    // NOTE: SMART App Launch 2.2.0, Scopes and Launch Context, §Finer-grained
    // resource constraints; the facade supplies no evaluator, so none admits.
    let scopes = parse("patient/Observation.rs?category=laboratory");
    assert_eq!(None, scopes.reach(Permission::Read, "Observation"));
    assert_eq!(None, scopes.reach(Permission::Search, "Observation"));
}

#[test]
fn a_scopeless_token_reaches_everything_only_when_allowlisted() {
    let allowed = Caller::new(String::from("svc"), None, "openid", None, true);
    assert_eq!(
        Some(Compartment::Any),
        allowed.reach(Permission::Create, "Condition")
    );
    let other = Caller::new(String::from("svc"), None, "", None, false);
    assert_eq!(None, other.reach(Permission::Read, "Condition"));
}

#[test]
fn an_allowlisted_subject_with_scopes_gets_only_its_scopes() {
    let caller = Caller::new(String::from("svc"), None, "system/Condition.r", None, true);
    assert_eq!(None, caller.reach(Permission::Create, "Condition"));
}

#[test]
fn a_patient_scope_without_a_patient_claim_reaches_nothing() {
    let without = Caller::new(String::from("app"), None, "patient/*.rs", None, false);
    assert_eq!(None, without.reach(Permission::Read, "Condition"));
    let with = Caller::new(
        String::from("app"),
        None,
        "patient/*.rs",
        Some(String::from("123")),
        false,
    );
    assert_eq!(
        Some(Compartment::Patient(String::from("123"))),
        with.reach(Permission::Read, "Condition")
    );
}
