// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The values a message is composed from: segments, fields, repetitions,
//! components and subcomponents, each subcomponent our own text or text
//! echoed as read.

/// One subcomponent: our own text, escaped when written, or text read from a
/// message in the writer's delimiters, written as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    /// Text the writer escapes.
    Text(String),
    /// Text already in the writer's delimiters, escape sequences included.
    Written(String),
}

/// One component to write: its subcomponents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Component {
    pub(super) subcomponents: Vec<Value>,
}

impl Component {
    /// Creates a component holding `text` as read in the writer's
    /// delimiters, written unchanged.
    pub(crate) fn written(text: &str) -> Self {
        Self {
            subcomponents: vec![Value::Written(String::from(text))],
        }
    }
}

impl From<&str> for Component {
    fn from(text: &str) -> Self {
        Self::from(String::from(text))
    }
}

impl From<String> for Component {
    fn from(text: String) -> Self {
        Self {
            subcomponents: vec![Value::Text(text)],
        }
    }
}

/// Collects subcomponents, each our own text, into one component.
impl<S: Into<String>> FromIterator<S> for Component {
    fn from_iter<I: IntoIterator<Item = S>>(subcomponents: I) -> Self {
        Self {
            subcomponents: subcomponents
                .into_iter()
                .map(|text| Value::Text(text.into()))
                .collect(),
        }
    }
}

/// One repetition of a field to write: its components.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repetition {
    pub(super) components: Vec<Component>,
}

impl From<&str> for Repetition {
    fn from(text: &str) -> Self {
        Self::from(Component::from(text))
    }
}

impl From<String> for Repetition {
    fn from(text: String) -> Self {
        Self::from(Component::from(text))
    }
}

impl From<Component> for Repetition {
    fn from(component: Component) -> Self {
        Self {
            components: vec![component],
        }
    }
}

/// Collects components into one repetition.
impl<C: Into<Component>> FromIterator<C> for Repetition {
    fn from_iter<I: IntoIterator<Item = C>>(components: I) -> Self {
        Self {
            components: components.into_iter().map(Into::into).collect(),
        }
    }
}

/// One field to write: its repetitions, or a field read in the writer's
/// delimiters, written as read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Field {
    pub(super) repetitions: Vec<Repetition>,
    pub(super) written: Option<String>,
}

impl Field {
    /// Creates a field holding `text` as read in the writer's delimiters,
    /// written unchanged.
    pub(crate) fn written(text: &str) -> Self {
        Self {
            repetitions: Vec::new(),
            written: Some(String::from(text)),
        }
    }
}

impl From<&str> for Field {
    fn from(text: &str) -> Self {
        Self::from(Repetition::from(text))
    }
}

impl From<String> for Field {
    fn from(text: String) -> Self {
        Self::from(Repetition::from(text))
    }
}

impl From<Component> for Field {
    fn from(component: Component) -> Self {
        Self::from(Repetition::from(component))
    }
}

impl From<Repetition> for Field {
    fn from(repetition: Repetition) -> Self {
        Self {
            repetitions: vec![repetition],
            written: None,
        }
    }
}

/// Collects repetitions into one field.
impl<R: Into<Repetition>> FromIterator<R> for Field {
    fn from_iter<I: IntoIterator<Item = R>>(repetitions: I) -> Self {
        Self {
            repetitions: repetitions.into_iter().map(Into::into).collect(),
            written: None,
        }
    }
}

/// One segment to write: its id and its fields.
///
/// The fields of an `MSH` segment begin at MSH-3: the writer writes MSH-1
/// and MSH-2 from its own delimiters (HL7 v2.9 chapter 2 §2.14.9.1 and
/// §2.14.9.2). Every other segment's fields begin at field 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub(super) id: String,
    pub(super) fields: Vec<Field>,
}

impl Segment {
    /// Creates the segment `id` with `fields` in position order.
    #[must_use]
    pub fn new<F: Into<Field>>(id: &str, fields: impl IntoIterator<Item = F>) -> Self {
        Self {
            id: String::from(id),
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the segment id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}
