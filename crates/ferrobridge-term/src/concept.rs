// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The values the operations answer with, in one shape for every wire
//! version; [`Designation`] is shared with the `ValueSet/$expand` entries.
//!
//! R4 and R4B declare the same `out` parameters for `CodeSystem/$lookup`,
//! `ConceptMap/$translate` and `ValueSet/$validate-code`. STU3 declares a
//! subset of them: its `property.value` is a `code`, and its
//! `$validate-code` has no `issues`. One set of types carries all three, and
//! the release decides which generated module reads the `Parameters`.

/// A code in a code system, the neutral form of a FHIR `Coding`
/// (<https://hl7.org/fhir/R4/datatypes.html#Coding>).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Concept {
    /// `Coding.system`.
    pub system: Option<String>,
    /// `Coding.version`.
    pub version: Option<String>,
    /// `Coding.code`.
    pub code: Option<String>,
    /// `Coding.display`.
    pub display: Option<String>,
}

/// One alternative representation of a concept, as the `designation` out
/// parameter of `CodeSystem/$lookup` carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Designation {
    /// The language this designation is defined for.
    pub language: Option<String>,
    /// How the designation is used.
    pub usage: Option<Concept>,
    /// The text value.
    pub value: String,
}

/// The value of one property, kept as the FHIR choice member that carried it.
///
/// `CodeSystem/$lookup` types `property.value` as `Element`
/// (`OperationDefinition-CodeSystem-lookup`, R4), so any FHIR type may arrive.
/// Keeping the member name beside the lexical JSON loses nothing, and the
/// lexical form is what preserves a decimal exactly
/// (<https://hl7.org/fhir/R4/datatypes.html#decimal>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyValue {
    /// The `value[x]` member name, for example `valueCode` or `valueCoding`.
    pub member: String,
    /// The lexical JSON the member carried.
    pub value: fhir_types::codec::Value,
}

impl PropertyValue {
    /// Returns the value as text when the member carried a JSON string.
    ///
    /// Every FHIR primitive whose JSON form is a string reads through this:
    /// `code`, `string`, `dateTime` and the rest. A `valueCoding` or a numeric
    /// member answers `None`, and the caller reads [`PropertyValue::value`].
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            fhir_types::codec::Value::String(text) => Some(text.as_str()),
            _ => None,
        }
    }
}

/// One property of a concept, as the `property` out parameter of
/// `CodeSystem/$lookup` carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The property code.
    pub code: String,
    /// The value, when the server sent one.
    pub value: Option<PropertyValue>,
    /// The human-readable rendering of the value.
    pub description: Option<String>,
}

/// How a translation match relates to the code that was translated.
///
/// The codes are the `ConceptMapEquivalence` value set
/// (<https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>), which
/// the `match.equivalence` documentation names; STU3 defines the same ten
/// codes in the same hierarchy
/// (<https://hl7.org/fhir/STU3/valueset-concept-map-equivalence.html>). The
/// parameter declares no
/// binding, so a code outside the set is kept as [`Equivalence::Other`] rather
/// than refused; it can never be accepted, because [`Equivalence::accepted`]
/// answers `true` for `equivalent` and `equal` alone.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Equivalence {
    /// `relatedto`: the concepts are related but the exact relationship is
    /// unknown.
    RelatedTo,
    /// `equivalent`: the definitions are the same in meaning.
    Equivalent,
    /// `equal`: the definitions are exactly the same.
    Equal,
    /// `wider`: the target is broader in meaning.
    Wider,
    /// `subsumes`: the target subsumes the source.
    Subsumes,
    /// `narrower`: the target is narrower in meaning.
    Narrower,
    /// `specializes`: the target specializes the source.
    Specializes,
    /// `inexact`: the definitions overlap without either subsuming the other.
    Inexact,
    /// `unmatched`: no translation exists.
    Unmatched,
    /// `disjoint`: the source is mapped away from this target.
    Disjoint,
    /// A code the value set does not define, kept as the server sent it.
    Other(String),
}

impl Equivalence {
    /// Returns the equivalence `code` names.
    #[must_use]
    pub fn new(code: &str) -> Self {
        match code {
            "relatedto" => Self::RelatedTo,
            "equivalent" => Self::Equivalent,
            "equal" => Self::Equal,
            "wider" => Self::Wider,
            "subsumes" => Self::Subsumes,
            "narrower" => Self::Narrower,
            "specializes" => Self::Specializes,
            "inexact" => Self::Inexact,
            "unmatched" => Self::Unmatched,
            "disjoint" => Self::Disjoint,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Returns the code as the value set spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::RelatedTo => "relatedto",
            Self::Equivalent => "equivalent",
            Self::Equal => "equal",
            Self::Wider => "wider",
            Self::Subsumes => "subsumes",
            Self::Narrower => "narrower",
            Self::Specializes => "specializes",
            Self::Inexact => "inexact",
            Self::Unmatched => "unmatched",
            Self::Disjoint => "disjoint",
            Self::Other(code) => code.as_str(),
        }
    }

    /// Returns whether this equivalence makes the match a translation.
    ///
    /// Only `equivalent` and `equal` state that the target means what the
    /// source means; `wider`, `narrower` and `inexact` state that it does not,
    /// and writing one of those into a target system would assert a clinical
    /// fact the map never made.
    #[must_use]
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Equivalent | Self::Equal)
    }
}

/// One match of `ConceptMap/$translate`.
///
/// The operation "returns a set of parameters including a `result` for whether
/// there is an acceptable match, and a list of possible matches. Note that the
/// list of matches may include notes of codes for which mapping is
/// specifically excluded, so implementers have to check the
/// `match.equivalence` for each match"
/// (`OperationDefinition-ConceptMap-translate`, R4), which is why every match
/// is returned and [`Match::accepted`] decides. R5 and R6 state
/// `match.relationship` in place of the equivalence
/// (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Match {
    /// `match.equivalence`, which STU3 to R4B state.
    pub equivalence: Option<Equivalence>,
    /// `match.relationship`, which R5 and R6 state.
    pub relationship: Option<Relationship>,
    /// `match.concept`, the concept that was mapped to.
    pub concept: Option<Concept>,
    /// `match.source`, the concept map that produced this match (STU3 to
    /// R4B).
    pub source: Option<String>,
    /// `match.originMap`, the concept map that produced this match (R5 and
    /// R6, and the terminology ecosystem's addition to R4 and R4B).
    pub origin_map: Option<String>,
    /// `match.noMap`, set when the map states that the code maps to nothing.
    pub no_map: Option<bool>,
    /// `match.sourceConcept`, the source the match was made from.
    pub source_concept: Option<Concept>,
    /// `match.sourceComment`.
    pub source_comment: Option<String>,
    /// `match.targetComment`.
    pub target_comment: Option<String>,
    /// `match.product`, the other values the mapping produces.
    pub products: Vec<MatchValue>,
    /// `match.dependsOn`, the other values the mapping depends on (R5 and
    /// R6).
    pub depends_on: Vec<MatchValue>,
    /// `match.property`, the properties of the mapping (R5 and R6).
    pub properties: Vec<MatchValue>,
}

impl Match {
    /// Returns whether this match is a translation.
    ///
    /// A match with neither an equivalence nor a relationship is not one: the
    /// specification asks the caller to check each match, and an absent value
    /// states nothing.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.equivalence.as_ref().is_some_and(Equivalence::accepted)
            || self
                .relationship
                .as_ref()
                .is_some_and(Relationship::accepted)
    }
}

/// One `product`, `dependsOn` or `property` part of a match: what it is
/// about and its value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchValue {
    /// `element` (STU3 to R4B), `attribute` or `uri` (R5 and R6).
    pub name: Option<String>,
    /// `concept` (STU3 to R4B) or `value` (R5 and R6), as the member that
    /// carried it and its JSON.
    pub value: Option<PropertyValue>,
}

/// A `ConceptMapRelationship` code, which R5 and R6 state on a match
/// (<https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Relationship {
    /// `related-to`.
    RelatedTo,
    /// `equivalent`: the source and target concepts mean the same.
    Equivalent,
    /// `source-is-narrower-than-target`.
    SourceIsNarrowerThanTarget,
    /// `source-is-broader-than-target`.
    SourceIsBroaderThanTarget,
    /// `not-related-to`: the source maps to no target.
    NotRelatedTo,
    /// A code outside the value set, kept as the server wrote it.
    Other(String),
}

impl Relationship {
    /// Returns the relationship `code` names.
    #[must_use]
    pub fn new(code: &str) -> Self {
        match code {
            "related-to" => Self::RelatedTo,
            "equivalent" => Self::Equivalent,
            "source-is-narrower-than-target" => Self::SourceIsNarrowerThanTarget,
            "source-is-broader-than-target" => Self::SourceIsBroaderThanTarget,
            "not-related-to" => Self::NotRelatedTo,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Returns the code, as the server wrote it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::RelatedTo => "related-to",
            Self::Equivalent => "equivalent",
            Self::SourceIsNarrowerThanTarget => "source-is-narrower-than-target",
            Self::SourceIsBroaderThanTarget => "source-is-broader-than-target",
            Self::NotRelatedTo => "not-related-to",
            Self::Other(code) => code.as_str(),
        }
    }

    /// Returns whether a match of this relationship is a translation, which
    /// only `equivalent` is.
    #[must_use]
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Equivalent)
    }
}

/// A concept as a FHIR `CodeableConcept`: its codings and its text
/// (<https://hl7.org/fhir/R4/datatypes.html#CodeableConcept>).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeableConcept {
    /// `CodeableConcept.coding`, in order.
    pub coding: Vec<Concept>,
    /// `CodeableConcept.text`.
    pub text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Concept, Equivalence, Match, PropertyValue};

    #[test]
    fn only_equivalent_and_equal_are_accepted() {
        for code in ["equivalent", "equal"] {
            assert!(
                Equivalence::new(code).accepted(),
                "{code} should be a translation"
            );
        }
        for code in [
            "relatedto",
            "wider",
            "subsumes",
            "narrower",
            "specializes",
            "inexact",
            "unmatched",
            "disjoint",
        ] {
            assert!(
                !Equivalence::new(code).accepted(),
                "{code} should not be a translation"
            );
        }
    }

    #[test]
    fn an_unknown_equivalence_keeps_its_code_and_is_never_accepted() {
        let equivalence = Equivalence::new("source-is-narrower-than-target");
        assert_eq!(
            Equivalence::Other("source-is-narrower-than-target".to_owned()),
            equivalence
        );
        assert_eq!("source-is-narrower-than-target", equivalence.as_str());
        assert!(!equivalence.accepted());
    }

    #[test]
    fn a_match_without_an_equivalence_is_not_a_translation() {
        let unstated = Match {
            equivalence: None,
            concept: Some(Concept::default()),
            ..Match::default()
        };
        assert!(!unstated.accepted());
    }

    #[test]
    fn a_string_property_value_reads_as_text() {
        let text = PropertyValue {
            member: "valueString".to_owned(),
            value: fhir_types::codec::Value::String("a note".to_owned()),
        };
        assert_eq!(Some("a note"), text.as_str());
        let flag = PropertyValue {
            member: "valueBoolean".to_owned(),
            value: fhir_types::codec::Value::Bool(true),
        };
        assert_eq!(None, flag.as_str());
    }
}
