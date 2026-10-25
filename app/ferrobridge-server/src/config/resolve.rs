// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of each lane of the tree into the settings the run path
//! holds.

use omocl::engine::concept::VocabularyAliases;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use super::CdmSettings;
use super::FacadeSettings;
use super::Hl7v2Settings;
use super::MappingSettings;
use super::credentials::Upstream;
use super::credentials::cdm_connection_key;
use super::credentials::resolve_credentials;
use super::credentials::secret;
use super::error::Error;
use super::section::Cdm;
use super::section::Cdr;
use super::section::Etl;
use super::section::Hl7v2;
use super::section::Hl7v2Sender;
use super::section::Mappings;
use super::section::Operator;
use super::section::Terminology;
use super::section::facade::Facade;
use crate::credentials::UpstreamToken;

/// Returns the mapping-set paths `mappings` names, when it names both.
///
/// A set with no `templates` resolves to `None` here and is carried by
/// [`Settings::mapping_directory`] alone: the facade and the two operations
/// then take their templates from the CDR at boot. A `templates` with no
/// `directory` names nothing to compile.
///
/// # Errors
///
/// Returns [`Error::Missing`] when `templates` is set and `directory` is not.
///
/// [`Settings::mapping_directory`]: super::Settings::mapping_directory
pub(super) fn resolve_mappings(mappings: &Mappings) -> Result<Option<MappingSettings>, Error> {
    match (mappings.directory.as_ref(), mappings.templates.as_ref()) {
        (Some(directory), Some(templates)) => Ok(Some(MappingSettings {
            directory: directory.clone(),
            templates: templates.clone(),
        })),
        (None, Some(_)) => Err(Error::Missing {
            key: String::from("mappings.directory"),
        }),
        _ => Ok(None),
    }
}

/// Returns the facade settings `facade` describes.
pub(super) fn resolve_facade(facade: &Facade) -> Result<FacadeSettings, Error> {
    let ehr_policy = ehr_policy_of("facade.ehr_policy", &facade.ehr_policy)?;
    for (key, value) in [
        ("facade.base_url", &facade.base_url),
        ("facade.composition_language", &facade.composition_language),
        (
            "facade.composition_territory",
            &facade.composition_territory,
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
            subject: subject_link(facade)?,
            system_id: facade.system_id.clone(),
            language: facade.composition_language.clone(),
            territory: facade.composition_territory.clone(),
            ehr_status_system: (!facade.ehr_status_identifier_system.is_empty())
                .then(|| facade.ehr_status_identifier_system.clone()),
        },
        identity_store: facade.identity_store.clone(),
        search: search_limits(facade)?,
        references: reference_policy(facade)?,
        identifier: identifier_search(facade)?,
        party_search: party_search(facade)?,
    })
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

/// Returns the subject link `facade` configures.
fn subject_link(facade: &Facade) -> Result<crate::facade::subject::SubjectLink, Error> {
    const FORM: &str = "facade.subject_id_form";
    let form = crate::facade::subject::IdForm::parse(&facade.subject_id_form).ok_or_else(|| {
        Error::SubjectIdForm {
            key: String::from(FORM),
            value: facade.subject_id_form.clone(),
        }
    })?;
    crate::facade::subject::SubjectLink::new(&facade.subject_namespace, form, &facade.subject_type)
        .map_err(|source| {
            let key = match source {
                crate::facade::subject::SubjectError::PartyType { .. } => "facade.subject_type",
                _ => "facade.subject_namespace",
            };
            Error::Subject {
                key: String::from(key),
                source: Box::new(source),
            }
        })
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
        let base = url_of(KEY, value)?;
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
        Duration::from_millis(facade.references.timeout_ms),
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

/// Returns the sender `sender` names under `key`.
fn sender_of(key: &str, sender: &Hl7v2Sender) -> Result<crate::hl7v2::Sender, Error> {
    let named = |text: &str| !text.is_empty();
    match (
        named(&sender.namespace_id),
        named(&sender.universal_id),
        named(&sender.universal_id_type),
    ) {
        (true, false, false) => Ok(crate::hl7v2::Sender::Namespace(sender.namespace_id.clone())),
        (false, true, true) => Ok(crate::hl7v2::Sender::Universal {
            id: sender.universal_id.clone(),
            kind: sender.universal_id_type.clone(),
        }),
        _ => Err(Error::Sender {
            key: key.to_owned(),
        }),
    }
}

/// Returns the EHR policy `value` names under `key`.
fn ehr_policy_of(key: &str, value: &str) -> Result<crate::facade::ehr::Policy, Error> {
    match value {
        "existing" => Ok(crate::facade::ehr::Policy::Existing),
        "create_on_first_write" => Ok(crate::facade::ehr::Policy::CreateOnFirstWrite),
        _ => Err(Error::EhrPolicy {
            key: key.to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Returns the timeout `milliseconds` states, `None` for `0`.
fn timeout_of(milliseconds: u64) -> Option<Duration> {
    (milliseconds > 0).then(|| Duration::from_millis(milliseconds))
}

/// Returns the HL7 v2 face `hl7v2` describes, refused when the facade it
/// writes through is off.
pub(super) fn resolve_hl7v2(hl7v2: &Hl7v2, facade: bool) -> Result<Hl7v2Settings, Error> {
    if !facade {
        return Err(Error::Needs {
            key: String::from("hl7v2.enabled"),
            needs: String::from("facade.enabled"),
        });
    }
    let listen = hl7v2
        .listen
        .parse::<SocketAddr>()
        .map_err(|source| Error::Listen {
            key: String::from("hl7v2.listen"),
            source,
        })?;
    let default_charset = ferrobridge_hl7v2::decode::Charset::from_code(&hl7v2.default_charset)
        .ok_or_else(|| Error::Charset {
            key: String::from("hl7v2.default_charset"),
            value: hl7v2.default_charset.clone(),
        })?;
    let concept_maps = hl7v2.concept_maps.clone().ok_or_else(|| Error::Missing {
        key: String::from("hl7v2.concept_maps"),
    })?;
    let unmapped = match hl7v2.unmapped_entries.as_str() {
        "skip_and_count" => crate::facade::ingest::UnmappedEntries::SkipAndCount,
        "refuse" => crate::facade::ingest::UnmappedEntries::Refuse,
        _ => {
            return Err(Error::UnmappedEntries {
                key: String::from("hl7v2.unmapped_entries"),
                value: hl7v2.unmapped_entries.clone(),
            });
        }
    };
    let ehr_policy = hl7v2
        .ehr_policy
        .as_deref()
        .map(|value| ehr_policy_of("hl7v2.ehr_policy", value))
        .transpose()?;
    let mut profiles = BTreeMap::new();
    for claimed in &hl7v2.profiles {
        if !fhir_types::r4::schema::SCHEMAS.is_resource(&claimed.resource_type) {
            return Err(Error::ResourceType {
                key: String::from("hl7v2.profiles.resource_type"),
                value: claimed.resource_type.clone(),
            });
        }
        url_of("hl7v2.profiles.profile", &claimed.profile)?;
        if profiles
            .insert(claimed.resource_type.clone(), claimed.profile.clone())
            .is_some()
        {
            return Err(Error::Duplicate {
                key: String::from("hl7v2.profiles"),
                value: claimed.resource_type.clone(),
            });
        }
    }
    if hl7v2.frame_limit_bytes == 0 {
        return Err(Error::FrameLimit {
            key: String::from("hl7v2.frame_limit_bytes"),
        });
    }
    Ok(Hl7v2Settings {
        listen,
        default_charset,
        concept_maps,
        supplements: hl7v2.supplements.clone(),
        unmapped,
        ehr_policy,
        profiles,
        timeouts: ferrobridge_hl7v2::mllp::Timeouts {
            idle: timeout_of(hl7v2.idle_timeout_ms),
            frame: timeout_of(hl7v2.frame_timeout_ms),
        },
        frame_limit: hl7v2.frame_limit_bytes,
        log_outcomes: hl7v2.log_outcomes,
        senders: hl7v2
            .senders
            .iter()
            .map(|sender| sender_of("hl7v2.senders", sender))
            .collect::<Result<Vec<_>, Error>>()?,
    })
}

/// Returns the CDR client configuration `cdr` describes.
pub(super) fn resolve_cdr(cdr: &Cdr) -> Result<crate::cdr::config::CdrConfig, Error> {
    let base_url = url_of("cdr.base_url", &cdr.base_url)?;
    let mut config = crate::cdr::config::CdrConfig::new(base_url)
        .with_timeout(Duration::from_millis(cdr.timeout_ms))
        .with_demographic(cdr.demographic)
        .with_retry(openehr_its::rest::client::RetryPolicy {
            // NOTE: no specification governs this: our own design; a u32 fits a
            // usize on every target the server builds for, so the clamp never bites.
            max_attempts: usize::try_from(cdr.retry.max_attempts).unwrap_or(usize::MAX),
            initial_backoff: Duration::from_millis(cdr.retry.initial_backoff_ms),
            max_backoff: Duration::from_millis(cdr.retry.max_backoff_ms),
        });
    match resolve_credentials("cdr.credentials", &cdr.credentials)? {
        Some(Upstream::Basic { user, password }) => {
            config = config.with_basic(user, password);
        }
        Some(Upstream::OAuth(grant)) => {
            config = config.with_oauth(*grant);
        }
        None => {}
    }
    Ok(config)
}

/// Returns the terminology client configuration `terminology` describes.
pub(super) fn resolve_terminology(
    terminology: &Terminology,
) -> Result<ferrobridge_term::config::Config, Error> {
    let base_url = url_of("terminology.base_url", &terminology.base_url)?;
    let wire_version = match terminology.wire_version.to_ascii_lowercase().as_str() {
        "stu3" => ferrobridge_term::config::WireVersion::Stu3,
        "r4" => ferrobridge_term::config::WireVersion::R4,
        "r4b" => ferrobridge_term::config::WireVersion::R4B,
        _ => {
            return Err(Error::WireVersion {
                key: String::from("terminology.wire_version"),
                value: terminology.wire_version.clone(),
            });
        }
    };
    let mut config = ferrobridge_term::config::Config::new(base_url, wire_version)
        .with_timeout(Duration::from_millis(terminology.timeout_ms))
        .with_retry(ferrobridge_term::config::RetryPolicy {
            max_attempts: terminology.retry.max_attempts,
            initial_backoff: Duration::from_millis(terminology.retry.initial_backoff_ms),
            max_backoff: Duration::from_millis(terminology.retry.max_backoff_ms),
        });
    match resolve_credentials("terminology.credentials", &terminology.credentials)? {
        Some(Upstream::Basic { user, password }) => {
            config = config
                .with_credentials(ferrobridge_term::config::Credentials::Basic { user, password });
        }
        Some(Upstream::OAuth(grant)) => {
            let source = UpstreamToken::new(*grant).map_err(|source| Error::Client {
                upstream: "terminology",
                source: Box::new(source),
            })?;
            config = config.with_credentials(ferrobridge_term::config::Credentials::OAuth(
                Arc::new(source),
            ));
        }
        None => {}
    }
    Ok(config)
}

/// Returns the CDM database `cdm` describes.
pub(super) fn resolve_cdm(cdm: &Cdm) -> Result<CdmSettings, Error> {
    let url =
        secret("cdm.url", cdm.url.as_ref(), cdm.url_file.as_deref())?.ok_or(Error::Missing {
            key: String::from("cdm.url"),
        })?;
    let schema_of = |key: &str, name: &str| {
        omop_cdm::ddl::SchemaName::new(name).map_err(|source| Error::Schema {
            key: key.to_owned(),
            source,
        })
    };
    let person_policy = match cdm.person_policy.as_str() {
        "create_on_first_sight" => omop_cdm::writer::input::PersonPolicy::CreateOnFirstSight,
        "existing" => omop_cdm::writer::input::PersonPolicy::Existing,
        _ => {
            return Err(Error::PersonPolicy {
                key: String::from("cdm.person_policy"),
                value: cdm.person_policy.clone(),
            });
        }
    };
    let ca = match (cdm.tls_ca.as_deref(), cdm.tls_ca_file.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(Error::Conflict {
                key: String::from("cdm.tls_ca"),
            });
        }
        (Some(pem), None) => Some(pem.as_bytes().to_vec()),
        (None, Some(path)) => Some(std::fs::read(path).map_err(|source| Error::Secret {
            key: String::from("cdm.tls_ca_file"),
            path: path.to_path_buf(),
            source,
        })?),
        (None, None) => None,
    };
    let pseudonymiser = cdm
        .pseudonymisation
        .as_ref()
        .map(pseudonymiser_of)
        .transpose()?;
    let connection = omop_cdm::connection::CdmConnection::new(url.expose_secret(), ca.as_deref())
        .map_err(|source| Error::CdmConnection {
        key: String::from(cdm_connection_key(&source, cdm)),
        source,
    })?;
    Ok(CdmSettings {
        url,
        connection,
        schema: schema_of("cdm.schema", &cdm.schema)?,
        bridge_schema: schema_of("cdm.bridge_schema", &cdm.bridge_schema)?,
        person_policy,
        pseudonymiser,
    })
}

/// Returns the pseudonymiser `section` describes, its key read and checked.
fn pseudonymiser_of(
    section: &crate::config::section::CdmPseudonymisation,
) -> Result<omop_cdm::writer::pseudonym::Pseudonymiser, Error> {
    let key = secret(
        "cdm.pseudonymisation.key",
        section.key.as_ref(),
        section.key_file.as_deref(),
    )?
    .ok_or(Error::Missing {
        key: String::from("cdm.pseudonymisation.key"),
    })?;
    omop_cdm::writer::pseudonym::Pseudonymiser::new(
        key.expose_secret().as_bytes(),
        section.date_shift_days,
    )
    .map_err(|source| Error::Pseudonymisation {
        key: String::from("cdm.pseudonymisation"),
        source,
    })
}

/// Returns the value of the required integer `key`.
pub(super) fn required(key: &str, value: Option<i32>) -> Result<i32, Error> {
    value.ok_or_else(|| Error::Missing {
        key: key.to_owned(),
    })
}

/// Returns the checked query `text` holds, refusing it under `key`.
fn query_of(
    key: &str,
    text: &str,
    check: fn(&str) -> Result<crate::etl::aql::CheckedQuery, crate::etl::aql::AqlError>,
) -> Result<crate::etl::aql::CheckedQuery, Error> {
    if text.is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    check(text).map_err(|source| Error::Aql {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the engine's vocabulary aliases with every alias `aliases` names
/// added over them, refusing an empty terminology id, an empty vocabulary and
/// a vocabulary wider than OMOP CDM v5.4 `VOCABULARY.vocabulary_id`.
///
/// No specification governs this table: our own design. An alias only names
/// the `vocabulary_id` a terminology id is looked up under; the concept is
/// still found by SQL over the loaded vocabulary.
fn resolve_aliases(aliases: &BTreeMap<String, String>) -> Result<VocabularyAliases, Error> {
    let mut table = VocabularyAliases::default();
    for (terminology, vocabulary) in aliases {
        if terminology.is_empty() {
            return Err(Error::AliasTerminology {
                key: String::from("etl.vocabulary_aliases"),
            });
        }
        let key = format!("etl.vocabulary_aliases.\"{terminology}\"");
        if vocabulary.is_empty() {
            return Err(Error::Missing { key });
        }
        if let Err(source) = omop_cdm::vocabulary::VocabularyId::new(vocabulary.as_str()) {
            return Err(Error::AliasVocabulary {
                key,
                value: vocabulary.clone(),
                source,
            });
        }
        table = table.with(terminology.as_str(), vocabulary.as_str());
    }
    Ok(table)
}

/// Returns the ETL job `etl` describes, with both queries checked.
pub(super) fn resolve_etl(etl: &Etl) -> Result<crate::etl::EtlSettings, Error> {
    // NOTE: no specification governs this: our own design; `etl fhir` reads no CDR, so
    // the composition query is optional here and `etl run` refuses a run without one.
    let compositions = if etl.aql.is_empty() {
        None
    } else {
        Some(query_of(
            "etl.aql",
            &etl.aql,
            crate::etl::aql::CheckedQuery::compositions,
        )?)
    };
    let page_size =
        crate::cdr::query::PageSize::new(etl.page_size).map_err(|source| Error::PageSize {
            key: String::from("etl.page_size"),
            source,
        })?;
    let visits = etl
        .visits
        .as_ref()
        .map(|visits| {
            Ok::<_, Error>(crate::etl::VisitSettings {
                query: query_of(
                    "etl.visits.aql",
                    &visits.aql,
                    crate::etl::aql::CheckedQuery::visits,
                )?,
                concepts: omop_cdm::writer::input::VisitConcepts {
                    visit_concept_id: required(
                        "etl.visits.visit_concept_id",
                        visits.visit_concept_id,
                    )?,
                    visit_type_concept_id: required(
                        "etl.visits.visit_type_concept_id",
                        visits.visit_type_concept_id,
                    )?,
                },
            })
        })
        .transpose()?;
    let deletions = match etl.deletions.as_str() {
        "remove" => crate::etl::deletions::DeletionPolicy::Remove,
        "report" => crate::etl::deletions::DeletionPolicy::Report,
        _ => {
            return Err(Error::Deletions {
                key: String::from("etl.deletions"),
                value: etl.deletions.clone(),
            });
        }
    };
    Ok(crate::etl::EtlSettings {
        compositions,
        page_size,
        type_concept_id: required("etl.type_concept_id", etl.type_concept_id)?,
        observation_period_type_concept_id: required(
            "etl.observation_period_type_concept_id",
            etl.observation_period_type_concept_id,
        )?,
        visits,
        deletions,
        aliases: resolve_aliases(&etl.vocabulary_aliases)?,
        quality_checks: etl.quality_checks,
    })
}

/// Returns the URL `value` holds, naming `key` when it does not parse.
pub(super) fn url_of(key: &str, value: &str) -> Result<url::Url, Error> {
    if value.is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    value.parse().map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })
}

/// Returns the operator API settings, refusing the API without `[auth]` so it
/// is never served to an unauthenticated caller.
pub(super) fn resolve_operator(
    operator: &Operator,
    auth: &ferrohealth_smart::config::AuthSection,
) -> Result<Option<crate::operator::OperatorSettings>, Error> {
    if operator.enabled && !auth.enabled {
        return Err(Error::Needs {
            key: String::from("operator.enabled"),
            needs: String::from("auth.enabled"),
        });
    }
    Ok(operator.enabled.then(|| crate::operator::OperatorSettings {
        mapping_dir: operator.mapping_dir.clone(),
    }))
}
