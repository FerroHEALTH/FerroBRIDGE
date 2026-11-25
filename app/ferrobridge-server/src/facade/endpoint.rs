// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Endpoint` resources of the draft `EEHRxF` Annex X group `EHDSEndpoint`,
//! read-only from the bridge's own configuration.
//!
//! No openEHR content carries a technical endpoint, so the facade serves the
//! ones it is configured with under `[facade.endpoint]`: its own FHIR base
//! and, when `dicomweb_wado_rs` names one, the `DICOMweb` WADO-RS service of the
//! imaging studies it maps. HL7 Europe Base and Core 2.0.1 gives Endpoint no
//! profile, so each is a FHIR R4 `Endpoint`
//! (<https://hl7.org/fhir/R4/endpoint.html>). No specification governs which
//! endpoints a bridge publishes: our own design.

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use fhir_search::token::System;
use fhir_search::token::Token;
use fhir_types::codec::Object;
use fhir_types::r4::capability_statement::CapabilityStatementRestResource;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;

use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::guard;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::answer::Written;
use crate::facade::owned::answer::answer;
use crate::facade::owned::answer::searchset;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The resource type.
pub const ENDPOINT: &str = "Endpoint";

/// The id of the facade's own FHIR base.
pub const FHIR_BASE: &str = "fhir";

/// The id of the configured `DICOMweb` WADO-RS service.
pub const WADO_RS: &str = "dicom-wado-rs";

/// The R4 code system of `Endpoint.connectionType`.
const CONNECTION_TYPE: &str = "http://terminology.hl7.org/CodeSystem/endpoint-connection-type";

/// The R4 code system of `Endpoint.payloadType`.
const PAYLOAD_TYPE: &str = "http://terminology.hl7.org/CodeSystem/endpoint-payload-type";

/// The R4 code system of `Endpoint.status`.
const STATUS_SYSTEM: &str = "http://hl7.org/fhir/endpoint-status";

/// The interactions the facade answers on `Endpoint`.
const INTERACTIONS: [&str; 2] = ["read", "search-type"];

/// The `rest.resource.documentation` of `Endpoint`.
const DOCUMENTATION: &str = "The technical endpoints of this bridge, the draft EEHRxF Annex X group EHDSEndpoint, read-only from its configuration: its FHIR base and its configured DICOMweb WADO-RS service. Read, and search by _id, connection-type and status.";

/// The endpoints one deployment is configured with beside its FHIR base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endpoints {
    /// The `DICOMweb` WADO-RS base URL, when one is configured.
    pub wado_rs: Option<String>,
}

/// One served endpoint.
struct Served {
    /// The logical id.
    id: &'static str,
    /// The `endpoint-connection-type` code.
    connection: &'static str,
    /// The human-readable name.
    name: &'static str,
    /// The payload media type.
    mime: &'static str,
    /// The address.
    address: String,
}

impl Served {
    /// Returns the R4 `Endpoint` of this one.
    fn resource(&self) -> Result<Object, Refusal> {
        serde_json::from_value(serde_json::json!({
            "resourceType": ENDPOINT,
            "id": self.id,
            "status": "active",
            "connectionType": { "system": CONNECTION_TYPE, "code": self.connection },
            "name": self.name,
            "payloadType": [{ "coding": [{ "system": PAYLOAD_TYPE, "code": "any" }] }],
            "payloadMimeType": [self.mime],
            "address": self.address,
        }))
        .map_err(|error| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception)
                    .diagnosing(format!("an Endpoint could not be written: {error}")),
            )
        })
    }
}

/// Returns every endpoint `facade` serves, its FHIR base first.
fn served(facade: &Facade) -> Vec<Served> {
    let mut served = vec![Served {
        id: FHIR_BASE,
        connection: "hl7-fhir-rest",
        name: "FerroBRIDGE FHIR R4 facade",
        mime: "application/fhir+json",
        address: facade.settings().base_url.clone(),
    }];
    if let Some(address) = facade
        .endpoints()
        .and_then(|endpoints| endpoints.wado_rs.as_ref())
    {
        served.push(Served {
            id: WADO_RS,
            connection: "dicom-wado-rs",
            name: "DICOMweb WADO-RS",
            mime: "application/dicom",
            address: address.clone(),
        });
    }
    served
}

/// Admits `access` to `permission` on `Endpoint`, which no patient
/// compartment holds (<https://hl7.org/fhir/R4/compartmentdefinition-patient.html>).
fn admit(access: &Access, permission: Permission) -> Result<(), Refusal> {
    match access.reach(permission, ENDPOINT) {
        Some(Compartment::Any) => Ok(()),
        Some(Compartment::Patient(_)) | None => Err(crate::facade::guard::forbidden()),
    }
}

/// `GET [base]/Endpoint/{id}`.
///
/// # Errors
/// Returns `404` for an id the facade serves no endpoint under, the guard's
/// refusals and `403` for a caller without `Endpoint` read.
pub fn read(
    facade: &Facade,
    access: &Access,
    id: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    guard(headers, uri, Body::Absent)?;
    admit(access, Permission::Read)?;
    let Some(endpoint) = served(facade).into_iter().find(|served| served.id == id) else {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!("no {ENDPOINT}/{id} is served")),
        ));
    };
    let resource = endpoint.resource()?;
    Ok(answer(
        facade,
        StatusCode::OK,
        headers,
        Written {
            resource_type: ENDPOINT,
            resource: &resource,
            version: 1,
            created: false,
        },
    ))
}

/// What one search asks for: each parameter a set of values any of which
/// matches, and every parameter given matching (R4 `search.html` §3.1.1.4.3
/// and §3.1.1.4.4).
#[derive(Debug, Default)]
struct Criteria {
    /// The ids `_id` names.
    ids: Vec<Vec<String>>,
    /// The connection types `connection-type` names.
    connections: Vec<Vec<Token>>,
    /// The statuses `status` names.
    statuses: Vec<Vec<Token>>,
}

impl Criteria {
    /// Reads `parameters`.
    fn read(parameters: &[(String, String)]) -> Result<Self, Refusal> {
        let refused = |name: &str, text: String| {
            reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::Invalid)
                    .diagnosing(text)
                    .at(name.to_owned()),
            )
        };
        let mut criteria = Self::default();
        for (name, value) in parameters {
            let parts = fhir_search::escape::split(value, ',');
            let tokens = || {
                parts
                    .iter()
                    .map(|part| {
                        Token::parse(part)
                            .map_err(|error| refused(name, format!("{name}: {error}")))
                    })
                    .collect::<Result<Vec<Token>, Refusal>>()
            };
            match name.as_str() {
                "_id" => criteria.ids.push(
                    parts
                        .iter()
                        .map(|part| {
                            fhir_search::escape::unescape(part)
                                .map_err(|error| refused(name, error.to_string()))
                        })
                        .collect::<Result<Vec<String>, Refusal>>()?,
                ),
                "connection-type" => criteria.connections.push(tokens()?),
                "status" => criteria.statuses.push(tokens()?),
                "_format" | "_pretty" => {}
                _ => {
                    return Err(reply::refusal(
                        StatusCode::BAD_REQUEST,
                        Issue::error(IssueType::NotSupported)
                            .diagnosing("an Endpoint search takes _id, connection-type and status")
                            .at(name.clone()),
                    ));
                }
            }
        }
        Ok(criteria)
    }

    /// Whether `endpoint` matches every parameter given.
    fn matches(&self, endpoint: &Served) -> bool {
        self.ids
            .iter()
            .all(|any| any.iter().any(|id| id == endpoint.id))
            && self.connections.iter().all(|any| {
                any.iter()
                    .any(|token| coded(token, CONNECTION_TYPE, endpoint.connection))
            })
            && self.statuses.iter().all(|any| {
                any.iter()
                    .any(|token| coded(token, STATUS_SYSTEM, "active"))
            })
    }
}

/// Whether `token` matches the coding `system|code` (R4 `search.html`
/// §3.1.1.4.10).
fn coded(token: &Token, system: &str, code: &str) -> bool {
    let system_matches = match token.system() {
        System::Any => true,
        System::Absent => false,
        System::Is(named) => named == system,
    };
    system_matches && token.code().is_none_or(|named| named == code)
}

/// `GET [base]/Endpoint?…` and `POST [base]/Endpoint/_search`.
///
/// # Errors
/// Returns `400` for a parameter outside `_id`, `connection-type` and
/// `status` or a malformed value, and `403` for a caller without `Endpoint`
/// search.
pub fn search(
    facade: &Facade,
    access: &Access,
    uri: &Uri,
    parameters: &[(String, String)],
) -> Result<axum::response::Response, Refusal> {
    admit(access, Permission::Search)?;
    let criteria = Criteria::read(parameters)?;
    let matched = served(facade)
        .iter()
        .filter(|endpoint| criteria.matches(endpoint))
        .map(Served::resource)
        .collect::<Result<Vec<Object>, Refusal>>()?;
    searchset(facade, ENDPOINT, uri, matched)
}

/// Returns the `rest.resource` entry of `Endpoint`.
#[must_use]
pub fn capability() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: ENDPOINT.into(),
        documentation: Some(DOCUMENTATION.into()),
        interaction: crate::facade::capability::interactions(INTERACTIONS),
        search_param: crate::facade::capability::declared_params(&[
            (
                "_id",
                "http://hl7.org/fhir/SearchParameter/Resource-id",
                "token",
            ),
            (
                "connection-type",
                "http://hl7.org/fhir/SearchParameter/Endpoint-connection-type",
                "token",
            ),
            (
                "status",
                "http://hl7.org/fhir/SearchParameter/Endpoint-status",
                "token",
            ),
        ]),
        ..CapabilityStatementRestResource::default()
    }
}
