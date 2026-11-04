// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two edits the face makes to a mapped Bundle before the hand-off: the
//! configured profile claims and the subject reference to the one `Patient`.

use fhir_types::codec::Value;
use std::collections::BTreeMap;

/// Writes the configured profile into `meta.profile` of every entry whose
/// resource type has one and whose resource claims none.
///
/// The facade selects a program by `meta.profile`, and the v2-to-FHIR maps
/// write none, so this is the one place a message's resources are pointed at
/// the context that maps them (no specification governs this: our own
/// design).
pub fn claim_profiles(bundle: &mut Value, profiles: &BTreeMap<String, String>) {
    if profiles.is_empty() {
        return;
    }
    let Value::Object(document) = bundle else {
        return;
    };
    let Some(Value::Array(entries)) = document.get_mut("entry") else {
        return;
    };
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        let Some(Value::Object(resource)) = entry.get_mut("resource") else {
            continue;
        };
        let Some(profile) = resource
            .get("resourceType")
            .and_then(Value::as_str)
            .and_then(|resource_type| profiles.get(resource_type))
        else {
            continue;
        };
        let profile = Value::String(profile.clone());
        let meta = resource
            .entry(String::from("meta"))
            .or_insert_with(|| Value::Object(BTreeMap::new()));
        let Value::Object(meta) = meta else {
            continue;
        };
        let claimed = meta
            .get("profile")
            .and_then(Value::as_array)
            .is_some_and(|claimed| !claimed.is_empty());
        if !claimed {
            meta.insert(String::from("profile"), Value::Array(vec![profile]));
        }
    }
}

/// Points every entry that names no subject at the message's one `Patient`.
///
/// The guide's maps write no `subject`, and its guidelines leave the
/// relationships between the resources one message builds to the
/// implementer ("the PID segment will create a Patient resource which is
/// then ... referenced by the Encounter resource", `mapping_guidelines.md`).
/// So an entry whose R4 type has a `subject` or a `patient` reference, and
/// which carries neither, references the Bundle's `Patient` by its
/// `fullUrl`; the ingest then reads the person from that entry's identifier.
/// A Bundle with no `Patient`, or with more than one, is left as it is and
/// its entries are refused for naming no subject (no specification governs
/// this: our own design).
pub fn link_subjects(bundle: &mut Value) {
    let Value::Object(document) = bundle else {
        return;
    };
    let Some(Value::Array(entries)) = document.get_mut("entry") else {
        return;
    };
    let patients: Vec<String> = entries
        .iter()
        .filter(|entry| {
            entry
                .get("resource")
                .and_then(|resource| resource.get("resourceType"))
                .and_then(Value::as_str)
                == Some("Patient")
        })
        .filter_map(|entry| entry.get("fullUrl").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let [patient] = patients.as_slice() else {
        return;
    };
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        let Some(Value::Object(resource)) = entry.get_mut("resource") else {
            continue;
        };
        if resource.contains_key("subject") || resource.contains_key("patient") {
            continue;
        }
        let Some(resource_type) = resource.get("resourceType").and_then(Value::as_str) else {
            continue;
        };
        let element = ["subject", "patient"].into_iter().find(|name| {
            fhir_types::r4::schema::SCHEMAS
                .element(&format!("{resource_type}.{name}"))
                .is_some_and(|(_, field)| field.types == ["Reference"] && !field.many)
        });
        if let Some(element) = element {
            let reference =
                BTreeMap::from([(String::from("reference"), Value::String(patient.clone()))]);
            resource.insert(String::from(element), Value::Object(reference));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_profiles, link_subjects};
    use fhir_types::codec::Value;
    use std::collections::BTreeMap;

    fn bundle() -> Value {
        Value::from_serde_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "message",
            "entry": [
                { "fullUrl": "urn:uuid:1", "resource": { "resourceType": "MessageHeader" } },
                { "fullUrl": "urn:uuid:2", "resource": { "resourceType": "Observation" } },
                { "fullUrl": "urn:uuid:3", "resource": {
                    "resourceType": "Observation",
                    "meta": { "profile": ["http://example.org/fhir/StructureDefinition/own"] }
                } }
            ]
        }))
    }

    #[test]
    fn a_configured_profile_is_claimed_only_where_the_run_claimed_none() {
        let mut bundle = bundle();
        let profiles = BTreeMap::from([(
            String::from("Observation"),
            String::from("http://example.org/fhir/StructureDefinition/lab"),
        )]);
        claim_profiles(&mut bundle, &profiles);
        let claimed = |index: usize| {
            bundle
                .get("entry")
                .and_then(Value::as_array)
                .and_then(|entries| entries.get(index))
                .and_then(|entry| entry.get("resource"))
                .and_then(|resource| resource.get("meta"))
                .and_then(|meta| meta.get("profile"))
                .and_then(Value::as_array)
                .and_then(|profiles| profiles.first())
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        assert_eq!(None, claimed(0), "a type with no configured profile");
        assert_eq!(
            Some(String::from(
                "http://example.org/fhir/StructureDefinition/lab"
            )),
            claimed(1)
        );
        assert_eq!(
            Some(String::from(
                "http://example.org/fhir/StructureDefinition/own"
            )),
            claimed(2),
            "a profile the run wrote stands"
        );
    }

    /// Returns the `reference` of `element` on entry `index` of `bundle`.
    fn referenced(bundle: &Value, index: usize, element: &str) -> Option<String> {
        bundle
            .get("entry")
            .and_then(Value::as_array)
            .and_then(|entries| entries.get(index))
            .and_then(|entry| entry.get("resource"))
            .and_then(|resource| resource.get(element))
            .and_then(|reference| reference.get("reference"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    #[test]
    fn an_entry_naming_no_subject_references_the_one_patient() {
        let mut bundle = Value::from_serde_json(serde_json::json!({
            "resourceType": "Bundle",
            "entry": [
                { "fullUrl": "urn:uuid:p", "resource": { "resourceType": "Patient" } },
                { "fullUrl": "urn:uuid:o", "resource": { "resourceType": "Observation" } },
                { "fullUrl": "urn:uuid:i", "resource": { "resourceType": "Immunization" } },
                { "fullUrl": "urn:uuid:h", "resource": { "resourceType": "MessageHeader" } },
                { "fullUrl": "urn:uuid:s", "resource": {
                    "resourceType": "Observation",
                    "subject": { "reference": "Patient/other" }
                } }
            ]
        }));
        link_subjects(&mut bundle);
        assert_eq!(
            Some(String::from("urn:uuid:p")),
            referenced(&bundle, 1, "subject")
        );
        assert_eq!(
            Some(String::from("urn:uuid:p")),
            referenced(&bundle, 2, "patient")
        );
        assert_eq!(
            None,
            referenced(&bundle, 3, "subject"),
            "no subject element"
        );
        assert_eq!(
            Some(String::from("Patient/other")),
            referenced(&bundle, 4, "subject"),
            "a subject the run wrote stands"
        );
    }

    #[test]
    fn two_patients_leave_every_subject_unset() {
        let mut bundle = Value::from_serde_json(serde_json::json!({
            "resourceType": "Bundle",
            "entry": [
                { "fullUrl": "urn:uuid:p", "resource": { "resourceType": "Patient" } },
                { "fullUrl": "urn:uuid:q", "resource": { "resourceType": "Patient" } },
                { "fullUrl": "urn:uuid:o", "resource": { "resourceType": "Observation" } }
            ]
        }));
        link_subjects(&mut bundle);
        assert_eq!(None, referenced(&bundle, 2, "subject"));
    }
}
