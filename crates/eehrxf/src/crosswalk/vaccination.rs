// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vaccination crosswalk (feature `vaccination`).
//!
//! The rows key on the Xt-EHR `EHDSImmunisation` 1.0.0 element paths and
//! name the elements of the MyHealth@EU Vaccination
//! `vaccination-myhealtheu-euvac` profile, 10.0.0-preview, whose logical
//! model `MyHealthEuVaccination` specialises MyHealth@EU Core's
//! `MyHealthEuImmunisation`. No eHealth Network guideline element is named:
//! the guideline datasets this crate reads carry no vaccination history.
//! The rows name elements of the one resource profile, so no row names a
//! context: the shipped project's sub-context writes that profile whole.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

/// The canonical URL of the MyHealth@EU Vaccination `Immunization` profile,
/// the one an immunisation entry claims.
pub const EUVAC_IMMUNIZATION: &str =
    "http://fhir.ehdsi.eu/euvac/StructureDefinition/vaccination-myhealtheu-euvac";

/// The vaccination crosswalk.
pub static VACCINATION: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImmunisation",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImmunisationObligations",
    },
    profile: EUVAC_IMMUNIZATION,
    companions: &[],
    sliced: "Immunization.performer",
    ehn: &[],
    rows: ROWS,
    gaps: &[],
};

/// The row of `path`, its producer obligation and the
/// `vaccination-myhealtheu-euvac` element that carries it, with no context.
const fn row(path: &'static str, producer: Option<Populate>, eps: &'static str) -> Row {
    Row {
        path,
        ehn: &[],
        producer,
        eps,
        contexts: &[],
    }
}

/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// The rows, in the model's order.
const ROWS: &[Row] = &[
    row(
        "EHDSImmunisation.header.subject",
        SHALL,
        "Immunization.patient",
    ),
    row(
        "EHDSImmunisation.header.identifier",
        SHOULD,
        "Immunization.identifier",
    ),
    row(
        "EHDSImmunisation.header.date",
        None,
        "Immunization.recorded",
    ),
    row(
        "EHDSImmunisation.header.status",
        SHALL,
        "Immunization.status",
    ),
    row(
        "EHDSImmunisation.header.source",
        None,
        "Immunization.extension:informationSource",
    ),
    row(
        "EHDSImmunisation.header.language",
        None,
        "Immunization.language",
    ),
    row(
        "EHDSImmunisation.diseaseOrAgentTargeted",
        None,
        "Immunization.protocolApplied.targetDisease",
    ),
    row(
        "EHDSImmunisation.vaccine",
        SHALL,
        "Immunization.vaccineCode",
    ),
    row(
        "EHDSImmunisation.administeredProduct",
        SHOULD,
        "Immunization.extension:administeredProduct",
    ),
    row(
        "EHDSImmunisation.administrationTime",
        SHALL,
        "Immunization.occurrence[x]",
    ),
    row(
        "EHDSImmunisation.administeringCentre",
        None,
        "Immunization.performer:administeringCentreOrHp",
    ),
    row(
        "EHDSImmunisation.administrator",
        None,
        "Immunization.performer:administeringCentreOrHp",
    ),
    row("EHDSImmunisation.note", None, "Immunization.note"),
];
