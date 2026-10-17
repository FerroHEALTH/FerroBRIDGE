// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of a configuration: every reason the server does not start on
//! what the file and the environment state.

use std::path::PathBuf;

use crate::config::ENV_PREFIX;
/// A configuration the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration file could not be read.
    #[error("the configuration file {} could not be read", path.display())]
    Read {
        /// The path that was tried.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The configuration does not parse, names an unknown key, or holds a
    /// value of the wrong type.
    #[error("the configuration is not valid")]
    Parse {
        /// What the TOML reader reported, with the offending key.
        #[source]
        source: toml::de::Error,
    },
    /// The merged configuration could not be written back for re-reading.
    #[error("the configuration could not be assembled")]
    Assemble {
        /// What the TOML writer reported.
        #[source]
        source: toml::ser::Error,
    },
    /// An environment override names no key under the prefix.
    #[error("{name} names no configuration key; use {ENV_PREFIX}<SECTION>__<KEY>")]
    EnvName {
        /// The variable that was read.
        name: String,
    },
    /// An environment override addresses a key under a value that is not a
    /// section.
    #[error("{name} addresses a key under a value that is not a section")]
    EnvShape {
        /// The variable that was read.
        name: String,
    },
    /// A value and its `_file` sibling are both set.
    #[error("{key} is set together with {key}_file; set one of them")]
    Conflict {
        /// The inline key.
        key: String,
    },
    /// A `_file` sibling could not be read.
    #[error("{key} names {}, which could not be read", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A section is present and a key it needs is not set.
    #[error("{key} is not set, and its section is present")]
    Missing {
        /// The key that carries no value.
        key: String,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Listen {
        /// The key that holds it.
        key: String,
        /// What the address parser reported.
        #[source]
        source: std::net::AddrParseError,
    },
    /// The EHR policy names neither of the two values.
    #[error("{key} is `{value}`; the policies are existing and create_on_first_write")]
    EhrPolicy {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// The identity store could not be opened at boot.
    #[error("the identity store at {} could not be opened", path.display())]
    IdentityStore {
        /// The path that was tried.
        path: PathBuf,
        /// What the store reported.
        #[source]
        source: Box<crate::facade::identity::store::StoreError>,
    },
    /// A URL does not parse.
    #[error("{key} is not a URL")]
    Url {
        /// The key that holds it.
        key: String,
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// A FHIR release name is not one this client speaks.
    #[error("{key} is `{value}`; the releases this client speaks are stu3, r4 and r4b")]
    WireVersion {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A credentials section names both a bearer token and a user.
    #[error("{section} names both a bearer token and a user; set one scheme")]
    Scheme {
        /// The credentials section.
        section: String,
    },
    /// A credentials section names the OAuth grant beside a static scheme.
    #[error("{section} names the oauth grant beside a bearer token or a user; set one scheme")]
    OAuthScheme {
        /// The credentials section.
        section: String,
    },
    /// An OAuth section names both an issuer and a token endpoint.
    #[error("{section} names both an issuer and a token_endpoint; set one of them")]
    OAuthEndpoint {
        /// The OAuth section.
        section: String,
    },
    /// An issuer is not an `https` URL free of a query and a fragment, the
    /// form RFC 8414 §2 gives an issuer identifier.
    #[error("{key} is not an https URL without a query or a fragment (RFC 8414 §2)")]
    Issuer {
        /// The key that holds it.
        key: String,
    },
    /// An algorithm is no asymmetric JWS algorithm (RFC 7518 §3.1).
    #[error(
        "{key} names `{value}`; the algorithms are RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384 and EdDSA"
    )]
    Algorithm {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// The token profile names neither of the two values.
    #[error("{key} is `{value}`; the profiles are rfc9068 and jwt")]
    TokenProfile {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A resource indicator carries a fragment, which RFC 8707 §2 forbids.
    #[error("{key} carries a fragment, which a resource indicator may not (RFC 8707 §2)")]
    Resource {
        /// The key that holds it.
        key: String,
    },
    /// A scope is not a space-delimited list of scope tokens (RFC 6749 §3.3).
    #[error("{key} is not a space-delimited list of scope tokens (RFC 6749 §3.3)")]
    Scope {
        /// The key that holds it.
        key: String,
    },
    /// A reference fetch base is no http or https base.
    #[error("{key} holds `{value}`, which is no http or https base without a query or a fragment")]
    FetchBase {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// An upstream client refused the configuration it was handed.
    #[error("the {upstream} client refused its configuration")]
    Client {
        /// Which upstream it was.
        upstream: &'static str,
        /// What the client reported.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A schema name is not an unquoted PostgreSQL identifier.
    #[error("{key} is not a schema name")]
    Schema {
        /// The key that holds it.
        key: String,
        /// Why the name is refused.
        #[source]
        source: omop_cdm::ddl::SchemaNameError,
    },
    /// The CDM database URL, or the CA it is checked against, is refused.
    #[error("{key} is refused")]
    CdmConnection {
        /// The key that holds the refused value.
        key: String,
        /// Why it is refused, the `sslmode` named when that is the fault.
        #[source]
        source: omop_cdm::connection::ConnectionError,
    },
    /// The person policy names neither of the two values.
    #[error("{key} is `{value}`; the policies are create_on_first_sight and existing")]
    PersonPolicy {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A search page size outside the range a search can answer.
    #[error("{key} is {value}; {reason}")]
    SearchCount {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: u32,
        /// Why it is refused.
        reason: &'static str,
    },
    /// The deletion policy names neither of the two values.
    #[error("{key} is `{value}`; the policies are remove and report")]
    Deletions {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A vocabulary alias names an empty terminology id.
    #[error("{key} names an empty terminology id")]
    AliasTerminology {
        /// The table that holds it.
        key: String,
    },
    /// A vocabulary alias names a vocabulary OMOP CDM v5.4
    /// `VOCABULARY.vocabulary_id` cannot hold.
    #[error("{key} is `{value}`, which is no OMOP CDM v5.4 vocabulary_id")]
    AliasVocabulary {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
        /// Why the column refuses it.
        #[source]
        source: omop_cdm::value::ValueError,
    },
    /// A page size that would never advance.
    #[error("{key} is 0; a page holds at least one row")]
    PageSize {
        /// The key that holds it.
        key: String,
        /// What the page size reported.
        #[source]
        source: crate::cdr::query::PageSizeError,
    },
    /// A configured AQL the runner cannot read.
    #[error("{key} is refused")]
    Aql {
        /// The key that holds it.
        key: String,
        /// Why the query is refused.
        #[source]
        source: Box<crate::etl::aql::AqlError>,
    },
    /// A lane is switched on and a lane it needs is not.
    #[error("{key} is set, and it needs {needs}")]
    Needs {
        /// The key that switches the lane on.
        key: String,
        /// The lane it needs, as the key that switches it on.
        needs: String,
    },
    /// A character set code is not one of HL7 table 0211 that is decoded.
    #[error(
        "{key} is `{value}`; the character sets are ASCII, UNICODE UTF-8, 8859/1 to 8859/9 and 8859/15"
    )]
    Charset {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// The unmapped-entry policy names neither of the two values.
    #[error("{key} is `{value}`; the policies are skip_and_count and refuse")]
    UnmappedEntries {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A resource type the R4 element table does not name.
    #[error("{key} is `{value}`, which names no R4 resource type")]
    ResourceType {
        /// The key that holds it.
        key: String,
        /// The value it holds.
        value: String,
    },
    /// A resource type named twice.
    #[error("{key} names {value} twice")]
    Duplicate {
        /// The list that holds it.
        key: String,
        /// The value named twice.
        value: String,
    },
    /// A sending facility named neither by a namespace id alone nor by a
    /// universal id with its type.
    #[error(
        "{key} names a sender by namespace_id alone, or by universal_id with universal_id_type"
    )]
    Sender {
        /// The key that holds it.
        key: String,
    },
    /// A size that would refuse every message.
    #[error("{key} is 0; a frame carries at least one byte")]
    FrameLimit {
        /// The key that holds it.
        key: String,
    },
    /// A collector endpoint that carries credentials or is plain `http` to a
    /// host other than a loopback one.
    #[error("{key} is not an https URL without credentials; only a loopback collector may be http")]
    ReportEndpoint {
        /// The key that holds it.
        key: String,
    },
    /// The mapping set the operations lane runs did not load.
    #[error("the mapping set could not be loaded")]
    Mappings {
        /// Why it did not load, the template and upstream status included.
        #[source]
        source: Box<crate::mappings::Error>,
    },
}
