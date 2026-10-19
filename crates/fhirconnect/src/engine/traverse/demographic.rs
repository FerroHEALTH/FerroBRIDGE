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
//! identifier.

use openehr_base::containers::NonEmptyVec;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;

use crate::engine::demographics::PartyIdentity;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::party::PartyLens;
use crate::engine::rm::RmValue;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;
use crate::resolve::program::mapping::Mapping;
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
