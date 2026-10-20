// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept methods, the slots and the `manual` entries of a mapping.

use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::matching::node_id_matches;
use openehr_mapping_core::position::Located;

use core::str::FromStr;

use crate::model::ast::ContextMappingFile;
use crate::model::ast::ManualEntry;
use crate::model::ast::Mapping;
use crate::model::ast::ModelMappingFile;
use crate::model::ast::keyword::ContextScope;
use crate::model::ast::keyword::Direction;
use crate::model::ast::keyword::Variable;
use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::Node;
use crate::resolve::extensions::diagnostic;
use crate::resolve::program::binding::ResourceType;
use crate::resolve::program::manual::Manual;
use crate::resolve::program::manual::ManualPath;
use crate::resolve::program::manual::ManualValue;
use crate::resolve::program::mapping::Method;
use crate::resolve::program::target::Target;

use crate::resolve::compile::Compiler;
use crate::resolve::compile::Enclosing;
use crate::resolve::compile::Scope;
use crate::resolve::compile::Site;
use crate::resolve::compile::fhir::root_expression;
use crate::resolve::compile::header::archetype_of;
use crate::resolve::compile::party;

impl<'a> Compiler<'a> {
    /// Decides what a mapping method does beyond mapping its two paths.
    pub(super) fn method(&mut self, node: &Node<'a>, scope: &Scope, path: &ModelPath) -> Method {
        let method = node.mapping;
        let file = node.origin;
        let owner = file.header().name().value();
        let declared = [
            method.reference.as_ref().map(|_| "reference"),
            method.slot_archetype.as_ref().map(|_| "slotArchetype"),
            method.slot_context.as_ref().map(|_| "slotContext"),
            method.link.as_ref().map(|_| "link"),
            method.mapping_code.as_ref().map(|_| "mappingCode"),
            method
                .participations_function
                .as_ref()
                .map(|_| "participationsFunction"),
        ];
        let written: Vec<&str> = declared.into_iter().flatten().collect();
        if written.len() > 1 {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::ConflictingMappingMethods,
                method.position,
                path,
                format!(
                    "`{}` writes {}, and a mapping method is one of them",
                    method.name.value(),
                    written.join(", ")
                ),
            ));
        }
        if let Some(ref reference) = method.reference {
            let resource = ResourceType::new(reference.resource_type.value().clone());
            if !self.table.is_resource(resource.as_str()) {
                self.diagnostics.push(diagnostic(
                    file.file(),
                    owner,
                    ResolveCode::UnknownResourceType,
                    reference.resource_type.position(),
                    &path.field("reference").field("resourceType"),
                    format!("`{resource}` names no resource type of this FHIR version"),
                ));
                return Method::Value;
            }
            // NOTE: `path_operators.adoc` never says what `^` does at a
            // `reference` boundary, and crossing it for no `^` is the reading
            // its worked example needs (reported on issue #183).
            let mut enclosing = vec![Enclosing {
                resource: scope.resource.clone(),
                fhir: scope.fhir.clone(),
            }];
            enclosing.extend(scope.enclosing.iter().cloned());
            let inner = Scope {
                resource: resource.clone(),
                fhir: root_expression(),
                enclosing,
                ..scope.clone()
            };
            return Method::Reference {
                resource,
                mappings: self.mappings(&node.reference, &inner),
            };
        }
        if let Some(ref slot) = method.slot_archetype
            && !(self.party.is_some() && slots_composition(method))
        {
            return self.slot(file, slot, scope, &path.field("slotArchetype"));
        }
        if let Some(ref entered) = method.slot_context {
            return self.slot_context(file, entered, scope, &path.field("slotContext"));
        }
        if let Some(ref link) = method.link {
            if self.party_refuses_link(file.file(), owner, method, path) {
                return Method::Value;
            }
            return Method::Link {
                meaning: link.meaning.as_ref().map(|value| value.value().clone()),
                link_type: link.link_type.as_ref().map(|value| value.value().clone()),
            };
        }
        if let Some(ref code) = method.mapping_code {
            if !self.codes.contains(code.value()) {
                self.diagnostics.push(diagnostic(
                    file.file(),
                    owner,
                    ResolveCode::UnknownContextReference,
                    code.position(),
                    &path.field("mappingCode"),
                    format!("`{}` names no function the engine registers", code.value()),
                ));
            }
            return Method::Programmed {
                code: code.value().clone(),
            };
        }
        if let Some(ref function) = method.participations_function {
            if self.party_refuses_participation(file.file(), owner, method, path) {
                return Method::Value;
            }
            return Method::Participation {
                function: function.value().clone(),
            };
        }
        Method::Value
    }

    /// Compiles the model mapping a `slotArchetype` hands over to.
    ///
    /// The slot names the openEHR path the slotted mapping is transformed at
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/SlotArchetypes.adoc`),
    /// so `$archetype` inside it is that path. The specification says nothing
    /// about the FHIR side of the hand-over, so the slotted mapping keeps the
    /// caller's `$fhirRoot` and `$resource` stays the enclosing resource. No
    /// specification governs this: our own design.
    fn slot(
        &mut self,
        file: &'a ModelMappingFile,
        slot: &Located<MappingName>,
        scope: &Scope,
        path: &ModelPath,
    ) -> Method {
        let owner = file.header().name().value();
        let at = path.clone();
        let Some(slotted) = self.set.model(slot.value()) else {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::UnknownContextReference,
                slot.position(),
                &at,
                format!("`{}` is no model mapping of the loaded set", slot.value()),
            ));
            return Method::Value;
        };
        if scope.chain.contains(slot.value()) {
            let chain: Vec<&str> = scope.chain.iter().map(MappingName::as_str).collect();
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::SlotCycle,
                slot.position(),
                &at,
                format!(
                    "`{}` slots `{}`, which is already on the chain {}",
                    owner,
                    slot.value(),
                    chain.join(" -> ")
                ),
            ));
            return Method::Value;
        }
        let archetype = archetype_of(slotted).cloned();
        if let (Some(archetype), Ok(reached)) = (archetype, self.locate(&scope.openehr)) {
            let node = reached.node;
            match node.node_id() {
                Some(carried) if node_id_matches(archetype.as_str(), carried) => {
                    self.check_revision(slotted, Some(carried));
                }
                carried => self.diagnostics.push(diagnostic(
                    file.file(),
                    owner,
                    ResolveCode::ArchetypeMismatch,
                    slot.position(),
                    &at,
                    format!(
                        "`{}` declares the archetype `{archetype}` and the slot resolves to the \
                         node `{}`",
                        slot.value(),
                        carried.unwrap_or("with no archetype")
                    ),
                )),
            }
        }
        let mut chain = scope.chain.clone();
        chain.push(slot.value().clone());
        let inner = Scope {
            archetype: scope.openehr.clone(),
            direction: slotted
                .spec()
                .unidirectional
                .as_ref()
                .map_or(scope.direction, |located| Some(*located.value())),
            file: slotted.file().to_path_buf(),
            conceptmap: slotted
                .spec()
                .conceptmap
                .as_ref()
                .map(|url| url.value().clone()),
            prefix: String::new(),
            chain,
            ..scope.clone()
        };
        let merged = self.merge(slotted);
        Method::Slot {
            model: slot.value().clone(),
            preprocessors: self.preprocessors(slotted, &inner),
            mappings: self.mappings(&merged.mappings, &inner),
        }
    }

    /// Compiles the context a `slotContext` enters.
    ///
    /// "The engine enters the referenced context, maps from its `start` with
    /// its own `archetypes` and `extensions`, and returns", the sub-context
    /// maps "into the parent composition", and it "must be listed under
    /// `contexts`"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`,
    /// §`slotContext` and §Rules). So the entered context's `start` is slotted
    /// as a `slotArchetype` would slot it, under the entered context's
    /// extensions, and the composition model a root context's `start` slots
    /// is skipped there.
    fn slot_context(
        &mut self,
        file: &'a ModelMappingFile,
        entered: &Located<MappingName>,
        scope: &Scope,
        path: &ModelPath,
    ) -> Method {
        let owner = file.header().name().value();
        let name = entered.value();
        let Some(sub) = self.set.context(name) else {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::UnknownContextReference,
                entered.position(),
                path,
                format!("`{name}` is no context mapping of the loaded set"),
            ));
            return refused_entry(name);
        };
        if !self.may_enter(file, entered, sub, path) {
            return refused_entry(name);
        }
        let declaration = sub.context();
        let start = Located::new(
            declaration.start.position(),
            declaration.start.value().clone(),
        );
        self.check_embedded_template(file, entered, sub, scope, path);
        let extensions = self.extensions_of(sub);
        let saved = core::mem::replace(&mut self.extensions, extensions);
        self.entered.push(sub);
        if !self.subs.iter().any(|seen| core::ptr::eq(*seen, sub)) {
            self.subs.push(sub);
        }
        let inner = Scope {
            nested: true,
            ..scope.clone()
        };
        let slotted = self.slot(file, &start, &inner, path);
        self.entered.pop();
        self.extensions = saved;
        match slotted {
            Method::Slot {
                model,
                preprocessors,
                mappings,
            } => Method::SlotContext {
                context: name.clone(),
                model,
                preprocessors,
                mappings,
            },
            other => other,
        }
    }

    /// Checks that the `template.id` of `sub` is the template the parent's
    /// operational template embeds at the slot node (`sub-contexts.adoc`,
    /// §Rules and §Validation errors, "template mismatch").
    ///
    /// A slot node at which the operational template states no embedded
    /// template leaves the rule undecided, and that is a warning. No
    /// specification governs the undecided case: our own design.
    fn check_embedded_template(
        &mut self,
        file: &'a ModelMappingFile,
        entered: &Located<MappingName>,
        sub: &'a ContextMappingFile,
        scope: &Scope,
        path: &ModelPath,
    ) {
        let Some(declared) = sub.context().template.id.as_ref() else {
            return;
        };
        let Ok(reached) = self.locate(&scope.openehr) else {
            return;
        };
        let node = reached.node;
        if !reached.tail.segments.is_empty() {
            return;
        }
        let owner = file.header().name().value();
        let name = entered.value();
        match self.template.embedded_template(node) {
            Some(embedded) if embedded == declared.value() => {}
            Some(embedded) => self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::SubContextTemplateMismatch,
                entered.position(),
                path,
                format!(
                    "`{name}` declares the template `{}` and the operational template embeds \
                     `{embedded}` at `{}`",
                    declared.value(),
                    node.aql_path().as_str()
                ),
            )),
            None => self.diagnostics.push(
                Diagnostic::warning(
                    file.file().to_path_buf(),
                    ResolveCode::SubContextTemplateUndecided.into(),
                    format!(
                        "`{name}` declares the template `{}` and the operational template \
                         states no embedded template at `{}`, so the two cannot be compared",
                        declared.value(),
                        node.aql_path().as_str()
                    ),
                )
                .with_position(entered.position())
                .with_mapping_name(owner.clone())
                .with_model_path(path.clone()),
            ),
        }
    }

    /// Answers whether the walk may enter `sub` at a `slotContext` naming it:
    /// the entering context lists it under `contexts`, it is not already on
    /// the chain of entered contexts, and a `scope: sub` context's `start`
    /// slots no composition (`sub-contexts.adoc`, §Rules and §Validation
    /// errors). Each refusal is reported.
    fn may_enter(
        &mut self,
        file: &'a ModelMappingFile,
        entered: &Located<MappingName>,
        sub: &'a ContextMappingFile,
        path: &ModelPath,
    ) -> bool {
        let owner = file.header().name().value();
        let name = entered.value();
        if let Some(parent) = self.entered.last()
            && !parent
                .context()
                .contexts
                .iter()
                .any(|listed| listed.value() == name)
        {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::UndeclaredSubContext,
                entered.position(),
                path,
                format!(
                    "`{name}` is entered here and `{}` does not list it under `contexts`",
                    parent.header().name().value()
                ),
            ));
            return false;
        }
        if self
            .entered
            .iter()
            .any(|context| context.header().name().value() == name)
        {
            let chain: Vec<&str> = self
                .entered
                .iter()
                .map(|context| context.header().name().value().as_str())
                .collect();
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::CyclicSubContext,
                entered.position(),
                path,
                format!(
                    "`{name}` is entered again on the chain {}",
                    chain.join(" -> ")
                ),
            ));
            return false;
        }
        let declaration = sub.context();
        let start = declaration.start.value();
        let site = party::Site {
            file: file.file(),
            owner,
            position: entered.position(),
            path,
        };
        let slots = self
            .set
            .model(start)
            .is_some_and(|model| model.mappings().iter().any(slots_composition));
        if self.party_refuses_entry(&site, name, start, slots) {
            return false;
        }
        if declaration.scope() == ContextScope::Sub
            && let Some(model) = self.set.model(start)
            && let Some(composition) = model
                .mappings()
                .iter()
                .find(|mapping| slots_composition(mapping))
        {
            self.diagnostics.push(diagnostic(
                model.file(),
                start,
                ResolveCode::SubContextComposition,
                composition.position,
                &ModelPath::root().field("mappings"),
                format!(
                    "`{start}` starts the `scope: sub` context `{name}` and slots a composition \
                     model, and a sub-context never creates a composition"
                ),
            ));
            return false;
        }
        true
    }

    /// Compiles one `manual` entry.
    pub(super) fn manual(
        &mut self,
        file: &'a ModelMappingFile,
        entry: &ManualEntry,
        scope: &Scope,
        path: &ModelPath,
    ) -> Manual {
        let owner = file.header().name().value();
        let pinned = entry
            .unidirectional
            .as_ref()
            .map_or(scope.direction, |located| Some(*located.value()));
        let fhir = entry
            .fhir
            .iter()
            .enumerate()
            .filter_map(|(index, manual)| {
                let at = path.field("fhir").index(index);
                let value = self.manual_value(file, &manual.value, &at)?;
                self.fhir_target(
                    file.file(),
                    owner,
                    &manual.path,
                    scope,
                    &at,
                    Site::Write(pinned),
                )
                .map(|target| ManualPath::new(Target::Fhir(Box::new(target)), value))
            })
            .collect();
        let openehr = entry
            .openehr
            .iter()
            .enumerate()
            .filter_map(|(index, manual)| {
                let at = path.field("openehr").index(index);
                let value = self.manual_value(file, &manual.value, &at)?;
                let target = self.openehr_target(file.file(), owner, &manual.path, scope, &at)?;
                self.carried_tail(
                    file,
                    &format!("{}.{}", scope.prefix, entry.name.value()),
                    &target,
                    true,
                    manual.path.position(),
                    &at,
                );
                Some(ManualPath::new(Target::Openehr(Box::new(target)), value))
            })
            .collect();
        Manual::new(
            entry.name.value().clone(),
            fhir,
            openehr,
            entry.fhir_condition.as_ref().and_then(|condition| {
                self.condition(
                    file,
                    condition,
                    scope,
                    Direction::FhirToOpenehr,
                    &path.field("fhirCondition"),
                    scope.fhir.as_str(),
                )
            }),
            entry.openehr_condition.as_ref().and_then(|condition| {
                self.condition(
                    file,
                    condition,
                    scope,
                    Direction::OpenehrToFhir,
                    &path.field("openehrCondition"),
                    &scope.openehr.to_string(),
                )
            }),
            entry.value.as_ref().map(|value| value.value().clone()),
            entry.unidirectional.as_ref().map(|value| *value.value()),
        )
    }

    /// Reads what one `manual` path writes: a literal or a `$context` member.
    ///
    /// "`$context` holds values passed in on the REST call ... a context value
    /// is referenced from a `manual` `value`"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/basics/Variables.adoc`,
    /// §`$context`), and every example names a member of it, so a bare
    /// `$context` names no value and is refused.
    fn manual_value(
        &mut self,
        file: &'a ModelMappingFile,
        written: &Located<String>,
        path: &ModelPath,
    ) -> Option<ManualValue> {
        let text = written.value();
        let Some(rest) = text.strip_prefix("$context") else {
            return Some(ManualValue::Literal(text.clone()));
        };
        let Some(member) = rest.strip_prefix('.').filter(|name| !name.is_empty()) else {
            self.diagnostics.push(diagnostic(
                file.file(),
                file.header().name().value(),
                ResolveCode::MalformedContextValue,
                written.position(),
                &path.field("value"),
                format!("`{text}` names no `$context` member, so it carries no value"),
            ));
            return None;
        };
        Some(ManualValue::Context(member.to_owned()))
    }
}

/// The method of a `slotContext` the compiler refused: the entry into the
/// named context with nothing compiled under it, so the refusal is not
/// followed by one about the mapping as a plain value.
fn refused_entry(context: &MappingName) -> Method {
    Method::SlotContext {
        context: context.clone(),
        model: context.clone(),
        preprocessors: Vec::new(),
        mappings: Vec::new(),
    }
}

/// Whether `mapping` slots a composition model: a `slotArchetype` at
/// `openehr: $composition`, the composition slot a root context's `start`
/// carries (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/composition-element.adoc`).
pub(super) fn slots_composition(mapping: &Mapping) -> bool {
    mapping.slot_archetype.is_some()
        && mapping
            .with
            .as_ref()
            .and_then(|with| with.openehr.as_ref())
            .is_some_and(|openehr| {
                Variable::from_str(openehr.value())
                    .is_ok_and(|variable| variable == Variable::Composition)
            })
}
