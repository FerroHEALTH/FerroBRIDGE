// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issue model: one finding about an instance, in the shape of an R4
//! `OperationOutcome.issue` (<https://hl7.org/fhir/R4/operationoutcome.html>).
//!
//! R4 defines no validator messages (validation.html is informative), so the
//! message ids of [`MessageId`] are our own vocabulary; no specification
//! governs it.

use std::fmt;

/// How severe an issue is: the R4 `issue-severity` value set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The issue stopped the validation.
    Fatal,
    /// The instance does not conform.
    Error,
    /// The instance conforms, and something about it deserves attention.
    Warning,
    /// A finding with no bearing on conformance.
    Information,
}

impl Severity {
    /// Returns the R4 code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Fatal => "fatal",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Information => "information",
        }
    }
}

/// The kind of an issue: the R4 `issue-type` code system, every code of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IssueType {
    /// `invalid`: content invalid against the specification or a profile.
    Invalid,
    /// `structure`: a structural issue in the content.
    Structure,
    /// `required`: a required element is missing.
    Required,
    /// `value`: an element or header value is invalid.
    Value,
    /// `invariant`: a content validation rule failed.
    Invariant,
    /// `security`: an authentication or authorization problem.
    Security,
    /// `login`: the client needs to log in.
    Login,
    /// `unknown`: the user or system was not able to be authenticated.
    Unknown,
    /// `expired`: the user session expired.
    Expired,
    /// `forbidden`: the user does not have the rights.
    Forbidden,
    /// `suppressed`: some information was not or might not be returned.
    Suppressed,
    /// `processing`: processing issues.
    Processing,
    /// `not-supported`: the content or operation is not supported.
    NotSupported,
    /// `duplicate`: an attempt was made to create a duplicate record.
    Duplicate,
    /// `multiple-matches`: multiple matching records were found.
    MultipleMatches,
    /// `not-found`: the reference provided was not found.
    NotFound,
    /// `deleted`: the reference pointed at deleted content.
    Deleted,
    /// `too-long`: provided content is too long.
    TooLong,
    /// `code-invalid`: the code or system could not be understood.
    CodeInvalid,
    /// `extension`: an extension was not acceptable or not recognized.
    Extension,
    /// `too-costly`: the operation was stopped to protect server resources.
    TooCostly,
    /// `business-rule`: a business rule was violated.
    BusinessRule,
    /// `conflict`: a content or version conflict.
    Conflict,
    /// `transient`: a transient processing issue.
    Transient,
    /// `lock-error`: a resource or record lock failed.
    LockError,
    /// `no-store`: the persistent store is unavailable.
    NoStore,
    /// `exception`: an unexpected internal error.
    Exception,
    /// `timeout`: an internal timeout occurred.
    Timeout,
    /// `incomplete`: not all data sources returned results.
    Incomplete,
    /// `throttled`: the system is not prepared to handle this request.
    Throttled,
    /// `informational`: a message unrelated to the processing success.
    Informational,
}

impl IssueType {
    /// Returns the R4 code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::Structure => "structure",
            Self::Required => "required",
            Self::Value => "value",
            Self::Invariant => "invariant",
            Self::Security => "security",
            Self::Login => "login",
            Self::Unknown => "unknown",
            Self::Expired => "expired",
            Self::Forbidden => "forbidden",
            Self::Suppressed => "suppressed",
            Self::Processing => "processing",
            Self::NotSupported => "not-supported",
            Self::Duplicate => "duplicate",
            Self::MultipleMatches => "multiple-matches",
            Self::NotFound => "not-found",
            Self::Deleted => "deleted",
            Self::TooLong => "too-long",
            Self::CodeInvalid => "code-invalid",
            Self::Extension => "extension",
            Self::TooCostly => "too-costly",
            Self::BusinessRule => "business-rule",
            Self::Conflict => "conflict",
            Self::Transient => "transient",
            Self::LockError => "lock-error",
            Self::NoStore => "no-store",
            Self::Exception => "exception",
            Self::Timeout => "timeout",
            Self::Incomplete => "incomplete",
            Self::Throttled => "throttled",
            Self::Informational => "informational",
        }
    }
}

/// The message ids of the validator, our own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageId {
    /// The instance is no JSON object.
    NotAnObject,
    /// `resourceType` is absent or names no R4 resource.
    UnknownResourceType,
    /// The profile constrains another resource type than the instance's.
    ProfileTypeMismatch,
    /// A property the type does not define (json.html §2.6.2).
    UnknownElement,
    /// A value of the wrong JSON type for its element.
    WrongJsonType,
    /// A repeating element written as a single value, or the reverse
    /// (json.html §2.6.2).
    WrongShape,
    /// An empty object (json.html §2.6.2).
    EmptyObject,
    /// An empty array (json.html §2.6.2.1).
    EmptyArray,
    /// An empty string (json.html §2.6.2).
    EmptyString,
    /// A `null` where no aligned primitive array needs it (json.html §2.6.2.3).
    NullValue,
    /// The value array and the `_` array of a repeating primitive differ in
    /// length (json.html §2.6.2.3).
    MisalignedArrays,
    /// Two forms of one choice element.
    DuplicateChoice,
    /// A choice form whose type the definition does not allow.
    ChoiceTypeNotAllowed,
    /// Fewer occurrences than the minimum cardinality.
    MinCardinality,
    /// More occurrences than the maximum cardinality.
    MaxCardinality,
    /// A primitive value outside the lexical form of its type.
    LexicalForm,
    /// An integer outside the range of its type (datatypes.html §2.24.0.1).
    IntegerRange,
    /// A value longer than the length the primitive's definition allows.
    TooLong,
    /// A value longer than the profile's `maxLength`.
    MaxLength,
    /// A value that differs from the profile's `fixed[x]`.
    FixedMismatch,
    /// A value that does not contain the profile's `pattern[x]`.
    PatternMismatch,
    /// An extension whose `url` the package set does not define.
    UnknownExtension,
    /// A modifier extension whose `url` the package set does not define
    /// (extensibility.html §2.5.0.2).
    UnknownModifierExtension,
    /// An extension `url` that resolves to a `StructureDefinition` of another
    /// type.
    NotAnExtension,
    /// A relative `url` outside a complex extension (extensibility.html
    /// §2.5.0.1).
    RelativeExtensionUrl,
    /// An extension in `extension` defined as a modifier, or one in
    /// `modifierExtension` defined as none.
    ModifierMismatch,
    /// An item of a closed slicing that matches no slice (profiling.html
    /// §5.1.0.10).
    UnmatchedSliceItem,
    /// An item of an ordered slicing that comes before an item of a slice
    /// declared earlier.
    SliceOrder,
    /// An item of a slicing with the rules `openAtEnd` that matches a slice
    /// after an item that matched none.
    OpenAtEnd,
    /// An item that matches more than one slice (profiling.html §5.1.0.11).
    OverlappingSlices,
    /// A reference a discriminator resolves names no resource where it
    /// points.
    UnresolvedReference,
    /// Content that conforms to none of the profiles its type names.
    TypeProfileMismatch,
    /// A resource of a type none of its element's types names.
    ResourceTypeNotAllowed,
    /// A value with no code from the value set of a required binding or of a
    /// maximum value set (terminologies.html §4.1.5.1).
    NotInValueSet,
    /// A value with no code from the value set of an extensible binding
    /// (terminologies.html §4.1.5.2).
    NotInExtensibleValueSet,
    /// A value with no code from the value set of a preferred binding
    /// (terminologies.html §4.1.5.3).
    NotInPreferredValueSet,
    /// A coding whose display is none the code system defines for its code.
    DisplayMismatch,
    /// A reference that matches more than one entry of the Bundle around it
    /// (bundle.html §2.36.4.1).
    AmbiguousReference,
    /// A reference whose target conforms to none of the target profiles of
    /// its element.
    TargetProfileMismatch,
    /// A reference aggregated in a way the aggregation of its element does
    /// not allow (elementdefinition.html §2.30.0.4.4).
    AggregationMismatch,
    /// A resource that does not conform to a profile it claims in
    /// `meta.profile`.
    ClaimedProfileMismatch,
    /// A resource the Composition of a document references that the document
    /// Bundle does not hold (documents.html §3.3.1).
    DocumentReferenceMissing,
    /// An entry of a document Bundle that is neither referenced from the
    /// Composition, directly or indirectly, nor a stylesheet Binary or a
    /// Provenance of the document (documents.html §3.3.1).
    DocumentEntryUnreferenced,
    /// A constraint the validator does not evaluate.
    ConstraintUnevaluated,
    /// The instance raised no issue and every constraint was evaluated.
    AllOk,
}

impl MessageId {
    /// Returns the message id as written in an `OperationOutcome`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotAnObject => "not-an-object",
            Self::UnknownResourceType => "unknown-resource-type",
            Self::ProfileTypeMismatch => "profile-type-mismatch",
            Self::UnknownElement => "unknown-element",
            Self::WrongJsonType => "wrong-json-type",
            Self::WrongShape => "wrong-shape",
            Self::EmptyObject => "empty-object",
            Self::EmptyArray => "empty-array",
            Self::EmptyString => "empty-string",
            Self::NullValue => "null-value",
            Self::MisalignedArrays => "misaligned-arrays",
            Self::DuplicateChoice => "duplicate-choice",
            Self::ChoiceTypeNotAllowed => "choice-type-not-allowed",
            Self::MinCardinality => "min-cardinality",
            Self::MaxCardinality => "max-cardinality",
            Self::LexicalForm => "lexical-form",
            Self::IntegerRange => "integer-range",
            Self::TooLong => "too-long",
            Self::MaxLength => "max-length",
            Self::FixedMismatch => "fixed-mismatch",
            Self::PatternMismatch => "pattern-mismatch",
            Self::UnknownExtension => "unknown-extension",
            Self::UnknownModifierExtension => "unknown-modifier-extension",
            Self::NotAnExtension => "not-an-extension",
            Self::RelativeExtensionUrl => "relative-extension-url",
            Self::ModifierMismatch => "modifier-mismatch",
            Self::UnmatchedSliceItem => "unmatched-slice-item",
            Self::SliceOrder => "slice-order",
            Self::OpenAtEnd => "open-at-end",
            Self::OverlappingSlices => "overlapping-slices",
            Self::UnresolvedReference => "unresolved-reference",
            Self::TypeProfileMismatch => "type-profile-mismatch",
            Self::ResourceTypeNotAllowed => "resource-type-not-allowed",
            Self::NotInValueSet => "not-in-value-set",
            Self::NotInExtensibleValueSet => "not-in-extensible-value-set",
            Self::NotInPreferredValueSet => "not-in-preferred-value-set",
            Self::DisplayMismatch => "display-mismatch",
            Self::AmbiguousReference => "ambiguous-reference",
            Self::TargetProfileMismatch => "target-profile-mismatch",
            Self::AggregationMismatch => "aggregation-mismatch",
            Self::ClaimedProfileMismatch => "claimed-profile-mismatch",
            Self::DocumentReferenceMissing => "document-reference-missing",
            Self::DocumentEntryUnreferenced => "document-entry-unreferenced",
            Self::ConstraintUnevaluated => "constraint-unevaluated",
            Self::AllOk => "all-ok",
        }
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// What an issue reports against: one of our message ids, or the key of a
/// constraint a `StructureDefinition` declares.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// A message id of the validator.
    Message(MessageId),
    /// A constraint of an element definition
    /// (<https://hl7.org/fhir/R4/elementdefinition.html>).
    Constraint {
        /// The constraint key, for example `ele-1`.
        key: String,
        /// The canonical of the `StructureDefinition` that declares it.
        source: String,
    },
}

/// One finding about an instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Issue {
    /// How severe it is.
    pub severity: Severity,
    /// Its R4 issue type.
    pub code: IssueType,
    /// The FHIRPath location of the element, with indexes on repeating
    /// elements (`Patient.name[0].given[1]`).
    pub expression: String,
    /// The message id or constraint it reports.
    pub rule: Rule,
    /// The human-readable message, naming paths and the values the profile
    /// expects and never an instance value.
    pub message: String,
}
