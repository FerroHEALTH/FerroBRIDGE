// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The walk over an instance: every object against its type in the R4
//! element table, every member's occurrences counted per parent, and every
//! element definition in play applied to each occurrence.

use std::collections::BTreeSet;
use std::sync::Arc;

mod content;
pub(crate) mod members;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;

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
use crate::validate::slicing::Item;
use crate::validate::slicing::Site;
use crate::validate::structure::Structure;
use crate::validate::terminology::binding::Bound;
use members::forms;
use members::knows;

/// The definition an object is held to: a snapshot and the id of the
/// object's own element in it.
#[derive(Debug, Clone)]
pub(crate) struct Cursor {
    /// The snapshot.
    pub(crate) structure: Arc<Structure>,
    /// The id of the object's own element.
    pub(crate) prefix: String,
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
pub(crate) struct Occurrence<'v> {
    /// The value, absent for a primitive written only as its `_` sibling.
    pub(crate) value: Option<&'v Value>,
    /// The `_` sibling of a primitive.
    pub(crate) element: Option<&'v Value>,
    /// The FHIRPath location.
    pub(crate) location: String,
}

/// The form a member takes in one object: the element itself, or one type
/// of a choice.
#[derive(Debug, Clone)]
pub(crate) struct Form {
    /// The JSON member name.
    pub(crate) key: String,
    /// The kind it carries.
    pub(crate) kind: Kind,
    /// The type code of a choice form.
    pub(crate) choice: Option<&'static str>,
}

/// The element one occurrence is held to: the element of its member, and
/// the slices it was matched to, outermost first.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Held<'a> {
    /// The element id of the member.
    pub(crate) id: &'a str,
    /// The member's element definition, when the snapshot carries one.
    pub(crate) element: Option<&'a ElementDefinition>,
    /// The ids of the slices the occurrence belongs to.
    pub(crate) slices: &'a [String],
}

/// One validation in progress.
#[derive(Debug)]
pub(crate) struct Walk<'v, 'set, 'i> {
    validator: &'v Validator<'set>,
    out: Collector,
    /// The resources the walk is inside, the outermost first.
    scope: Vec<&'i Value>,
    /// The locations whose coding display was already checked.
    displayed: BTreeSet<String>,
}

impl<'v, 'set, 'i> Walk<'v, 'set, 'i> {
    /// Starts a validation.
    pub(crate) fn new(validator: &'v Validator<'set>) -> Self {
        Self {
            validator,
            out: Collector::default(),
            scope: Vec::new(),
            displayed: BTreeSet::new(),
        }
    }

    /// Starts a speculative validation inside the scope of this one, whose
    /// findings are its own.
    pub(crate) fn fork(&self) -> Self {
        Self {
            validator: self.validator,
            out: Collector::default(),
            scope: self.scope.clone(),
            displayed: BTreeSet::new(),
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

    /// Returns the collector, ending the walk.
    pub(crate) fn into_out(self) -> Collector {
        self.out
    }

    /// Answers whether the display at `location` is not yet checked, and
    /// marks it checked.
    pub(crate) fn first_display(&mut self, location: &str) -> bool {
        self.displayed.insert(location.to_owned())
    }

    /// Returns the resources the walk is inside, the outermost first.
    pub(crate) fn scope(&self) -> &[&'i Value] {
        &self.scope
    }

    /// Validates `resource`, against `profile` or its core definition.
    pub(crate) fn run(
        mut self,
        resource: &'i Value,
        profile: Option<&str>,
    ) -> Result<Report, ValidateError> {
        let profile = profile
            .map(|profile| self.validator.structure(profile, None))
            .transpose()?;
        self.resource(resource, None, profile)?;
        Ok(self.out.finish())
    }

    /// Validates one resource: the root, or one inside another, against
    /// `profile` or the core definition of its type.
    pub(crate) fn resource(
        &mut self,
        value: &'i Value,
        location: Option<&str>,
        profile: Option<Arc<Structure>>,
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
            Some(profile) => profile,
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
        self.scope.push(value);
        let walked = self.object(
            object,
            schema,
            &Cursor::new(structure, name),
            &location,
            true,
        );
        self.scope.pop();
        walked
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
        object: &'i Object,
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
            if !cursor.prefix.contains('.') && schema.name == cursor.structure.type_name() {
                let held = (cursor.prefix.as_str(), element);
                let bound = Bound::Object(object);
                self.binding(&cursor.structure, held, schema.name, bound, location)?;
            }
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
        object: &'i Object,
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
        let items: Vec<Item<'_, 'i>> = walked
            .iter()
            .flat_map(|(form, occurrences)| {
                occurrences
                    .iter()
                    .map(move |occurrence| Item { form, occurrence })
            })
            .collect();
        let site = Site {
            schema,
            field,
            cursor,
            id: &element_id,
            element: profiled,
            location,
        };
        let mut chains = match profiled.filter(|element| element.slicing.is_some()) {
            Some(entry) => self.slicing(&site, entry, &items)?,
            None => Vec::new(),
        }
        .into_iter();
        for (form, occurrences) in &walked {
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
            for occurrence in occurrences {
                let slices = chains.next().unwrap_or_default();
                let held = Held {
                    id: &id,
                    element,
                    slices: &slices,
                };
                self.apply(schema, field, form, occurrence, cursor, held)?;
            }
        }
        Ok(())
    }

    /// Holds one occurrence to its member's element and to every slice it
    /// was matched to, and walks its content against the innermost of them
    /// that the snapshot defines children for.
    pub(crate) fn apply(
        &mut self,
        schema: &'static TypeSchema,
        field: &'static FieldSchema,
        form: &Form,
        occurrence: &Occurrence<'i>,
        cursor: &Cursor,
        held: Held<'_>,
    ) -> Result<(), ValidateError> {
        let structure = Arc::clone(&cursor.structure);
        let code = form.choice.or_else(|| match form.kind {
            Kind::Complex(name) => Some(name),
            _ => field.types.first().copied(),
        });
        let mut content = held.id;
        let mut element = held.element;
        if let Some(base) = held.element {
            self.checks(&structure, held.id, base, occurrence, code)?;
        }
        for slice in held.slices {
            let Some(defined) = structure.element(slice) else {
                continue;
            };
            self.checks(&structure, slice, defined, occurrence, code)?;
            element = Some(defined);
            if structure.has_children(slice) {
                content = slice;
            }
        }
        let target = Held {
            id: content,
            element,
            slices: if content == held.id { &[] } else { held.slices },
        };
        self.occurrence(schema, field, form, occurrence, cursor, target)
    }

    /// Applies one element definition's own constraints to an occurrence.
    pub(crate) fn checks(
        &mut self,
        structure: &Structure,
        id: &str,
        element: &ElementDefinition,
        occurrence: &Occurrence<'_>,
        code: Option<&str>,
    ) -> Result<(), ValidateError> {
        self.list(structure, id, element, &occurrence.location);
        if let (Some(code), Some(value)) = (code, occurrence.value.and_then(Bound::of)) {
            self.binding(structure, (id, element), code, value, &occurrence.location)?;
        }
        self.fixed_and_pattern(
            element,
            structure.url(),
            id,
            occurrence.value,
            &occurrence.location,
        )?;
        let limit = element.max_length.as_ref().and_then(|length| length.value);
        self.max_length(occurrence.value, limit, id, &occurrence.location);
        Ok(())
    }

    /// Walks a value that SHALL be an object of the type `name`.
    pub(crate) fn complex(
        &mut self,
        value: &'i Value,
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
