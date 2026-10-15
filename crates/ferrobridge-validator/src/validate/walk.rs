// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The walk over an instance: every object against its type in the R4
//! element table, every member's occurrences counted per parent, and every
//! element definition in play applied to each occurrence.

use std::sync::Arc;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;
use fhir_types::schema::ValueKind;

use crate::canonical::Canonical;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Collector;
use crate::report::Constraint;
use crate::report::Reason;
use crate::report::Report;
use crate::validate::ValidateError;
use crate::validate::Validator;
use crate::validate::extension::Placement;
use crate::validate::structure::Structure;

/// The definition an object is held to: a snapshot and the id of the
/// object's own element in it.
#[derive(Debug, Clone)]
pub(crate) struct Cursor {
    structure: Arc<Structure>,
    prefix: String,
}

impl Cursor {
    /// Creates a cursor at element `prefix` of `structure`.
    pub(crate) fn new(structure: Arc<Structure>, prefix: &str) -> Self {
        Self {
            structure,
            prefix: prefix.to_owned(),
        }
    }
}

/// One occurrence of a member: its value, its `_` sibling for a primitive,
/// and where it sits.
#[derive(Debug)]
struct Occurrence<'v> {
    value: Option<&'v Value>,
    element: Option<&'v Value>,
    location: String,
}

/// The form a member takes in one object: the element itself, or one type
/// of a choice.
#[derive(Debug, Clone)]
struct Form {
    /// The JSON member name.
    key: String,
    /// The kind it carries.
    kind: Kind,
    /// The type code of a choice form.
    choice: Option<&'static str>,
}

/// One validation in progress.
#[derive(Debug)]
pub(crate) struct Walk<'v, 'set> {
    validator: &'v Validator<'set>,
    out: Collector,
}

impl<'v, 'set> Walk<'v, 'set> {
    /// Starts a validation.
    pub(crate) fn new(validator: &'v Validator<'set>) -> Self {
        Self {
            validator,
            out: Collector::default(),
        }
    }

    /// Returns the validator.
    pub(crate) const fn validator(&self) -> &'v Validator<'set> {
        self.validator
    }

    /// Returns the collector.
    pub(crate) const fn out(&mut self) -> &mut Collector {
        &mut self.out
    }

    /// Validates `resource`, against `profile` or its core definition.
    pub(crate) fn run(
        mut self,
        resource: &Value,
        profile: Option<&str>,
    ) -> Result<Report, ValidateError> {
        self.resource(resource, None, profile)?;
        Ok(self.out.finish())
    }

    /// Validates one resource: the root, or one inside another.
    fn resource(
        &mut self,
        value: &Value,
        location: Option<&str>,
        profile: Option<&str>,
    ) -> Result<(), ValidateError> {
        let severity = if location.is_none() {
            Severity::Fatal
        } else {
            Severity::Error
        };
        let Some(object) = value.as_object() else {
            self.out.issue(
                severity,
                IssueType::Structure,
                MessageId::NotAnObject,
                location.unwrap_or_default(),
                "a resource is a JSON object".to_owned(),
            );
            return Ok(());
        };
        let resource_type = object.get("resourceType").and_then(Value::as_str);
        let Some((name, schema)) = resource_type
            .filter(|name| SCHEMAS.is_resource(name))
            .and_then(|name| Some((name, SCHEMAS.type_named(name)?)))
        else {
            self.out.issue(
                severity,
                IssueType::Structure,
                MessageId::UnknownResourceType,
                location.unwrap_or_default(),
                "resourceType names no R4 resource".to_owned(),
            );
            return Ok(());
        };
        let location = location.unwrap_or(name).to_owned();
        let mut structure = match profile {
            Some(profile) => self.validator.structure(profile)?,
            None => self.validator.core(name)?,
        };
        if structure.type_name() != name {
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::ProfileTypeMismatch,
                &location,
                format!(
                    "the profile {} constrains {}, not {name}",
                    structure.url(),
                    structure.type_name()
                ),
            );
            structure = self.validator.core(name)?;
        }
        self.claims(object, &structure, name, &location);
        self.object(
            object,
            schema,
            &Cursor::new(structure, name),
            &location,
            true,
        )
    }

    /// Lists each profile the resource claims and is not validated against.
    fn claims(&mut self, object: &Object, structure: &Structure, name: &str, location: &str) {
        let claimed = object
            .get("meta")
            .and_then(|meta| meta.get("profile"))
            .and_then(Value::as_array)
            .unwrap_or_default();
        for url in claimed.iter().filter_map(Value::as_str) {
            if Canonical::parse(url).url() != structure.url() {
                // TODO(#441): validate the resource against every profile it claims.
                self.out.unevaluated(
                    url,
                    name,
                    Constraint::ClaimedProfile {
                        url: url.to_owned(),
                    },
                    location,
                    Reason::Profiles,
                );
            }
        }
    }

    /// Walks one object of the type `schema` against `cursor`.
    fn object(
        &mut self,
        object: &Object,
        schema: &'static TypeSchema,
        cursor: &Cursor,
        location: &str,
        resource: bool,
    ) -> Result<(), ValidateError> {
        if object.is_empty() {
            // NOTE: json.html §2.6.2: "Objects are never empty".
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::EmptyObject,
                location,
                "the object is empty".to_owned(),
            );
        }
        if let Some(element) = cursor.structure.element(&cursor.prefix) {
            self.list(&cursor.structure, &cursor.prefix, element, location);
        }
        for key in object.keys() {
            if resource && key == "resourceType" {
                continue;
            }
            if !knows(schema, key) {
                self.out.issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::UnknownElement,
                    &format!("{location}.{key}"),
                    format!("{} defines no element {key}", schema.path),
                );
            }
        }
        for field in schema.fields {
            self.field(object, schema, field, cursor, location)?;
        }
        Ok(())
    }

    /// Walks every occurrence of `field` in `object`.
    fn field(
        &mut self,
        object: &Object,
        schema: &'static TypeSchema,
        field: &'static FieldSchema,
        cursor: &Cursor,
        location: &str,
    ) -> Result<(), ValidateError> {
        let forms = forms(field, object);
        if forms.len() > 1 {
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::DuplicateChoice,
                &format!("{location}.{}", field.name),
                format!("{} carries more than one type", field.path),
            );
        }
        let segment = field.path.rsplit('.').next().unwrap_or(field.name);
        let element_id = format!("{}.{segment}", cursor.prefix);
        let mut count = 0_usize;
        let mut walked = Vec::new();
        for form in forms {
            let occurrences = self.occurrences(object, field, &form, location);
            count = count.saturating_add(occurrences.len());
            walked.push((form, occurrences));
        }
        let profiled = cursor.structure.element(&element_id);
        self.cardinality(field, profiled, count, &element_id, location);
        for (form, occurrences) in walked {
            // NOTE: no specification governs this lookup: our own design; a snapshot that
            // names a choice by one type's form (`Observation.valueQuantity`) wins over `[x]`.
            let renamed = format!("{}.{}", cursor.prefix, form.key);
            let (id, element) = match cursor.structure.element(&renamed) {
                Some(element) if form.choice.is_some() => (renamed, Some(element)),
                _ => (element_id.clone(), profiled),
            };
            if let (Some(code), Some(element)) = (form.choice, element)
                && !element.r#type.is_empty()
                && !element
                    .r#type
                    .iter()
                    .any(|kind| kind.code.value.as_deref() == Some(code))
            {
                let at = occurrences
                    .first()
                    .map_or(location, |first| first.location.as_str());
                self.out.issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::ChoiceTypeNotAllowed,
                    at,
                    format!("{id} does not allow the type {code}"),
                );
            }
            for occurrence in &occurrences {
                if let Some(element) = element {
                    self.list(&cursor.structure, &id, element, &occurrence.location);
                    self.fixed_and_pattern(
                        element,
                        cursor.structure.url(),
                        &id,
                        occurrence.value,
                        &occurrence.location,
                    )?;
                    let limit = element.max_length.as_ref().and_then(|length| length.value);
                    self.max_length(occurrence.value, limit, &id, &occurrence.location);
                }
                self.occurrence(schema, field, &form, occurrence, cursor, &id)?;
            }
        }
        Ok(())
    }

    /// Collects the occurrences of one form, refusing a wrong shape, an empty
    /// array, a stray `null` and misaligned primitive arrays.
    fn occurrences<'o>(
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
    fn cardinality(
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

    /// Walks the content of one occurrence by the kind its form carries.
    fn occurrence(
        &mut self,
        schema: &'static TypeSchema,
        field: &'static FieldSchema,
        form: &Form,
        occurrence: &Occurrence<'_>,
        cursor: &Cursor,
        id: &str,
    ) -> Result<(), ValidateError> {
        let code = form.choice.or_else(|| field.types.first().copied());
        let primitive_code = || {
            code.ok_or_else(|| ValidateError::UnknownType {
                name: field.path.to_owned(),
            })
        };
        match form.kind {
            Kind::Attribute => {
                if let Some(value) = occurrence.value {
                    let code = primitive_code()?;
                    self.primitive_value(value, code, ValueKind::Text, &occurrence.location)?;
                }
            }
            Kind::Primitive(kind) => {
                let code = primitive_code()?;
                if let Some(value) = occurrence.value {
                    self.primitive_value(value, code, kind, &occurrence.location)?;
                }
                if let Some(element) = occurrence.element {
                    let inner = if cursor.structure.has_children(id) {
                        Cursor::new(Arc::clone(&cursor.structure), id)
                    } else {
                        Cursor::new(self.validator.core(code)?, code)
                    };
                    self.complex(element, "Element", &inner, &occurrence.location)?;
                }
            }
            Kind::Complex(name) => {
                let Some(value) = occurrence.value else {
                    return Ok(());
                };
                let inner = if let Some(target) = field.content_reference {
                    Cursor::new(Arc::clone(&cursor.structure), target)
                } else if name == "Extension" {
                    let placement = if field.name == "modifierExtension" {
                        Placement::Modifier
                    } else if schema.name == "Extension" {
                        Placement::Child
                    } else {
                        Placement::Extension
                    };
                    match value.as_object() {
                        Some(object) => {
                            self.extension_cursor(object, placement, &occurrence.location)?
                        }
                        None => Cursor::new(self.validator.core(name)?, name),
                    }
                } else if cursor.structure.has_children(id) || type_named(name)?.path.contains('.')
                {
                    Cursor::new(Arc::clone(&cursor.structure), id)
                } else {
                    Cursor::new(self.validator.core(name)?, name)
                };
                self.complex(value, name, &inner, &occurrence.location)?;
            }
            Kind::Resource => {
                if let Some(value) = occurrence.value {
                    self.resource(value, Some(&occurrence.location), None)?;
                }
            }
            Kind::Xhtml => match occurrence.value {
                Some(Value::String(text)) if text.is_empty() => {
                    self.out.issue(
                        Severity::Error,
                        IssueType::Value,
                        MessageId::EmptyString,
                        &occurrence.location,
                        "the value is an empty string".to_owned(),
                    );
                }
                Some(Value::String(_)) | None => {}
                Some(_) => self.out.issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::WrongJsonType,
                    &occurrence.location,
                    "an xhtml is written as a JSON string".to_owned(),
                ),
            },
            Kind::Choice(_) => {}
        }
        Ok(())
    }

    /// Walks a value that SHALL be an object of the type `name`.
    fn complex(
        &mut self,
        value: &Value,
        name: &str,
        cursor: &Cursor,
        location: &str,
    ) -> Result<(), ValidateError> {
        let Some(object) = value.as_object() else {
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::WrongJsonType,
                location,
                format!("a {name} is written as a JSON object"),
            );
            return Ok(());
        };
        self.object(object, type_named(name)?, cursor, location, false)
    }
}

/// Returns the element-table entry of the type `name`.
fn type_named(name: &str) -> Result<&'static TypeSchema, ValidateError> {
    SCHEMAS
        .type_named(name)
        .ok_or_else(|| ValidateError::UnknownType {
            name: name.to_owned(),
        })
}

/// The forms of `field` present in `object`: the element itself, or each
/// type of a choice the object carries a member for, value or `_` sibling.
fn forms(field: &'static FieldSchema, object: &Object) -> Vec<Form> {
    let present = |key: &str, kind: Kind| {
        object.contains_key(key)
            || (matches!(kind, Kind::Primitive(_)) && object.contains_key(&format!("_{key}")))
    };
    match field.kind {
        Kind::Choice(variants) => variants
            .iter()
            .filter_map(|(suffix, kind)| {
                let code = field
                    .types
                    .iter()
                    .copied()
                    .find(|code| upper_first(code) == *suffix)
                    .unwrap_or(suffix);
                let key = format!("{}{suffix}", field.name);
                present(&key, *kind).then_some(Form {
                    key,
                    kind: *kind,
                    choice: Some(code),
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

/// Answers whether `key` names a member of `schema`: an element, a choice
/// form, or the `_` sibling of a primitive one (json.html §2.6.2.3).
fn knows(schema: &TypeSchema, key: &str) -> bool {
    let (name, sibling) = match key.strip_prefix('_') {
        Some(name) => (name, true),
        None => (key, false),
    };
    schema.fields.iter().any(|field| match field.kind {
        Kind::Choice(variants) => variants.iter().any(|(suffix, kind)| {
            name.strip_prefix(field.name) == Some(*suffix)
                && (!sibling || matches!(kind, Kind::Primitive(_)))
        }),
        kind => field.name == name && (!sibling || matches!(kind, Kind::Primitive(_))),
    })
}

/// `code` with its first letter in upper case, the suffix a choice form
/// carries (json.html §2.6.2).
fn upper_first(code: &str) -> String {
    let mut chars = code.chars();
    chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}
