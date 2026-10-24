// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where one literal reference points, and the literals a resource holds.
//!
//! A relative `<Type>/<id>`, or an absolute reference under this server's own
//! base, names a resource the facade serves; an absolute reference under an
//! allow-listed base is fetched from there
//! (<https://hl7.org/fhir/R4/references.html#literal>).

use fhir_types::codec::Value;
use url::Url;

use crate::facade::identity::FhirResourceId;

use super::Policy;
use super::Reason;

/// Where one literal reference points.
#[derive(Debug)]
pub(super) enum Target {
    /// A resource this facade serves.
    Local {
        /// Its type.
        resource_type: String,
        /// Its logical id.
        id: FhirResourceId,
    },
    /// A resource of an allow-listed base.
    Remote {
        /// The base it is fetched from.
        base: String,
        /// The URL it is fetched at.
        url: Url,
        /// Its type.
        resource_type: String,
        /// Its logical id.
        id: String,
    },
}

/// Returns where `literal` points under `policy`, with this server's own
/// `own_base`.
pub(super) fn target(literal: &str, policy: &Policy, own_base: &str) -> Result<Target, Reason> {
    let Ok(url) = Url::parse(literal) else {
        let (resource_type, id) = type_and_id(literal).ok_or(Reason::Unreadable)?;
        return Ok(Target::Local {
            resource_type: resource_type.to_owned(),
            id,
        });
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Reason::Unreadable);
    }
    let full = String::from(url.path());
    let path = full.trim_end_matches('/');
    let (head, id_text) = path.rsplit_once('/').ok_or(Reason::Unreadable)?;
    let (head, resource_type) = head.rsplit_once('/').ok_or(Reason::Unreadable)?;
    let (_, id) = type_and_id(&format!("{resource_type}/{id_text}")).ok_or(Reason::Unreadable)?;
    let mut base = url.clone();
    base.set_path(head);
    let base_text = String::from(trimmed(base.as_str()));
    if base_text == trimmed(own_base) {
        return Ok(Target::Local {
            resource_type: resource_type.to_owned(),
            id,
        });
    }
    // NOTE: no specification governs this: our own design, `engine/references.adoc`
    // names the GET and nothing else, so only an allow-listed base is fetched.
    if !policy.allows(&base_text) {
        return Err(Reason::NotAllowed);
    }
    Ok(Target::Remote {
        base: base_text,
        url,
        resource_type: resource_type.to_owned(),
        id: String::from(id.as_str()),
    })
}

/// Returns the type and id a relative `<Type>/<id>` names.
///
/// R4 names a resource type in upper camel case and an id in
/// `[A-Za-z0-9\-\.]{1,64}` (<https://hl7.org/fhir/R4/resource.html>); a
/// versioned `<Type>/<id>/_history/<vid>` is no such pair.
fn type_and_id(literal: &str) -> Option<(&str, FhirResourceId)> {
    let (resource_type, id) = literal.split_once('/')?;
    let typed = resource_type
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && resource_type.chars().all(|c| c.is_ascii_alphabetic());
    if !typed {
        return None;
    }
    // NOTE: a parse failure IS the answer here: a text outside the R4 id
    // grammar names no resource (<https://hl7.org/fhir/R4/resource.html>).
    let id = FhirResourceId::new(id).ok()?;
    Some((resource_type, id))
}

/// Returns the resource type a literal reference names, when it names one:
/// the segment before its last.
pub(super) fn type_of(literal: &str) -> Option<String> {
    // NOTE: a parse failure IS the answer here: a literal that is no absolute
    // URL is relative (<https://hl7.org/fhir/R4/references.html#literal>).
    let path = Url::parse(literal)
        .map_or_else(|_relative| literal.to_owned(), |url| url.path().to_owned());
    let mut segments = path.trim_end_matches('/').rsplit('/');
    segments.next()?;
    segments.next().map(str::to_owned)
}

/// Returns `text` without a trailing `/`.
pub(super) fn trimmed(text: &str) -> &str {
    text.trim_end_matches('/')
}

/// Returns every `reference` string `value` holds, at any depth, in document
/// order.
pub(super) fn references_in(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect(value, &mut found);
    found
}

/// Adds every `reference` string under `value` to `found`.
fn collect(value: &Value, found: &mut Vec<String>) {
    match *value {
        Value::Object(ref object) => {
            for (key, member) in object {
                if key == "reference"
                    && let Value::String(ref text) = *member
                {
                    found.push(text.clone());
                    continue;
                }
                collect(member, found);
            }
        }
        Value::Array(ref items) => {
            for item in items {
                collect(item, found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::Policy;
    use super::Reason;
    use super::Target;
    use super::target;
    use super::type_of;
    use std::time::Duration;

    /// This server's own base in the cases.
    const OWN: &str = "http://ferrobridge.invalid/fhir";

    fn allowing(base: &str) -> Policy {
        Policy::new(
            vec![base.parse().expect("a legal base")],
            2,
            Duration::from_secs(1),
        )
        .expect("the client builds")
    }

    #[test]
    fn a_relative_reference_and_one_under_the_own_base_are_local() {
        for literal in [
            "Observation/obs-1",
            "http://ferrobridge.invalid/fhir/Observation/obs-1",
        ] {
            let found = target(literal, &Policy::default(), OWN).expect("a readable reference");
            assert!(
                matches!(found, Target::Local { ref resource_type, ref id } if resource_type == "Observation" && id.as_str() == "obs-1"),
                "{literal}: {found:?}"
            );
        }
    }

    #[test]
    fn an_absolute_reference_off_the_allow_list_is_not_fetched() {
        let refused = target(
            "https://elsewhere.invalid/fhir/Observation/obs-1",
            &Policy::default(),
            OWN,
        )
        .expect_err("no base is allowed by default");
        assert_eq!(Reason::NotAllowed, refused);
    }

    #[test]
    fn an_absolute_reference_on_the_allow_list_is_remote() {
        let found = target(
            "https://sender.invalid/fhir/Observation/obs-1",
            &allowing("https://sender.invalid/fhir/"),
            OWN,
        )
        .expect("an allowed base");
        assert!(
            matches!(found, Target::Remote { ref base, .. } if base == "https://sender.invalid/fhir"),
            "{found:?}"
        );
    }

    #[test]
    fn a_reference_that_names_no_type_and_id_is_unreadable() {
        for literal in [
            "Observation/obs-1/_history/2",
            "observation/obs-1",
            "urn:uuid:9c2b",
            "https://sender.invalid/fhir/Observation/obs-1?x=1",
        ] {
            assert_eq!(
                Err(Reason::Unreadable),
                target(literal, &allowing("https://sender.invalid/fhir"), OWN).map(|_| ()),
                "{literal}"
            );
        }
    }

    #[test]
    fn the_type_of_a_reference_is_read_from_its_last_two_segments() {
        assert_eq!(
            Some(String::from("Observation")),
            type_of("Observation/obs-1")
        );
        assert_eq!(
            Some(String::from("Observation")),
            type_of("https://elsewhere.invalid/fhir/Observation/obs-1")
        );
        assert_eq!(None, type_of("obs-1"));
    }
}
