// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every `PARTY_IDENTIFIED` a run writes or reads goes through the
//! demographics endpoint ([`crate::engine::demographics`]).
//!
//! Going into openEHR, the reference a FHIR `Reference` carries is resolved
//! and its identity is added to the party, so the reference is never dropped:
//! `PARTY_IDENTIFIED.adoc` resolves `reference` and adds "the resource
//! identifier" to `identifiers`. Coming out, an identity the endpoint minted
//! becomes `Reference.reference` again, and every other identifier stays an
//! identifier. A `PARTY_REF` tail (`ROLE.performer`) resolves the same way and
//! must name a party the endpoint minted.

use fhir_types::r4::primitives::String as FhirString;
use openehr_base::containers::NonEmptyVec;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_mapping_core::index::ResolvedNode;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;

use crate::engine::demographics::PartyIdentity;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::party::PartyLens;
use crate::engine::lens::party_ref::PartyRefLens;
use crate::engine::lens::party_ref::target_of;
use crate::engine::rm::RmValue;
use crate::engine::traverse::Run;
use crate::engine::traverse::build::COMPOSER;
use crate::engine::traverse::error::EngineError;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::tree::element::Table;

/// Where a route writes the identity of a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// In `PARTY_IDENTIFIED.identifiers`, where the FLAT format carries it.
    Identifier,
    /// In `PARTY_PROXY.external_ref`, where the FLAT format carries only the
    /// party reference: the composer's `ctx/` keys (master06).
    ExternalRef,
}

impl Shape {
    /// Returns the shape a party at `node` is written in: `external_ref` on
    /// the composer, whose `ctx/` keys carry no identifiers, `identifiers`
    /// everywhere else.
    pub(super) fn at(node: &ResolvedNode) -> Self {
        if node.aql_path().as_str() == COMPOSER {
            Self::ExternalRef
        } else {
            Self::Identifier
        }
    }
}

impl<T: Table + ?Sized> Run<'_, T> {
    /// Returns the identity of the reference `literal` names, through the
    /// demographics endpoint.
    pub(super) fn identity_of(
        &self,
        mapping: &Mapping,
        literal: &str,
    ) -> Result<PartyIdentity, EngineError> {
        self.seams
            .demographics()
            .resolve(literal)
            .map_err(|source| EngineError::Demographics {
                mapping: String::from(mapping.name()),
                source: Box::new(source),
            })
    }

    /// Returns the `PARTY_REF` the reference `view` carries, for a tail that
    /// ends on one (`ROLE.performer`).
    ///
    /// The reference resolves through the demographics endpoint, which
    /// refuses a reference to a resource of the request; one the endpoint did
    /// not mint names a party of another server and is refused. What remains
    /// is the relative literal the [`PartyRefLens`] carries, of the one type
    /// the element admits. No specification governs this: our own design.
    pub(super) fn party_ref_of(
        &self,
        mapping: &Mapping,
        target: &FhirTarget,
        view: &FhirValue,
    ) -> Result<RmValue, EngineError> {
        let refuse = |source: LensError| EngineError::Cell {
            mapping: String::from(mapping.name()),
            element: String::from(target.expression().as_str()),
            source: Box::new(source),
        };
        let FhirValue::Reference(reference) = view else {
            return Err(refuse(LensError::NoCell {
                rm_type: String::from("PARTY_REF"),
                kind: view.kind().as_str(),
                direction: Direction::FhirToOpenehr,
            }));
        };
        let lens = PartyRefLens::new(self.reference_target(mapping, target)?);
        let mut relative = reference.as_ref().clone();
        if let Some(literal) = PartyLens::deferred(reference) {
            let identity = self.identity_of(mapping, literal)?;
            let found = self
                .seams
                .demographics()
                .reference_of(&identity)
                .ok_or_else(|| EngineError::ForeignParty {
                    mapping: String::from(mapping.name()),
                    reference: String::from(literal),
                })?;
            relative.reference = Some(FhirString::from(found));
        }
        lens.put(&relative, None)
            .map(RmValue::PartyRef)
            .map_err(refuse)
    }

    /// Returns the reference a `PARTY_REF` names, read as the one type the
    /// element `target` admits.
    pub(super) fn reference_of_party(
        &self,
        mapping: &Mapping,
        target: &FhirTarget,
        party: &PartyRef,
    ) -> Result<FhirValue, EngineError> {
        PartyRefLens::new(self.reference_target(mapping, target)?)
            .get(party)
            .map(|reference| FhirValue::Reference(Box::new(reference)))
            .map_err(|source| EngineError::Cell {
                mapping: String::from(mapping.name()),
                element: String::from(target.expression().as_str()),
                source: Box::new(source),
            })
    }

    /// Returns the one resource type the reference of `target` admits.
    fn reference_target(
        &self,
        mapping: &Mapping,
        target: &FhirTarget,
    ) -> Result<&'static str, EngineError> {
        target_of(self.table, target.resolved().target_profiles()).ok_or_else(|| {
            EngineError::PartyRefTarget {
                mapping: String::from(mapping.name()),
                element: String::from(target.resolved().leaf()),
            }
        })
    }

    /// Returns `value` with the identity of the reference `view` carries, when
    /// `value` is a party and `view` carries one, in the `shape` the route
    /// writes.
    pub(super) fn identified(
        &self,
        mapping: &Mapping,
        view: &FhirValue,
        value: RmValue,
        shape: Shape,
    ) -> Result<RmValue, EngineError> {
        let (RmValue::Party(party), FhirValue::Reference(reference)) = (value.clone(), view) else {
            return Ok(value);
        };
        let Some(literal) = PartyLens::deferred(reference) else {
            return Ok(value);
        };
        let identity = self.identity_of(mapping, literal)?;
        Ok(RmValue::Party(match shape {
            Shape::Identifier => with_identifier(party, &identity),
            Shape::ExternalRef => PartyIdentifiedData {
                external_ref: Some(identity.to_party_ref()),
                ..party
            },
        }))
    }

    /// Returns `value` without the identity this endpoint minted, with the
    /// reference that identity names, when `value` is a party carrying one.
    ///
    /// The identity is read from `identifiers` and from `external_ref`, the
    /// two shapes a route writes it in.
    pub(super) fn minted(&self, value: &RmValue) -> (RmValue, Option<String>) {
        let RmValue::Party(ref party) = *value else {
            return (value.clone(), None);
        };
        let demographics = self.seams.demographics();
        let named = |identity: Option<PartyIdentity>| {
            identity.and_then(|identity| demographics.reference_of(&identity))
        };
        let mut reference = None;
        let mut kept = Vec::new();
        for identifier in party.identifiers.as_deref().unwrap_or_default() {
            match named(PartyIdentity::from_identifier(identifier)) {
                Some(found) if reference.is_none() => reference = Some(found),
                _ => kept.push(identifier.clone()),
            }
        }
        let mut external_ref = party.external_ref.clone();
        if reference.is_none()
            && let Some(found) = named(
                external_ref
                    .as_ref()
                    .and_then(PartyIdentity::from_party_ref),
            )
        {
            reference = Some(found);
            external_ref = None;
        }
        let stripped = PartyIdentifiedData {
            external_ref,
            name: party.name.clone(),
            identifiers: NonEmptyVec::new(kept).ok(),
        };
        (RmValue::Party(stripped), reference)
    }
}

/// Returns `party` with `identity` as its first identifier.
fn with_identifier(party: PartyIdentifiedData, identity: &PartyIdentity) -> PartyIdentifiedData {
    let mut identifiers = party
        .identifiers
        .map(NonEmptyVec::into_vec)
        .unwrap_or_default();
    identifiers.insert(0, identity.to_identifier());
    PartyIdentifiedData {
        external_ref: party.external_ref,
        name: party.name,
        identifiers: NonEmptyVec::new(identifiers).ok(),
    }
}
