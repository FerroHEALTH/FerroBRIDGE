// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `UID_BASED_ID` against FHIR `Identifier`, and `LOCATABLE_REF` against FHIR
//! `Reference`, going out of openEHR.
//!
//! `LOCATABLE.uid` names one archetyped object and `COMPOSITION.uid` the
//! version of a composition (openEHR RM Common, the `LOCATABLE` class), and
//! `INSTRUCTION_DETAILS.instruction_id` names the `INSTRUCTION` an `ACTION`
//! carries out (openEHR RM EHR, the `INSTRUCTION_DETAILS` class). The
//! data-type chapter has no row for any of them, so both cells are our own
//! design: no specification governs them. Both run going out only, since the
//! CDR assigns every uid a composition carries.
//!
//! The identifier names the object across its versions: an
//! `OBJECT_VERSION_ID` gives its `object_id`, the versioned object (BASE, the
//! `OBJECT_VERSION_ID` class), and a `HIER_OBJECT_ID` gives its whole value.
//! A root with no extension is the URI the identifier value holds, under the
//! `urn:ietf:rfc:3986` system
//! (<https://hl7.org/fhir/R4/terminologies-systems.html>); a root with an
//! extension is the namespace of the extension, the shape the R4 `Identifier`
//! RIM mapping gives an `II` root and extension
//! (<https://hl7.org/fhir/R4/datatypes-mappings.html#Identifier>). A UUID
//! root is the lowercase `urn:uuid:` URI and an ISO OID root the `urn:oid:`
//! URI (<https://hl7.org/fhir/R4/datatypes.html#uri>); an internet id has no
//! URI form, so it is refused.

use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::reference::Reference;
use openehr_base::v1_3::base_types::identification::locatable_ref::LocatableRef;
use openehr_base::v1_3::base_types::identification::uid::Uid;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::model::ast::keyword::Direction;

/// The name the `UID_BASED_ID` cell carries in a diagnostic.
pub const UID_CELL: &str = "UID_BASED_ID against Identifier";

/// The name the `LOCATABLE_REF` cell carries in a diagnostic.
pub const LOCATABLE_REF_CELL: &str = "LOCATABLE_REF against Reference";

/// The identifier system whose values are URIs
/// (<https://hl7.org/fhir/R4/terminologies-systems.html>).
pub const URI_SYSTEM: &str = "urn:ietf:rfc:3986";

/// `UID_BASED_ID` against `Identifier`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UidLens;

impl Lens<UidBasedId, Identifier> for UidLens {
    fn cell(&self) -> &'static str {
        UID_CELL
    }

    fn get(&self, source: &UidBasedId) -> Result<Identifier, LensError> {
        identifier_of(UID_CELL, source)
    }

    fn put(
        &self,
        _view: &Identifier,
        _existing: Option<&UidBasedId>,
    ) -> Result<UidBasedId, LensError> {
        Err(one_way(UID_CELL))
    }

    fn fallbacks(&self, _view: &Identifier) -> Vec<Fallback> {
        Vec::new()
    }
}

/// `LOCATABLE_REF` against `Reference`, for an element that admits one
/// resource type.
///
/// The reference names its target by the identifier [`UidLens`] gives the
/// version container the `LOCATABLE_REF` points into. The `path` within it
/// has no `Reference` counterpart and is not carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocatableRefLens<'target> {
    /// The resource type the element's reference names.
    target: &'target str,
}

impl<'target> LocatableRefLens<'target> {
    /// Returns the cell for an element whose reference names a `target`.
    #[must_use]
    pub const fn new(target: &'target str) -> Self {
        Self { target }
    }
}

impl Lens<LocatableRef, Reference> for LocatableRefLens<'_> {
    fn cell(&self) -> &'static str {
        LOCATABLE_REF_CELL
    }

    fn get(&self, source: &LocatableRef) -> Result<Reference, LensError> {
        Ok(Reference {
            r#type: Some(Uri::from(String::from(self.target))),
            identifier: Some(Box::new(identifier_of(LOCATABLE_REF_CELL, &source.id)?)),
            ..Reference::default()
        })
    }

    fn put(
        &self,
        _view: &Reference,
        _existing: Option<&LocatableRef>,
    ) -> Result<LocatableRef, LensError> {
        Err(one_way(LOCATABLE_REF_CELL))
    }

    fn fallbacks(&self, _view: &Reference) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the identifier of the object `uid` names, across its versions.
fn identifier_of(cell: &'static str, uid: &UidBasedId) -> Result<Identifier, LensError> {
    let (root, extension) = match *uid {
        UidBasedId::ObjectVersionId(ref version) => (version.object_id(), ""),
        UidBasedId::HierObjectId(ref object) => (object.root(), object.extension()),
    };
    let namespace = match root {
        Uid::Uuid(_) => format!("urn:uuid:{}", root.value()),
        Uid::IsoOid(_) => format!("urn:oid:{}", root.value()),
        Uid::InternetId(_) => {
            return Err(LensError::Shape {
                cell,
                attribute: "UID_BASED_ID.root",
                found: root.value().into_owned(),
                reason: "an INTERNET_ID has no URI form for Identifier.system",
                direction: Direction::OpenehrToFhir,
            });
        }
    };
    let (system, value) = if extension.is_empty() {
        (String::from(URI_SYSTEM), namespace)
    } else {
        (namespace, String::from(extension))
    };
    Ok(Identifier {
        system: Some(Uri::from(system)),
        value: Some(primitives::String::from(value)),
        ..Identifier::default()
    })
}

/// Returns the refusal of a cell run going into openEHR.
const fn one_way(cell: &'static str) -> LensError {
    LensError::OneWay {
        cell,
        defined: Direction::OpenehrToFhir,
        direction: Direction::FhirToOpenehr,
    }
}

#[cfg(test)]
mod tests {
    use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
    use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

    use super::*;

    fn identifier(system: &str, value: &str) -> Identifier {
        Identifier {
            system: Some(Uri::from(String::from(system))),
            value: Some(primitives::String::from(value)),
            ..Identifier::default()
        }
    }

    #[test]
    fn a_version_names_its_versioned_object() {
        let uid = UidBasedId::ObjectVersionId(
            ObjectVersionId::new("5F0C2A9E-0000-4000-8000-000000000001::synthetic.example.org::2")
                .expect("an OBJECT_VERSION_ID"),
        );
        assert_eq!(
            UidLens.get(&uid).expect("it reads"),
            identifier(URI_SYSTEM, "urn:uuid:5f0c2a9e-0000-4000-8000-000000000001")
        );
    }

    #[test]
    fn an_object_id_with_an_extension_is_the_extension_in_its_root_namespace() {
        let uid = UidBasedId::HierObjectId(
            HierObjectId::new("2.16.840.1.113883.2.4.6.3::rx-0001").expect("a HIER_OBJECT_ID"),
        );
        assert_eq!(
            UidLens.get(&uid).expect("it reads"),
            identifier("urn:oid:2.16.840.1.113883.2.4.6.3", "rx-0001")
        );
    }

    #[test]
    fn an_internet_id_root_is_refused() {
        let uid = UidBasedId::HierObjectId(
            HierObjectId::new("example.org").expect("an INTERNET_ID is a UID"),
        );
        assert!(matches!(UidLens.get(&uid), Err(LensError::Shape { .. })));
    }

    #[test]
    fn neither_cell_runs_into_openehr() {
        assert!(matches!(
            UidLens.put(&Identifier::default(), None),
            Err(LensError::OneWay { .. })
        ));
        assert!(matches!(
            LocatableRefLens::new("MedicationRequest").put(&Reference::default(), None),
            Err(LensError::OneWay { .. })
        ));
    }

    #[test]
    fn a_locatable_ref_names_its_version_container_by_identifier() {
        let order = LocatableRef {
            namespace: String::from("local"),
            r#type: String::from("INSTRUCTION"),
            id: UidBasedId::ObjectVersionId(
                ObjectVersionId::new(
                    "8849182c-82ad-4088-a07f-48ead4180515::synthetic.example.org::1",
                )
                .expect("an OBJECT_VERSION_ID"),
            ),
            path: Some(String::from(
                "/content[openEHR-EHR-INSTRUCTION.medication_order.v3]",
            )),
        };
        let reference = LocatableRefLens::new("MedicationRequest")
            .get(&order)
            .expect("it reads");
        assert_eq!(
            reference.r#type.map(|uri| uri.value),
            Some(Some(String::from("MedicationRequest")))
        );
        assert_eq!(
            reference.identifier.as_deref(),
            Some(&identifier(
                URI_SYSTEM,
                "urn:uuid:8849182c-82ad-4088-a07f-48ead4180515"
            ))
        );
    }
}
