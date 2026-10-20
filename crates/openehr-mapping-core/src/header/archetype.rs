// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `spec.openEhrConfig.archetype` value.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::num::ParseIntError;
use core::str::FromStr;

use openehr_base::v1_2::base_types::identification::archetype_id;
use openehr_base::v1_2::base_types::identification::lexical::IdError;

/// Why a `spec.openEhrConfig.archetype` value was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArchetypeIdError {
    /// The value does not match the BASE 1.2.0 `archetype_id` production.
    #[error("`{value}` is not `rm_originator-rm_name-rm_entity.concept.vN`")]
    Lexical {
        /// The refused value.
        value: String,
        /// The refusal `openehr-base` reports.
        #[source]
        source: IdError,
    },
    /// The version number does not fit the range this crate counts in.
    #[error("the version of `{value}` is out of range")]
    VersionRange {
        /// The refused value.
        value: String,
        /// Why the version number did not parse.
        #[source]
        source: ParseIntError,
    },
}

/// An openEHR archetype id, as a mapping header names it.
///
/// The FHIRconnect header gives `openEhrConfig.archetype` as the archetype the
/// model mapping maps, in the form `openEHR-EHR-ACTION.informed_consent.v0`
/// (FHIRconnect v1.0.0, Header §Spec), and the model-mapping schema types it
/// as a plain string. The value is parsed by `openehr-base` against the
/// `archetype_id` production of openEHR BASE Release 1.2.0 (§5.4.10
/// `ARCHETYPE_ID` Class and the Base Types syntax appendix,
/// <https://specifications.openehr.org/releases/BASE/Release-1.2.0/base_types.html>).
// NOTE: FHIRconnect v1.0.0 never names a namespaced ADL 2 id in a header, so no
// specification governs this: our own design keeps the BASE 1.2.0 form.
#[derive(Debug, Clone)]
pub struct ArchetypeId {
    id: archetype_id::ArchetypeId,
    version: u32,
}

impl ArchetypeId {
    /// Returns the archetype id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.id.value
    }

    /// Returns the number of the `version_id` axis.
    #[must_use]
    pub const fn version_number(&self) -> u32 {
        self.version
    }

    /// Returns the `openehr-base` archetype id this value was parsed into.
    #[must_use]
    pub const fn base(&self) -> &archetype_id::ArchetypeId {
        &self.id
    }
}

impl PartialEq for ArchetypeId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ArchetypeId {}

impl PartialOrd for ArchetypeId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ArchetypeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for ArchetypeId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Display for ArchetypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArchetypeId {
    type Err = ArchetypeIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = archetype_id::ArchetypeId::from_str(value).map_err(|source| {
            ArchetypeIdError::Lexical {
                value: value.to_owned(),
                source,
            }
        })?;
        let version =
            id.major_version()
                .parse()
                .map_err(|source| ArchetypeIdError::VersionRange {
                    value: value.to_owned(),
                    source,
                })?;
        Ok(Self { id, version })
    }
}
