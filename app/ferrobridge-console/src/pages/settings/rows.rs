// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows of each Settings tab and of each Configuration tab, as the
//! FerroBRIDGE console design lays them out: each its words, its keys and
//! how it draws them.

use ferrobridge_operator_api::configuration::Configuration;

use super::{Row, Shape};
use crate::locale::messages::Msg;

/// Declares one row.
const fn row(label: Msg, description: Msg, keys: &'static [(&'static str, Shape)]) -> Row {
    Row {
        label,
        description,
        keys,
    }
}

/// The FHIR facade and its operations.
pub const FACADE: [Row; 9] = [
    row(
        Msg::RowFacade,
        Msg::RowFacadeDesc,
        &[("facade.enabled", Shape::Switch)],
    ),
    row(
        Msg::RowBaseUrl,
        Msg::RowFacadeBaseUrlDesc,
        &[("facade.base_url", Shape::Code)],
    ),
    row(
        Msg::RowEhrs,
        Msg::RowFacadeEhrsDesc,
        &[("facade.ehr_policy", Shape::Word)],
    ),
    row(
        Msg::RowIdentityStore,
        Msg::RowIdentityStoreDesc,
        &[("facade.identity_store", Shape::Code)],
    ),
    row(
        Msg::RowLanguageTerritory,
        Msg::RowLanguageTerritoryDesc,
        &[
            ("facade.composition_language", Shape::Word),
            ("facade.composition_territory", Shape::Word),
        ],
    ),
    row(
        Msg::RowSearchPage,
        Msg::RowSearchPageDesc,
        &[
            ("facade.search_count", Shape::Number),
            ("facade.search_count_max", Shape::Number),
        ],
    ),
    row(
        Msg::RowReferenceBases,
        Msg::RowReferenceBasesDesc,
        &[
            ("facade.references.fetch_bases", Shape::List),
            ("facade.references.max_depth", Shape::Number),
        ],
    ),
    row(
        Msg::RowIdentifierSearch,
        Msg::RowIdentifierSearchDesc,
        &[("facade.identifier_search.enabled", Shape::Switch)],
    ),
    row(
        Msg::RowOperations,
        Msg::RowOperationsDesc,
        &[("operations.enabled", Shape::Switch)],
    ),
];

/// The HL7 v2 face.
pub const HL7V2: [Row; 11] = [
    row(
        Msg::RowHl7v2,
        Msg::RowHl7v2Desc,
        &[("hl7v2.enabled", Shape::Switch)],
    ),
    row(
        Msg::RowAddress,
        Msg::RowAddressDesc,
        &[("hl7v2.listen", Shape::Code)],
    ),
    row(
        Msg::RowCharset,
        Msg::RowCharsetDesc,
        &[("hl7v2.default_charset", Shape::Word)],
    ),
    row(
        Msg::RowConceptMaps,
        Msg::RowConceptMapsDesc,
        &[("hl7v2.concept_maps", Shape::Code)],
    ),
    row(
        Msg::RowSupplements,
        Msg::RowSupplementsDesc,
        &[("hl7v2.supplements", Shape::List)],
    ),
    row(
        Msg::RowEhrs,
        Msg::RowHl7v2EhrsDesc,
        &[("hl7v2.ehr_policy", Shape::Word)],
    ),
    row(
        Msg::RowProfiles,
        Msg::RowProfilesDesc,
        &[("hl7v2.profiles", Shape::Count)],
    ),
    row(
        Msg::RowUnmapped,
        Msg::RowUnmappedDesc,
        &[("hl7v2.unmapped_entries", Shape::Word)],
    ),
    row(
        Msg::RowTimeouts,
        Msg::RowTimeoutsDesc,
        &[
            ("hl7v2.idle_timeout_ms", Shape::Millis),
            ("hl7v2.frame_timeout_ms", Shape::Millis),
        ],
    ),
    row(
        Msg::RowLargestFrame,
        Msg::RowLargestFrameDesc,
        &[("hl7v2.frame_limit_bytes", Shape::Bytes)],
    ),
    row(
        Msg::RowSenders,
        Msg::RowSendersDesc,
        &[("hl7v2.senders", Shape::Count)],
    ),
];

/// The OMOP ETL.
pub const OMOP: [Row; 7] = [
    row(
        Msg::RowCompositionQuery,
        Msg::RowCompositionQueryDesc,
        &[("etl.aql", Shape::Query)],
    ),
    row(
        Msg::RowPageSize,
        Msg::RowPageSizeDesc,
        &[("etl.page_size", Shape::Number)],
    ),
    row(
        Msg::RowDeletions,
        Msg::RowDeletionsDesc,
        &[("etl.deletions", Shape::Word)],
    ),
    row(
        Msg::RowTypeConcepts,
        Msg::RowTypeConceptsDesc,
        &[
            ("etl.type_concept_id", Shape::Number),
            ("etl.observation_period_type_concept_id", Shape::Number),
        ],
    ),
    row(
        Msg::RowAliases,
        Msg::RowAliasesDesc,
        &[("etl.vocabulary_aliases", Shape::Count)],
    ),
    row(
        Msg::RowVisitQuery,
        Msg::RowVisitQueryDesc,
        &[("etl.visits.aql", Shape::Query)],
    ),
    row(
        Msg::RowVisitConcepts,
        Msg::RowVisitConceptsDesc,
        &[
            ("etl.visits.visit_concept_id", Shape::Number),
            ("etl.visits.visit_type_concept_id", Shape::Number),
        ],
    ),
];

/// The openEHR CDR, before its credentials.
const CDR: [Row; 4] = [
    row(
        Msg::RowBaseUrl,
        Msg::RowCdrBaseUrlDesc,
        &[("cdr.base_url", Shape::Code)],
    ),
    row(
        Msg::RowTimeout,
        Msg::RowTimeoutDesc,
        &[("cdr.timeout_ms", Shape::Millis)],
    ),
    row(
        Msg::RowRetry,
        Msg::RowRetryDesc,
        &[
            ("cdr.retry.max_attempts", Shape::Number),
            ("cdr.retry.initial_backoff_ms", Shape::Millis),
            ("cdr.retry.max_backoff_ms", Shape::Millis),
        ],
    ),
    row(
        Msg::RowDemographic,
        Msg::RowDemographicDesc,
        &[("cdr.demographic", Shape::Switch)],
    ),
];

/// The CDR's client credentials grant.
const CDR_OAUTH: [Row; 3] = [
    row(
        Msg::RowTokenEndpoint,
        Msg::RowTokenEndpointDesc,
        &[("cdr.credentials.oauth.token_endpoint", Shape::Code)],
    ),
    row(
        Msg::RowClientId,
        Msg::RowClientIdDesc,
        &[("cdr.credentials.oauth.client_id", Shape::Code)],
    ),
    row(
        Msg::RowClientSecret,
        Msg::RowClientSecretDesc,
        &[("cdr.credentials.oauth.client_secret", Shape::Secret)],
    ),
];

/// The CDR's Basic credentials.
const CDR_BASIC: [Row; 2] = [
    row(
        Msg::RowUser,
        Msg::RowUserDesc,
        &[("cdr.credentials.user", Shape::Code)],
    ),
    row(
        Msg::RowPassword,
        Msg::RowPasswordDesc,
        &[("cdr.credentials.password", Shape::Secret)],
    ),
];

/// The terminology server, before its credentials.
const TERMINOLOGY: [Row; 5] = [
    row(
        Msg::RowBaseUrl,
        Msg::RowTerminologyBaseUrlDesc,
        &[("terminology.base_url", Shape::Code)],
    ),
    row(
        Msg::RowFhirVersion,
        Msg::RowFhirVersionDesc,
        &[("terminology.wire_version", Shape::Word)],
    ),
    row(
        Msg::RowTimeout,
        Msg::RowTimeoutDesc,
        &[("terminology.timeout_ms", Shape::Millis)],
    ),
    row(
        Msg::RowRetry,
        Msg::RowRetryDesc,
        &[
            ("terminology.retry.max_attempts", Shape::Number),
            ("terminology.retry.initial_backoff_ms", Shape::Millis),
            ("terminology.retry.max_backoff_ms", Shape::Millis),
        ],
    ),
    row(
        Msg::RowDisplayFallback,
        Msg::RowDisplayFallbackDesc,
        &[("terminology.display_fallback", Shape::Switch)],
    ),
];

/// The terminology server's client credentials grant.
const TERMINOLOGY_OAUTH: [Row; 3] = [
    row(
        Msg::RowTokenEndpoint,
        Msg::RowTokenEndpointDesc,
        &[("terminology.credentials.oauth.token_endpoint", Shape::Code)],
    ),
    row(
        Msg::RowClientId,
        Msg::RowClientIdDesc,
        &[("terminology.credentials.oauth.client_id", Shape::Code)],
    ),
    row(
        Msg::RowClientSecret,
        Msg::RowClientSecretDesc,
        &[("terminology.credentials.oauth.client_secret", Shape::Secret)],
    ),
];

/// The terminology server's Basic credentials.
const TERMINOLOGY_BASIC: [Row; 2] = [
    row(
        Msg::RowUser,
        Msg::RowUserDesc,
        &[("terminology.credentials.user", Shape::Code)],
    ),
    row(
        Msg::RowPassword,
        Msg::RowPasswordDesc,
        &[("terminology.credentials.password", Shape::Secret)],
    ),
];

/// The OMOP CDM database.
const CDM: [Row; 4] = [
    row(
        Msg::RowConnectionUrl,
        Msg::RowConnectionUrlDesc,
        &[("cdm.url", Shape::Secret)],
    ),
    row(
        Msg::RowTlsCa,
        Msg::RowTlsCaDesc,
        &[("cdm.tls_ca_file", Shape::Code)],
    ),
    row(
        Msg::RowSchemas,
        Msg::RowSchemasDesc,
        &[
            ("cdm.schema", Shape::Code),
            ("cdm.bridge_schema", Shape::Code),
        ],
    ),
    row(
        Msg::RowNewPersons,
        Msg::RowNewPersonsDesc,
        &[("cdm.person_policy", Shape::Word)],
    ),
];

/// The Logging tab of the Configuration page.
pub const LOGGING: [Row; 3] = [
    row(
        Msg::RowFormat,
        Msg::RowFormatDesc,
        &[("telemetry.format", Shape::Word)],
    ),
    row(
        Msg::RowFilter,
        Msg::RowFilterDesc,
        &[("telemetry.filter", Shape::Code)],
    ),
    row(
        Msg::RowLoggedParameters,
        Msg::RowLoggedParametersDesc,
        &[("telemetry.logged_query_parameters", Shape::List)],
    ),
];

/// The Mappings tab of the Configuration page.
pub const MAPPINGS: [Row; 3] = [
    row(
        Msg::RowMappingDirectory,
        Msg::RowMappingDirectoryDesc,
        &[("mappings.directory", Shape::Code)],
    ),
    row(
        Msg::RowTemplates,
        Msg::RowTemplatesDesc,
        &[("mappings.templates", Shape::Code)],
    ),
    row(
        Msg::RowOmoclDirectory,
        Msg::RowOmoclDirectoryDesc,
        &[("mappings.omocl", Shape::Code)],
    ),
];

/// The Server and access tab of the Configuration page.
pub const CORE: [Row; 8] = [
    row(
        Msg::RowListener,
        Msg::RowListenerDesc,
        &[("server.listen", Shape::Code)],
    ),
    row(
        Msg::RowRequestLimits,
        Msg::RowRequestLimitsDesc,
        &[
            ("server.request_timeout_ms", Shape::Millis),
            ("server.body_limit_bytes", Shape::Bytes),
        ],
    ),
    row(
        Msg::RowIssuers,
        Msg::RowIssuersDesc,
        &[("auth.issuer", Shape::Code)],
    ),
    row(
        Msg::RowAudience,
        Msg::RowAudienceDesc,
        &[("auth.audience", Shape::List)],
    ),
    row(
        Msg::RowOperatorScope,
        Msg::RowOperatorScopeDesc,
        &[("auth.product_scope", Shape::Code)],
    ),
    row(
        Msg::RowOperatorApi,
        Msg::RowOperatorApiDesc,
        &[("operator.enabled", Shape::Switch)],
    ),
    row(
        Msg::RowConsoleDirectory,
        Msg::RowConsoleDirectoryDesc,
        &[("operator.mapping_dir", Shape::Code)],
    ),
    row(
        Msg::RowUsageReport,
        Msg::RowUsageReportDesc,
        &[("usage_report.enabled", Shape::Switch)],
    ),
];

/// How an upstream signs in to its server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credentials {
    /// The OAuth 2.0 client credentials grant.
    Oauth,
    /// HTTP Basic.
    Basic,
    /// None.
    None,
}

/// Returns how the upstream `name` (`cdr` or `terminology`) signs in.
#[must_use]
pub fn credentials(configuration: &Configuration, name: &str) -> Credentials {
    if configuration
        .entry(&format!("{name}.credentials.oauth.client_id"))
        .is_some()
    {
        Credentials::Oauth
    } else if configuration
        .entry(&format!("{name}.credentials.user"))
        .is_some_and(|entry| {
            !matches!(&entry.value, ferrobridge_operator_api::configuration::Value::String(user) if user.is_empty())
        })
    {
        Credentials::Basic
    } else {
        Credentials::None
    }
}

/// Returns the rows of the upstream `name`'s Settings tab, its credentials
/// as it signs in.
#[must_use]
pub fn connection(configuration: &Configuration, name: &str) -> Vec<Row> {
    match name {
        "cdr" => [
            CDR.as_slice(),
            match credentials(configuration, name) {
                Credentials::Oauth => CDR_OAUTH.as_slice(),
                Credentials::Basic => CDR_BASIC.as_slice(),
                Credentials::None => &[],
            },
        ]
        .concat(),
        "terminology" => [
            TERMINOLOGY.as_slice(),
            match credentials(configuration, name) {
                Credentials::Oauth => TERMINOLOGY_OAUTH.as_slice(),
                Credentials::Basic => TERMINOLOGY_BASIC.as_slice(),
                Credentials::None => &[],
            },
        ]
        .concat(),
        _ => CDM.to_vec(),
    }
}
