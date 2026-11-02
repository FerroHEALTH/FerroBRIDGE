// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Composition build and read, behind one path and value seam.
//!
//! Both mapping engines write and read openEHR content as a set of template
//! nodes with values, so both reach a composition through this one seam: a
//! list of [`NodeValue`] builds a canonical composition, and a resolved node
//! plus its occurrences reads one value back out of it. Canonical JSON is the
//! mandatory composition representation of ITS-REST 1.1.0, so it is what
//! crosses the CDR wire in both directions.
//!
//! Two index bases meet here and are separate types because confusing them
//! silently mis-keys a value: an RM positional predicate is 1-based (openEHR
//! BASE Release 1.2.0 §Paths and Locators) and a FLAT instance index is
//! 0-based (Simplified Formats, §Instance Indexing).

use openehr_sdt::flat::path::Segment;
use openehr_sdt::flat::path::Suffix;
use serde_json::Value;

use crate::index::ResolvedNode;
use crate::index::paths::FlatId;
use crate::template::Generation;

/// The 1-based position of one instance, as an openEHR path writes it.
///
/// openEHR BASE Release 1.2.0 §Paths and Locators gives the positional
/// predicate as the `XPath` one, whose first item is `[1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RmPosition(u32);

impl RmPosition {
    /// Creates a position.
    ///
    /// # Errors
    ///
    /// Returns [`PositionError::Zero`] when `position` is 0, which no openEHR
    /// path may write.
    pub const fn new(position: u32) -> Result<Self, PositionError> {
        if position == 0 {
            return Err(PositionError::Zero);
        }
        Ok(Self(position))
    }

    /// Returns the position of the first instance.
    #[must_use]
    pub const fn first() -> Self {
        Self(1)
    }

    /// Returns the position.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl core::fmt::Display for RmPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The 0-based index of one instance, as a FLAT key writes it.
///
/// The Simplified Formats specification gives the instance index of a FLAT key
/// as zero-based (§Instance Indexing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlatIndex(u32);

impl FlatIndex {
    /// Creates an index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl core::fmt::Display for FlatIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<RmPosition> for FlatIndex {
    /// Converts a 1-based RM position into the 0-based FLAT index of the same
    /// instance.
    fn from(position: RmPosition) -> Self {
        Self(position.0.saturating_sub(1))
    }
}

impl TryFrom<FlatIndex> for RmPosition {
    type Error = PositionError;

    /// Converts a 0-based FLAT index into the 1-based RM position of the same
    /// instance.
    ///
    /// # Errors
    ///
    /// Returns [`PositionError::Overflow`] for the last representable index,
    /// which has no position.
    fn try_from(index: FlatIndex) -> Result<Self, Self::Error> {
        index
            .0
            .checked_add(1)
            .map(Self)
            .ok_or(PositionError::Overflow)
    }
}

/// Why an instance index could not be read as a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PositionError {
    /// The position was 0, and openEHR positions start at 1.
    #[error("an openEHR positional predicate starts at 1")]
    Zero,
    /// The index is the last representable one, so it has no position.
    #[error("the instance index has no 1-based position")]
    Overflow,
}

/// One value to write, at one node of the template or under the context.
///
/// The occurrences name the instance of each repeating node on the way to the
/// target, outermost first, and the suffixes name which part of a data value
/// the value is (Simplified Formats, §Attribute Suffixes). A node with one
/// unsuffixed value carries no suffix, and a whole canonical value travels
/// under the `raw` suffix (Simplified Formats, §Raw canonical JSON). The key
/// is spelled through [`FlatKey`], the FLAT key model of `openehr-sdt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeValue {
    /// Where the key starts: a template node or the `ctx/` namespace.
    pub(crate) place: Place,
    /// The segments below the place, outermost first.
    pub(crate) sub_path: Vec<Segment>,
    /// The attribute suffix chain on the last segment.
    pub(crate) suffixes: Vec<Suffix>,
    /// The value itself.
    pub(crate) value: Value,
}

/// Where the FLAT key of a [`NodeValue`] starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    /// A node of the template, at one instance of each repeating node on the
    /// way to it.
    Node {
        /// The node the value belongs to.
        flat_id: FlatId,
        /// The instance of each repeating node on the way to the target.
        occurrences: Vec<RmPosition>,
    },
    /// The `ctx/` namespace (Simplified Formats, §Context).
    Context,
}

impl NodeValue {
    /// Creates a value at `node`, with no occurrences and no suffix.
    #[must_use]
    pub fn new(node: &ResolvedNode, value: Value) -> Self {
        Self {
            place: Place::Node {
                flat_id: node.flat_id().clone(),
                occurrences: Vec::new(),
            },
            sub_path: Vec::new(),
            suffixes: Vec::new(),
            value,
        }
    }

    /// Creates a value under the `ctx/` key `field`.
    ///
    /// The context vocabulary sets defaults the builder resolves into the
    /// reference-model tree (Simplified Formats, master06 §Context
    /// Information); the builder refuses a field outside it.
    #[must_use]
    pub fn context(field: impl Into<String>, value: Value) -> Self {
        Self {
            place: Place::Context,
            sub_path: vec![Segment {
                name: field.into(),
                index: None,
            }],
            suffixes: Vec::new(),
            value,
        }
    }

    /// Returns this value under a family of segments below its place.
    ///
    /// A reference-model attribute the template does not carry is a
    /// `_`-prefixed segment below the node (Simplified Formats, §RM
    /// Attributes prefix), and an indexed family carries its own instance
    /// index.
    #[must_use]
    pub fn under(mut self, sub_path: Vec<Segment>) -> Self {
        self.sub_path.extend(sub_path);
        self
    }

    /// Returns this value with the instance of each repeating node on the way
    /// to its own, outermost first.
    ///
    /// A context value belongs to no node, so it takes no occurrences.
    #[must_use]
    pub fn with_occurrences(mut self, occurrences: Vec<RmPosition>) -> Self {
        if let Place::Node {
            occurrences: ref mut held,
            ..
        } = self.place
        {
            *held = occurrences;
        }
        self
    }

    /// Returns this value as the named part of a data value, the one suffix
    /// of its key.
    #[must_use]
    pub fn with_datum(mut self, datum: impl Into<String>) -> Self {
        self.suffixes = vec![Suffix {
            name: datum.into(),
            index: None,
        }];
        self
    }

    /// Returns the node the value belongs to, `None` for a context value.
    #[must_use]
    pub const fn flat_id(&self) -> Option<&FlatId> {
        match self.place {
            Place::Node { ref flat_id, .. } => Some(flat_id),
            Place::Context => None,
        }
    }

    /// Returns the value itself.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

/// A composition in the canonical JSON of ITS-REST 1.1.0.
///
/// The template identifier and the generation travel with the document,
/// because reading a value back out of it needs the index of the same
/// template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalComposition {
    /// The canonical JSON of the composition.
    value: Value,
    /// The template the composition was built against.
    template_id: String,
    /// The generation of that template.
    generation: Generation,
}

impl CanonicalComposition {
    /// Wraps canonical JSON a CDR served as a composition of `template_id`.
    #[must_use]
    pub fn new(value: Value, template_id: impl Into<String>, generation: Generation) -> Self {
        Self {
            value,
            template_id: template_id.into(),
            generation,
        }
    }

    /// Returns the canonical JSON of the composition.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the canonical JSON of the composition, consuming it.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }

    /// Returns the template the composition was built against.
    #[must_use]
    pub fn template_id(&self) -> &str {
        &self.template_id
    }

    /// Returns the generation of the template.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
}

/// A canonical document an index reads node values out of.
///
/// A composition and a demographic party are both read through
/// [`WebTemplateIndex::read`], because both are archetyped trees whose paths
/// the template names.
pub trait CanonicalDocument {
    /// Returns the canonical JSON of the document.
    fn document(&self) -> &Value;

    /// Returns the template the document was built against.
    fn document_template_id(&self) -> &str;
}

impl CanonicalDocument for CanonicalComposition {
    fn document(&self) -> &Value {
        &self.value
    }

    fn document_template_id(&self) -> &str {
        &self.template_id
    }
}

mod read;

#[cfg(test)]
mod tests {
    use super::FlatIndex;
    use super::PositionError;
    use super::RmPosition;

    #[test]
    fn the_two_index_bases_differ_by_one() {
        let first = RmPosition::first();
        assert_eq!(first.get(), 1);
        assert_eq!(FlatIndex::from(first).get(), 0);
        let third = RmPosition::new(3).expect("3 is a position");
        assert_eq!(FlatIndex::from(third).get(), 2);
        assert_eq!(
            RmPosition::try_from(FlatIndex::new(2)).expect("2 is an index"),
            third
        );
    }

    #[test]
    fn a_zero_position_and_the_last_index_are_refused() {
        assert_eq!(RmPosition::new(0), Err(PositionError::Zero));
        assert_eq!(
            RmPosition::try_from(FlatIndex::new(u32::MAX)),
            Err(PositionError::Overflow)
        );
    }
}
