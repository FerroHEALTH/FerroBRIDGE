// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[facade]` into the settings of the FHIR facade.
//!
//! No specification governs the configuration: our own design.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::config::FacadeSettings;
use crate::config::SummarySettings;
use crate::config::error::Error;
use crate::config::section::facade::Facade;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use openehr_rm::draft::access_control::terminology::TERMINOLOGY_ID_ACCESS;
use openehr_rm::draft::access_control::terminology::Term;

/// Returns the facade settings `facade` describes, asking `identity` for
/// every EHR subject.
fn settings(
    facade: &Facade,
    identity: std::sync::Arc<crate::identity_source::IdentitySource>,
) -> Result<FacadeSettings, Error> {
    let ehr_policy = ehr_policy_of("facade.ehr_policy", &facade.ehr_policy)?;
    for (key, value) in [
        ("facade.base_url", &facade.base_url),
        ("facade.composition_language", &facade.composition_language),
        (
            "facade.composition_territory",
            &facade.composition_territory,
        ),
        ("facade.custodian.name", &facade.custodian.name),
        (
            "facade.custodian.identifier_system",
            &facade.custodian.identifier_system,
        ),
        (
            "facade.custodian.identifier_value",
            &facade.custodian.identifier_value,
        ),
    ] {
        if value.is_empty() {
            return Err(Error::Missing {
                key: String::from(key),
            });
        }
    }
    fhirconnect::engine::demographics::IssuerDemographics::new(&facade.base_url).map_err(
        |source| Error::DemographicsIssuer {
            key: String::from("facade.base_url"),
            source: Box::new(source),
        },
    )?;
    Ok(FacadeSettings {
        settings: crate::facade::Settings {
            base_url: facade.base_url.clone(),
            ehr_policy,
            subject: crate::identity::subject::SubjectLink::new(identity),
            system_id: facade.system_id.clone(),
            language: facade.composition_language.clone(),
            territory: facade.composition_territory.clone(),
            ehr_status_system: (!facade.ehr_status_identifier_system.is_empty())
                .then(|| facade.ehr_status_identifier_system.clone()),
            custodian: crate::facade::Custodian {
                name: facade.custodian.name.clone(),
                system: facade.custodian.identifier_system.clone(),
                value: facade.custodian.identifier_value.clone(),
            },
        },
        search: search_limits(facade)?,
        references: reference_policy(facade)?,
        identifier: identifier_search(facade)?,
        party_search: party_search(facade)?,
        binding_issuers: facade.binding_identifier_issuers.clone(),
        consent: consent(facade)?,
        app_state: facade.app_state.enabled,
        launch_task: facade.launch_task.enabled,
        cors: cors(facade)?,
        summary: summary(facade)?,
        endpoints: endpoints(&facade.endpoint)?,
        mhd: mhd(facade)?,
    })
}

/// Returns whether the MHD Document Responder is served: ITI-66 and ITI-67
/// SHALL process `patient.identifier` (ITI TF-2: 3.66.4.1.2.1,
/// 3.67.4.1.2.1), which the identifier search resolves.
fn mhd(facade: &Facade) -> Result<bool, Error> {
    if facade.mhd.enabled {
        super::needs(
            "facade.mhd.enabled",
            &[(
                facade.identifier_search.enabled,
                "facade.identifier_search.enabled",
            )],
        )?;
    }
    Ok(facade.mhd.enabled)
}

/// Returns the endpoints `endpoint` configures, `None` when `Endpoint` serves
/// none.
fn endpoints(
    endpoint: &crate::config::section::facade::FacadeEndpoint,
) -> Result<Option<crate::facade::endpoint::Endpoints>, Error> {
    if !endpoint.enabled {
        return Ok(None);
    }
    let wado_rs = if endpoint.dicomweb_wado_rs.is_empty() {
        None
    } else {
        super::url_of(
            "facade.endpoint.dicomweb_wado_rs",
            &endpoint.dicomweb_wado_rs,
        )?;
        Some(endpoint.dicomweb_wado_rs.clone())
    };
    Ok(Some(crate::facade::endpoint::Endpoints { wado_rs }))
}

/// Returns the patient summary `facade` configures, `None` when it is off.
fn summary(facade: &Facade) -> Result<Option<SummarySettings>, Error> {
    let summary = &facade.summary;
    if !summary.enabled {
        return Ok(None);
    }
    if summary.packages.is_empty() {
        return Err(Error::Missing {
            key: String::from("facade.summary.packages"),
        });
    }
    if summary.operator.trim().is_empty() {
        return Err(Error::Missing {
            key: String::from("facade.summary.operator"),
        });
    }
    Ok(Some(SummarySettings {
        packages: summary.packages.clone(),
        operator: summary.operator.clone(),
    }))
}

/// Returns the CORS policy `facade` configures: every listed origin read in
/// its ASCII serialization (RFC 6454 §6.2), so `*` and `null` are refused.
fn cors(facade: &Facade) -> Result<crate::facade::cors::Cors, Error> {
    const KEY: &str = "facade.cors.allowed_origins";
    let origins = facade
        .cors
        .allowed_origins
        .iter()
        .map(|value| {
            crate::facade::cors::Origin::parse(value).map_err(|source| Error::CorsOrigin {
                key: String::from(KEY),
                value: value.clone(),
                source,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(crate::facade::cors::Cors::new(origins))
}

/// Returns the access directive exchange `facade` configures, `None` when it
/// is off.
fn consent(facade: &Facade) -> Result<Option<crate::consent::exchange::Settings>, Error> {
    let section = &facade.consent;
    if !section.enabled {
        return Ok(None);
    }
    if section.default_access.is_empty() {
        return Err(Error::Missing {
            key: String::from("facade.consent.default_access"),
        });
    }
    let default_access = AccessDefault::read(TERMINOLOGY_ID_ACCESS, &section.default_access)
        .map_err(|_outside| Error::AccessDefault {
            key: String::from("facade.consent.default_access"),
            value: section.default_access.clone(),
        })?;
    let extensions_root = super::url_of("facade.consent.extensions_url", &section.extensions_url)?;
    Ok(Some(crate::consent::exchange::Settings {
        extensions_root,
        default_access,
    }))
}

/// Returns the identifier resolve `facade` configures, `None` when it is off.
fn identifier_search(
    facade: &Facade,
) -> Result<Option<crate::facade::search::identifier::IdentifierSearch>, Error> {
    let section = &facade.identifier_search;
    if !section.enabled {
        return Ok(None);
    }
    if section.query.is_empty() {
        return Err(Error::Missing {
            key: String::from("facade.identifier_search.query"),
        });
    }
    let refused = |key: &str| {
        let key = key.to_owned();
        move |source| Error::IdentifierQuery { key, source }
    };
    let alone = crate::facade::search::identifier::IdentifierSearch::new(&section.query, None)
        .map_err(refused("facade.identifier_search.query"))?;
    if section.system_query.is_empty() {
        return Ok(Some(alone));
    }
    crate::facade::search::identifier::IdentifierSearch::new(
        &section.query,
        Some(&section.system_query),
    )
    .map(Some)
    .map_err(refused("facade.identifier_search.system_query"))
}

/// Returns the party searches `facade` configures, `None` when they are off.
fn party_search(
    facade: &Facade,
) -> Result<Option<crate::facade::search::party::PartySearch>, Error> {
    let section = &facade.party_search;
    if !section.enabled {
        return Ok(None);
    }
    if section.types.is_empty() {
        return Err(Error::Missing {
            key: String::from("facade.party_search.types"),
        });
    }
    let mut types = BTreeMap::new();
    for (resource_type, queries) in &section.types {
        let key = |field: &str| format!("facade.party_search.types.{resource_type}.{field}");
        let system = (!queries.system_query.is_empty()).then_some(queries.system_query.as_str());
        if queries.query.is_empty() && system.is_some() {
            return Err(Error::Missing { key: key("query") });
        }
        let field = if system.is_some() {
            "system_query"
        } else {
            "query"
        };
        let identifier = (!queries.query.is_empty())
            .then(|| {
                crate::facade::search::identifier::IdentifierSearch::new(&queries.query, system)
                    .map_err(|source| Error::IdentifierQuery {
                        key: key(field),
                        source,
                    })
            })
            .transpose()?;
        let name = (!queries.name_query.is_empty())
            .then(|| {
                crate::facade::search::party::NameSearch::new(&queries.name_query).map_err(
                    |source| Error::IdentifierQuery {
                        key: key("name_query"),
                        source,
                    },
                )
            })
            .transpose()?;
        types.insert(
            resource_type.clone(),
            crate::facade::search::party::PartyQueries::new(identifier, name),
        );
    }
    Ok(Some(crate::facade::search::party::PartySearch::new(types)))
}

/// Returns the reference policy `facade` configures.
///
/// Every fetch base is an absolute http or https URL with no query and no
/// fragment, because a reference is resolved by appending `<Type>/<id>` to
/// it (<https://hl7.org/fhir/R4/references.html#literal>).
fn reference_policy(facade: &Facade) -> Result<crate::facade::references::Policy, Error> {
    const KEY: &str = "facade.references.fetch_bases";
    let mut bases = Vec::with_capacity(facade.references.fetch_bases.len());
    for value in &facade.references.fetch_bases {
        let base = super::url_of(KEY, value)?;
        if !matches!(base.scheme(), "http" | "https")
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(Error::FetchBase {
                key: String::from(KEY),
                value: value.clone(),
            });
        }
        bases.push(base);
    }
    crate::facade::references::Policy::new(
        bases,
        facade.references.max_depth,
        facade.references.timeout.get(),
    )
    .map_err(|source| Error::Client {
        upstream: "reference fetch",
        source: Box::new(source),
    })
}

/// Returns the search page sizes `facade` configures.
///
/// A page holds at least one match, and the default page fits under the
/// largest one.
fn search_limits(facade: &Facade) -> Result<crate::facade::search::Limits, Error> {
    if facade.search_count_max == 0 {
        return Err(Error::SearchCount {
            key: String::from("facade.search_count_max"),
            value: 0,
            reason: "a page holds at least one match",
        });
    }
    if facade.search_count == 0 || facade.search_count > facade.search_count_max {
        return Err(Error::SearchCount {
            key: String::from("facade.search_count"),
            value: facade.search_count,
            reason: "the default page holds at least one match and no more than facade.search_count_max",
        });
    }
    Ok(crate::facade::search::Limits {
        default_count: facade.search_count,
        max_count: facade.search_count_max,
    })
}

/// Returns the EHR policy `value` names under `key`.
fn ehr_policy_of(key: &str, value: &str) -> Result<crate::identity::ehr::Policy, Error> {
    match value {
        "existing" => Ok(crate::identity::ehr::Policy::Existing),
        "create_on_first_write" => Ok(crate::identity::ehr::Policy::CreateOnFirstWrite),
        _ => Err(Error::EhrPolicy {
            key: key.to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Returns the facade settings of `[facade]`, when the facade is on.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(
    config: &Config,
    identity: Option<std::sync::Arc<crate::identity_source::IdentitySource>>,
) -> Result<Option<FacadeSettings>, Error> {
    match (config.facade.enabled, identity) {
        (true, Some(identity)) => settings(&config.facade, identity).map(Some),
        _ => Ok(None),
    }
}
