// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The content of one occurrence, walked by the kind its form carries: a
//! primitive's lexical form, a datatype or backbone element against its
//! cursor and the profiles its type names, an extension against the
//! definition its url resolves to, and a resource inside another.

use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::definition::ElementDefinitionRead;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;
use fhir_types::schema::ValueKind;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::release::Release;
use crate::validate::ValidateError;
use crate::validate::extension::Placement;
use crate::validate::profiles::Subject;
use crate::validate::profiles::profiles_for;
use crate::validate::profiles::resource_profiles;
use crate::validate::walk::Cursor;
use crate::validate::walk::Form;
use crate::validate::walk::Held;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;
use crate::validate::walk::type_named;

impl<'i, R: Release> Walk<'_, '_, 'i, R> {
    /// Walks the content of one occurrence by the kind its form carries.
    pub(super) fn occurrence(
        &mut self,
        schema: &'static TypeSchema,
        field: &'static FieldSchema,
        form: &Form,
        occurrence: &Occurrence<'i>,
        cursor: &Cursor<R>,
        target: Held<'_, R>,
    ) -> Result<(), ValidateError> {
        let id = target.id;
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
                self.primitive_profiles(cursor, target, occurrence, code)?;
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
                let location = &occurrence.location;
                if name == "Reference" {
                    self.reference(value, location);
                }
                if field.content_reference.is_none() && name == "Extension" {
                    let placement = if field.name == "modifierExtension" {
                        Placement::Modifier
                    } else if schema.name == "Extension" {
                        Placement::Child
                    } else {
                        Placement::Extension
                    };
                    return self.extension(value, placement, cursor, target, location);
                }
                let inline = field.content_reference.is_some()
                    || cursor.structure.has_children(id)
                    || type_named(R::schemas(), name)?.path.contains('.');
                let profiles = profiles_for(target.element, name);
                let subject = Subject::Complex { value, name };
                if !profiles.is_empty()
                    && !self.type_profiles(cursor, target, subject, &profiles, location)?
                    && !inline
                {
                    return Ok(());
                }
                let inner = if let Some(reference) = field.content_reference {
                    Cursor::new(Arc::clone(&cursor.structure), reference)
                } else if inline {
                    Cursor::new(Arc::clone(&cursor.structure), id)
                } else {
                    Cursor::new(self.validator.core(name)?, name)
                };
                self.complex(value, name, &inner, location)?;
            }
            Kind::Resource => {
                if let Some(value) = occurrence.value {
                    self.inner_resource(value, cursor, target, &occurrence.location)?;
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

    /// Walks a resource inside another: its type against its element's types,
    /// and its content against the profiles they name or its core definition.
    fn inner_resource(
        &mut self,
        value: &'i Value,
        cursor: &Cursor<R>,
        target: Held<'_, R>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let kind = value.get("resourceType").and_then(Value::as_str);
        if let Some(kind) = kind {
            self.resource_type(target, kind, location);
        }
        let profiles = kind.map_or_else(Vec::new, |kind| {
            resource_profiles(R::schemas(), target.element, kind)
        });
        let subject = Subject::Resource { value };
        if profiles.is_empty()
            || self.type_profiles(cursor, target, subject, &profiles, location)?
        {
            self.resource(value, Some(location), None)?;
        }
        Ok(())
    }

    /// Reports a resource of the type `kind` that none of the types of its
    /// element names or is specialized by (ElementDefinition.type: "The data
    /// type or resource that the value of this element is permitted to be").
    ///
    /// A type names its specializations too: `Resource` takes any resource
    /// and `DomainResource` any resource whose base chain reaches it
    /// (<https://hl7.org/fhir/R4/domainresource.html>), as the element
    /// table's base types state.
    fn resource_type(&mut self, target: Held<'_, R>, kind: &str, location: &str) {
        let Some(element) = target.element else {
            return;
        };
        let types = element.types();
        let codes: Vec<&str> = types.iter().filter_map(|entry| entry.code).collect();
        let schemas = R::schemas();
        let allowed = codes.is_empty() || codes.iter().any(|code| schemas.specializes(kind, code));
        if !allowed {
            self.out.issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::ResourceTypeNotAllowed,
                location,
                format!(
                    "{} allows the resource types {}, not {kind}",
                    target.id,
                    codes.join(", ")
                ),
            );
        }
    }

    /// Walks an extension against the definition its url resolves to, or
    /// against the slice it was matched to when the snapshot defines that
    /// slice inline.
    fn extension(
        &mut self,
        value: &'i Value,
        placement: Placement,
        cursor: &Cursor<R>,
        target: Held<'_, R>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let resolved = match value.as_object() {
            Some(object) => {
                self.extension_cursor((value, object), placement, (location, &cursor.structure))?
            }
            None => Cursor::new(self.validator.core("Extension")?, "Extension"),
        };
        // NOTE: profiling.html §5.1.0.11: extensions are sliced by url, so an extension
        // matched to a slice the snapshot defines inline is held to that slice.
        let inner = if target.slices.is_empty() {
            resolved
        } else {
            Cursor::new(Arc::clone(&cursor.structure), target.id)
        };
        self.extension_profiles(cursor, target, value, location)?;
        self.complex(value, "Extension", &inner, location)
    }
}
