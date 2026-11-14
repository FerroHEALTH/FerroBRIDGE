// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The exchange of one EHR's access directives with the CDR.
//!
//! It reads the current `EHR_ACCESS` and the `ACCESS_DIRECTIVE_SETTINGS` it
//! carries and the controller's `ACCESS_POLICY` catalogue, commits imported
//! directives, and translates a foreign action code.
//!
//! The CDR commits and checks; the bridge reads a document into directives,
//! appends them to the ones in force and commits the whole settings as the
//! next `EHR_ACCESS` version over the one it read, so a person's
//! instructions keep their history as versions (openEHR BASE Architecture
//! Overview, Security and Confidentiality). Each imported directive gets a
//! `uid` when it carries none, the key its `Consent` id is derived from. No
//! specification governs the exchange: our own design.

use openehr_base::containers::NonEmptyVec;
use openehr_base::serde_support::OpenSubtype;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_rm::draft::access_control::access_control::access_directive::AccessDirective;
use openehr_rm::draft::access_control::access_control::access_directive_settings::AccessDirectiveSettings;
use openehr_rm::draft::access_control::access_control::access_policy::AccessPolicy;
use openehr_rm::draft::access_control::terminology::{
    AccessDefault, OperationClass, TERMINOLOGY_ID_ACCESS_CONTROL, Term,
};
use openehr_rm::v1_2::ehr::ehr_access::EhrAccess;
use openehr_term::codes::VersionLifecycleState;

use crate::cdr::CdrClient;
use crate::cdr::ehr_access::AccessError;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;
use crate::consent::fhir::model::{Code, Document};
use crate::consent::rm::coded;
use crate::facade::commit::Change;
use crate::facade::commit::Committer;

/// The namespace a directive's key is derived in when it carries no UUID
/// `uid`: the v5 UUID of the URL `https://ferrobridge.eu/fhir/Consent`.
const KEY_NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x95a5_6637_776a_53a1_b210_1ad2_3317_81ab);

/// What a deployment configures for the exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The root the CDR serves its extensions under, where the `EHR_ACCESS`
    /// reads and the controller's `ACCESS_POLICY` are.
    pub extensions_root: url::Url,
    /// The `default_access` the first `ACCESS_DIRECTIVE_SETTINGS` of an EHR
    /// whose `EHR_ACCESS` carries none is written with.
    pub default_access: AccessDefault,
}

/// Why an exchange reached no answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExchangeError {
    /// The CDR call did not answer usably.
    #[error("the CDR exchange of the access directives failed")]
    Access(#[from] AccessError),
    /// The `EHR_ACCESS` carries settings of another access control scheme.
    #[error("the EHR_ACCESS settings are no ACCESS_DIRECTIVE_SETTINGS")]
    Scheme(#[source] serde_json::Error),
    /// The terminology server failed to translate an action code.
    #[error("the terminology server did not translate an action code")]
    Terminology(#[source] ferrobridge_term::error::Error),
}

/// The current `EHR_ACCESS` of one EHR, read as directive settings.
#[derive(Debug, Clone)]
pub struct Current {
    /// The version the read answered.
    pub version: ObjectVersionId,
    /// The `EHR_ACCESS` itself.
    pub access: EhrAccess,
    /// The settings it carries, `None` when it carries none.
    pub settings: Option<AccessDirectiveSettings>,
}

impl Current {
    /// Returns the directives in force.
    #[must_use]
    pub fn directives(&self) -> &[AccessDirective] {
        self.settings
            .as_ref()
            .and_then(|settings| settings.directives.as_deref())
            .unwrap_or_default()
    }

    /// Returns the `default_access` of the settings, `fallback` when there
    /// are none or they name no access default.
    #[must_use]
    pub fn default_access(&self, fallback: AccessDefault) -> AccessDefault {
        self.settings
            .as_ref()
            .and_then(|settings| AccessDefault::read_coded(&settings.default_access).ok())
            .unwrap_or(fallback)
    }
}

/// Reads the current `EHR_ACCESS` of `ehr_id` under the extensions root of
/// `settings`.
///
/// # Errors
/// Returns [`ExchangeError::Access`] for a read the CDR refused and
/// [`ExchangeError::Scheme`] for settings of another scheme.
pub async fn current(
    client: &CdrClient,
    settings: &Settings,
    ehr_id: &EhrId,
) -> Result<Current, ExchangeError> {
    let read = client.ehr_access(&settings.extensions_root, ehr_id).await?;
    let settings = read
        .access
        .settings
        .as_ref()
        .map(AccessDirectiveSettings::try_from)
        .transpose()
        .map_err(ExchangeError::Scheme)?;
    Ok(Current {
        version: read.version,
        access: read.access,
        settings,
    })
}

/// Reads the controller's `ACCESS_POLICY` under `settings.extensions_root`,
/// `None` when the CDR holds no policy.
///
/// # Errors
/// Returns [`ExchangeError::Access`] for a read the CDR refused.
pub async fn policy(
    client: &CdrClient,
    settings: &Settings,
) -> Result<Option<AccessPolicy>, ExchangeError> {
    Ok(client.access_policy(&settings.extensions_root).await?)
}

/// Returns the catalogue of `policy`, empty when there is none.
#[must_use]
pub fn catalogue(policy: Option<&AccessPolicy>) -> &[AccessDirective] {
    policy
        .and_then(|policy| policy.catalogue.as_deref())
        .unwrap_or_default()
}

/// Commits `incoming` appended to the directives of `current` as the next
/// `EHR_ACCESS` version, and returns the contribution with the directives
/// as committed.
///
/// A directive whose `uid` is absent or names a directive already in force
/// gets a new one.
///
/// # Errors
/// Returns [`ExchangeError::Access`] for a commit the CDR refused, and
/// [`ExchangeError::Scheme`] when the settings do not encode.
pub async fn append(
    client: &CdrClient,
    ehr_id: &EhrId,
    current: &Current,
    incoming: Vec<AccessDirective>,
    settings: &Settings,
    committer: &Committer,
    system_id: &str,
) -> Result<(ContributionUid, Vec<AccessDirective>), ExchangeError> {
    let mut next = current
        .settings
        .clone()
        .unwrap_or_else(|| AccessDirectiveSettings {
            default_access: coded(
                TERMINOLOGY_ID_ACCESS_CONTROL,
                settings.default_access.code(),
                settings.default_access.code(),
            ),
            gate_keepers: None,
            directives: None,
            labels: None,
        });
    let mut directives = next.directives.take().map(Vec::from).unwrap_or_default();
    let mut keys: Vec<uuid::Uuid> = directives.iter().map(key).collect();
    let mut added = Vec::with_capacity(incoming.len());
    for mut directive in incoming {
        if directive.uid.is_none() || keys.contains(&key(&directive)) {
            directive.uid = HierObjectId::new(uuid::Uuid::new_v4().to_string())
                .ok()
                .map(UidBasedId::HierObjectId);
        }
        keys.push(key(&directive));
        added.push(directive.clone());
        directives.push(directive);
    }
    // NOTE: draft access control BMM, an optional list is absent or non-empty, so an
    // empty one is legitimately absent.
    next.directives = NonEmptyVec::new(directives).ok();
    let mut access = current.access.clone();
    access.settings = Some(OpenSubtype::try_from(&next).map_err(ExchangeError::Scheme)?);
    let contribution = client
        .commit_ehr_access(
            ehr_id,
            &current.version,
            &access,
            crate::facade::commit::lifecycle_state(VersionLifecycleState::Complete),
            crate::facade::commit::audit(Change::Modification, system_id, committer),
        )
        .await?;
    Ok((contribution, added))
}

/// Returns the key `directive` is addressed by: the UUID its `uid` names, or
/// the v5 UUID of its canonical JSON when its `uid` is no UUID.
///
/// No specification governs this: our own design. A directive another
/// client wrote with no `uid` keeps its key as long as it is unchanged.
#[must_use]
pub fn key(directive: &AccessDirective) -> uuid::Uuid {
    let named = match directive.uid.as_ref() {
        Some(UidBasedId::HierObjectId(id)) => uuid::Uuid::try_parse(id.value()).ok(),
        Some(UidBasedId::ObjectVersionId(_)) | None => None,
    };
    named.unwrap_or_else(|| {
        uuid::Uuid::new_v5(
            &KEY_NAMESPACE,
            openehr_its::json::to_canonical_json(directive).as_bytes(),
        )
    })
}

/// Translates each foreign action coding of `document` to an operation
/// class of the access control terminology through `client`.
///
/// The call is `ConceptMap/$translate` with `targetsystem` (FHIR R4
/// `conceptmap-operation-translate.html`).
/// A coding the server translates to nothing, or to a code that is no
/// operation class, is left out, and the import refuses it naming the
/// coding. With no server every foreign action stays untranslated.
///
/// # Errors
/// Returns [`ExchangeError::Terminology`] when the server fails or refuses,
/// which is never read as a missing translation.
pub async fn operations(
    client: Option<&ferrobridge_term::client::Client>,
    document: &Document,
) -> Result<Vec<(Code, OperationClass)>, ExchangeError> {
    let Some(client) = client else {
        return Ok(Vec::new());
    };
    let mut translated = Vec::new();
    for coding in crate::consent::fhir::import::foreign_actions(document) {
        let (Some(system), Some(code)) = (&coding.system, &coding.code) else {
            continue;
        };
        let request = ferrobridge_term::request::Translation::new(system, code)
            .with_target_system(TERMINOLOGY_ID_ACCESS_CONTROL);
        let outcome = client
            .translate(&request)
            .await
            .map_err(ExchangeError::Terminology)?;
        let class = outcome
            .accepted()
            .filter_map(|found| found.concept.as_ref())
            .filter(|concept| {
                concept
                    .system
                    .as_deref()
                    .is_none_or(|found| found == TERMINOLOGY_ID_ACCESS_CONTROL)
            })
            .find_map(|concept| {
                // NOTE: no specification governs this: our own design, a target that is no
                // operation class is no translation, which the import refuses naming the coding.
                OperationClass::read(TERMINOLOGY_ID_ACCESS_CONTROL, concept.code.as_deref()?).ok()
            });
        if let Some(class) = class {
            translated.push((coding.clone(), class));
        }
    }
    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::{KEY_NAMESPACE, key};
    use crate::consent::rm::directive;
    use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
    use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
    use openehr_rm::draft::access_control::terminology::{DirectiveKind, Effect};

    #[test]
    fn the_key_namespace_is_the_v5_uuid_of_the_consent_url() {
        let derived = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            b"https://ferrobridge.eu/fhir/Consent",
        );
        assert_eq!(derived, KEY_NAMESPACE);
    }

    #[test]
    fn a_uuid_uid_is_the_key_and_any_other_directive_keys_by_its_content() {
        let mut named = directive(DirectiveKind::Consent, Effect::Permit);
        let uuid = uuid::Uuid::new_v4();
        named.uid = HierObjectId::new(uuid.to_string())
            .ok()
            .map(UidBasedId::HierObjectId);
        assert_eq!(key(&named), uuid);
        let plain = directive(DirectiveKind::Consent, Effect::Permit);
        assert_eq!(key(&plain), key(&plain.clone()));
        let other = directive(DirectiveKind::Consent, Effect::Deny);
        assert_ne!(key(&plain), key(&other));
    }
}
