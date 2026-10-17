// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where an extension may be used: the contexts and the context invariants
//! of its definition (<https://hl7.org/fhir/R4/defining-extensions.html#context>,
//! §2.5.1.2 and §2.5.1.3).
//!
//! "Extensions SHALL only be used on a target that appears in their context
//! list", and the three context types read as the R4 code system
//! `extension-context-type` defines them: a FHIRPath expression from the root
//! of the resource, an element id (`[url]#[elementid]`, or an id of the base
//! specification without a `#`), or the canonical of an extension with an
//! optional `#code` for a child of a complex extension. A context the walk
//! cannot decide is listed as unevaluated, and the extension is refused only
//! when every context is decided and none admits it.

use std::ptr;
use std::sync::Arc;

use ferrobridge_fhirpath::eval::Environment;
use ferrobridge_fhirpath::eval::Focus;
use ferrobridge_fhirpath::model::named_type;
use ferrobridge_fhirpath::model::r4::R4;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::r4::structure_definition::StructureDefinitionContext;

use super::CORE;
use super::invariant::Subject;
use super::invariant::chain;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::InvariantStage;
use crate::report::Reason;
use crate::validate::ValidateError;
use crate::validate::resolver::container;
use crate::validate::structure::Structure;
use crate::validate::walk::Host;
use crate::validate::walk::Walk;
use crate::validate::walk::occurrence_type;

/// How far a chain of types or of base definitions is followed before it is
/// taken to be a cycle.
const DEPTH: usize = 64;

/// Whether one context admits the element holding an extension.
#[derive(Debug)]
enum Fit {
    /// The context names the element.
    Yes,
    /// The context names another element.
    No,
    /// The walk cannot tell, for the reason given.
    Unknown(String),
}

/// The canonical `url` without a `|version` suffix.
fn versionless(url: &str) -> &str {
    url.split_once('|').map_or(url, |(url, _)| url)
}

/// The `url` of an extension object.
fn url_of(value: Option<&Value>) -> Option<&str> {
    value?.get("url")?.as_str()
}

impl<'i> Walk<'_, '_, 'i> {
    /// Holds the extension `extension` at `location`, defined by
    /// `structure`, to the contexts and context invariants its definition
    /// declares.
    pub(crate) fn extension_context(
        &mut self,
        structure: &Structure,
        extension: &'i Value,
        location: &str,
    ) -> Result<(), ValidateError> {
        let definition = structure.definition();
        if definition.context.is_empty() && definition.context_invariant.is_empty() {
            return Ok(());
        }
        // The walk pushes the extension's own occurrence last, so its holder sits below it.
        let hosts = self.hosts();
        let held = hosts.len().checked_sub(2);
        let Some((index, host)) = held.and_then(|index| Some((index, hosts.get(index)?.clone())))
        else {
            let reason = Reason::ExtensionContext(
                "the element holding the extension is not known to this walk".to_owned(),
            );
            self.out().unevaluated(
                structure.url(),
                "Extension",
                Constraint::ExtensionContext,
                location,
                reason,
            );
            return Ok(());
        };
        if !definition.context.is_empty() {
            self.contexts(structure, &definition.context, (index, &host), location)?;
        }
        for invariant in &definition.context_invariant {
            let text = invariant.value.as_deref().unwrap_or_default();
            self.context_invariant(structure, text, &host, extension, location);
        }
        Ok(())
    }

    /// Reports the extension at `location` when no context of `contexts`
    /// admits `host`, the element at `index` of the walk's hosts, and lists
    /// it when one the walk cannot decide might.
    fn contexts(
        &mut self,
        structure: &Structure,
        contexts: &[StructureDefinitionContext],
        (index, host): (usize, &Host<'i>),
        location: &str,
    ) -> Result<(), ValidateError> {
        let mut unknown = Vec::new();
        for context in contexts {
            let kind = context.r#type.value.as_deref().unwrap_or_default();
            let expression = context.expression.value.as_deref().unwrap_or_default();
            let fit = match kind {
                "element" => self.element_fits(expression, host)?,
                "extension" => self.extension_fits(expression, index),
                "fhirpath" => self.path_fits(expression, host),
                other => Fit::Unknown(format!("the context type {other} is none R4 defines")),
            };
            match fit {
                Fit::Yes => return Ok(()),
                Fit::No => {}
                Fit::Unknown(why) => unknown.push(format!("{kind} {expression}: {why}")),
            }
        }
        if unknown.is_empty() {
            let named: Vec<&str> = contexts
                .iter()
                .filter_map(|context| context.expression.value.as_deref())
                .collect();
            self.out().issue(
                Severity::Error,
                IssueType::Extension,
                MessageId::ExtensionContext,
                location,
                format!(
                    "the extension {} is used on {}, which none of its contexts ({}) names",
                    structure.url(),
                    host.path,
                    named.join(", ")
                ),
            );
        } else {
            self.out().unevaluated(
                structure.url(),
                "Extension",
                Constraint::ExtensionContext,
                location,
                Reason::ExtensionContext(unknown.join("; ")),
            );
        }
        Ok(())
    }

    /// Answers whether the element context `expression` names `host`.
    fn element_fits(&self, expression: &str, host: &Host<'i>) -> Result<Fit, ValidateError> {
        if let Some((url, id)) = expression.split_once('#') {
            // NOTE: R4 extension-context-type `element`: "[url]#[elementid]" names an element of
            // that definition, slice ids included; a derived profile keeps the ids of its base.
            if !self.derives(&host.structure, url)? {
                return Ok(Fit::Unknown(format!(
                    "the element is held to {}, which does not derive from {url}",
                    host.structure.url()
                )));
            }
            if host.ids.iter().any(|held| held == id) {
                return Ok(Fit::Yes);
            }
            // An item in no slice of the entry may be one its discriminators could not place.
            let unplaced = id.rsplit_once(':').is_some_and(|(entry, _)| {
                let inside = format!("{entry}:");
                host.ids.iter().any(|held| held == entry)
                    && !host.ids.iter().any(|held| held.starts_with(&inside))
            });
            return Ok(if unplaced {
                Fit::Unknown(format!(
                    "the element is in no slice of {}, and its placement may be undecided",
                    host.ids.first().map_or("", String::as_str)
                ))
            } else {
                Fit::No
            });
        }
        // NOTE: R4 core places its `Element`-context extensions (structuredefinition-fmm,
        // structuredefinition-wg) on StructureDefinition roots, so `Element` admits a resource too.
        if expression == "Element" || expression == host.path || host.reference == Some(expression)
        {
            return Ok(Fit::Yes);
        }
        // NOTE: no specification governs this: our own design; an element context naming a
        // type (`Element`, `HumanName`) admits every element of that type or a specialization.
        if !expression.contains('.')
            && let Some(code) = host.code
            && self.specializes(code, expression)?
        {
            return Ok(Fit::Yes);
        }
        let known = SCHEMAS.element(expression).is_some() || named_type(&R4, expression).is_some();
        Ok(if known {
            Fit::No
        } else {
            Fit::Unknown(format!("R4 defines no element {expression}"))
        })
    }

    /// Answers whether the extension context `expression`, a canonical with
    /// an optional `#code`, names the extension at `index` of the hosts.
    fn extension_fits(&self, expression: &str, index: usize) -> Fit {
        let hosts = self.hosts();
        let Some(host) = hosts.get(index) else {
            return Fit::No;
        };
        if host.code != Some("Extension") {
            return Fit::No;
        }
        let url = url_of(host.value);
        let fits = match expression.split_once('#') {
            Some((parent, code)) => {
                let outer = index
                    .checked_sub(1)
                    .and_then(|outer| hosts.get(outer))
                    .filter(|outer| outer.code == Some("Extension"))
                    .and_then(|outer| url_of(outer.value));
                url == Some(code) && outer.map(versionless) == Some(versionless(parent))
            }
            None => url.map(versionless) == Some(versionless(expression)),
        };
        if fits { Fit::Yes } else { Fit::No }
    }

    /// Answers whether the FHIRPath context `expression`, evaluated from the
    /// root of the resource, selects `host`.
    fn path_fits(&self, expression: &str, host: &Host<'i>) -> Fit {
        let scope = self.scope();
        let Some(resource) = scope.last().copied() else {
            return Fit::Unknown("the walk is inside no resource".to_owned());
        };
        let compiled = match self.validator().expression(expression) {
            Ok(compiled) => compiled,
            Err(error) => return Fit::Unknown(format!("{}: {error}", InvariantStage::Parse)),
        };
        let focus = Focus::resource(resource).within(Some(resource), container(scope));
        let items = match compiled.evaluate(&Environment::new(&R4), focus) {
            Ok(items) => items,
            Err(error) => {
                return Fit::Unknown(format!("{}: {}", InvariantStage::Evaluate, chain(&error)));
            }
        };
        let same = |left: Option<&Value>, right: Option<&Value>| matches!((left, right), (Some(left), Some(right)) if ptr::eq(left, right));
        // NOTE: R4 extension-context-type `fhirpath`: "all elements that match the FHIRPath
        // query", so the holder is admitted when it is one of the nodes the query selects.
        let selected = items.iter().filter_map(|item| item.as_node()).any(|node| {
            same(node.json(), host.value) || same(node.primitive_element(), host.element)
        });
        if selected { Fit::Yes } else { Fit::No }
    }

    /// Evaluates the context invariant `text` of `structure` on `host`, the
    /// element holding the extension at `location`.
    fn context_invariant(
        &mut self,
        structure: &Structure,
        text: &str,
        host: &Host<'i>,
        extension: &'i Value,
        location: &str,
    ) {
        let listed = Constraint::ContextInvariant {
            expression: text.to_owned(),
        };
        let refuse = |stage, error| Reason::InvariantRefused {
            stage,
            error,
            upstream: None,
        };
        let answer = (|| {
            let expression = self
                .validator()
                .expression(text)
                .map_err(|error| refuse(InvariantStage::Parse, error))?;
            let type_name = occurrence_type(host.value, host.code).ok_or_else(|| {
                refuse(
                    InvariantStage::Check,
                    "the type of the element is not known".to_owned(),
                )
            })?;
            let resource = SCHEMAS.is_resource(host.path) && host.code == Some(host.path);
            let subject = Subject {
                value: host.value,
                element: host.element,
                type_name: &type_name,
                location,
                resource,
            };
            // NOTE: structuredefinition-definitions.html#StructureDefinition.contextInvariant:
            // "the FHIRPath focus is the element that holds the extension".
            self.evaluate(&expression, structure, &subject, None, Some(extension))
                .map_err(|error| refuse(InvariantStage::Evaluate, error))?
                .ok_or(Reason::InvariantEmpty)
        })();
        match answer {
            Ok(true) => {}
            Ok(false) => self.out().issue(
                Severity::Error,
                IssueType::Invariant,
                MessageId::ContextInvariant,
                location,
                format!(
                    "the extension {} breaks its context invariant {text}",
                    structure.url()
                ),
            ),
            Err(reason) => {
                self.out()
                    .unevaluated(structure.url(), "Extension", listed, location, reason);
            }
        }
    }

    /// Answers whether the type `code` is `ancestor` or specializes it: by
    /// the element table for a complex type or a resource, by the core
    /// definitions' `baseDefinition` for a primitive.
    fn specializes(&self, code: &str, ancestor: &str) -> Result<bool, ValidateError> {
        if let Some(schema) = SCHEMAS.type_named(code) {
            // A backbone element is of the type its element names (BackboneElement or Element).
            let name = if schema.path.contains('.') {
                schema.base.unwrap_or(code)
            } else {
                code
            };
            return Ok(SCHEMAS.specializes(name, ancestor));
        }
        let mut current = code.to_owned();
        for _ in 0..DEPTH {
            if current == ancestor {
                return Ok(true);
            }
            let structure = self.validator().core(&current)?;
            let base = structure
                .definition()
                .base_definition
                .as_ref()
                .and_then(|base| base.value.as_deref())
                .and_then(|base| base.strip_prefix(CORE));
            match base {
                Some(base) => base.clone_into(&mut current),
                None => return Ok(false),
            }
        }
        Ok(false)
    }

    /// Answers whether `structure` is the definition `url` names or derives
    /// from it through its `baseDefinition` chain.
    fn derives(&self, structure: &Arc<Structure>, url: &str) -> Result<bool, ValidateError> {
        let url = versionless(url);
        let mut current = Arc::clone(structure);
        for _ in 0..DEPTH {
            if current.url() == url {
                return Ok(true);
            }
            let Some(base) = current
                .definition()
                .base_definition
                .as_ref()
                .and_then(|base| base.value.clone())
            else {
                return Ok(false);
            };
            current = match self.validator().structure(&base, Some(&current)) {
                Ok(found) => found,
                // NOTE: no specification governs this: our own design; a base the set does not
                // carry leaves the derivation unknown, which lists the context unevaluated.
                Err(ValidateError::Resolve(_)) => return Ok(false),
                Err(error) => return Err(error),
            };
        }
        Ok(false)
    }
}
