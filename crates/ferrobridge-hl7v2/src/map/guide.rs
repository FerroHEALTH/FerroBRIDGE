// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ConceptMaps` of the v2-to-FHIR guide, `hl7.fhir.uv.v2mappings` 1.0.0,
//! compiled in behind the `shipped-maps` feature.
//!
//! Every `ConceptMap-*.json` of the package vendored under the crate's
//! `vendor/hl7.fhir.uv.v2mappings/package/` is embedded whole, so a consumer
//! maps with no package on disk; the package declares CC0-1.0 (its
//! `PROVENANCE.md`). [`Corpus::shipped`] loads them, and
//! [`Corpus::load`] still reads a newer package from a directory.
//!
//! [`Corpus::shipped`]: crate::map::corpus::Corpus::shipped
//! [`Corpus::load`]: crate::map::corpus::Corpus::load

/// The directory the files are read from, relative to the crate root, which
/// names them in a load error.
pub const DIRECTORY: &str = "vendor/hl7.fhir.uv.v2mappings/package";

/// One entry of [`GUIDE`]: the file name and the file's text.
macro_rules! embedded {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/vendor/hl7.fhir.uv.v2mappings/package/",
                $name
            )),
        )
    };
}

/// Every `ConceptMap` of the guide, as its file name and its text, in file
/// name order.
pub const GUIDE: [(&str, &str); 263] = [
    embedded!("ConceptMap-datatype-ce-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-cf-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-cne-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-cnn-to-practitioner.json"),
    embedded!("ConceptMap-datatype-cq-to-code.json"),
    embedded!("ConceptMap-datatype-cq-to-decimal.json"),
    embedded!("ConceptMap-datatype-cq-to-quantity.json"),
    embedded!("ConceptMap-datatype-cq-to-unsignedint.json"),
    embedded!("ConceptMap-datatype-cwe-durationunit-to-timing.json"),
    embedded!("ConceptMap-datatype-cwe-supportinginfo-to-observation.json"),
    embedded!("ConceptMap-datatype-cwe-to-annotation.json"),
    embedded!("ConceptMap-datatype-cwe-to-code.json"),
    embedded!("ConceptMap-datatype-cwe-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-cwe-to-coding.json"),
    embedded!("ConceptMap-datatype-cwe-to-device.json"),
    embedded!("ConceptMap-datatype-cwe-to-duration.json"),
    embedded!("ConceptMap-datatype-cwe-to-identifier.json"),
    embedded!("ConceptMap-datatype-cwe-to-organization.json"),
    embedded!("ConceptMap-datatype-cwe-to-practitionerrole.json"),
    embedded!("ConceptMap-datatype-cwe-to-quantity.json"),
    embedded!("ConceptMap-datatype-cwe-to-string.json"),
    embedded!("ConceptMap-datatype-cwe-to-uri.json"),
    embedded!("ConceptMap-datatype-cx-mother-to-relatedperson.json"),
    embedded!("ConceptMap-datatype-cx-to-identifier.json"),
    embedded!("ConceptMap-datatype-cx-to-string.json"),
    embedded!("ConceptMap-datatype-dld-discharge-to-location.json"),
    embedded!("ConceptMap-datatype-dln-to-identifier.json"),
    embedded!("ConceptMap-datatype-dr-to-datetime.json"),
    embedded!("ConceptMap-datatype-dr-to-period.json"),
    embedded!("ConceptMap-datatype-dtm-time-to-annotation.json"),
    embedded!("ConceptMap-datatype-dtm-to-datetime.json"),
    embedded!("ConceptMap-datatype-ed-to-attachment.json"),
    embedded!("ConceptMap-datatype-ed-to-documentreference.json"),
    embedded!("ConceptMap-datatype-ei-defaultassigner-to-identifier.json"),
    embedded!("ConceptMap-datatype-ei-extension-to-identifier.json"),
    embedded!("ConceptMap-datatype-ei-organization-to-identifier.json"),
    embedded!("ConceptMap-datatype-ei-system-to-identifier.json"),
    embedded!("ConceptMap-datatype-ei-to-coding.json"),
    embedded!("ConceptMap-datatype-ei-to-condition.json"),
    embedded!("ConceptMap-datatype-ei-to-procedure.json"),
    embedded!("ConceptMap-datatype-ei-udicarrier-to-device.json"),
    embedded!("ConceptMap-datatype-eip-fillerassignedidentifier-to-identifier.json"),
    embedded!("ConceptMap-datatype-eip-placerassignedidentifier-to-identifier.json"),
    embedded!("ConceptMap-datatype-fn-to-humanname.json"),
    embedded!("ConceptMap-datatype-ft-comment-to-annotation.json"),
    embedded!("ConceptMap-datatype-hd-endpoint-to-messageheader-destination.json"),
    embedded!("ConceptMap-datatype-hd-endpoint-to-messageheader-source.json"),
    embedded!("ConceptMap-datatype-hd-name-to-messageheader-destination.json"),
    embedded!("ConceptMap-datatype-hd-name-to-messageheader-source.json"),
    embedded!("ConceptMap-datatype-hd-to-device.json"),
    embedded!("ConceptMap-datatype-hd-to-identifier.json"),
    embedded!("ConceptMap-datatype-hd-to-location.json"),
    embedded!("ConceptMap-datatype-hd-to-organization.json"),
    embedded!("ConceptMap-datatype-hd-to-uri.json"),
    embedded!("ConceptMap-datatype-id-to-boolean.json"),
    embedded!("ConceptMap-datatype-id-to-code.json"),
    embedded!("ConceptMap-datatype-id-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-id-to-coding.json"),
    embedded!("ConceptMap-datatype-id-to-string.json"),
    embedded!("ConceptMap-datatype-id-universalid-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-is-to-code.json"),
    embedded!("ConceptMap-datatype-is-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-is-to-string.json"),
    embedded!("ConceptMap-datatype-msg-to-code.json"),
    embedded!("ConceptMap-datatype-msg-to-coding.json"),
    embedded!("ConceptMap-datatype-msg-to-messageheader.json"),
    embedded!("ConceptMap-datatype-na-to-sampleddata.json"),
    embedded!("ConceptMap-datatype-ndl-to-practitionerrole.json"),
    embedded!("ConceptMap-datatype-nm-lengthofstay-to-quantity.json"),
    embedded!("ConceptMap-datatype-nm-to-positiveint.json"),
    embedded!("ConceptMap-datatype-nm-to-quantity.json"),
    embedded!("ConceptMap-datatype-nr-to-range.json"),
    embedded!("ConceptMap-datatype-og-subidentifier-to-extension.json"),
    embedded!("ConceptMap-datatype-pl-to-location.json"),
    embedded!("ConceptMap-datatype-pln-to-identifier.json"),
    embedded!("ConceptMap-datatype-pt-to-meta.json"),
    embedded!("ConceptMap-datatype-ri-to-timing.json"),
    embedded!("ConceptMap-datatype-rp-to-attachment.json"),
    embedded!("ConceptMap-datatype-rp-to-documentreference.json"),
    embedded!("ConceptMap-datatype-rpt-to-timing.json"),
    embedded!("ConceptMap-datatype-sad-to-address.json"),
    embedded!("ConceptMap-datatype-sn-to-quantity.json"),
    embedded!("ConceptMap-datatype-sn-to-range.json"),
    embedded!("ConceptMap-datatype-sn-to-ratio.json"),
    embedded!("ConceptMap-datatype-sps-source-to-specimen.json"),
    embedded!("ConceptMap-datatype-st-to-codeableconcept.json"),
    embedded!("ConceptMap-datatype-st-to-identifier.json"),
    embedded!("ConceptMap-datatype-tq-executionperiod-to-task.json"),
    embedded!("ConceptMap-datatype-tq-priority-to-task.json"),
    embedded!("ConceptMap-datatype-tq-to-appointment.json"),
    embedded!("ConceptMap-datatype-tq-to-medicationrequest.json"),
    embedded!("ConceptMap-datatype-tq-to-servicerequest.json"),
    embedded!("ConceptMap-datatype-ts-to-datetime.json"),
    embedded!("ConceptMap-datatype-xad-to-address.json"),
    embedded!("ConceptMap-datatype-xcn-author-to-annotation.json"),
    embedded!("ConceptMap-datatype-xcn-to-patient.json"),
    embedded!("ConceptMap-datatype-xcn-to-practitioner.json"),
    embedded!("ConceptMap-datatype-xcn-to-practitionerrole.json"),
    embedded!("ConceptMap-datatype-xcn-to-relatedperson.json"),
    embedded!("ConceptMap-datatype-xon-to-location.json"),
    embedded!("ConceptMap-datatype-xon-to-organization.json"),
    embedded!("ConceptMap-datatype-xon-to-string.json"),
    embedded!("ConceptMap-datatype-xpn-to-humanname.json"),
    embedded!("ConceptMap-datatype-xpn-to-string.json"),
    embedded!("ConceptMap-datatype-xtn-to-contactpoint.json"),
    embedded!("ConceptMap-message-adt-a01-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a02-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a05-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a06-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a09-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a11-to-bundle.json"),
    embedded!("ConceptMap-message-adt-a17-to-bundle.json"),
    embedded!("ConceptMap-message-mdm-t02-to-bundle.json"),
    embedded!("ConceptMap-message-oml-o21-to-bundle.json"),
    embedded!("ConceptMap-message-orm-o01-to-bundle.json"),
    embedded!("ConceptMap-message-oru-r01-to-bundle.json"),
    embedded!("ConceptMap-message-siu-s12-to-bundle.json"),
    embedded!("ConceptMap-message-vxu-v04-to-bundle.json"),
    embedded!("ConceptMap-segment-aig-to-appointment.json"),
    embedded!("ConceptMap-segment-ail-to-appointment.json"),
    embedded!("ConceptMap-segment-aip-to-appointment.json"),
    embedded!("ConceptMap-segment-ais-to-appointment.json"),
    embedded!("ConceptMap-segment-ais-to-servicerequest.json"),
    embedded!("ConceptMap-segment-al1-to-allergyintolerance.json"),
    embedded!("ConceptMap-segment-dg1-to-condition.json"),
    embedded!("ConceptMap-segment-dg1-to-encounter.json"),
    embedded!("ConceptMap-segment-dg1-to-episodeofcare.json"),
    embedded!("ConceptMap-segment-evn-to-provenance.json"),
    embedded!("ConceptMap-segment-iam-to-allergyintolerance.json"),
    embedded!("ConceptMap-segment-in1-to-coverage.json"),
    embedded!("ConceptMap-segment-in3-to-careteam.json"),
    embedded!("ConceptMap-segment-mrg-to-account.json"),
    embedded!("ConceptMap-segment-msa-to-messageheader.json"),
    embedded!("ConceptMap-segment-msh-operator-to-provenance.json"),
    embedded!("ConceptMap-segment-msh-source-to-provenance.json"),
    embedded!("ConceptMap-segment-msh-to-bundle.json"),
    embedded!("ConceptMap-segment-msh-to-encounter.json"),
    embedded!("ConceptMap-segment-msh-to-messageheader.json"),
    embedded!("ConceptMap-segment-msh-transformation-to-provenance.json"),
    embedded!("ConceptMap-segment-nk1-to-patient.json"),
    embedded!("ConceptMap-segment-nk1-to-relatedperson.json"),
    embedded!("ConceptMap-segment-nte-comment-to-appointment.json"),
    embedded!("ConceptMap-segment-nte-to-documentreference.json"),
    embedded!("ConceptMap-segment-nte-to-observation.json"),
    embedded!("ConceptMap-segment-nte-to-servicerequest.json"),
    embedded!("ConceptMap-segment-obr-to-diagnosticreport.json"),
    embedded!("ConceptMap-segment-obr-to-servicerequest.json"),
    embedded!("ConceptMap-segment-obr-to-specimen.json"),
    embedded!("ConceptMap-segment-obx-component-to-observation.json"),
    embedded!("ConceptMap-segment-obx-to-documentreference.json"),
    embedded!("ConceptMap-segment-obx-to-observation.json"),
    embedded!("ConceptMap-segment-orc-to-diagnosticreport.json"),
    embedded!("ConceptMap-segment-orc-to-immunization.json"),
    embedded!("ConceptMap-segment-orc-to-medicationadministration.json"),
    embedded!("ConceptMap-segment-orc-to-provenance.json"),
    embedded!("ConceptMap-segment-orc-to-servicerequest.json"),
    embedded!("ConceptMap-segment-pd1-livingwill-to-observation.json"),
    embedded!("ConceptMap-segment-pd1-to-patient.json"),
    embedded!("ConceptMap-segment-pid-patient-to-provenance.json"),
    embedded!("ConceptMap-segment-pid-to-account.json"),
    embedded!("ConceptMap-segment-pid-to-appointment.json"),
    embedded!("ConceptMap-segment-pid-to-patient.json"),
    embedded!("ConceptMap-segment-pr1-to-procedure.json"),
    embedded!("ConceptMap-segment-prt-generalpractitioner-practitioner-to-patient.json"),
    embedded!("ConceptMap-segment-prt-generalpractitioner-practitionerrole-to-patient.json"),
    embedded!("ConceptMap-segment-prt-location-to-observation.json"),
    embedded!("ConceptMap-segment-prt-to-careteam.json"),
    embedded!("ConceptMap-segment-prt-to-device.json"),
    embedded!("ConceptMap-segment-prt-to-practitionerrole.json"),
    embedded!("ConceptMap-segment-prt-to-relatedperson.json"),
    embedded!("ConceptMap-segment-pv1-encounterhistory-to-basic.json"),
    embedded!("ConceptMap-segment-pv1-to-coverage.json"),
    embedded!("ConceptMap-segment-pv1-to-encounter.json"),
    embedded!("ConceptMap-segment-pv1-to-patient.json"),
    embedded!("ConceptMap-segment-pv2-to-encounter.json"),
    embedded!("ConceptMap-segment-rol-generalpractitioner-to-patient.json"),
    embedded!("ConceptMap-segment-rol-practitionerrole-to-encounter.json"),
    embedded!("ConceptMap-segment-rol-to-careteam.json"),
    embedded!("ConceptMap-segment-rol-to-relatedperson.json"),
    embedded!("ConceptMap-segment-rxa-to-immunization.json"),
    embedded!("ConceptMap-segment-rxo-to-medicationrequest.json"),
    embedded!("ConceptMap-segment-rxr-to-immunization.json"),
    embedded!("ConceptMap-segment-rxr-to-medicationrequest.json"),
    embedded!("ConceptMap-segment-sch-to-appointment.json"),
    embedded!("ConceptMap-segment-sch-to-provenance.json"),
    embedded!("ConceptMap-segment-sch-to-servicerequest.json"),
    embedded!("ConceptMap-segment-sft-to-device.json"),
    embedded!("ConceptMap-segment-sft-to-messageheader.json"),
    embedded!("ConceptMap-segment-spm-to-specimen.json"),
    embedded!("ConceptMap-segment-tq1-to-medicationrequest.json"),
    embedded!("ConceptMap-segment-tq1-to-servicerequest.json"),
    embedded!("ConceptMap-segment-txa-to-documentreference.json"),
    embedded!("ConceptMap-segment-txa-to-provenance.json"),
    embedded!("ConceptMap-table-hl70001-to-administrative-gender.json"),
    embedded!("ConceptMap-table-hl70002-to-v3-maritalstatus.json"),
    embedded!("ConceptMap-table-hl70003-to-encounter-status.json"),
    embedded!("ConceptMap-table-hl70003-to-v2-0003.json"),
    embedded!("ConceptMap-table-hl70004-to-encounter-status.json"),
    embedded!("ConceptMap-table-hl70004-to-v3-actcode.json"),
    embedded!("ConceptMap-table-hl70006-to-v3-religiousaffiliation.json"),
    embedded!("ConceptMap-table-hl70007-to-v2-0007.json"),
    embedded!("ConceptMap-table-hl70052-to-diagnosis-role.json"),
    embedded!("ConceptMap-table-hl70062-to-v3-actreason.json"),
    embedded!("ConceptMap-table-hl70063-to-v3-rolecode.json"),
    embedded!("ConceptMap-table-hl70069-to-service-type.json"),
    embedded!("ConceptMap-table-hl70074-to-v2-0074.json"),
    embedded!("ConceptMap-table-hl70078-to-v3-observationinterpretation.json"),
    embedded!("ConceptMap-table-hl70080-to-v2-0080.json"),
    embedded!("ConceptMap-table-hl70085-to-observation-status.json"),
    embedded!("ConceptMap-table-hl70092-to-v2-0092.json"),
    embedded!("ConceptMap-table-hl70116-to-v2-0116.json"),
    embedded!("ConceptMap-table-hl70119-to-request-status.json"),
    embedded!("ConceptMap-table-hl70123-queries-to-diagnostic-report-status.json"),
    embedded!("ConceptMap-table-hl70127-to-allergy-intolerance-category.json"),
    embedded!("ConceptMap-table-hl70127-to-allergy-intolerance-type.json"),
    embedded!("ConceptMap-table-hl70127-to-v2-0127.json"),
    embedded!("ConceptMap-table-hl70128-original-to-v2-0128.json"),
    embedded!("ConceptMap-table-hl70128-to-allergy-intolerance-criticality.json"),
    embedded!("ConceptMap-table-hl70128-to-reaction-event-severity.json"),
    embedded!("ConceptMap-table-hl70130-to-v2-0130.json"),
    embedded!("ConceptMap-table-hl70131-to-v2-0131.json"),
    embedded!("ConceptMap-table-hl70136-to-special-values.json"),
    embedded!("ConceptMap-table-hl70136-to-specimen-status.json"),
    embedded!("ConceptMap-table-hl70161-to-v2-0161.json"),
    embedded!("ConceptMap-table-hl70162-to-v2-0162.json"),
    embedded!("ConceptMap-table-hl70163-to-v2-0163.json"),
    embedded!("ConceptMap-table-hl70165-to-sct.json"),
    embedded!("ConceptMap-table-hl70177-to-v3-confidentiality.json"),
    embedded!("ConceptMap-table-hl70185-to-v2-0185.json"),
    embedded!("ConceptMap-table-hl70190-to-address-type.json"),
    embedded!("ConceptMap-table-hl70190-to-address-use.json"),
    embedded!("ConceptMap-table-hl70200-to-name-use.json"),
    embedded!("ConceptMap-table-hl70201-to-contact-point-use.json"),
    embedded!("ConceptMap-table-hl70202-to-contact-point-system.json"),
    embedded!("ConceptMap-table-hl70203-to-v2-0203.json"),
    embedded!("ConceptMap-table-hl70204-to-v2-0204.json"),
    embedded!("ConceptMap-table-hl70215-to-v2-0215.json"),
    embedded!("ConceptMap-table-hl70217-to-v3-actpriority.json"),
    embedded!("ConceptMap-table-hl70272-to-v2-0272.json"),
    embedded!("ConceptMap-table-hl70276-to-sct.json"),
    embedded!("ConceptMap-table-hl70277-to-v2-0277.json"),
    embedded!("ConceptMap-table-hl70278-to-appointmentstatus.json"),
    embedded!("ConceptMap-table-hl70301-to-v2-0301.json"),
    embedded!("ConceptMap-table-hl70315-to-v2-0315.json"),
    embedded!("ConceptMap-table-hl70322-to-event-status.json"),
    embedded!("ConceptMap-table-hl70335-to-v2-0335.json"),
    embedded!("ConceptMap-table-hl70360-to-v2-0360.json"),
    embedded!("ConceptMap-table-hl70364-to-v2-0364.json"),
    embedded!("ConceptMap-table-hl70371-to-v2-0371.json"),
    embedded!("ConceptMap-table-hl70399-to-iso-3166-1.json"),
    embedded!("ConceptMap-table-hl70430-to-v2-0430.json"),
    embedded!("ConceptMap-table-hl70432-to-v2-0432.json"),
    embedded!("ConceptMap-table-hl70443-to-v3-participationtype.json"),
    embedded!("ConceptMap-table-hl70444-to-v2-0444.json"),
    embedded!("ConceptMap-table-hl70482-to-v3-rolecode.json"),
    embedded!("ConceptMap-table-hl70485-to-request-priority.json"),
    embedded!("ConceptMap-table-hl70487-to-v2-0487.json"),
    embedded!("ConceptMap-table-hl70488-to-v2-0488.json"),
    embedded!("ConceptMap-table-hl70490-to-v2-0490.json"),
    embedded!("ConceptMap-table-hl70493-to-v2-0493.json"),
    embedded!("ConceptMap-table-hl70528-to-v3-timingevent.json"),
    embedded!("ConceptMap-table-hl70550-to-v2-0550.json"),
    embedded!("ConceptMap-table-hl70912-to-practitioner-role.json"),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::{DIRECTORY, GUIDE};
    use crate::map::corpus::Corpus;

    #[test]
    fn every_concept_map_of_the_vendored_package_is_embedded_in_name_order() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY);
        let on_disk: Vec<String> = std::fs::read_dir(&directory)
            .expect("the vendored package reads")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| {
                name.starts_with("ConceptMap-")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|extension| extension == "json")
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let embedded: Vec<String> = GUIDE.iter().map(|(name, _)| (*name).to_owned()).collect();
        assert_eq!(embedded, on_disk);
        for (name, text) in GUIDE {
            let file = std::fs::read_to_string(directory.join(name)).expect("the file reads");
            assert_eq!(text, file, "{name} is embedded byte for byte");
        }
    }

    #[test]
    fn the_shipped_corpus_is_the_corpus_the_vendored_package_loads() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY);
        let loaded = Corpus::load(&directory).expect("the vendored package loads");
        let shipped = Corpus::shipped().expect("the embedded maps load");
        assert_eq!(shipped.maps().count(), GUIDE.len());
        assert!(shipped.maps().eq(loaded.maps()));
    }
}
