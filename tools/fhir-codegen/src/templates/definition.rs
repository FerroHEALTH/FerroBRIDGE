//! A release-neutral read of `ElementDefinition`.
//!
//! A profile validator reads the same facts from an element definition of
//! any release: the path, the cardinality, the types with their profiles,
//! the binding, the fixed and pattern values, the constraints, the slicing
//! and the content reference (<https://hl7.org/fhir/R4/elementdefinition.html>).
//! The releases spell some of them differently. STU3 states one
//! `type.profile` and one `type.targetProfile`, and its binding names the
//! value set as `valueSetUri` or `valueSetReference`
//! (<https://hl7.org/fhir/STU3/elementdefinition.html>); R4 and later state
//! lists of canonicals and one `binding.valueSet`. Every version module
//! implements [`ElementDefinitionRead`] for its own generated
//! `ElementDefinition`, emitted from that version's package, so a single
//! profile becomes a one-item list and either binding form becomes the value
//! set's URL. A fact a release does not define reads as absent.

use super::codec::{EncodeError, Value};

/// One `ElementDefinition.type` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRead<'a> {
    /// The type code (`type.code`).
    pub code: Option<&'a str>,
    /// The profiles the type is held to (`type.profile`), one item at most in
    /// STU3.
    pub profiles: Vec<&'a str>,
    /// The profiles a reference target is held to (`type.targetProfile`),
    /// one item at most in STU3.
    pub target_profiles: Vec<&'a str>,
    /// The aggregation modes of a reference (`type.aggregation`).
    pub aggregation: Vec<&'a str>,
    /// The versioning rule of a reference (`type.versioning`).
    pub versioning: Option<&'a str>,
}

/// The `ElementDefinition.binding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingRead<'a> {
    /// The binding strength (`binding.strength`).
    pub strength: Option<&'a str>,
    /// The URL of the value set: `binding.valueSet` from R4, and in STU3
    /// `valueSetUri` or the reference of `valueSetReference`.
    pub value_set: Option<&'a str>,
}

/// One `ElementDefinition.constraint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintRead<'a> {
    /// The constraint key (`constraint.key`).
    pub key: Option<&'a str>,
    /// `error` or `warning` (`constraint.severity`).
    pub severity: Option<&'a str>,
    /// The description a person reads (`constraint.human`).
    pub human: Option<&'a str>,
    /// The FHIRPath expression (`constraint.expression`).
    pub expression: Option<&'a str>,
    /// The XPath expression (`constraint.xpath`), absent from R5.
    pub xpath: Option<&'a str>,
    /// The definition that first stated the constraint (`constraint.source`).
    pub source: Option<&'a str>,
}

/// One `ElementDefinition.slicing.discriminator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscriminatorRead<'a> {
    /// The discriminator type (`discriminator.type`).
    pub kind: Option<&'a str>,
    /// The path the discriminator reads (`discriminator.path`).
    pub path: Option<&'a str>,
}

/// The `ElementDefinition.slicing`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlicingRead<'a> {
    /// The discriminators, in definition order.
    pub discriminators: Vec<DiscriminatorRead<'a>>,
    /// The description (`slicing.description`).
    pub description: Option<&'a str>,
    /// Whether the slices are ordered (`slicing.ordered`).
    pub ordered: Option<bool>,
    /// `closed`, `open` or `openAtEnd` (`slicing.rules`).
    pub rules: Option<&'a str>,
}

/// A `fixed[x]` or `pattern[x]` value in its JSON form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceValue {
    /// The type suffix of the member name, `Code` for `fixedCode`.
    pub suffix: &'static str,
    /// The JSON value of the member.
    pub value: Option<Value>,
    /// The `_` sibling a primitive value carries its `id` and extensions in.
    pub element: Option<Value>,
}

/// The facts of an `ElementDefinition` a validator reads, the same in every
/// release.
pub trait ElementDefinitionRead {
    /// The element id (`ElementDefinition.id`).
    fn element_id(&self) -> Option<&str>;

    /// The element path (`ElementDefinition.path`).
    fn path(&self) -> Option<&str>;

    /// The slice name (`ElementDefinition.sliceName`).
    fn slice_name(&self) -> Option<&str>;

    /// The minimum cardinality (`ElementDefinition.min`).
    fn min(&self) -> Option<u32>;

    /// The maximum cardinality as written, a number or `*`
    /// (`ElementDefinition.max`).
    fn max(&self) -> Option<&str>;

    /// The longest string the element admits (`ElementDefinition.maxLength`).
    fn max_length(&self) -> Option<i32>;

    /// The element whose definition this one reuses
    /// (`ElementDefinition.contentReference`).
    fn content_reference(&self) -> Option<&str>;

    /// The types, in definition order (`ElementDefinition.type`).
    fn types(&self) -> Vec<TypeRead<'_>>;

    /// The binding (`ElementDefinition.binding`).
    fn binding(&self) -> Option<BindingRead<'_>>;

    /// The constraints, in definition order (`ElementDefinition.constraint`).
    fn constraints(&self) -> Vec<ConstraintRead<'_>>;

    /// The slicing (`ElementDefinition.slicing`).
    fn slicing(&self) -> Option<SlicingRead<'_>>;

    /// The fixed value (`ElementDefinition.fixed[x]`) in its JSON form.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] when the held value has no JSON form.
    fn fixed(&self) -> Result<Option<ChoiceValue>, EncodeError>;

    /// The pattern value (`ElementDefinition.pattern[x]`) in its JSON form.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] when the held value has no JSON form.
    fn pattern(&self) -> Result<Option<ChoiceValue>, EncodeError>;
}
