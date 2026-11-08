// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each operation answers, as one enum per operation.
//!
//! A negative answer the operation states in its own `out` parameters is a
//! variant here; an answer the server refused is a
//! [`crate::error::Error`] carrying the upstream status and body. Nothing is
//! ever flattened into an empty value.

use crate::concept::{CodeableConcept, Designation, Match, Property, PropertyValue};
use crate::error::{Outcome, UpstreamError};

/// What `CodeSystem/$lookup` answered.
#[derive(Debug)]
#[non_exhaustive]
pub enum LookupOutcome {
    /// The server knows the code and returned its details.
    Found {
        /// `name`, the code system's name. The operation declares it `1..1`.
        name: String,
        /// `version`, the code system version the answer is from.
        version: Option<String>,
        /// `display`, the preferred display. The operation declares it `1..1`.
        display: String,
        /// `definition`, which R5 and R6 declare
        /// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
        definition: Option<String>,
        /// Every `designation` the server returned.
        designations: Vec<Designation>,
        /// Every `property` the server returned.
        properties: Vec<Property>,
    },
    /// The server does not know the code, or does not know its system.
    NotFound {
        /// The upstream status and body, with any `tx-issue-type` coding.
        outcome: Box<UpstreamError>,
    },
}

/// What `ConceptMap/$translate` answered.
#[derive(Debug)]
#[non_exhaustive]
pub enum TranslateOutcome {
    /// `result` was `true`: the server found at least one match.
    ///
    /// Every match is carried with its equivalence or relationship, including
    /// the ones that are not translations, because the specification asks the
    /// caller to check each one.
    Translated {
        /// The matches, in the order the server sent them.
        matches: Vec<Match>,
        /// `message`, when the server sent one.
        message: Option<String>,
        /// Every `used-conceptmap`, the maps the server consulted.
        used_maps: Vec<String>,
    },
    /// `result` was `false`: the server found no acceptable match.
    NoMatch {
        /// `message`, the server's error detail for a false `result`.
        message: Option<String>,
        /// The matches the answer still carries, such as a `noMap` match or a
        /// `not-related-to` one, in the order the server sent them.
        matches: Vec<Match>,
        /// Every `used-conceptmap`, the maps the server consulted.
        used_maps: Vec<String>,
    },
}

impl TranslateOutcome {
    /// Returns every match that is a translation.
    ///
    /// Only `equivalent` and `equal` matches (R4,
    /// <https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>) and
    /// `equivalent` relationships (R5,
    /// <https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>) are
    /// yielded, so a wider, narrower or inexact match never reaches a target
    /// system as a translation. A `NoMatch` answer yields none.
    pub fn accepted(&self) -> impl Iterator<Item = &Match> {
        let matches = match self {
            Self::Translated { matches, .. } => matches.as_slice(),
            Self::NoMatch { .. } => &[],
        };
        matches.iter().filter(|found| found.accepted())
    }
}

/// What `ValueSet/$validate-code` or `CodeSystem/$validate-code` answered.
#[derive(Debug)]
#[non_exhaustive]
pub enum ValidateOutcome {
    /// `result` was `true`: the code is valid where it was checked.
    Valid {
        /// `display`, "a valid display for the concept if the system wishes to
        /// display this to a user".
        display: Option<String>,
        /// `message`, which carries hints and warnings beside a true `result`.
        message: Option<String>,
        /// `issues`, the warnings the server itemised beside a true `result`.
        outcome: Option<Outcome>,
        /// The other elements of the answer.
        details: Box<ValidationDetails>,
    },
    /// `result` was `false`: the code is not valid where it was checked.
    Invalid {
        /// `display`, the display the server prefers for the code, when it
        /// knows the code.
        display: Option<String>,
        /// `message`, the server's error detail.
        message: Option<String>,
        /// `issues`, the itemised `OperationOutcome` the terminology ecosystem
        /// asks for beside a false `result`
        /// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        outcome: Option<Outcome>,
        /// The other elements of the answer.
        details: Box<ValidationDetails>,
    },
}

impl ValidateOutcome {
    /// Returns whether the code is valid where it was checked.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }
}

/// The elements of a `$validate-code` answer beside `result`, `message`,
/// `display` and `issues`.
///
/// R4 to R6 declare them with the terminology ecosystem's additions
/// (`OperationDefinition-ValueSet-validate-code`,
/// <https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>); STU3 declares
/// none, so an STU3 answer leaves every one empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationDetails {
    /// `code`, the code that was validated.
    pub code: Option<String>,
    /// `system`, the code system it was validated against.
    pub system: Option<String>,
    /// `version`, the code system version it was validated in.
    pub version: Option<String>,
    /// `codeableConcept`, the concept the server echoed.
    pub codeable_concept: Option<CodeableConcept>,
    /// `normalized-code`, the code as the system spells it.
    pub normalized_code: Option<String>,
    /// `inactive`, whether the concept is inactive in its code system.
    pub inactive: Option<bool>,
    /// `status`, the concept's status where its system states one.
    pub status: Option<String>,
    /// Every `x-caused-by-unknown-system`, the code systems the server does
    /// not hold that the answer turned on.
    pub caused_by_unknown_systems: Vec<String>,
    /// Every `x-unknown-system`, a code system of a coding the server does
    /// not hold.
    pub unknown_systems: Vec<String>,
}

/// What one batch entry answered, in the shape of the request that produced
/// it.
#[derive(Debug)]
#[non_exhaustive]
pub enum BatchOutcome {
    /// The answer to a [`Request::Lookup`](crate::request::Request::Lookup).
    Lookup(LookupOutcome),
    /// The answer to a [`Request::Translate`](crate::request::Request::Translate).
    Translate(TranslateOutcome),
    /// The answer to a [`Request::ValidateCode`](crate::request::Request::ValidateCode).
    ValidateCode(ValidateOutcome),
    /// The answer to a [`Request::CodeSystemValidateCode`](crate::request::Request::CodeSystemValidateCode).
    CodeSystemValidateCode(ValidateOutcome),
    /// The answer to a [`Request::Expand`](crate::request::Request::Expand).
    Expand(ExpandOutcome),
    /// The answer to a [`Request::Subsumes`](crate::request::Request::Subsumes).
    Subsumes(SubsumesOutcome),
}

/// What `ValueSet/$expand` answered.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExpandOutcome {
    /// The server expanded the value set.
    Expanded(Expanded),
    /// The server does not know the value set.
    NotFound {
        /// The upstream status and body, with any `tx-issue-type` coding.
        outcome: Box<UpstreamError>,
    },
}

/// The expanded value set `ValueSet/$expand` returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    /// `ValueSet.url`, when the answer states it.
    pub url: Option<String>,
    /// `ValueSet.version`, when the answer states it.
    pub version: Option<String>,
    /// `expansion.total`, the number of codes in the whole expansion, when
    /// the server states it.
    pub total: Option<i32>,
    /// `expansion.offset`, where this page starts, when the server states it.
    pub offset: Option<i32>,
    /// `expansion.parameter`, the inputs the server says it applied, in its
    /// order.
    pub parameters: Vec<ExpansionParameter>,
    /// The `valueset-unclosed` mark: present when the server says the value
    /// set holds codes this expansion does not list.
    pub unclosed: Option<Unclosed>,
    /// `expansion.contains`, with its nesting kept.
    pub contains: Vec<ExpansionContains>,
}

/// One `ValueSet.expansion.parameter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionParameter {
    /// `parameter.name`.
    pub name: String,
    /// `parameter.value[x]`, as the member that carried it and its JSON.
    pub value: Option<PropertyValue>,
}

/// The `valueset-unclosed` extension on `ValueSet.expansion`, set `true`
/// (<https://hl7.org/fhir/R4/extension-valueset-unclosed.html>).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unclosed {
    /// Every `valueset-unclosed-reason` string, in the server's order.
    pub reasons: Vec<String>,
}

/// One entry of `ValueSet.expansion.contains`, the same in every release
/// (<https://hl7.org/fhir/R4/valueset-definitions.html#ValueSet.expansion.contains>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionContains {
    /// `system`.
    pub system: Option<String>,
    /// `version`.
    pub version: Option<String>,
    /// `code`, absent on an entry that only groups its children.
    pub code: Option<String>,
    /// `display`.
    pub display: Option<String>,
    /// `abstract`: when `true`, the entry is not selectable.
    pub r#abstract: Option<bool>,
    /// `inactive`: when `true`, the concept is inactive in its code system.
    pub inactive: Option<bool>,
    /// `designation`, when the server returned any.
    pub designations: Vec<Designation>,
    /// `contains`, the entries nested under this one.
    pub contains: Vec<ExpansionContains>,
}

/// What `CodeSystem/$subsumes` answered: the `outcome` code.
///
/// The parameter is bound `required` to the `ConceptSubsumptionOutcome`
/// value set (`OperationDefinition-CodeSystem-subsumes`, R4 and R4B), and
/// STU3 names the same four codes, so a code outside them is a defective
/// answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubsumesOutcome {
    /// `equivalent`: A and B are equivalent.
    Equivalent,
    /// `subsumes`: A subsumes B.
    Subsumes,
    /// `subsumed-by`: A is subsumed by B.
    SubsumedBy,
    /// `not-subsumed`: neither subsumes the other.
    NotSubsumed,
}

impl SubsumesOutcome {
    /// Returns the outcome `code` names, `None` for a code outside the
    /// value set.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "equivalent" => Some(Self::Equivalent),
            "subsumes" => Some(Self::Subsumes),
            "subsumed-by" => Some(Self::SubsumedBy),
            "not-subsumed" => Some(Self::NotSubsumed),
            _ => None,
        }
    }

    /// Returns the code as the value set spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equivalent => "equivalent",
            Self::Subsumes => "subsumes",
            Self::SubsumedBy => "subsumed-by",
            Self::NotSubsumed => "not-subsumed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SubsumesOutcome, TranslateOutcome, ValidateOutcome, ValidationDetails};
    use crate::concept::{Concept, Equivalence, Match, Relationship};

    fn found(equivalence: &str, code: &str) -> Match {
        Match {
            equivalence: Some(Equivalence::new(equivalence)),
            concept: Some(Concept {
                system: Some("http://example.org/target".to_owned()),
                code: Some(code.to_owned()),
                ..Concept::default()
            }),
            ..Match::default()
        }
    }

    #[test]
    fn accepted_yields_only_the_equivalent_and_equal_matches() {
        let outcome = TranslateOutcome::Translated {
            matches: vec![
                found("wider", "broad"),
                found("equivalent", "same"),
                found("inexact", "close"),
                found("equal", "identical"),
            ],
            message: None,
            used_maps: Vec::new(),
        };
        let accepted: Vec<Option<String>> = outcome
            .accepted()
            .map(|found| {
                found
                    .concept
                    .as_ref()
                    .and_then(|concept| concept.code.clone())
            })
            .collect();
        assert_eq!(
            vec![Some("same".to_owned()), Some("identical".to_owned())],
            accepted
        );
    }

    #[test]
    fn accepted_yields_only_the_equivalent_relationship() {
        let related = |code: &str| Match {
            relationship: Some(Relationship::new(code)),
            ..Match::default()
        };
        let outcome = TranslateOutcome::Translated {
            matches: vec![
                related("source-is-narrower-than-target"),
                related("equivalent"),
                related("related-to"),
            ],
            message: None,
            used_maps: Vec::new(),
        };
        assert_eq!(1, outcome.accepted().count());
    }

    #[test]
    fn a_no_match_outcome_accepts_nothing() {
        let outcome = TranslateOutcome::NoMatch {
            message: Some("no map".to_owned()),
            matches: vec![found("equivalent", "same")],
            used_maps: Vec::new(),
        };
        assert_eq!(0, outcome.accepted().count());
    }

    #[test]
    fn an_invalid_outcome_is_not_valid() {
        let outcome = ValidateOutcome::Invalid {
            display: None,
            message: Some("not in the value set".to_owned()),
            outcome: None,
            details: Box::new(ValidationDetails::default()),
        };
        assert!(!outcome.is_valid());
    }

    #[test]
    fn every_subsumption_outcome_reads_back_its_code() {
        for code in ["equivalent", "subsumes", "subsumed-by", "not-subsumed"] {
            let outcome = SubsumesOutcome::from_code(code).expect("a code of the value set");
            assert_eq!(code, outcome.as_str());
        }
        assert_eq!(None, SubsumesOutcome::from_code("related"));
    }
}
