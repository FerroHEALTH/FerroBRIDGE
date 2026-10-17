// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The content rules of a document Bundle that no invariant encodes
//! (<https://hl7.org/fhir/R4/documents.html#content>, §3.3.1): "Resources
//! referenced by the Composition as listed below SHALL be included in the
//! bundle", and "The document bundle SHALL include only" the Composition with
//! what it references directly or indirectly, a stylesheet `Binary` and the
//! `Provenance` of what the document includes.
//!
//! The Composition is the first entry; a document whose first entry is none
//! is the invariant `bdl-11`'s to report, and these rules are not applied to
//! it.

use std::collections::BTreeSet;

use fhir_types::codec::Value;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::validate::reference::address;
use crate::validate::reference::literal;
use crate::validate::resolver::Resolution;
use crate::validate::resolver::Via;
use crate::validate::walk::Walk;

/// One reference the Composition holds that SHALL resolve inside the
/// document: its location below the Composition, its element path, and the
/// `Reference`.
type Required<'i> = (String, String, &'i Value);

impl<'i> Walk<'_, '_, 'i> {
    /// Applies the document rules to `bundle`, the Bundle at `location`,
    /// when its type is `document`.
    pub(crate) fn document(&mut self, bundle: &'i Value, location: &str) {
        if bundle.get("type").and_then(Value::as_str) != Some("document") {
            return;
        }
        let entries = bundle
            .get("entry")
            .and_then(Value::as_array)
            .unwrap_or_default();
        let Some(composition) = entries
            .first()
            .and_then(|entry| entry.get("resource"))
            .filter(|resource| resource_type(resource) == Some("Composition"))
        else {
            return;
        };
        let mut required = Vec::new();
        composition_references(composition, &mut required);
        for (path, element, reference) in required {
            let Some(reference) = literal(reference) else {
                continue;
            };
            match self.resolve_from(bundle, composition, reference) {
                Resolution::Found { .. } | Resolution::Ambiguous => {}
                Resolution::Missing | Resolution::OutOfScope => self.out().issue(
                    Severity::Error,
                    IssueType::NotFound,
                    MessageId::DocumentReferenceMissing,
                    &format!("{location}.entry[0].resource.{path}"),
                    format!(
                        "the resource {element} references is not in the document Bundle, which SHALL include it"
                    ),
                ),
            }
        }
        let included = self.included(composition);
        for (index, entry) in entries.iter().enumerate().skip(1) {
            let Some(resource) = entry.get("resource") else {
                continue;
            };
            if included.contains(&address(resource))
                || stylesheet(resource)
                || self.provenance_of(bundle, resource, &included)
            {
                continue;
            }
            self.out().issue(
                Severity::Error,
                IssueType::Invalid,
                MessageId::DocumentEntryUnreferenced,
                &format!("{location}.entry[{index}]"),
                format!(
                    "the document Bundle holds a {} the Composition does not reference, directly or indirectly, and that is no stylesheet Binary or Provenance of the document",
                    resource_type(resource).unwrap_or("resource")
                ),
            );
        }
    }

    /// Resolves `reference`, held by `resource`, an entry of `bundle`.
    fn resolve_from(
        &self,
        bundle: &'i Value,
        resource: &'i Value,
        reference: &str,
    ) -> Resolution<'i> {
        let mut scope = self.scope().to_vec();
        scope.push(bundle);
        scope.push(resource);
        self.resolve_reference_in(reference, &scope)
    }

    /// The addresses of the Composition and of every entry it references,
    /// directly or indirectly, through the references the walk resolved.
    fn included(&self, composition: &'i Value) -> BTreeSet<usize> {
        let mut included = BTreeSet::from([address(composition)]);
        let mut pending = vec![address(composition)];
        while let Some(from) = pending.pop() {
            for to in self.links(from) {
                if included.insert(to) {
                    pending.push(to);
                }
            }
        }
        included
    }

    /// Answers whether `resource` is a Provenance with a target the
    /// document includes.
    fn provenance_of(
        &self,
        bundle: &'i Value,
        resource: &'i Value,
        included: &BTreeSet<usize>,
    ) -> bool {
        resource_type(resource) == Some("Provenance")
            && members(resource, "target").into_iter().any(|(_, target)| {
                literal(target).is_some_and(|reference| {
                    matches!(
                        self.resolve_from(bundle, resource, reference),
                        Resolution::Found { target, via: Via::Entry { .. } }
                            if included.contains(&address(target))
                    )
                })
            })
    }
}

/// The `resourceType` of `resource`.
fn resource_type(resource: &Value) -> Option<&str> {
    resource.get("resourceType").and_then(Value::as_str)
}

/// Answers whether `resource` is a Binary carrying a CSS stylesheet.
fn stylesheet(resource: &Value) -> bool {
    // NOTE: documents.html §3.3.1.1: "Stylesheet references can only refer to a CSS
    // stylesheet", so the Binary of §3.3.1 item 2 is one whose contentType is text/css.
    resource_type(resource) == Some("Binary")
        && resource
            .get("contentType")
            .and_then(Value::as_str)
            .is_some_and(|kind| {
                kind.split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/css"))
            })
}

/// The occurrences of the member `key` of `value`, each with its location
/// step.
fn members<'i>(value: &'i Value, key: &str) -> Vec<(String, &'i Value)> {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("{key}[{index}]"), item))
            .collect(),
        Some(item) => vec![(key.to_owned(), item)],
        None => Vec::new(),
    }
}

/// Collects the references documents.html §3.3.1 lists: `subject`,
/// `encounter`, `author`, `attester.party`, `custodian`, `event.detail`, and
/// `section.author`, `section.focus` and `section.entry` at every depth.
fn composition_references<'i>(composition: &'i Value, out: &mut Vec<Required<'i>>) {
    for key in ["subject", "encounter", "author", "custodian"] {
        for (path, reference) in members(composition, key) {
            out.push((path, format!("Composition.{key}"), reference));
        }
    }
    for (outer, inner) in [("attester", "party"), ("event", "detail")] {
        for (step, parent) in members(composition, outer) {
            for (path, reference) in members(parent, inner) {
                out.push((
                    format!("{step}.{path}"),
                    format!("Composition.{outer}.{inner}"),
                    reference,
                ));
            }
        }
    }
    sections(composition, "", out);
}

/// Collects the references of the sections of `parent` and of every section
/// below them.
fn sections<'i>(parent: &'i Value, prefix: &str, out: &mut Vec<Required<'i>>) {
    for (step, section) in members(parent, "section") {
        let at = format!("{prefix}{step}");
        for key in ["author", "focus", "entry"] {
            for (path, reference) in members(section, key) {
                out.push((
                    format!("{at}.{path}"),
                    format!("Composition.section.{key}"),
                    reference,
                ));
            }
        }
        sections(section, &format!("{at}."), out);
    }
}
