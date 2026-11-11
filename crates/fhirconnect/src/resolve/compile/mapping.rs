// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One mapping and its conditions.

use std::collections::BTreeSet;

use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::header::archetype::ArchetypeId;
use openehr_mapping_core::header::metadata::MappingType;
use openehr_mapping_core::position::Located;
use openehr_mapping_core::position::Position;

use crate::model::ast::Condition;
use crate::model::ast::ModelMappingFile;
use crate::model::ast::keyword::DataType;
use crate::model::ast::keyword::Direction;
use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::Node;
use crate::resolve::extensions::diagnostic;
use crate::resolve::program::condition;
use crate::resolve::program::condition::ConditionParts;
use crate::resolve::program::manual::Manual;
use crate::resolve::program::mapping::Derived;
use crate::resolve::program::mapping::LinkSide;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::mapping::MappingParts;
use crate::resolve::program::mapping::Method;
use crate::resolve::program::target::Attachment;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::resolve::program::target::Target;
use crate::tree::element::Location;

use crate::resolve::compile::Compiler;
use crate::resolve::compile::Derivation;
use crate::resolve::compile::Scope;
use crate::resolve::compile::Site;
use crate::resolve::compile::header::archetype_of;
use crate::resolve::compile::method::declared_methods;
use crate::resolve::compile::method::slots_composition;
use crate::resolve::compile::openehr::walks_below;

impl<'a> Compiler<'a> {
    /// Compiles a list of mapping methods under one scope.
    ///
    /// Each node carries where it sits in the file that wrote it, so a nested
    /// refusal names its own depth rather than the index of the top-level
    /// method it hangs under.
    pub(super) fn mappings(&mut self, nodes: &[Node<'a>], scope: &Scope) -> Vec<Mapping> {
        let mut compiled = Vec::with_capacity(nodes.len());
        for node in nodes {
            // NOTE: composition-element.adoc: a context entered via `slotContext` "maps into the
            // parent composition; if its `start` slots a composition model, that mapping is skipped".
            if (scope.nested && slots_composition(node.mapping))
                || self.ignores_excluded(node, scope)
                || self.ends_recursion(node, scope)
            {
                continue;
            }
            let path = node.path.clone();
            compiled.push(self.mapping(node, scope, &path));
        }
        compiled
    }

    /// Returns whether a method ends a recursive slot where the template does.
    ///
    /// A slot chain may enter a model already on it
    /// ([`crate::resolve::program::SLOT_DEPTH_LIMIT`]), and the recursion
    /// descends while the template carries a node. Inside a chain that has
    /// entered an archetype again, or at a method that slots an archetype the
    /// chain already holds (two models of one archetype, as a medication and
    /// its substance are, count once each), a method whose place lies where
    /// the template stops nesting an archetype of the chain is where no
    /// composition can hold a value, so it is left out with everything under
    /// it: the template, never the mapping, sets the depth. Any other
    /// place the template does not carry is refused where it stands. No
    /// specification governs a recursive slot: our own design.
    fn ends_recursion(&self, node: &Node<'a>, scope: &Scope) -> bool {
        let slotted = node.mapping.slot_archetype.as_ref().map(Located::value);
        let archetypes: Vec<&str> = scope
            .chain
            .iter()
            .chain(slotted)
            .filter_map(|model| self.set.model(model))
            .filter_map(archetype_of)
            .map(ArchetypeId::as_str)
            .collect();
        let mut seen = BTreeSet::new();
        if archetypes.iter().all(|archetype| seen.insert(*archetype)) {
            return false;
        }
        node.mapping
            .with
            .as_ref()
            .and_then(|with| with.openehr.as_ref())
            .is_some_and(|written| self.nesting_ends(written, scope, &archetypes))
    }

    /// Returns whether a mapping method names a node the template excludes,
    /// and records the warning that says it is ignored.
    ///
    /// Model mappings "contain some mappings that are not part of the
    /// corresponding Profile/Templates, as they have (0..0) occurrences. These
    /// are ignored and not executed"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/model-mappings.adoc`,
    /// §Mapping 0..0 fields), so the method and everything under it are left
    /// out of the program. The rule is the model mapping's, which is written
    /// for every template of its archetype; an extension is written for one
    /// template, so a method it contributes is still refused.
    fn ignores_excluded(&mut self, node: &Node<'a>, scope: &Scope) -> bool {
        let from_model = node
            .origin
            .header()
            .mapping_type()
            .map(|kind| *kind.value())
            == Some(MappingType::Model);
        if !from_model {
            return false;
        }
        let Some(written) = node
            .mapping
            .with
            .as_ref()
            .and_then(|with| with.openehr.as_ref())
        else {
            return false;
        };
        let Some(message) = self.excluded(written, scope) else {
            return false;
        };
        let file = node.origin;
        self.diagnostics.push(
            Diagnostic::warning(
                file.file().to_path_buf(),
                ResolveCode::ExcludedTemplateNode.into(),
                format!(
                    "`{}` is ignored with everything under it: {message}",
                    node.name()
                ),
            )
            .with_position(written.position())
            .with_mapping_name(file.header().name().value().clone())
            .with_model_path(node.path.field("with").field("openehr")),
        );
        true
    }

    /// Refuses a `reference` written into a choice element whose type the
    /// path does not name.
    fn reference_into_choice(
        &mut self,
        node: &Node<'a>,
        method_kind: &Method,
        fhir: Option<&FhirTarget>,
        path: &ModelPath,
    ) {
        let (Method::Reference { .. }, Some(target)) = (method_kind, fhir) else {
            return;
        };
        if !matches!(*target.resolved().location(), Location::Choice(_)) {
            return;
        }
        let file = node.origin;
        self.diagnostics.push(diagnostic(
            file.file(),
            file.header().name().value(),
            ResolveCode::ReferenceIntoChoice,
            node.mapping.position,
            &path.field("with").field("fhir"),
            format!(
                "`{}` is a choice element, which FHIR names by the type it holds \
                 (<https://hl7.org/fhir/R4/formats.html#choice>), so a `reference` writes \
                 into it only where the path names the type, as `as(Reference)` does",
                target.expression()
            ),
        ));
    }

    /// Refuses a `link` whose FHIR side the engine cannot carry.
    ///
    /// The engine "checks the `meta.url` contained in the FHIR resource"
    /// (`types-of-mappings/concept-type/concept-mappings.adoc`, §Linked
    /// mappings), so the side names the resource: as a `Reference` or its
    /// `reference` string, as the section's example does, or as the resource
    /// an enclosing `reference` mapping entered (`$fhirRoot` inside it).
    fn link_side(
        &mut self,
        node: &Node<'a>,
        method_kind: &Method,
        fhir: Option<&FhirTarget>,
        scope: &Scope,
        path: &ModelPath,
    ) {
        let (Method::Link { .. }, Some(target)) = (method_kind, fhir) else {
            return;
        };
        // NOTE: a mapping writing two concept methods is refused as such, and which one
        // it means is undecided, so its sides are not judged as a link's.
        if declared_methods(node.mapping).len() > 1
            || LinkSide::of(target, scope.enclosing.len()).is_some()
        {
            return;
        }
        let file = node.origin;
        self.diagnostics.push(diagnostic(
            file.file(),
            file.header().name().value(),
            ResolveCode::LinkSide,
            node.mapping.position,
            &path.field("with").field("fhir"),
            format!(
                "`{}` is neither a `Reference`, its `reference` string, nor the resource an \
                 enclosing `reference` mapping entered, so it names no resource whose \
                 composition the `link` targets",
                target.expression()
            ),
        ));
    }

    /// Compiles one mapping method and everything under it.
    fn mapping(&mut self, node: &Node<'a>, scope: &Scope, path: &ModelPath) -> Mapping {
        let file = node.origin;
        let owner = file.header().name().value();
        let method = node.mapping;
        let name = scope.name_of(node.name());
        let (inherited, conceptmap) = inherited_spec(file, scope);
        let direction = method
            .unidirectional
            .as_ref()
            .map_or(inherited, |located| Some(*located.value()));
        let with = method.with.as_ref();
        let fhir = with.and_then(|with| {
            self.fhir_side(
                file,
                with,
                scope,
                &path.field("with"),
                Site::Write(direction),
            )
        });
        let openehr =
            with.and_then(|with| self.openehr_side(file, with, scope, &path.field("with")));
        let anchored = fhir
            .as_ref()
            .map_or_else(|| scope.clone(), |target| scope.anchored_at(target));
        let inner = Scope {
            openehr: openehr
                .as_ref()
                .map_or_else(|| scope.openehr.clone(), |target| target.path().clone()),
            direction,
            file: file.file().to_path_buf(),
            conceptmap: conceptmap.clone(),
            prefix: name.clone(),
            ..anchored
        };
        let (fhir_condition, openehr_condition) =
            self.conditions(node, scope, fhir.as_ref(), openehr.as_ref(), path);
        let manual: Vec<Manual> = method
            .manual
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                self.manual(file, entry, &inner, &path.field("manual").index(index))
            })
            .collect();
        let method_kind = self.method(node, &inner, path);
        let method_kind = self.ports(node, method_kind, openehr.as_ref(), &inner, path);
        self.reference_into_choice(node, &method_kind, fhir.as_ref(), path);
        self.link_side(node, &method_kind, fhir.as_ref(), scope, path);
        let followed_by = self.mappings(&node.followed_by, &inner);
        let data_type = with.and_then(|with| with.data_type.as_ref().map(|kind| *kind.value()));
        let plain =
            manual.is_empty() && matches!(method_kind, Method::Value) && !declares_method(method);
        let derived = self.derived_of(
            file,
            &Derivation {
                fhir: fhir.as_ref(),
                openehr: openehr.as_ref(),
                data_type,
                direction,
                plain,
                children: !followed_by.is_empty(),
            },
            method.position,
            &path.field("with"),
        );
        if let (Some(_), Some(target)) = (&fhir, &openehr)
            && plain
            && data_type != Some(DataType::None)
            && derived != Some(Derived::Anchor)
        {
            self.carried_tail(
                file,
                &name,
                target,
                false,
                method.position,
                &path.field("with").field("openehr"),
            );
        }
        self.party_ref_target(file, &name, (fhir.as_ref(), openehr.as_ref()), method, path);
        Mapping::new(MappingParts {
            name,
            model: owner.clone(),
            fhir,
            openehr,
            data_type,
            derived,
            value: with.and_then(|with| with.value.as_ref().map(|value| value.value().clone())),
            direction,
            fhir_condition,
            openehr_condition,
            manual,
            conceptmap: method
                .conceptmap
                .as_ref()
                .map(|url| url.value().clone())
                .or(conceptmap),
            method: method_kind,
            followed_by,
        })
    }

    /// Refuses an openEHR tail the engine cannot carry to the wire.
    ///
    /// A value reaches the wire whole under `|raw`, so a tail is carried when
    /// it names attributes the reference model gives the node's class, walks no
    /// `Set` or `Hash`, ends on no container and sits below a data value or a
    /// FLAT family
    /// (`engine::rm::carried`); the crate refuses any other tail at load,
    /// never on the request that first reaches it. A `manual` path writes text,
    /// so an empty one writes the node's `value` and a tail that ends on no
    /// string attribute is refused. No specification governs the walk below a
    /// node beyond the RM attribute model: our own design.
    pub(super) fn carried_tail(
        &mut self,
        file: &'a ModelMappingFile,
        mapping: &str,
        target: &OpenehrTarget,
        manual: bool,
        position: Position,
        path: &ModelPath,
    ) {
        let segments: Vec<&str> = target
            .tail()
            .segments
            .iter()
            .map(|segment| segment.attribute.as_str())
            .collect();
        let written: &[&str] = match (segments.is_empty(), manual) {
            (true, false) => return,
            (true, true) => &["value"],
            (false, _) => &segments,
        };
        let class = target.tail_class();
        let carried = crate::engine::rm::carried::carried(class, written)
            .is_some_and(|(_, held)| !manual || held == crate::engine::rm::carried::Carried::Text);
        if carried {
            return;
        }
        let message = if target.ends_on_list() && !manual {
            format!(
                "`{mapping}` names the list `{}` below the {class} node `{}` and no child that \
                 fills one of its elements, so no element would reach the wire",
                written.join("/"),
                target.node().aql_path().as_str()
            )
        } else {
            format!(
                "`{mapping}` names `{}` below the {class} node `{}`, which the engine cannot \
                 carry as one {class} value, so the value would never reach the wire",
                written.join("/"),
                target.node().aql_path().as_str()
            )
        };
        self.diagnostics.push(diagnostic(
            file.file(),
            file.header().name().value(),
            ResolveCode::UncarriedTail,
            position,
            path,
            message,
        ));
    }

    /// Compiles the `fhirCondition` and the `openehrCondition` of one mapping
    /// method, each guarded by the side it reads.
    fn conditions(
        &mut self,
        node: &Node<'a>,
        scope: &Scope,
        fhir: Option<&FhirTarget>,
        openehr: Option<&OpenehrTarget>,
        path: &ModelPath,
    ) -> (Option<condition::Condition>, Option<condition::Condition>) {
        let file = node.origin;
        let method = node.mapping;
        let fhir_condition = method.fhir_condition.as_ref().and_then(|condition| {
            let guard = fhir.map_or(scope.fhir.as_str(), |target| target.expression().as_str());
            self.condition(
                file,
                condition,
                scope,
                Direction::FhirToOpenehr,
                &path.field("fhirCondition"),
                guard,
            )
        });
        let openehr_condition = method.openehr_condition.as_ref().and_then(|condition| {
            let guard = openehr.map_or_else(
                || scope.openehr.to_string(),
                |target| target.path().to_string(),
            );
            self.condition(
                file,
                condition,
                scope,
                Direction::OpenehrToFhir,
                &path.field("openehrCondition"),
                &guard,
            )
        });
        (fhir_condition, openehr_condition)
    }

    /// Compiles one condition, on the side the direction names.
    pub(super) fn condition(
        &mut self,
        file: &'a ModelMappingFile,
        condition: &Condition,
        scope: &Scope,
        direction: Direction,
        path: &ModelPath,
        guard: &str,
    ) -> Option<condition::Condition> {
        let owner = file.header().name().value();
        let on_fhir = direction == Direction::FhirToOpenehr;
        let root = if on_fhir {
            self.fhir_target(
                file.file(),
                owner,
                &condition.target_root,
                scope,
                &path.field("targetRoot"),
                Site::ReadOnly,
            )
            .map(|target| Target::Fhir(Box::new(target)))
        } else {
            self.openehr_target(
                file.file(),
                owner,
                &condition.target_root,
                scope,
                &path.field("targetRoot"),
            )
            .map(|target| Target::Openehr(Box::new(target)))
        }?;
        let inner = match root {
            Target::Fhir(ref target) => scope.anchored_at(target),
            Target::Openehr(ref target) => Scope {
                openehr: target.path().clone(),
                ..scope.clone()
            },
        };
        let attributes = condition
            .target_attributes
            .iter()
            .enumerate()
            .filter_map(|(index, attribute)| {
                let at = path.field("targetAttributes").index(index);
                if on_fhir {
                    self.fhir_target(file.file(), owner, attribute, &inner, &at, Site::ReadOnly)
                        .map(|target| Target::Fhir(Box::new(target)))
                } else {
                    self.openehr_target(file.file(), owner, attribute, &inner, &at)
                        .map(|target| Target::Openehr(Box::new(target)))
                }
            })
            .collect();
        let attachment = attachment_of(&root, guard);
        Some(condition::Condition::new(ConditionParts {
            direction,
            target: root,
            attributes,
            operator: *condition.operator.value(),
            criteria: condition
                .criteria
                .iter()
                .map(|criteria| criteria.value().clone())
                .collect(),
            identifying: condition
                .identifying
                .as_ref()
                .is_some_and(|value| *value.value()),
            attachment,
        }))
    }
}

/// Returns whether a method writes one of the mapping methods that do more
/// than map its two paths, whether or not the method compiled.
fn declares_method(method: &crate::model::ast::Mapping) -> bool {
    method.reference.is_some()
        || method.slot_archetype.is_some()
        || method.link.is_some()
        || method.mapping_code.is_some()
        || method.participations_function.is_some()
}

/// Returns the `spec` keys a method inherits from the file that wrote it.
///
/// An extension file is a file
/// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/extension-methods.adoc`),
/// so its `spec` governs the methods it contributes, wherever they land in the
/// merged model mapping.
fn inherited_spec(file: &ModelMappingFile, scope: &Scope) -> (Option<Direction>, Option<String>) {
    if file.file() == scope.file {
        return (scope.direction, scope.conceptmap.clone());
    }
    let direction = file
        .spec()
        .unidirectional
        .as_ref()
        .map_or(scope.direction, |located| Some(*located.value()));
    let conceptmap = file
        .spec()
        .conceptmap
        .as_ref()
        .map(|url| url.value().clone())
        .or_else(|| scope.conceptmap.clone());
    (direction, conceptmap)
}

/// Decides how a condition's `targetRoot` stands to the path it guards.
///
/// Both paths are anchored by the time this runs, so the comparison is over
/// the resolved paths rather than the text a file wrote.
fn attachment_of(root: &Target, guard: &str) -> Attachment {
    let written = match *root {
        Target::Fhir(ref target) => target.expression().as_str().to_owned(),
        Target::Openehr(ref target) => target.path().to_string(),
    };
    if written == guard {
        return Attachment::Element;
    }
    if walks_below(&written, guard) {
        return Attachment::Descendant;
    }
    if walks_below(guard, &written) {
        return Attachment::Ancestor;
    }
    Attachment::Unrelated
}
