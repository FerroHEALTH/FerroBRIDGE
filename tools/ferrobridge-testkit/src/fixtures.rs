// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic fixtures the suites commit and map.
//!
//! Every byte here is invented for the tests: no clinical content, no real
//! identifier, no extract from any system. Identifier and code systems sit
//! under `http://example.org`, which RFC 6761 §6.5 reserves for documentation
//! and examples, except where a FHIR required binding fixes the system.
//!
//! Some fixtures name systems outside that domain. [`KDS_DIAGNOSE_OPT`],
//! [`KDS_PERSON_OPT`], [`KDS_LABORAUFTRAG_OPT`] and [`KDS_LABORBERICHT_OPT`]
//! are published operational templates vendored verbatim
//! with their provenance beside them: a template carries constraints, never
//! patient content.
//! [`KDS_DIAGNOSE_CONDITION`] is synthetic, and names the profile and the
//! systems and extension URLs the published KDS mappings match on, because a
//! mapping condition only admits the value it names.

/// A synthetic operational template: one `EVALUATION` with one `DV_TEXT`
/// element under an `ITEM_TREE`.
///
/// The template identifier is `ferrobridge.minimal_evaluation.v1`.
pub const MINIMAL_EVALUATION_OPT: &str = include_str!("../fixtures/opt/minimal_evaluation.opt");

/// The template identifier [`MINIMAL_EVALUATION_OPT`] declares.
pub const MINIMAL_EVALUATION_TEMPLATE_ID: &str = "ferrobridge.minimal_evaluation.v1";

/// A synthetic party operational template rooted at one `PERSON`.
///
/// The root is the synthetic archetype
/// `openEHR-DEMOGRAPHIC-PERSON.ferrobridge_person.v1`, with one
/// `PARTY_IDENTITY` holding a `DV_TEXT` name element and a `details` tree
/// holding a `DV_IDENTIFIER` element. Every `openEHR-DEMOGRAPHIC-*` archetype in the international CKM is a v0
/// draft, so the party tests run against this synthetic template. The
/// template identifier is `ferrobridge.person.v1`.
pub const PERSON_OPT: &str = include_str!("../fixtures/opt/person.opt");

/// The template identifier [`PERSON_OPT`] declares.
pub const PERSON_TEMPLATE_ID: &str = "ferrobridge.person.v1";

/// A synthetic party operational template rooted at one `ORGANISATION`.
///
/// The root is the synthetic archetype
/// `openEHR-DEMOGRAPHIC-ORGANISATION.ferrobridge_organisation.v1`, shaped as
/// [`PERSON_OPT`] is. The template identifier is
/// `ferrobridge.organisation.v1`.
pub const ORGANISATION_OPT: &str = include_str!("../fixtures/opt/organisation.opt");

/// The template identifier [`ORGANISATION_OPT`] declares.
pub const ORGANISATION_TEMPLATE_ID: &str = "ferrobridge.organisation.v1";

/// A synthetic party operational template rooted at one `ROLE`.
///
/// The root is the synthetic archetype
/// `openEHR-DEMOGRAPHIC-ROLE.ferrobridge_role.v1`, shaped as [`PERSON_OPT`]
/// is. The template identifier is `ferrobridge.role.v1`.
pub const ROLE_OPT: &str = include_str!("../fixtures/opt/role.opt");

/// The template identifier [`ROLE_OPT`] declares.
pub const ROLE_TEMPLATE_ID: &str = "ferrobridge.role.v1";

/// A synthetic operational template shaped like a diagnosis mapping chain.
///
/// One `COMPOSITION` holds one `EVALUATION` with the at-codes and the two
/// slotted `CLUSTER`s the diagnosis mappings of the FHIRconnect mapping
/// library write. The template identifier is `ferrobridge.diagnose.v1`.
pub const DIAGNOSE_OPT: &str = include_str!("../fixtures/opt/diagnose.opt");

/// The template identifier [`DIAGNOSE_OPT`] declares.
pub const DIAGNOSE_TEMPLATE_ID: &str = "ferrobridge.diagnose.v1";

/// A synthetic operational template shaped like a laboratory report.
///
/// One `COMPOSITION` holds any number of
/// `OBSERVATION.laboratory_test_result.v1`, each with one repeating `EVENT`
/// whose item tree slots any number of `CLUSTER.laboratory_test_analyte.v1`
/// (a coded name, a `DV_QUANTITY` result and a result time) and at most one
/// `CLUSTER.specimen.v1` (a coded type and a received time). The at-codes are
/// the ones the OMOCL laboratory mappings write. The template identifier is
/// `ferrobridge.laboratory_report.v1`.
pub const LABORATORY_REPORT_OPT: &str = include_str!("../fixtures/opt/laboratory_report.opt");

/// The template identifier [`LABORATORY_REPORT_OPT`] declares.
pub const LABORATORY_REPORT_TEMPLATE_ID: &str = "ferrobridge.laboratory_report.v1";

/// A synthetic FLAT composition of [`LABORATORY_REPORT_OPT`]: one result with
/// two analytes (the second without a result time) and one specimen.
pub const LABORATORY_REPORT_FLAT: &str =
    include_str!("../fixtures/composition/laboratory_report.flat.json");

/// The published `KDS_Diagnose` operational template, vendored verbatim.
///
/// One `COMPOSITION.report.v1` holding `EVALUATION.problem_diagnosis.v1` with
/// its slotted clusters, the template the FHIR round trip runs against.
/// `scripts/vendor/kds-diagnose-opt.sh` fetches it at the commit
/// `docs/VERSIONS.md` pins, and `PROVENANCE.md` beside it records its source
/// and licence.
pub const KDS_DIAGNOSE_OPT: &str = include_str!("../fixtures/opt/kds/KDS_Diagnose.opt");

/// The template identifier [`KDS_DIAGNOSE_OPT`] declares.
pub const KDS_DIAGNOSE_TEMPLATE_ID: &str = "KDS_Diagnose";

/// The published `KDS_Person` operational template, vendored verbatim.
///
/// One `COMPOSITION.person.v0` holding `ADMIN_ENTRY.person_data.v0` with its
/// slotted person, name, address and birth clusters, `EVALUATION.gender.v1`
/// and `ADMIN_ENTRY.versicherungsinformationen.v0`, the template the FHIR
/// `Patient` round trip runs against. `scripts/vendor/kds-person-opt.sh`
/// fetches it at the commit `docs/VERSIONS.md` pins, and `PROVENANCE.md`
/// beside it records its source and licence.
pub const KDS_PERSON_OPT: &str = include_str!("../fixtures/opt/kds-person/KDS_Person.opt");

/// The template identifier [`KDS_PERSON_OPT`] declares.
pub const KDS_PERSON_TEMPLATE_ID: &str = "KDS_Person";

/// The published `KDS_Prozedur` operational template, vendored verbatim.
///
/// One `COMPOSITION.report.v1` holding `ACTION.procedure.v1` with its
/// anatomical location and case identification clusters.
/// `scripts/vendor/kds-prozedur-opt.sh` fetches it at the commit
/// `docs/VERSIONS.md` pins, and `PROVENANCE.md` beside it records its source
/// and licence.
pub const KDS_PROZEDUR_OPT: &str = include_str!("../fixtures/opt/kds-prozedur/KDS_Prozedur.opt");

/// The template identifier [`KDS_PROZEDUR_OPT`] declares.
pub const KDS_PROZEDUR_TEMPLATE_ID: &str = "KDS_Prozedur";

/// The published `KDS_Laborauftrag` operational template, vendored verbatim.
///
/// One `COMPOSITION.request.v1` holding `INSTRUCTION.service_request.v1` with
/// its slotted specimen, case identification and requester organisation
/// clusters, the template the FHIR `ServiceRequest` and `Organization` round
/// trip runs against. `scripts/vendor/kds-laborauftrag-opt.sh` fetches it at
/// the commit `docs/VERSIONS.md` pins, and `PROVENANCE.md` beside it records
/// its source and licence.
pub const KDS_LABORAUFTRAG_OPT: &str =
    include_str!("../fixtures/opt/kds-laborauftrag/KDS_Laborauftrag.opt");

/// The template identifier [`KDS_LABORAUFTRAG_OPT`] declares.
pub const KDS_LABORAUFTRAG_TEMPLATE_ID: &str = "KDS_Laborauftrag";

/// The published `KDS_Laborbericht` operational template.
///
/// One `COMPOSITION.report-result.v1` holding
/// `OBSERVATION.laboratory_test_result.v1` with its analyte and specimen
/// clusters, the template the HL7 Europe Laboratory 2.0.0 round trip runs
/// against. `scripts/vendor/kds-laborbericht-opt.sh` fetches it at the commit
/// `docs/VERSIONS.md` pins, and `PROVENANCE.md` beside it records its source
/// and licence.
pub const KDS_LABORBERICHT_OPT: &str =
    include_str!("../fixtures/opt/kds-laborbericht/KDS_Laborbericht.opt");

/// A synthetic FLAT composition of [`KDS_PERSON_OPT`]: one person with a
/// name, the birth date `1984-03-17` and the administrative gender
/// `SYN-GENDER-F` of the synthetic `FB-GENDER` code system.
pub const KDS_PERSON_FLAT: &str = include_str!("../fixtures/composition/kds_person.flat.json");

/// A synthetic canonical COMPOSITION built against
/// [`MINIMAL_EVALUATION_OPT`].
pub const MINIMAL_EVALUATION_COMPOSITION: &str =
    include_str!("../fixtures/composition/minimal_evaluation.json");

/// The `DV_TEXT` value the one element of [`MINIMAL_EVALUATION_COMPOSITION`]
/// carries.
pub const MINIMAL_EVALUATION_NOTE: &str =
    "A synthetic note committed by the FerroBRIDGE end-to-end test";

/// A synthetic FHIR R4 `CodeSystem` with two concepts.
pub const TERM_SOURCE_CODE_SYSTEM: &str =
    include_str!("../fixtures/terminology/codesystem-source.json");

/// A synthetic FHIR R4 `CodeSystem` the concept map translates into.
pub const TERM_TARGET_CODE_SYSTEM: &str =
    include_str!("../fixtures/terminology/codesystem-target.json");

/// A synthetic FHIR R4 `ValueSet` holding one of the two source concepts.
pub const TERM_VALUE_SET: &str = include_str!("../fixtures/terminology/valueset-members.json");

/// A synthetic FHIR R4 `ConceptMap` from the source system to the target one.
pub const TERM_CONCEPT_MAP: &str =
    include_str!("../fixtures/terminology/conceptmap-source-to-target.json");

/// The canonical URL of [`TERM_SOURCE_CODE_SYSTEM`].
pub const TERM_SOURCE_SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrobridge-source";

/// The canonical URL of [`TERM_TARGET_CODE_SYSTEM`].
pub const TERM_TARGET_SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrobridge-target";

/// The canonical URL of [`TERM_VALUE_SET`].
pub const TERM_VALUE_SET_URL: &str = "http://example.org/fhir/ValueSet/ferrobridge-members";

/// The canonical URL of [`TERM_CONCEPT_MAP`].
pub const TERM_CONCEPT_MAP_URL: &str =
    "http://example.org/fhir/ConceptMap/ferrobridge-source-to-target";

/// The source concept that is a value-set member and maps `equivalent`.
pub const TERM_MEMBER_CODE: &str = "alpha";

/// The display of [`TERM_MEMBER_CODE`].
pub const TERM_MEMBER_DISPLAY: &str = "Alpha finding";

/// The target code [`TERM_MEMBER_CODE`] maps to.
pub const TERM_MEMBER_TARGET_CODE: &str = "A1";

/// The source concept that is not a value-set member and maps `wider`.
pub const TERM_NON_MEMBER_CODE: &str = "beta";

/// A code no fixture defines, for the not-found cases.
pub const TERM_UNKNOWN_CODE: &str = "gamma";

/// A synthetic FHIR R4 `Condition`.
pub const R4_CONDITION: &str = include_str!("../fixtures/fhir/r4/condition.json");

/// A synthetic FHIR R4 `Condition` shaped like the MII Diagnose profile.
///
/// It claims the profile the `KDS_Diagnose` mappings compile against, codes
/// its diagnosis in the ICD-10-GM system with the Diagnosesicherheit
/// extension, dates its onset with a period whose start carries the
/// lebensphase extension, and carries the asserted-date extension, the two
/// status elements, a body site and a note. Every code and every other system
/// is invented under `http://example.org`.
pub const KDS_DIAGNOSE_CONDITION: &str =
    include_str!("../fixtures/fhir/r4/kds_diagnose_condition.json");

/// A synthetic FHIR R4 `Patient` shaped like the MII Person profile.
///
/// It claims the profile the `KDS_Person` mappings compile against, carries
/// one identifier in an invented system and one in the statutory insurance
/// system the published mappings match on, an official and a maiden name, a
/// gender, a birth date, a deceased flag and a managing organization by
/// display. Every name and number is invented.
pub const KDS_PERSON_PATIENT: &str = include_str!("../fixtures/fhir/r4/kds_person_patient.json");

/// A synthetic FHIR R4 `ServiceRequest` shaped like the MII laboratory
/// request profile, whose requester is a contained `Organization`.
///
/// It claims the profile the `KDS_Laborauftrag` mappings compile against and
/// the `laboratory` category and `completed` status their conditions match
/// on; the order code, the identifiers and the organisation's name are
/// invented under `http://example.org`.
pub const KDS_LABORAUFTRAG_SERVICE_REQUEST: &str =
    include_str!("../fixtures/fhir/r4/kds_laborauftrag_service_request.json");

/// A synthetic FHIR R4 `Observation`.
pub const R4_OBSERVATION: &str = include_str!("../fixtures/fhir/r4/observation.json");

#[cfg(test)]
mod tests {
    use super::{
        MINIMAL_EVALUATION_COMPOSITION, MINIMAL_EVALUATION_NOTE, MINIMAL_EVALUATION_OPT,
        MINIMAL_EVALUATION_TEMPLATE_ID, R4_CONDITION, R4_OBSERVATION,
    };

    /// The code systems the R4 required bindings of the two `Condition`
    /// status elements admit.
    ///
    /// `Condition.clinicalStatus` and `Condition.verificationStatus` bind at
    /// strength `required` to `ValueSet/condition-clinical` and
    /// `ValueSet/condition-ver-status` (`StructureDefinition-Condition`, R4),
    /// and each value set includes exactly the system below.
    const REQUIRED_BINDING_SYSTEMS: [&str; 2] = [
        "http://terminology.hl7.org/CodeSystem/condition-clinical",
        "http://terminology.hl7.org/CodeSystem/condition-ver-status",
    ];

    #[test]
    fn the_kds_condition_names_only_invented_codes_outside_the_matched_systems() {
        use super::KDS_DIAGNOSE_CONDITION;
        let matched = [
            "www.medizininformatik-initiative.de/fhir/core/modul-diagnose/StructureDefinition/Diagnose",
            "hl7.org/fhir/StructureDefinition/condition-assertedDate",
            "fhir.de/StructureDefinition/icd-10-gm-diagnosesicherheit",
            "fhir.de/CodeSystem/bfarm/icd-10-gm",
            "fhir.de/StructureDefinition/lebensphase",
        ];
        let bound: Vec<&str> = REQUIRED_BINDING_SYSTEMS
            .iter()
            .map(|system| system.trim_start_matches("http://"))
            .collect();
        for scheme in ["http://", "https://"] {
            for url in KDS_DIAGNOSE_CONDITION.split(scheme).skip(1) {
                assert!(
                    url.starts_with("example.org/")
                        || matched.iter().any(|system| url.starts_with(system))
                        || bound.iter().any(|system| url.starts_with(system)),
                    "the KDS Condition names a URL outside the matched systems: {url}"
                );
            }
        }
    }

    #[test]
    fn the_laboratory_template_declares_its_identifier_and_the_mapped_archetypes() {
        use super::{LABORATORY_REPORT_OPT, LABORATORY_REPORT_TEMPLATE_ID};
        let declared = format!("<template_id>\n    <value>{LABORATORY_REPORT_TEMPLATE_ID}</value>");
        assert!(LABORATORY_REPORT_OPT.contains(&declared));
        for archetype in [
            "openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
            "openEHR-EHR-CLUSTER.laboratory_test_analyte.v1",
            "openEHR-EHR-CLUSTER.specimen.v1",
        ] {
            assert!(
                LABORATORY_REPORT_OPT.contains(archetype),
                "the laboratory template does not slot {archetype}"
            );
        }
    }

    #[test]
    fn the_kds_template_declares_its_identifier() {
        use super::{KDS_DIAGNOSE_OPT, KDS_DIAGNOSE_TEMPLATE_ID};
        let declared = format!("<template_id>\n    <value>{KDS_DIAGNOSE_TEMPLATE_ID}</value>");
        assert!(
            KDS_DIAGNOSE_OPT.contains(&declared),
            "the vendored KDS template does not declare {KDS_DIAGNOSE_TEMPLATE_ID}"
        );
    }

    #[test]
    fn the_kds_person_template_declares_its_identifier() {
        use super::{KDS_PERSON_OPT, KDS_PERSON_TEMPLATE_ID};
        let declared = format!("<template_id>\n        <value>{KDS_PERSON_TEMPLATE_ID}</value>");
        assert!(
            KDS_PERSON_OPT.contains(&declared),
            "the vendored KDS person template does not declare {KDS_PERSON_TEMPLATE_ID}"
        );
    }

    #[test]
    fn the_composition_names_the_template_and_carries_the_note() {
        assert!(MINIMAL_EVALUATION_OPT.contains(MINIMAL_EVALUATION_TEMPLATE_ID));
        assert!(MINIMAL_EVALUATION_COMPOSITION.contains(MINIMAL_EVALUATION_TEMPLATE_ID));
        assert!(MINIMAL_EVALUATION_COMPOSITION.contains(MINIMAL_EVALUATION_NOTE));
    }

    #[test]
    fn the_terminology_fixtures_name_their_canonicals_and_codes() {
        use super::{
            TERM_CONCEPT_MAP, TERM_CONCEPT_MAP_URL, TERM_MEMBER_CODE, TERM_MEMBER_DISPLAY,
            TERM_MEMBER_TARGET_CODE, TERM_NON_MEMBER_CODE, TERM_SOURCE_CODE_SYSTEM,
            TERM_SOURCE_SYSTEM, TERM_TARGET_CODE_SYSTEM, TERM_TARGET_SYSTEM, TERM_UNKNOWN_CODE,
            TERM_VALUE_SET, TERM_VALUE_SET_URL,
        };
        assert!(TERM_SOURCE_CODE_SYSTEM.contains(TERM_SOURCE_SYSTEM));
        assert!(TERM_SOURCE_CODE_SYSTEM.contains(TERM_MEMBER_DISPLAY));
        assert!(TERM_TARGET_CODE_SYSTEM.contains(TERM_TARGET_SYSTEM));
        assert!(TERM_TARGET_CODE_SYSTEM.contains(TERM_MEMBER_TARGET_CODE));
        assert!(TERM_VALUE_SET.contains(TERM_VALUE_SET_URL));
        assert!(TERM_CONCEPT_MAP.contains(TERM_CONCEPT_MAP_URL));
        assert!(
            TERM_VALUE_SET.contains(TERM_MEMBER_CODE),
            "the member is not in the value set"
        );
        assert!(
            !TERM_VALUE_SET.contains(TERM_NON_MEMBER_CODE),
            "the non-member is in the value set"
        );
        for fixture in [
            TERM_SOURCE_CODE_SYSTEM,
            TERM_TARGET_CODE_SYSTEM,
            TERM_VALUE_SET,
            TERM_CONCEPT_MAP,
        ] {
            assert!(
                !fixture.contains(TERM_UNKNOWN_CODE),
                "a fixture defines the code the not-found cases rely on being absent"
            );
            for url in fixture.split("http://").skip(1) {
                assert!(
                    url.starts_with("example.org/"),
                    "a terminology fixture names a URL outside http://example.org"
                );
            }
        }
    }

    #[test]
    fn the_condition_names_the_required_binding_systems() {
        for system in REQUIRED_BINDING_SYSTEMS {
            assert!(
                R4_CONDITION.contains(system),
                "the synthetic Condition does not name {system}"
            );
        }
    }

    #[test]
    fn every_other_fhir_url_is_under_the_example_domain() {
        let bound: Vec<&str> = REQUIRED_BINDING_SYSTEMS
            .iter()
            .map(|system| system.trim_start_matches("http://"))
            .collect();
        for (name, resource) in [("Condition", R4_CONDITION), ("Observation", R4_OBSERVATION)] {
            for url in resource.split("http://").skip(1) {
                assert!(
                    url.starts_with("example.org/")
                        || bound.iter().any(|system| url.starts_with(system)),
                    "the synthetic {name} names a URL that is neither under \
                     http://example.org nor a system a required binding fixes"
                );
            }
        }
    }
}
