// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The author an openEHR composition records, for a form whose authors are
//! the record's ([`super::form::Authorship::Record`]).
//!
//! The MyHealth@EU `Composition` profiles allow a `Practitioner`,
//! `PractitionerRole` or `Organization` author and no `Device`
//! (`composition-hdr-myhealtheu` `Composition.author`), so the software that
//! writes the document is not its author. openEHR RM 1.1.0 names who
//! authored the content: `COMPOSITION.composer`, written as a `Practitioner`
//! when it is a `PARTY_IDENTIFIED` with a name, and otherwise the
//! `EVENT_CONTEXT.health_care_facility`, written as an `Organization`. Both
//! claim their MyHealth@EU Core profile, whose HL7 Europe base requires the
//! name (`practitioner-eu-core`, `organization-eu-core` `name` 1..).

use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::practitioner::Practitioner;
use fhir_types::r4::resource::Resource;
use serde_json::Value;

use crate::document::{DocumentError, claiming};

/// The canonical URL of the MyHealth@EU Core `Practitioner` profile.
pub const MYHEALTHEU_PRACTITIONER: &str =
    "http://fhir.ehdsi.eu/core/StructureDefinition/practitioner-myhealtheu-core";

/// The canonical URL of the MyHealth@EU Core `Organization` profile.
pub const MYHEALTHEU_ORGANIZATION: &str =
    "http://fhir.ehdsi.eu/core/StructureDefinition/organization-myhealtheu-core";

/// The `id` the recorded author is written under.
const AUTHOR_ID: &str = "author";

/// The author `composition`, an openEHR composition in canonical JSON, records.
///
/// It is the composer as a `Practitioner` when the composer is a
/// `PARTY_IDENTIFIED` with a name, else the context's health care facility
/// as an `Organization` when that has a name.
///
/// # Errors
///
/// [`DocumentError::Unauthored`] when the composition names neither, and
/// [`DocumentError::AuthorIdentifier`] for an identifier of the author with
/// no `id`.
pub fn recorded_author(composition: &Value) -> Result<Resource, DocumentError> {
    let composer = composition
        .get("composer")
        .filter(|party| party.get("_type").and_then(Value::as_str) == Some("PARTY_IDENTIFIED"));
    if let Some((name, identifier)) = composer.map(named).transpose()?.flatten() {
        return Ok(Resource::Practitioner(Box::new(Practitioner {
            id: Some(AUTHOR_ID.to_owned()),
            meta: Some(claiming(MYHEALTHEU_PRACTITIONER)),
            identifier,
            name: vec![HumanName {
                text: Some(name.into()),
                ..HumanName::default()
            }],
            ..Practitioner::default()
        })));
    }
    let facility = composition.pointer("/context/health_care_facility");
    if let Some((name, identifier)) = facility.map(named).transpose()?.flatten() {
        return Ok(Resource::Organization(Box::new(Organization {
            id: Some(AUTHOR_ID.to_owned()),
            meta: Some(claiming(MYHEALTHEU_ORGANIZATION)),
            identifier,
            name: Some(name.into()),
            ..Organization::default()
        })));
    }
    Err(DocumentError::Unauthored)
}

/// The name of `party`, a `PARTY_IDENTIFIED`, and its identifiers, each
/// `DV_IDENTIFIER.id` as an identifier value; none for a party with no name.
fn named(party: &Value) -> Result<Option<(&str, Vec<Identifier>)>, DocumentError> {
    let Some(name) = party
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(None);
    };
    let identifiers = party
        .get("identifiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|identifier| {
            identifier
                .get("id")
                .and_then(Value::as_str)
                .map(|id| Identifier {
                    value: Some(id.into()),
                    ..Identifier::default()
                })
                .ok_or(DocumentError::AuthorIdentifier)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((name, identifiers)))
}
