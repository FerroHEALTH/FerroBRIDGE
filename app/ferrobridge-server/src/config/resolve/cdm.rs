// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[cdm]` into the OMOP CDM database settings.
//!
//! No specification governs the configuration: our own design.

use secrecy::ExposeSecret;

use crate::config::CdmSettings;
use crate::config::Config;
use crate::config::credentials::ConnectionKeys;
use crate::config::credentials::secret;
use crate::config::error::Error;
use crate::config::section::cdm::Cdm;

/// Returns the CDM database `cdm` describes.
fn settings(cdm: &Cdm) -> Result<CdmSettings, Error> {
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
    let keys = ConnectionKeys {
        section: "cdm",
        url_file: cdm.url_file.is_some(),
        tls_ca: cdm.tls_ca.as_deref(),
        tls_ca_file: cdm.tls_ca_file.as_deref(),
    };
    let ca = keys.ca()?;
    let pseudonymiser = cdm
        .pseudonymisation
        .as_ref()
        .map(pseudonymiser_of)
        .transpose()?;
    let session = omop_cdm::connection::session::Session {
        statement_timeout: cdm.statement_timeout.get(),
        lock_timeout: cdm.lock_timeout.get(),
        idle_in_transaction_session_timeout: cdm.idle_in_transaction_session_timeout.get(),
        statement_cache_capacity: cdm.statement_cache_capacity,
        max_lifetime: cdm.max_lifetime.get(),
        idle_timeout: cdm.idle_timeout.get(),
    };
    let connection = keys.connection(
        url.expose_secret(),
        ca.as_deref(),
        session,
        |key, source| Error::CdmConnection { key, source },
    )?;
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
    section: &crate::config::section::cdm::CdmPseudonymisation,
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

/// Returns the CDM database of `[cdm]`, when it is present.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<CdmSettings>, Error> {
    config.cdm.as_ref().map(settings).transpose()
}
