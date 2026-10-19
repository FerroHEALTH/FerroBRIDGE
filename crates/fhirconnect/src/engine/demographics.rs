// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The demographics endpoint a run resolves a party reference through.
//!
//! FHIRconnect sends each demographic resource to "a demographics endpoint
//! … located outside FHIRconnect" that returns an external id, stored in the
//! `PARTY_IDENTIFIED` the mapping writes, and "If a demographics service is
//! not implemented, a FHIRconnect engine should use the IDs maintained by the
//! FHIRconnect engine's FHIR-server"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/engine/demographics.adoc`).
//! The specification defines no interface for that endpoint, so the seam is
//! our own design: [`Demographics::resolve`] turns a FHIR reference into a
//! [`PartyIdentity`], and [`Demographics::reference_of`] turns an identity it
//! minted back into the reference. [`NoDemographics`], the default, refuses,
//! so a demographic reference with no endpoint fails closed;
//! [`IssuerDemographics`] is the specification's fallback, the ids of the
//! FHIR server the bridge itself is.
//!
//! An identity travels in one of two shapes, whichever the route can carry.
//! Where the FLAT format writes `PARTY_IDENTIFIED.identifiers`, it is a
//! `DV_IDENTIFIER`, the attribute `PARTY_IDENTIFIED.adoc` names for "the
//! resource identifier". Where it writes only the party reference (the
//! composer, `ctx/composer_id` and `ctx/id_namespace` in Simplified Formats
//! master06; a participation's performer in the master05 `PARTICIPATION`
//! table), it is `PARTY_PROXY.external_ref`, "Optional reference to more
//! detailed demographic or identification information for this party, in an
//! external system" (RM Common IM, `PARTY_PROXY`). The marker, the issuer as
//! `DV_IDENTIFIER.issuer` and `PARTY_REF.namespace`, and the value
//! `<type>/<id>` are our own design.

use core::error::Error;
use core::fmt;

use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;

/// The `DV_IDENTIFIER.type` and `GENERIC_ID.scheme` an identity carries.
pub const MARKER: &str = "fhir-reference";

/// The `PARTY_REF.type` an identity carries in `external_ref`.
const PARTY_TYPE: &str = "PERSON";

/// The demographic identity of one party, as the bridge stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyIdentity {
    /// The FHIR server base the resource lives on.
    issuer: String,
    /// The resource type, the first half of `value`.
    resource_type: String,
    /// The resource, `<type>/<id>`.
    value: String,
}

impl PartyIdentity {
    /// Returns the identity of the resource `resource_type`/`id` on the
    /// server `issuer`.
    #[must_use]
    pub fn new(issuer: &str, resource_type: &str, id: &str) -> Self {
        Self {
            issuer: String::from(issuer),
            resource_type: String::from(resource_type),
            value: format!("{resource_type}/{id}"),
        }
    }

    /// Returns the FHIR server base the resource lives on.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the resource type.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the resource as `<type>/<id>`.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the identity as the `DV_IDENTIFIER` a `PARTY_IDENTIFIED`
    /// carries in `identifiers`.
    #[must_use]
    pub fn to_identifier(&self) -> DvIdentifier {
        DvIdentifier {
            issuer: Some(self.issuer.clone()),
            assigner: None,
            id: self.value.clone(),
            r#type: Some(String::from(MARKER)),
        }
    }

    /// Returns the identity as the `PARTY_REF` a `PARTY_PROXY` carries in
    /// `external_ref`.
    ///
    /// `PARTY_REF.type` names an RM party class, and the FLAT routes that
    /// carry the reference have no key for it: the builder writes `PERSON`
    /// (Simplified Formats master05 lists only `id`, `id_scheme` and
    /// `id_namespace`), so the identity writes `PERSON` too and the resource
    /// type travels in the id value.
    #[must_use]
    pub fn to_party_ref(&self) -> PartyRef {
        PartyRef {
            namespace: self.issuer.clone(),
            r#type: String::from(PARTY_TYPE),
            id: ObjectId::GenericId(GenericId {
                value: self.value.clone(),
                scheme: String::from(MARKER),
            }),
        }
    }

    /// Returns the identity a `DV_IDENTIFIER` carries, when it carries one.
    #[must_use]
    pub fn from_identifier(identifier: &DvIdentifier) -> Option<Self> {
        if identifier.r#type.as_deref() != Some(MARKER) {
            return None;
        }
        Self::from_parts(identifier.issuer.as_deref()?, &identifier.id)
    }

    /// Returns the identity a `PARTY_REF` carries, when it carries one.
    #[must_use]
    pub fn from_party_ref(reference: &PartyRef) -> Option<Self> {
        let ObjectId::GenericId(ref id) = reference.id else {
            return None;
        };
        if id.scheme != MARKER {
            return None;
        }
        Self::from_parts(&reference.namespace, &id.value)
    }

    /// Returns the identity of `value`, `<type>/<id>`, on `issuer`.
    fn from_parts(issuer: &str, value: &str) -> Option<Self> {
        let (resource_type, id) = value.split_once('/')?;
        literal(resource_type, id).then(|| Self::new(issuer, resource_type, id))
    }
}

/// The demographics endpoint a run resolves party references through.
pub trait Demographics: fmt::Debug + Send + Sync {
    /// Returns the identity the endpoint keeps for the resource `reference`
    /// names.
    ///
    /// # Errors
    ///
    /// Returns [`DemographicsError`] when the endpoint cannot resolve the
    /// reference, which refuses the run rather than dropping the reference.
    fn resolve(&self, reference: &str) -> Result<PartyIdentity, DemographicsError>;

    /// Returns the reference an identity this endpoint minted names, or
    /// `None` for any other identity.
    fn reference_of(&self, identity: &PartyIdentity) -> Option<String>;
}

/// The endpoint that resolves nothing, for a run with no demographics
/// endpoint: a demographic reference refuses the run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoDemographics;

impl Demographics for NoDemographics {
    fn resolve(&self, reference: &str) -> Result<PartyIdentity, DemographicsError> {
        Err(DemographicsError::NoEndpoint {
            reference: String::from(reference),
        })
    }

    fn reference_of(&self, _identity: &PartyIdentity) -> Option<String> {
        None
    }
}

/// The ids of the FHIR server at one base: the specification's fallback when
/// no demographics service is implemented.
///
/// A relative reference, or an absolute one under the base, is the resource
/// on this server; an absolute reference to another server is kept as it
/// came, with that server's base as its issuer. The endpoint is stateless:
/// the identity carries the reference, so nothing is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerDemographics {
    /// The FHIR server base, with no trailing `/`.
    base: String,
}

impl IssuerDemographics {
    /// Returns the endpoint for the FHIR server at `base`.
    ///
    /// # Errors
    ///
    /// Returns [`DemographicsError::Issuer`] when `base` cannot be a
    /// `PARTY_REF.namespace`, whose legal values match
    /// `[a-zA-Z][a-zA-Z0-9_.:\/&?=+-]*` (openEHR BASE, `OBJECT_REF.namespace`).
    pub fn new(base: &str) -> Result<Self, DemographicsError> {
        let base = base.trim_end_matches('/');
        if namespace(base) {
            Ok(Self {
                base: String::from(base),
            })
        } else {
            Err(DemographicsError::Issuer {
                base: String::from(base),
            })
        }
    }

    /// Returns the FHIR server base this endpoint mints identities under.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }
}

impl Demographics for IssuerDemographics {
    fn resolve(&self, reference: &str) -> Result<PartyIdentity, DemographicsError> {
        let unresolvable = |reason| DemographicsError::Unresolvable {
            reference: String::from(reference),
            reason,
        };
        if reference.starts_with("urn:") || reference.starts_with('#') {
            return Err(unresolvable(Unresolvable::NotOnAServer));
        }
        if reference.contains("://") {
            let (resource_type, id, prefix) =
                split_absolute(reference).ok_or_else(|| unresolvable(Unresolvable::NotALiteral))?;
            let issuer = prefix.trim_end_matches('/');
            if !namespace(issuer) {
                return Err(unresolvable(Unresolvable::Issuer));
            }
            return Ok(PartyIdentity::new(issuer, resource_type, id));
        }
        let (resource_type, id) =
            split_relative(reference).ok_or_else(|| unresolvable(Unresolvable::NotALiteral))?;
        Ok(PartyIdentity::new(&self.base, resource_type, id))
    }

    fn reference_of(&self, identity: &PartyIdentity) -> Option<String> {
        (identity.issuer == self.base).then(|| identity.value.clone())
    }
}

/// Returns the type and id of a relative literal reference,
/// `<type>/<id>[/_history/<version>]` (<https://hl7.org/fhir/R4/references.html#literal>).
fn split_relative(path: &str) -> Option<(&str, &str)> {
    let mut segments = path.split('/');
    let resource_type = segments.next()?;
    let id = segments.next()?;
    match (segments.next(), segments.next(), segments.next()) {
        (None, _, _) | (Some("_history"), Some(_), None) => {}
        _ => return None,
    }
    literal(resource_type, id).then_some((resource_type, id))
}

/// Returns the type, the id and the server base of an absolute literal
/// reference.
fn split_absolute(reference: &str) -> Option<(&str, &str, &str)> {
    let without_history = match reference.rsplit_once("/_history/") {
        Some((head, version)) if !version.is_empty() && !version.contains('/') => head,
        _ => reference,
    };
    let (rest, id) = without_history.rsplit_once('/')?;
    let (prefix, resource_type) = rest.rsplit_once('/')?;
    literal(resource_type, id).then_some((resource_type, id, prefix))
}

/// Answers whether `resource_type` and `id` read as a FHIR literal reference:
/// a type name and an id inside `[A-Za-z0-9\-\.]{1,64}`
/// (<https://hl7.org/fhir/R4/datatypes.html#id>).
fn literal(resource_type: &str, id: &str) -> bool {
    let typed = resource_type
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && resource_type.chars().all(|c| c.is_ascii_alphanumeric());
    let identified = (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    typed && identified
}

/// Answers whether `text` can be a `PARTY_REF.namespace`:
/// `[a-zA-Z][a-zA-Z0-9_.:\/&?=+-]*`.
fn namespace(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:/&?=+-".contains(c))
}

/// Why a reference reads as no resource of a FHIR server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unresolvable {
    /// A `urn:` or a `#` reference names a resource of the request, not of a
    /// server.
    NotOnAServer,
    /// The reference is no `<type>/<id>` literal.
    NotALiteral,
    /// The server base cannot be a `PARTY_REF.namespace`.
    Issuer,
}

impl fmt::Display for Unresolvable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::NotOnAServer => "it names a resource of the request, not of a server",
            Self::NotALiteral => "it is no <type>/<id> literal reference",
            Self::Issuer => "its server base cannot be a PARTY_REF namespace",
        })
    }
}

/// Why the demographics endpoint did not resolve a reference.
#[derive(Debug)]
pub enum DemographicsError {
    /// The run has no demographics endpoint.
    NoEndpoint {
        /// The reference left unresolved.
        reference: String,
    },
    /// The reference names no resource the endpoint can identify.
    Unresolvable {
        /// The reference.
        reference: String,
        /// Why.
        reason: Unresolvable,
    },
    /// The configured base cannot be an issuer.
    Issuer {
        /// The base.
        base: String,
    },
    /// The endpoint failed.
    Endpoint {
        /// The reference.
        reference: String,
        /// What the endpoint reported.
        source: Box<dyn Error + Send + Sync>,
    },
}

impl fmt::Display for DemographicsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NoEndpoint { ref reference } => {
                write!(f, "no demographics endpoint resolves {reference}")
            }
            Self::Unresolvable {
                ref reference,
                reason,
            } => write!(f, "{reference} cannot be resolved: {reason}"),
            Self::Issuer { ref base } => {
                write!(f, "{base} cannot be the issuer of a demographic identity")
            }
            Self::Endpoint { ref reference, .. } => {
                write!(f, "the demographics endpoint failed on {reference}")
            }
        }
    }
}

impl Error for DemographicsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::Endpoint { ref source, .. } => Some(source.as_ref()),
            Self::NoEndpoint { .. } | Self::Unresolvable { .. } | Self::Issuer { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> IssuerDemographics {
        IssuerDemographics::new("https://bridge.example/fhir/").expect("a legal base")
    }

    #[test]
    fn a_relative_reference_is_a_resource_of_the_bridge() {
        let identity = bridge().resolve("Practitioner/p-1").expect("resolves");
        assert_eq!("https://bridge.example/fhir", identity.issuer());
        assert_eq!("Practitioner/p-1", identity.value());
        assert_eq!(
            Some(String::from("Practitioner/p-1")),
            bridge().reference_of(&identity)
        );
    }

    #[test]
    fn an_absolute_reference_under_the_base_is_a_resource_of_the_bridge() {
        let identity = bridge()
            .resolve("https://bridge.example/fhir/Organization/o-1/_history/2")
            .expect("resolves");
        assert_eq!("https://bridge.example/fhir", identity.issuer());
        assert_eq!("Organization/o-1", identity.value());
    }

    #[test]
    fn a_reference_to_another_server_keeps_its_server_and_reads_back_as_none() {
        let identity = bridge()
            .resolve("https://other.example/r4/Practitioner/x.9")
            .expect("resolves");
        assert_eq!("https://other.example/r4", identity.issuer());
        assert_eq!(None, bridge().reference_of(&identity));
    }

    #[test]
    fn a_request_local_reference_is_refused() {
        assert!(matches!(
            bridge().resolve("urn:uuid:5b0e0c9c-0000-4000-8000-000000000001"),
            Err(DemographicsError::Unresolvable {
                reason: Unresolvable::NotOnAServer,
                ..
            })
        ));
        assert!(matches!(
            bridge().resolve("practitioner"),
            Err(DemographicsError::Unresolvable {
                reason: Unresolvable::NotALiteral,
                ..
            })
        ));
    }

    #[test]
    fn no_endpoint_refuses_every_reference() {
        assert!(matches!(
            NoDemographics.resolve("Practitioner/p-1"),
            Err(DemographicsError::NoEndpoint { .. })
        ));
    }

    #[test]
    fn both_shapes_carry_the_identity_back() {
        let identity = PartyIdentity::new("https://bridge.example/fhir", "Practitioner", "p-1");
        assert_eq!(
            Some(identity.clone()),
            PartyIdentity::from_identifier(&identity.to_identifier())
        );
        assert_eq!(
            Some(identity.clone()),
            PartyIdentity::from_party_ref(&identity.to_party_ref())
        );
        let mut other = identity.to_identifier();
        other.r#type = Some(String::from("MR"));
        assert_eq!(None, PartyIdentity::from_identifier(&other));
    }

    #[test]
    fn a_base_outside_the_namespace_grammar_is_refused() {
        assert!(IssuerDemographics::new("https://bridge.example/fhir%20x").is_err());
    }
}
