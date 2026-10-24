// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `PARTY_REF` against FHIR `Reference`, the cell `ROLE.performer` maps
//! through in a party context.
//!
//! `ROLE.performer` is a `PARTY_REF` to "the Version container of Actor
//! playing the role" (RM Demographic IM, the `ROLE` class), and the facade
//! serves each party under its `versioned_object_uid`, so the reference
//! `<type>/<uid>` and the `PARTY_REF` whose `HIER_OBJECT_ID` is `<uid>` name
//! one party. The data-type chapter has no row for the pair, so the cell is
//! our own design: no specification governs it. The lens reads and writes
//! the relative literal only; the run resolves any other spelling through
//! the demographics endpoint first and names the resource type the element
//! admits.

use fhir_types::r4::primitives;
use fhir_types::r4::reference::Reference;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;
use crate::tree::element::Table;

/// The name the cell carries in a diagnostic.
pub const PARTY_REF_CELL: &str = "PARTY_REF against Reference";

/// The `OBJECT_REF.namespace` of a party the CDR's demographic store holds.
///
/// `demographic` is the example the BASE `OBJECT_REF` class gives for the
/// attribute, whose names "are not yet standardised".
pub const NAMESPACE: &str = "demographic";

/// The `PARTY_REF.type` the cell writes.
///
/// `ROLE.performer` names an `ACTOR`, and `PARTY_REF` admits an abstract
/// supertype when the concrete class is unknown (BASE, the `PARTY_REF`
/// class), which it is to a FHIR reference.
pub const ACTOR: &str = "ACTOR";

/// `PARTY_REF` against `Reference`, for an element that admits one resource
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartyRefLens<'target> {
    /// The resource type the element's reference names.
    target: &'target str,
}

impl<'target> PartyRefLens<'target> {
    /// Returns the cell for an element whose reference names a `target`.
    #[must_use]
    pub const fn new(target: &'target str) -> Self {
        Self { target }
    }

    /// Returns the refusal of a reference the cell cannot carry.
    fn refuse(attribute: &'static str, found: String, reason: &'static str) -> LensError {
        LensError::Shape {
            cell: PARTY_REF_CELL,
            attribute,
            found,
            reason,
            direction: Direction::FhirToOpenehr,
        }
    }
}

impl Lens<PartyRef, Reference> for PartyRefLens<'_> {
    fn cell(&self) -> &'static str {
        PARTY_REF_CELL
    }

    fn get(&self, source: &PartyRef) -> Result<Reference, LensError> {
        let refuse = |attribute, found, reason| LensError::Shape {
            cell: PARTY_REF_CELL,
            attribute,
            found,
            reason,
            direction: Direction::OpenehrToFhir,
        };
        if source.namespace != NAMESPACE {
            return Err(refuse(
                "PARTY_REF.namespace",
                source.namespace.clone(),
                "only a party of the demographic store is a resource of this server",
            ));
        }
        let ObjectId::HierObjectId(ref id) = source.id else {
            return Err(refuse(
                "PARTY_REF.id",
                format!("{:?}", source.id),
                "a party's version container is named by a HIER_OBJECT_ID",
            ));
        };
        if !fhir_id(id.value()) {
            return Err(refuse(
                "PARTY_REF.id",
                String::from(id.value()),
                "a FHIR id is 1 to 64 of [A-Za-z0-9-.]",
            ));
        }
        Ok(Reference {
            reference: Some(primitives::String::from(format!("{}/{}", self.target, id.value()))),
            ..Reference::default()
        })
    }

    fn put(&self, view: &Reference, _existing: Option<&PartyRef>) -> Result<PartyRef, LensError> {
        let bare = Reference {
            reference: view.reference.clone(),
            ..Reference::default()
        };
        if *view != bare {
            return Err(Self::refuse(
                "Reference",
                String::from("type, identifier, display, id or extension"),
                "a PARTY_REF holds the party's id and nothing else",
            ));
        }
        let literal = text_of(view.reference.as_ref().map(|reference| &reference.value))
            .ok_or_else(|| Self::refuse("Reference.reference", String::new(), "it is absent"))?;
        let id = literal
            .split_once('/')
            .filter(|&(resource_type, id)| resource_type == self.target && fhir_id(id))
            .map(|(_, id)| id)
            .ok_or_else(|| {
                Self::refuse(
                    "Reference.reference",
                    String::from(literal),
                    "it is no <type>/<id> literal of the type the element admits",
                )
            })?;
        Ok(PartyRef {
            namespace: String::from(NAMESPACE),
            r#type: String::from(ACTOR),
            id: ObjectId::HierObjectId(HierObjectId::new(id).map_err(|_malformed| {
                Self::refuse(
                    "Reference.reference",
                    String::from(literal),
                    "its id is no HIER_OBJECT_ID, a UID with an optional extension",
                )
            })?),
        })
    }

    fn fallbacks(&self, _view: &Reference) -> Vec<Fallback> {
        Vec::new()
    }
}

/// The canonical URL prefix of a base resource's `StructureDefinition`
/// (<https://hl7.org/fhir/R4/structuredefinition.html>, the `url` of each
/// resource definition).
const BASE: &str = "http://hl7.org/fhir/StructureDefinition/";

/// Returns the one resource type an element's reference admits, from the
/// `targetProfile` canonicals the element table gives it, `None` when it
/// admits several, any resource or none.
#[must_use]
pub fn target_of<T: Table + ?Sized>(
    table: &T,
    profiles: &'static [&'static str],
) -> Option<&'static str> {
    match *profiles {
        [only] => only
            .strip_prefix(BASE)
            .filter(|name| table.is_resource(name)),
        _ => None,
    }
}

/// Answers whether `id` is a FHIR id, `[A-Za-z0-9\-\.]{1,64}`
/// (<https://hl7.org/fhir/R4/datatypes.html#id>).
fn fhir_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn practitioner(id: &str) -> Reference {
        Reference {
            reference: Some(primitives::String::from(format!("Practitioner/{id}"))),
            ..Reference::default()
        }
    }

    #[test]
    fn a_practitioner_reference_round_trips_through_a_party_ref() {
        let lens = PartyRefLens::new("Practitioner");
        let id = "8f0e7d4c-0000-4000-8000-000000000001";
        let party = lens.put(&practitioner(id), None).expect("it carries");
        assert_eq!(party.namespace, NAMESPACE);
        assert_eq!(party.r#type, ACTOR);
        assert_eq!(
            party.id,
            ObjectId::HierObjectId(HierObjectId::new(id).expect("a UUID is a UID"))
        );
        assert_eq!(lens.get(&party).expect("it reads"), practitioner(id));
    }

    #[test]
    fn a_reference_of_another_type_is_refused() {
        let refused = PartyRefLens::new("Practitioner").put(
            &Reference {
                reference: Some(primitives::String::from("Patient/p-1")),
                ..Reference::default()
            },
            None,
        );
        assert!(
            matches!(refused, Err(LensError::Shape { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn a_display_beside_the_reference_is_refused_rather_than_dropped() {
        let mut view = practitioner("p-1");
        view.display = Some(primitives::String::from("Dr Synthetic"));
        let refused = PartyRefLens::new("Practitioner").put(&view, None);
        assert!(
            matches!(refused, Err(LensError::Shape { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn a_party_ref_outside_the_demographic_namespace_is_refused() {
        let party = PartyRef {
            namespace: String::from("local"),
            r#type: String::from("PERSON"),
            id: ObjectId::HierObjectId(
                HierObjectId::new("8f0e7d4c-0000-4000-8000-000000000001").expect("a UID"),
            ),
        };
        let refused = PartyRefLens::new("Practitioner").get(&party);
        assert!(
            matches!(refused, Err(LensError::Shape { .. })),
            "{refused:?}"
        );
    }
}
