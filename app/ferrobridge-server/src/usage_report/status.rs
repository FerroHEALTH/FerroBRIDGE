// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroBRIDGE's `status` object in the report, as `FerroPULSE`'s report v2
//! contract defines it (`docs/report-v2.md` §`status`).
//!
//! Every type here is closed: a member that is not declared cannot be
//! serialized, so a report names no mapping, no URL and no host. No
//! specification governs the report: our own design.

use serde::Serialize;

/// The programs one mapping lane compiled, bucketed, so a report never states
/// the size of a deployment's mapping set exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MappingBucket {
    /// None.
    #[serde(rename = "0")]
    None,
    /// 1 to 10.
    #[serde(rename = "1-10")]
    UpTo10,
    /// 11 to 50.
    #[serde(rename = "11-50")]
    UpTo50,
    /// 51 to 200.
    #[serde(rename = "51-200")]
    UpTo200,
    /// More than 200.
    #[serde(rename = "201+")]
    Above200,
}

impl MappingBucket {
    /// Returns the bucket holding `programs`.
    #[must_use]
    pub const fn of(programs: usize) -> Self {
        match programs {
            0 => Self::None,
            1..=10 => Self::UpTo10,
            11..=50 => Self::UpTo50,
            51..=200 => Self::UpTo200,
            _ => Self::Above200,
        }
    }
}

/// A lane of this deployment, the only way a report names what runs.
///
/// The declaration order is the order of the serialized names, so a sorted
/// list serializes sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaneToken {
    /// The OMOP ETL, which runs under `ferrobridge etl run`.
    Etl,
    /// The FHIR R4 facade.
    Facade,
    /// The HL7 v2 face.
    Hl7v2,
    /// The two FHIRconnect operations.
    Operations,
    /// The FHIR terminology client.
    Terminology,
}

impl LaneToken {
    /// Returns the token of the lane the boot summary calls `name`, or `None`
    /// for a name the report does not carry.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        match name {
            "etl" => Some(Self::Etl),
            "facade" => Some(Self::Facade),
            "hl7v2" => Some(Self::Hl7v2),
            "operations" => Some(Self::Operations),
            "terminology" => Some(Self::Terminology),
            _ => None,
        }
    }
}

/// FerroBRIDGE's `status` object, carried by both events.
///
/// An absent member is a lane or an upstream this deployment does not
/// configure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The lanes the configuration switches on, unique and sorted.
    pub lanes: Vec<LaneToken>,
    /// Whether the FHIR routes check a bearer token (`[auth]`).
    pub auth: bool,
    /// The programs the facade compiled, bucketed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facade_mappings: Option<MappingBucket>,
    /// The programs the operations lane compiled, bucketed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operations_mappings: Option<MappingBucket>,
    /// Whether the openEHR CDR answered its readiness check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdr_ok: Option<bool>,
    /// Whether the terminology server answered its readiness check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminology_ok: Option<bool>,
    /// Whether the CDM database answered its readiness check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdm_ok: Option<bool>,
    /// Whether the facade's identity store answered its readiness check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_store_ok: Option<bool>,
    /// Whether the HL7 v2 listener accepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hl7v2_listener_ok: Option<bool>,
}
