// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `link` and `participationsFunction` mappings over the reference-model
//! families of a node.

use fhir_types::codec::Value;
use openehr_mapping_core::composition::RmPosition;
use openehr_rm::v1_2::paths::EhrUri;

use crate::engine::demographics::MARKER;
use crate::engine::family;
use crate::engine::linked::Linked;
use crate::engine::linked::LinkedCompositions;
use crate::engine::outcome::Loss;
use crate::engine::outcome::SkipReason;
use crate::engine::outcome::Warning;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::binding::ResourceType;
use crate::resolve::program::mapping::LinkSide;
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
    /// `LINK` table). The FHIR side reaches the resource whose composition
    /// the link targets (`concept-mappings.adoc` §Linked mappings, see
    /// [`LinkSide`]): an `ehr:` URI is written as it stands, and a resource is
    /// the linked composition whose URI the [`LinkedCompositions`] seam
    /// answers. A run with no such seam produces one composition and refuses.
    pub(super) fn link(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
        meaning: &str,
        link_type: &str,
    ) -> Result<Vec<Binding>, EngineError> {
        let (target, openehr) = Self::family_sides(mapping, "links")?;
        let Some(side) = LinkSide::of(target, self.frames.len()) else {
            return Err(EngineError::LinkedComposition {
                mapping: String::from(mapping.name()),
            });
        };
        match self.direction {
            Direction::FhirToOpenehr => {
                let mut bindings = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let binding = parent.keeping_openehr(
                        input.clone(),
                        mapping.fhir().map_or(parent.level, FhirTarget::level),
                    );
                    match self.link_target(mapping, side, input)? {
                        Ok(literal) => {
                            let positions = self.bound_positions(openehr, &binding);
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
                        Err(reason) => {
                            let location = self.locate(mapping, input);
                            self.warnings.push(Warning::new(
                                Loss::Skipped {
                                    mapping: String::from(mapping.name()),
                                    reason,
                                },
                                location,
                            ));
                        }
                    }
                    bindings.push(binding);
                }
                Ok(bindings)
            }
            Direction::OpenehrToFhir => {
                let (bindings, targets) =
                    self.family_bindings(mapping, openehr, parent, |node| {
                        family::link_targets(node, meaning, link_type)
                    })?;
                for (binding, literal) in bindings.iter().zip(targets) {
                    if side == LinkSide::Resource {
                        self.link_resource(mapping, literal)?;
                    } else {
                        self.write_reference(mapping, binding, &literal, None)?;
                    }
                }
                Ok(bindings)
            }
        }
    }

    /// Returns the `ehr:` URI one input of a `link` targets going into
    /// openEHR, or why the link is skipped.
    ///
    /// A `Reference` side names the resource by its literal, which is read
    /// through the reference seam; a resource side is the document the
    /// innermost `reference` mapping entered, under the literal it was entered
    /// by, so it is read once.
    fn link_target(
        &self,
        mapping: &Mapping,
        side: LinkSide,
        input: &Occurrence,
    ) -> Result<Result<String, SkipReason>, EngineError> {
        let literal = if side == LinkSide::Resource {
            // NOTE: LinkSide::of admits a resource side only at the depth of the innermost
            // reference, so `references` holds the literal that reference was entered by.
            self.references.last().cloned()
        } else {
            self.reference_at(mapping, input)?
        };
        let Some(literal) = literal else {
            return Ok(Err(SkipReason::EmptyReference));
        };
        let Err(source) = literal.parse::<EhrUri>() else {
            return Ok(Ok(literal));
        };
        let Some(linked) = self.seams.linked() else {
            return Err(EngineError::LinkTarget {
                mapping: String::from(mapping.name()),
                target: literal,
                source,
            });
        };
        if side == LinkSide::Resource {
            return Self::linked_composition(mapping, linked, &literal, &self.fhir);
        }
        let Some(resource) = self.fetch_linked(mapping, &literal)? else {
            return Ok(Err(SkipReason::UnresolvedReference { reference: literal }));
        };
        Self::linked_composition(mapping, linked, &literal, &resource)
    }

    /// Reads the resource the literal `literal` names through the reference
    /// seam, as a `reference` mapping reads one (`engine/references.adoc`).
    fn fetch_linked(&self, mapping: &Mapping, literal: &str) -> Result<Option<Value>, EngineError> {
        // NOTE: R4 references.html §2.3.0.1: a relative literal is `<type>/<id>`, so the
        // segment before the id names the type; any other literal names none here.
        let kind = literal
            .rsplit('/')
            .nth(1)
            .filter(|kind| kind.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .unwrap_or("Resource");
        self.seams
            .references()
            .fetch(literal, &ResourceType::new(kind))
            .map_err(|source| EngineError::Reference {
                mapping: String::from(mapping.name()),
                source: Box::new(source),
            })
    }

    /// Returns the `ehr:` URI of the composition `resource`, which the literal
    /// `literal` names, was committed as, or why the link is skipped.
    ///
    /// A resource no context maps is skipped (`concept-mappings.adoc`
    /// §Linked mappings).
    fn linked_composition(
        mapping: &Mapping,
        linked: &dyn LinkedCompositions,
        literal: &str,
        resource: &Value,
    ) -> Result<Result<String, SkipReason>, EngineError> {
        let answer =
            linked
                .composition(literal, resource)
                .map_err(|source| EngineError::Reference {
                    mapping: String::from(mapping.name()),
                    source: Box::new(source),
                })?;
        Ok(match answer {
            Linked::At(uri) => Ok(uri.to_string()),
            Linked::Unmapped => Err(SkipReason::UnmappedLink {
                reference: String::from(literal),
            }),
        })
    }

    /// Records that the resource the innermost `reference` mapping creates
    /// stands in the linked composition at `target`.
    ///
    /// Going out of openEHR a reference-side link writes its target as the
    /// reference, so a resource-side link makes its target the reference the
    /// enclosing mapping writes, which holds one.
    fn link_resource(&mut self, mapping: &Mapping, target: String) -> Result<(), EngineError> {
        if let Some(ref first) = self.linked_to {
            return Err(EngineError::LinkedTwice {
                mapping: String::from(mapping.name()),
                first: first.clone(),
                second: target,
            });
        }
        self.linked_to = Some(target);
        Ok(())
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
                    let binding = parent.keeping_openehr(
                        input.clone(),
                        mapping.fhir().map_or(parent.level, FhirTarget::level),
                    );
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
                    let positions = self.bound_positions(openehr, &binding);
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
                let (bindings, found) = self.family_bindings(mapping, openehr, parent, |node| {
                    family::participations(node, list, function)
                })?;
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

    /// Reads the entries of one family off the node instance `parent` binds,
    /// and returns them with one output binding each.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Family`] when `read` refuses the node's family,
    /// and the errors of reading the node and binding its occurrences.
    fn family_bindings<E>(
        &mut self,
        mapping: &Mapping,
        openehr: &OpenehrTarget,
        parent: &Binding,
        read: impl FnOnce(&serde_json::Value) -> Result<Vec<E>, family::FamilyError>,
    ) -> Result<(Vec<Binding>, Vec<E>), EngineError> {
        let positions = self.bound_positions(openehr, parent);
        let Some(node) = self.node_value(mapping.name(), openehr, &positions)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        let entries = read(&node).map_err(|source| EngineError::Family {
            mapping: String::from(mapping.name()),
            source: Box::new(source),
        })?;
        let occurrences = vec![openehr_occurrence(mapping.name(), &positions)?; entries.len()];
        let bindings = self.bindings(mapping, parent, &occurrences)?;
        Ok((bindings, entries))
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
