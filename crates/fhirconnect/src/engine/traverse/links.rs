// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `link` and `participationsFunction` mappings over the reference-model
//! families of a node.

use fhir_types::codec::Value;
use openehr_mapping_core::composition::RmPosition;
use openehr_rm::v1_2::paths::EhrUri;

use crate::engine::demographics::MARKER;
use crate::engine::family;
use crate::engine::fhir::FhirKind;
use crate::engine::linked::Linked;
use crate::engine::linked::LinkedCompositions;
use crate::engine::outcome::Loss;
use crate::engine::outcome::SkipReason;
use crate::engine::outcome::Warning;
use crate::engine::seam::ReferenceError;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::binding::ResourceType;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::Occurrence;

use crate::tree::element::Table;

use crate::engine::traverse::Binding;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::place::openehr_occurrence;
use crate::engine::traverse::tail::tail_segments;
use crate::engine::traverse::tail::unsupported_tail;

impl<T: Table + ?Sized> Run<'_, T> {
    /// Runs a `link` mapping.
    ///
    /// The openEHR side names the `links` of a `LOCATABLE`, and each link is
    /// written as one `_link:i` family on the node (Simplified Formats, the
    /// `LINK` table). The FHIR side is the reference the link targets: an
    /// `ehr:` URI is written as it stands, and a reference to a resource is
    /// the linked composition of `concept-mappings.adoc` §Linked mappings,
    /// whose URI the [`LinkedCompositions`] seam answers. A run with no such
    /// seam, or a side that is no reference, produces one composition and
    /// refuses.
    pub(super) fn link(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
        meaning: &str,
        link_type: &str,
    ) -> Result<Vec<Binding>, EngineError> {
        let (target, openehr) = Self::family_sides(mapping, "links")?;
        if !matches!(
            FhirKind::at(target.resolved().location()),
            Some(FhirKind::Reference | FhirKind::String)
        ) {
            return Err(EngineError::LinkedComposition {
                mapping: String::from(mapping.name()),
            });
        }
        match self.direction {
            Direction::FhirToOpenehr => {
                let mut bindings = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let binding = Binding {
                        fhir: input.clone(),
                        openehr: parent.openehr.clone(),
                        level: mapping.fhir().map_or(parent.level, FhirTarget::level),
                    };
                    let Some(literal) = self.reference_at(mapping, input)? else {
                        let location = self.locate(mapping, input);
                        self.warnings.push(Warning::new(
                            Loss::Skipped {
                                mapping: String::from(mapping.name()),
                                reason: SkipReason::EmptyReference,
                            },
                            location,
                        ));
                        bindings.push(binding);
                        continue;
                    };
                    let literal = match literal.parse::<EhrUri>() {
                        Ok(_) => literal,
                        Err(source) => {
                            let Some(linked) = self.seams.linked() else {
                                return Err(EngineError::LinkTarget {
                                    mapping: String::from(mapping.name()),
                                    target: literal,
                                    source,
                                });
                            };
                            match self.linked_target(mapping, linked, &literal)? {
                                Ok(uri) => uri,
                                Err(reason) => {
                                    let location = self.locate(mapping, input);
                                    self.warnings.push(Warning::new(
                                        Loss::Skipped {
                                            mapping: String::from(mapping.name()),
                                            reason,
                                        },
                                        location,
                                    ));
                                    bindings.push(binding);
                                    continue;
                                }
                            }
                        }
                    };
                    {
                        let positions = self.family_positions(openehr, &binding.openehr);
                        let index = self.next_family(openehr, &positions, "_link");
                        self.families.extend(family::link(
                            openehr.node(),
                            &positions,
                            index,
                            &family::LinkParts {
                                meaning,
                                link_type,
                                target: &literal,
                            },
                        ));
                    }
                    bindings.push(binding);
                }
                Ok(bindings)
            }
            Direction::OpenehrToFhir => {
                let positions = self.family_positions(openehr, &parent.openehr);
                let Some(node) = self.node_value(mapping.name(), openehr, &positions)? else {
                    return Ok(Vec::new());
                };
                let targets =
                    family::link_targets(&node, meaning, link_type).map_err(|source| {
                        EngineError::Family {
                            mapping: String::from(mapping.name()),
                            source: Box::new(source),
                        }
                    })?;
                let occurrences =
                    vec![openehr_occurrence(mapping.name(), &positions)?; targets.len()];
                let bindings = self.bindings(mapping, parent, &occurrences)?;
                for (binding, literal) in bindings.iter().zip(targets.iter()) {
                    self.write_reference(mapping, binding, literal, None)?;
                }
                Ok(bindings)
            }
        }
    }

    /// Returns the `ehr:` URI of the composition the resource `literal` names
    /// was committed as, or why the link is skipped.
    ///
    /// The resource is read through the reference seam, as a `reference`
    /// mapping reads one, and a reference that resolves to nothing is the
    /// same declared skip (`engine/references.adoc`). A resource no context
    /// maps is skipped too (`concept-mappings.adoc` §Linked mappings).
    fn linked_target(
        &self,
        mapping: &Mapping,
        linked: &dyn LinkedCompositions,
        literal: &str,
    ) -> Result<Result<String, SkipReason>, EngineError> {
        let refuse = |source: ReferenceError| EngineError::Reference {
            mapping: String::from(mapping.name()),
            source: Box::new(source),
        };
        // NOTE: R4 references.html §2.3.0.1: a relative literal is `<type>/<id>`, so the
        // segment before the id names the type; any other literal names none here.
        let kind = literal
            .rsplit('/')
            .nth(1)
            .filter(|kind| kind.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .unwrap_or("Resource");
        let Some(resource) = self
            .seams
            .references()
            .fetch(literal, &ResourceType::new(kind))
            .map_err(refuse)?
        else {
            return Ok(Err(SkipReason::UnresolvedReference {
                reference: String::from(literal),
            }));
        };
        Ok(
            match linked.composition(literal, &resource).map_err(refuse)? {
                Linked::At(uri) => Ok(uri.to_string()),
                Linked::Unmapped => Err(SkipReason::UnmappedLink {
                    reference: String::from(literal),
                }),
            },
        )
    }

    /// Runs a `participationsFunction` mapping.
    ///
    /// "The function in this method is not something that is
    /// auto-transformable. Therefore, it is added to the `with:` statement"
    /// (`concept-mappings.adoc`, §Participation mappings): the FHIR side is
    /// the `Reference` of the participant, the openEHR side the
    /// `other_participations` of an `ENTRY` or the `participations` of the
    /// `EVENT_CONTEXT` (`family::participation_list`), and the function is
    /// the method's. The performer carries the identity the demographics
    /// endpoint gives the reference as its `external_ref`, the one party
    /// reference the master05 `PARTICIPATION` table inlines
    /// ([`crate::engine::demographics`]), and the reference's `display` as
    /// its name; the allocation is our own design.
    pub(super) fn participation(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
        function: &str,
    ) -> Result<Vec<Binding>, EngineError> {
        let list = mapping
            .openehr()
            .and_then(|openehr| {
                let tail = tail_segments(openehr);
                family::participation_list(openehr.node().rm_type(), &tail)
            })
            .unwrap_or(family::ENTRY_PARTICIPATIONS);
        let (_target, openehr) = Self::family_sides(mapping, list.attribute)?;
        match self.direction {
            Direction::FhirToOpenehr => {
                let mut bindings = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let binding = Binding {
                        fhir: input.clone(),
                        openehr: parent.openehr.clone(),
                        level: mapping.fhir().map_or(parent.level, FhirTarget::level),
                    };
                    let value = self.fhir_value_at(mapping, input)?;
                    let literal = value
                        .as_ref()
                        .and_then(|value| value.get("reference"))
                        .and_then(Value::as_str);
                    let display = value
                        .as_ref()
                        .and_then(|value| value.get("display"))
                        .and_then(Value::as_str);
                    let Some(literal) = literal else {
                        let location = self.locate(mapping, input);
                        self.warnings.push(Warning::new(
                            Loss::Skipped {
                                mapping: String::from(mapping.name()),
                                reason: SkipReason::EmptyReference,
                            },
                            location,
                        ));
                        bindings.push(binding);
                        continue;
                    };
                    let identity = self.identity_of(mapping, literal)?;
                    let positions = self.family_positions(openehr, &binding.openehr);
                    let index = self.next_family(openehr, &positions, list.family);
                    self.families.extend(family::participation(
                        openehr.node(),
                        list,
                        &positions,
                        index,
                        &family::ParticipationParts {
                            function,
                            name: display,
                            id: identity.value(),
                            id_scheme: MARKER,
                            id_namespace: identity.issuer(),
                        },
                    ));
                    bindings.push(binding);
                }
                Ok(bindings)
            }
            Direction::OpenehrToFhir => {
                let positions = self.family_positions(openehr, &parent.openehr);
                let Some(node) = self.node_value(mapping.name(), openehr, &positions)? else {
                    return Ok(Vec::new());
                };
                let found = family::participations(&node, list, function).map_err(|source| {
                    EngineError::Family {
                        mapping: String::from(mapping.name()),
                        source: Box::new(source),
                    }
                })?;
                let occurrences =
                    vec![openehr_occurrence(mapping.name(), &positions)?; found.len()];
                let bindings = self.bindings(mapping, parent, &occurrences)?;
                for (binding, participant) in bindings.iter().zip(found.iter()) {
                    let Some(literal) = self.participant_reference(participant) else {
                        continue;
                    };
                    self.write_reference(mapping, binding, &literal, participant.name.as_deref())?;
                }
                Ok(bindings)
            }
        }
    }

    /// Returns the reference a participant's performer names.
    ///
    /// A performer this endpoint identified reads back as the reference it
    /// minted, one identified on another server as that server's absolute
    /// reference, and any other performer as its `external_ref.id.value`.
    fn participant_reference(&self, participant: &family::Participation) -> Option<String> {
        match participant.identity {
            Some(ref identity) => Some(
                self.seams
                    .demographics()
                    .reference_of(identity)
                    .unwrap_or_else(|| format!("{}/{}", identity.issuer(), identity.value())),
            ),
            None => participant.id.clone(),
        }
    }

    /// Returns the two sides of a mapping that writes a reference-model
    /// family, refusing an openEHR side whose tail is not that family.
    fn family_sides<'mapping>(
        mapping: &'mapping Mapping,
        attribute: &str,
    ) -> Result<(&'mapping FhirTarget, &'mapping OpenehrTarget), EngineError> {
        let Some(openehr) = mapping.openehr() else {
            return Err(EngineError::LinkedComposition {
                mapping: String::from(mapping.name()),
            });
        };
        if tail_segments(openehr) != [attribute] {
            return Err(unsupported_tail(mapping, openehr));
        }
        let Some(target) = mapping.fhir() else {
            return Err(EngineError::UnknownElement {
                mapping: String::from(mapping.name()),
                element: String::from(attribute),
            });
        };
        Ok((target, openehr))
    }

    /// Returns the positions a family on the target's node is written at.
    fn family_positions(&self, target: &OpenehrTarget, bound: &[RmPosition]) -> Vec<RmPosition> {
        target
            .occurrences()
            .iter()
            .enumerate()
            .map(|(depth, axis)| {
                bound
                    .get(depth)
                    .copied()
                    .or_else(|| self.pin(axis))
                    .unwrap_or_else(RmPosition::first)
            })
            .collect()
    }

    /// Returns the next free index of one family on one node instance.
    fn next_family(
        &mut self,
        target: &OpenehrTarget,
        positions: &[RmPosition],
        family: &str,
    ) -> usize {
        let key = (
            format!("{family}@{}", target.node().flat_id()),
            positions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join("."),
        );
        let next = self.counters.entry(key).or_insert(0);
        let taken = *next;
        *next = next.saturating_add(1);
        taken
    }
}
