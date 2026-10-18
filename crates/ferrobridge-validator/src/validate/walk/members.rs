// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members of one object: the forms a field takes, its occurrences in
//! the JSON shapes json.html allows, and their count against the base and
//! profiled cardinality.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::Kind;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::validate::walk::Form;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;

impl Walk<'_, '_, '_> {
    /// Collects the occurrences of one form, refusing a wrong shape, an empty
    /// array, a stray `null` and misaligned primitive arrays.
    pub(super) fn occurrences<'o>(
        &mut self,
        object: &'o Object,
        field: &FieldSchema,
        form: &Form,
        location: &str,
    ) -> Vec<Occurrence<'o>> {
        let value = object.get(&form.key);
        let element = if matches!(form.kind, Kind::Primitive(_)) {
            object.get(&format!("_{}", form.key))
        } else {
            None
        };
        let base = match form.choice {
            Some(code) => format!("{location}.{}.ofType({code})", field.name),
            None => format!("{location}.{}", form.key),
        };
        let mut found = Vec::new();
        if !field.many {
            if value.is_some_and(|value| value.as_array().is_some())
                || element.is_some_and(|value| value.as_array().is_some())
            {
                self.shape(&base, "a single element is not written as an array");
            } else if value.is_some_and(Value::is_null) || element.is_some_and(Value::is_null) {
                self.null(&base);
            } else if value.is_some() || element.is_some() {
                found.push(Occurrence {
                    value,
                    element,
                    location: base,
                });
            }
            return found;
        }
        let values = match value.map(|value| value.as_array().ok_or(())) {
            Some(Err(())) => return self.shape_none(&base),
            Some(Ok(items)) => Some(items),
            None => None,
        };
        let elements = match element.map(|value| value.as_array().ok_or(())) {
            Some(Err(())) => return self.shape_none(&base),
            Some(Ok(items)) => Some(items),
            None => None,
        };
        for items in [values, elements].into_iter().flatten() {
            if items.is_empty() {
                self.out.issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::EmptyArray,
                    &base,
                    "the array is empty".to_owned(),
                );
            }
        }
        if let (Some(values), Some(elements)) = (values, elements)
            && values.len() != elements.len()
        {
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::MisalignedArrays,
                &base,
                format!(
                    "the value array and the _{} array differ in length",
                    form.key
                ),
            );
        }
        let length = values
            .map_or(0, <[Value]>::len)
            .max(elements.map_or(0, <[Value]>::len));
        for index in 0..length {
            let at = format!("{base}[{index}]");
            let value = values
                .and_then(|items| items.get(index))
                .filter(|item| !item.is_null());
            let element = elements
                .and_then(|items| items.get(index))
                .filter(|item| !item.is_null());
            if value.is_none() && element.is_none() {
                self.null(&at);
                continue;
            }
            found.push(Occurrence {
                value,
                element,
                location: at,
            });
        }
        found
    }

    /// Reports a wrong shape and yields no occurrence.
    fn shape_none<'o>(&mut self, location: &str) -> Vec<Occurrence<'o>> {
        self.shape(location, "a repeating element is written as an array");
        Vec::new()
    }

    /// Reports a member written in the wrong shape (json.html §2.6.2).
    fn shape(&mut self, location: &str, message: &str) {
        self.out.issue(
            Severity::Error,
            IssueType::Structure,
            MessageId::WrongShape,
            location,
            message.to_owned(),
        );
    }

    /// Reports a `null` no aligned primitive array needs (json.html §2.6.2.3).
    fn null(&mut self, location: &str) {
        self.out.issue(
            Severity::Error,
            IssueType::Structure,
            MessageId::NullValue,
            location,
            "the value is null".to_owned(),
        );
    }

    /// Checks the occurrence count of a member against its base cardinality
    /// and, when the base is met, against the profile's.
    pub(super) fn cardinality(
        &mut self,
        field: &FieldSchema,
        profiled: Option<&ElementDefinition>,
        count: usize,
        id: &str,
        location: &str,
    ) {
        let base = (field.min, field.max);
        let profile = profiled.map(|element| {
            let min = element
                .min
                .as_ref()
                .and_then(|min| min.value)
                .unwrap_or(field.min);
            let max = match element.max.as_ref().and_then(|max| max.value.as_deref()) {
                Some("*") => None,
                Some(max) => max.parse().ok().or(field.max),
                None => field.max,
            };
            (min, max)
        });
        for (min, max) in std::iter::once(base).chain(profile) {
            let below = usize::try_from(min).is_ok_and(|min| count < min);
            let above = max
                .and_then(|max| usize::try_from(max).ok())
                .is_some_and(|max| count > max);
            if below {
                self.out.issue(
                    Severity::Error,
                    IssueType::Required,
                    MessageId::MinCardinality,
                    location,
                    format!("{id} occurs {count} times, below its minimum of {min}"),
                );
                return;
            }
            if above {
                let limit = max.map_or_else(|| "*".to_owned(), |max| max.to_string());
                self.out.issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::MaxCardinality,
                    &format!("{location}.{}", field.name),
                    format!("{id} occurs {count} times, above its maximum of {limit}"),
                );
                return;
            }
        }
    }
}
/// The forms of `field` present in `object`: the element itself, or each
/// type of a choice the object carries a member for, value or `_` sibling.
pub(super) fn forms(field: &'static FieldSchema, object: &Object) -> Vec<Form> {
    let present = |key: &str, kind: Kind| {
        object.contains_key(key)
            || (matches!(kind, Kind::Primitive(_)) && object.contains_key(&format!("_{key}")))
    };
    match field.kind {
        Kind::Choice(_) => field
            .choices()
            .filter_map(|choice| {
                let key = format!("{}{}", field.name, choice.suffix);
                present(&key, choice.kind).then_some(Form {
                    key,
                    kind: choice.kind,
                    choice: Some(choice.code),
                })
            })
            .collect(),
        kind => {
            if present(field.name, kind) {
                vec![Form {
                    key: field.name.to_owned(),
                    kind,
                    choice: None,
                }]
            } else {
                Vec::new()
            }
        }
    }
}
