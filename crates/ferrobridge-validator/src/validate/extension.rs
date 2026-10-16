// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Extensions: the `url` resolved to its `StructureDefinition`, which the
//! extension is then walked against (<https://hl7.org/fhir/R4/extensibility.html>).

use fhir_types::codec::Object;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::Reason;
use crate::set::ResolveError;
use crate::validate::ValidateError;
use crate::validate::structure::Structure;
use crate::validate::walk::Cursor;
use crate::validate::walk::Walk;

/// Where an extension sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// In `extension` of an element other than an extension.
    Extension,
    /// In `modifierExtension`.
    Modifier,
    /// In `extension` of an extension: a child of a complex extension.
    Child,
}

impl Walk<'_, '_, '_> {
    /// Returns the cursor an extension is walked with: the definition its
    /// `url` names, resolved as the definition `from` the walk holds the
    /// extension's parent to, or the core `Extension` when the url does not
    /// resolve.
    pub(crate) fn extension_cursor(
        &mut self,
        object: &Object,
        placement: Placement,
        (location, from): (&str, &Structure),
    ) -> Result<Cursor, ValidateError> {
        let fallback = |walk: &Self| -> Result<Cursor, ValidateError> {
            Ok(Cursor::new(
                walk.validator().core("Extension")?,
                "Extension",
            ))
        };
        // A missing or non-string url is reported by the walk over the element
        // table (Extension.url is 1..1 uri), so it falls back without an issue here.
        let Some(url) = object.get("url").and_then(|url| url.as_str()) else {
            return fallback(self);
        };
        if !url.contains(':') {
            if placement != Placement::Child {
                self.out().issue(
                    Severity::Error,
                    IssueType::Extension,
                    MessageId::RelativeExtensionUrl,
                    location,
                    format!("the extension url {url} is relative outside a complex extension"),
                );
            }
            // NOTE: extensibility.html §2.5.0.1: a relative url names a child the complex extension
            // defines, so the slice it is matched to by url holds it, never a definition of its own.
            return fallback(self);
        }
        let structure = match self.validator().structure(url, Some(from)) {
            Ok(structure) => structure,
            // A url of the instance that names no definition the set resolves (absent, a `#`
            // fragment, two definitions under one canonical) is the instance's unknown extension;
            // a definition the set carries and cannot read stays an error of the package set.
            Err(ValidateError::Resolve(
                error @ (ResolveError::NotFound { .. }
                | ResolveError::Fragment { .. }
                | ResolveError::Ambiguous { .. }),
            )) => {
                // NOTE: extensibility.html §2.5.0.2 forbids processing an unknown modifier, while
                // §2.5.0.3 says to ignore other unknown extensions; the warning is our own design.
                let (severity, id) = if placement == Placement::Modifier {
                    (Severity::Error, MessageId::UnknownModifierExtension)
                } else {
                    (Severity::Warning, MessageId::UnknownExtension)
                };
                let why = match error {
                    ResolveError::Ambiguous { .. } => {
                        "the package set carries different definitions of the extension"
                    }
                    _ => "the package set defines no extension",
                };
                self.out().issue(
                    severity,
                    IssueType::Extension,
                    id,
                    location,
                    format!("{why} {url}"),
                );
                return fallback(self);
            }
            Err(error) => return Err(error),
        };
        if structure.type_name() != "Extension" {
            self.out().issue(
                Severity::Error,
                IssueType::Extension,
                MessageId::NotAnExtension,
                location,
                format!(
                    "{url} defines a {}, not an extension",
                    structure.type_name()
                ),
            );
            return fallback(self);
        }
        let modifier = structure
            .element("Extension")
            .and_then(|root| root.is_modifier.as_ref())
            .and_then(|flag| flag.value)
            .unwrap_or(false);
        if modifier != (placement == Placement::Modifier) {
            // NOTE: extensibility.html §2.5.0.2: a modifier extension is carried in
            // modifierExtension, and an extension defined as none is not.
            let wanted = if modifier {
                "modifierExtension"
            } else {
                "extension"
            };
            self.out().issue(
                Severity::Error,
                IssueType::Extension,
                MessageId::ModifierMismatch,
                location,
                format!("the extension {url} belongs in {wanted}"),
            );
        }
        if !structure.definition().context.is_empty() {
            // TODO(#481): check the contexts StructureDefinition.context allows an extension in.
            self.out().unevaluated(
                structure.url(),
                "Extension",
                Constraint::ExtensionContext,
                location,
                Reason::ExtensionContext,
            );
        }
        Ok(Cursor::new(structure, "Extension"))
    }
}
