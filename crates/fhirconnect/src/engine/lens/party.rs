// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `PARTY_IDENTIFIED` against FHIR `Reference`, and `DV_IDENTIFIER` against
//! `Identifier`.
//!
//! The two cells are
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/PARTY_IDENTIFIED.adoc`
//! and `.../DV_IDENTIFIER.adoc`. `display` pairs with `name`, `identifier`
//! with `identifiers`, and `type` has no openEHR counterpart.
//!
//! `reference` is the demographics row: the page sends it to the demographics
//! server and puts the resolved resource's identifier in `identifiers`. That
//! resolution is a call, so the lens neither invents an identifier for it nor
//! drops it: [`PartyLens::deferred`] hands the reference to the run, which
//! resolves it through the demographics endpoint
//! ([`crate::engine::demographics`]).

use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::reference::Reference;
use openehr_base::containers::NonEmptyVec;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;
use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `Reference` cell carries in a diagnostic.
pub const PARTY_CELL: &str = "PARTY_IDENTIFIED against Reference";

/// The name the `Identifier` cell carries in a diagnostic.
pub const IDENTIFIER_CELL: &str = "DV_IDENTIFIER against Identifier";

/// The separator the specification writes between a system and a code.
///
/// `String.adoc` §Coding and §Identifier both encode the pair as
/// `system::value`, "since it represents a unique combination that rarely
/// appears in text".
pub const SEPARATOR: &str = "::";

/// `PARTY_IDENTIFIED` against `Reference`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartyLens;

impl PartyLens {
    /// Returns the reference string the run resolves through the
    /// demographics endpoint.
    ///
    /// A `Reference` that names a resource carries no identifier of its own
    /// until the endpoint resolves it, so the lens leaves it to the run.
    #[must_use]
    pub fn deferred(view: &Reference) -> Option<&str> {
        text_of(view.reference.as_ref().map(|reference| &reference.value))
    }
}

impl Lens<PartyIdentifiedData, Reference> for PartyLens {
    fn cell(&self) -> &'static str {
        PARTY_CELL
    }

    fn get(&self, source: &PartyIdentifiedData) -> Result<Reference, LensError> {
        let identifiers = source.identifiers.as_deref().unwrap_or_default();
        let first = identifiers.first();
        if identifiers.len() > 1 {
            return Err(LensError::Shape {
                cell: PARTY_CELL,
                attribute: "PARTY_IDENTIFIED.identifiers",
                found: identifiers.len().to_string(),
                reason: "Reference.identifier is 0..1, so the extra identifiers have nowhere to go",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(Reference {
            identifier: first
                .map(|identifier| IdentifierLens.get(identifier))
                .transpose()?
                .map(Box::new),
            display: source.name.clone().map(primitives::String::from),
            ..Reference::default()
        })
    }

    fn put(
        &self,
        view: &Reference,
        existing: Option<&PartyIdentifiedData>,
    ) -> Result<PartyIdentifiedData, LensError> {
        let held = existing
            .and_then(|party| party.identifiers.as_deref())
            .and_then(<[DvIdentifier]>::first);
        let identifiers = view
            .identifier
            .as_deref()
            .map(|identifier| IdentifierLens.put(identifier, held))
            .transpose()?;
        Ok(PartyIdentifiedData {
            external_ref: existing.and_then(|party| party.external_ref.clone()),
            name: text_of(view.display.as_ref().map(|display| &display.value)).map(String::from),
            identifiers: NonEmptyVec::new(identifiers.into_iter().collect::<Vec<DvIdentifier>>())
                .ok(),
        })
    }

    fn fallbacks(&self, _view: &Reference) -> Vec<Fallback> {
        Vec::new()
    }
}

/// `DV_IDENTIFIER` against `Identifier`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IdentifierLens;

impl Lens<DvIdentifier, Identifier> for IdentifierLens {
    fn cell(&self) -> &'static str {
        IDENTIFIER_CELL
    }

    fn get(&self, source: &DvIdentifier) -> Result<Identifier, LensError> {
        if source.id.is_empty() {
            return Err(LensError::Missing {
                cell: IDENTIFIER_CELL,
                attribute: "Identifier.value",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(Identifier {
            system: source.issuer.clone().map(Uri::from),
            value: Some(primitives::String::from(source.id.clone())),
            r#type: source.r#type.as_deref().map(concept_of),
            assigner: source.assigner.as_deref().map(reference_of).map(Box::new),
            ..Identifier::default()
        })
    }

    fn put(
        &self,
        view: &Identifier,
        existing: Option<&DvIdentifier>,
    ) -> Result<DvIdentifier, LensError> {
        let id =
            text_of(view.value.as_ref().map(|value| &value.value)).ok_or(LensError::Missing {
                cell: IDENTIFIER_CELL,
                attribute: "DV_IDENTIFIER.id",
                direction: Direction::FhirToOpenehr,
            })?;
        Ok(DvIdentifier {
            // NOTE: no specification governs this: our own design, the
            // DV_IDENTIFIER table has no row for `Identifier.system`, and the
            // RM calls `issuer` the authority that issues the kind of id.
            issuer: text_of(view.system.as_ref().map(|system| &system.value)).map(String::from),
            assigner: view.assigner.as_deref().and_then(assigner_of).or_else(|| {
                existing
                    .filter(|_held| view.assigner.is_none())
                    .and_then(|held| held.assigner.clone())
            }),
            id: String::from(id),
            r#type: view.r#type.as_ref().and_then(coded_of),
        })
    }

    fn fallbacks(&self, _view: &Identifier) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the `CodeableConcept` a `DV_IDENTIFIER.type` string stands for.
///
/// The RM types `type` as a free `String` (RM Release 1.1.0, Data Types,
/// `DV_IDENTIFIER`), and `String.adoc` §Coding maps the string to `code`,
/// reading `system` and `code` only when it contains `::`, so a plain type
/// such as `login` is a coding with a code and no system.
fn concept_of(kind: &str) -> fhir_types::r4::codeable_concept::CodeableConcept {
    let (system, code) = match kind.split_once(SEPARATOR) {
        Some((system, code)) => (Some(Uri::from(system)), code),
        None => (None, kind),
    };
    fhir_types::r4::codeable_concept::CodeableConcept {
        coding: vec![fhir_types::r4::coding::Coding {
            system,
            code: Some(primitives::Code::from(code)),
            ..fhir_types::r4::coding::Coding::default()
        }],
        ..fhir_types::r4::codeable_concept::CodeableConcept::default()
    }
}

/// Returns the `DV_IDENTIFIER.type` string a `CodeableConcept` stands for.
///
/// `String.adoc` §Coding takes the first coding and writes `system::code`, or
/// the bare `code` when the coding names no system; a concept with no coded
/// code has nothing to encode.
fn coded_of(concept: &fhir_types::r4::codeable_concept::CodeableConcept) -> Option<String> {
    let coding = concept.coding.first()?;
    let code = text_of(coding.code.as_ref().map(|code| &code.value))?;
    Some(
        match text_of(coding.system.as_ref().map(|system| &system.value)) {
            Some(system) => format!("{system}{SEPARATOR}{code}"),
            None => String::from(code),
        },
    )
}

/// Returns the `Reference` an assigner string stands for.
///
/// `String.adoc` §References makes the identifier form the default, so a
/// string carrying `::` becomes an identifier, a literal reference stays the
/// reference string the page allows, and any other text is the reference's
/// `display`.
// NOTE: FHIRconnect 1.0.0 `String.adoc` is silent on free text; R4 references.html#literal admits
// only a URL in `Reference.reference`, so text goes to `display`: our own design (#2021).
fn reference_of(assigner: &str) -> Reference {
    match assigner.split_once(SEPARATOR) {
        Some((system, value)) => Reference {
            identifier: Some(Box::new(Identifier {
                system: Some(Uri::from(system)),
                value: Some(primitives::String::from(value)),
                ..Identifier::default()
            })),
            ..Reference::default()
        },
        None if literal(assigner) => Reference {
            reference: Some(primitives::String::from(assigner)),
            ..Reference::default()
        },
        None => Reference {
            display: Some(primitives::String::from(assigner)),
            ..Reference::default()
        },
    }
}

/// Whether `text` is a literal reference: a relative `[type]/[id]`, an
/// absolute URL or a URN (<https://hl7.org/fhir/R4/references.html#literal>).
fn literal(text: &str) -> bool {
    if text.starts_with("urn:") || text.contains("://") {
        return !text.chars().any(char::is_whitespace);
    }
    let Some((kind, id)) = text.split_once('/') else {
        return false;
    };
    kind.chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && kind
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
}

/// Returns the assigner string a `Reference` stands for: its identifier, its
/// reference or its display, in that order.
fn assigner_of(assigner: &Reference) -> Option<String> {
    if let Some(identifier) = assigner.identifier.as_deref() {
        let system = text_of(identifier.system.as_ref().map(|system| &system.value));
        let value = text_of(identifier.value.as_ref().map(|value| &value.value));
        if let (Some(system), Some(value)) = (system, value) {
            return Some(format!("{system}{SEPARATOR}{value}"));
        }
    }
    text_of(
        assigner
            .reference
            .as_ref()
            .map(|reference| &reference.value),
    )
    .or_else(|| text_of(assigner.display.as_ref().map(|display| &display.value)))
    .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::IdentifierLens;
    use super::PartyLens;
    use crate::engine::lens::Lens;
    use fhir_types::r4::identifier::Identifier;
    use fhir_types::r4::primitives;
    use fhir_types::r4::primitives::Uri;
    use fhir_types::r4::reference::Reference;
    use openehr_base::containers::NonEmptyVec;
    use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;
    use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;
    use proptest::prop_compose;
    use proptest::proptest;

    prop_compose! {
        /// A `DV_IDENTIFIER` inside the subset the cell carries losslessly.
        fn identifier()(
            id in "[A-Za-z0-9-]{1,12}",
            issuer in proptest::option::of("[a-z]{1,6}://[a-z.]{1,10}"),
            kind in proptest::option::of("([a-z]{1,6}://[a-z.]{1,8}::)?[A-Za-z]{1,8}"),
            assigner in proptest::option::of(
                "[a-z]{1,6}://[a-z.]{1,8}::[A-Za-z0-9]{1,6}|Organization/[a-z0-9]{1,8}|[A-Za-z][a-z ]{0,12}[a-z]",
            ),
        ) -> DvIdentifier {
            DvIdentifier { issuer, assigner, id, r#type: kind }
        }
    }

    prop_compose! {
        /// A `PARTY_IDENTIFIED` carrying at most one identifier.
        fn party()(
            name in proptest::option::of("[A-Za-z ]{1,12}"),
            identifier in proptest::option::of(identifier()),
        ) -> PartyIdentifiedData {
            PartyIdentifiedData {
                external_ref: None,
                name,
                identifiers: NonEmptyVec::new(identifier.into_iter().collect::<Vec<DvIdentifier>>())
                    .ok(),
            }
        }
    }

    proptest! {
        #[test]
        fn identifier_get_put(source in identifier()) {
            let view: Identifier = IdentifierLens.get(&source).expect("an id is present");
            let back = IdentifierLens.put(&view, Some(&source)).expect("the view carries a value");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn party_get_put(source in party()) {
            let view: Reference = PartyLens.get(&source).expect("the generated subset is carried");
            let back = PartyLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn identifier_put_get(source in identifier()) {
            let view: Identifier = IdentifierLens.get(&source).expect("an id is present");
            let back = IdentifierLens.put(&view, None).expect("the view carries a value");
            let again: Identifier = IdentifierLens.get(&back).expect("an id is present");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn party_put_get(source in party()) {
            let view: Reference = PartyLens.get(&source).expect("the generated subset is carried");
            let back = PartyLens.put(&view, None).expect("the view is complete");
            let again: Reference = PartyLens.get(&back).expect("the generated subset is carried");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn the_display_and_the_name_are_one_pair() {
        let source = PartyIdentifiedData {
            external_ref: None,
            name: Some(String::from("Ward B")),
            identifiers: None,
        };
        let view: Reference = PartyLens.get(&source).expect("a name travels");
        assert_eq!(
            view.display
                .as_ref()
                .and_then(|display| display.value.as_deref()),
            Some("Ward B"),
            "the name fills the display"
        );
        assert_eq!(
            PartyLens
                .put(&view, Some(&source))
                .expect("the view is complete"),
            source,
            "the display fills the name"
        );
    }

    #[test]
    fn a_free_text_assigner_is_the_display_and_a_literal_one_the_reference() {
        let assigner = |text: &str| {
            IdentifierLens
                .get(&DvIdentifier {
                    issuer: None,
                    assigner: Some(String::from(text)),
                    id: String::from("SYN-1"),
                    r#type: None,
                })
                .expect("an id is present")
                .assigner
                .map(|assigner| *assigner)
                .unwrap_or_default()
        };
        let text = |value: Option<&primitives::String>| value.and_then(|value| value.value.clone());
        let free = assigner("Synthetic assigner");
        assert_eq!(
            text(free.display.as_ref()).as_deref(),
            Some("Synthetic assigner")
        );
        assert_eq!(free.reference, None, "free text is no literal reference");
        for literal in [
            "Organization/org-1",
            "https://fhir.example.org/Organization/org-1",
            "urn:uuid:5a1c3e20-0000-4000-8000-000000002021",
        ] {
            let held = assigner(literal);
            assert_eq!(text(held.reference.as_ref()).as_deref(), Some(literal));
            assert_eq!(held.display, None, "{literal}");
        }
        assert!(assigner("Organization/").reference.is_none());
        assert!(assigner("organization/org-1").reference.is_none());
    }

    #[test]
    fn a_reference_string_is_left_for_the_facade_to_resolve() {
        let view = Reference {
            reference: Some(primitives::String::from("Organization/ward-b")),
            ..Reference::default()
        };
        assert_eq!(
            PartyLens::deferred(&view),
            Some("Organization/ward-b"),
            "the reference is reported for demographics resolution"
        );
        let source: PartyIdentifiedData = PartyLens
            .put(&view, None)
            .expect("a reference alone is complete");
        assert!(
            source.identifiers.is_none(),
            "the lens invents no identifier for an unresolved reference"
        );
    }

    #[test]
    fn several_identifiers_do_not_fit_one_reference() {
        let source = PartyIdentifiedData {
            external_ref: None,
            name: None,
            identifiers: NonEmptyVec::new(vec![
                DvIdentifier {
                    issuer: None,
                    assigner: None,
                    id: String::from("one"),
                    r#type: None,
                },
                DvIdentifier {
                    issuer: None,
                    assigner: None,
                    id: String::from("two"),
                    r#type: None,
                },
            ])
            .ok(),
        };
        let error = <PartyLens as Lens<PartyIdentifiedData, Reference>>::get(&PartyLens, &source)
            .expect_err("Reference.identifier is 0..1");
        assert!(
            error.to_string().contains("PARTY_IDENTIFIED.identifiers"),
            "the refusal names the attribute: {error}"
        );
    }

    #[test]
    fn a_coded_type_travels_as_system_and_code() {
        let source = DvIdentifier {
            issuer: Some(String::from("http://example.org/ids")),
            assigner: None,
            id: String::from("A-1"),
            r#type: Some(String::from("http://example.org/kinds::MR")),
        };
        let view: Identifier = IdentifierLens.get(&source).expect("an id is present");
        assert_eq!(
            view.r#type
                .as_ref()
                .and_then(|kind| kind.coding.first())
                .and_then(|coding| coding.code.as_ref())
                .and_then(|code| code.value.as_deref()),
            Some("MR"),
            "the code half of the pair becomes the coding code"
        );
        assert_eq!(
            view.system
                .as_ref()
                .and_then(|system| system.value.as_deref()),
            Some("http://example.org/ids"),
            "the issuer becomes the identifier system"
        );
    }

    #[test]
    fn a_plain_type_reads_as_a_code_with_no_system() {
        // String.adoc §Coding: the openEHR string is the `code`, and a system only when it holds `::`.
        let source = DvIdentifier {
            issuer: Some(String::from("http://example.org/ids")),
            assigner: None,
            id: String::from("SYN-0001"),
            r#type: Some(String::from("login")),
        };
        let view: Identifier = IdentifierLens.get(&source).expect("a plain type is legal");
        let coding = view
            .r#type
            .as_ref()
            .and_then(|kind| kind.coding.first())
            .expect("the type becomes a coding");
        assert_eq!(
            coding.code.as_ref().and_then(|code| code.value.as_deref()),
            Some("login")
        );
        assert_eq!(coding.system, None, "a plain type names no system");
    }

    #[test]
    fn a_coding_without_a_system_writes_its_bare_code() {
        let view = Identifier {
            value: Some(primitives::String::from("SYN-0001")),
            r#type: Some(fhir_types::r4::codeable_concept::CodeableConcept {
                coding: vec![fhir_types::r4::coding::Coding {
                    code: Some(primitives::Code::from("login")),
                    ..fhir_types::r4::coding::Coding::default()
                }],
                ..fhir_types::r4::codeable_concept::CodeableConcept::default()
            }),
            ..Identifier::default()
        };
        let written = IdentifierLens.put(&view, None).expect("a value is present");
        assert_eq!(written.r#type.as_deref(), Some("login"));
    }

    #[test]
    fn an_identifier_without_a_value_is_refused() {
        let view = Identifier {
            system: Some(Uri::from("http://example.org/ids")),
            ..Identifier::default()
        };
        let error = IdentifierLens
            .put(&view, None)
            .expect_err("DV_IDENTIFIER.id is mandatory");
        assert!(
            error.to_string().contains("DV_IDENTIFIER.id"),
            "the refusal names the mandatory attribute: {error}"
        );
    }
}
