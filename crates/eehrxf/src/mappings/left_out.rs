// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements each shipped project leaves out ([`super::Project::left_out`]):
//! every element of an entry it maps, in FerroBRIDGE's template, that no
//! element of its profiles carries. No specification governs the lists: our
//! own design.

use super::LeftOut;

/// The archetype of the entry the `euvac` project maps, the CKM medication
/// action the template's immunisations section holds.
#[cfg(feature = "vaccination")]
pub(super) const VACCINATION_ENTRY: &str = "openEHR-EHR-ACTION.medication.v1";

/// The entry archetype of the HDR project's diagnostic summary.
#[cfg(feature = "discharge")]
pub(super) const HDR_DIAGNOSIS: &str = "openEHR-EHR-EVALUATION.problem_diagnosis.v1";

/// The entry archetype of the HDR project's significant procedures.
#[cfg(feature = "discharge")]
pub(super) const HDR_PROCEDURE: &str = "openEHR-EHR-ACTION.procedure.v1";

/// The entry archetype of the HDR project's discharge medications.
#[cfg(feature = "discharge")]
pub(super) const HDR_ORDER: &str = "openEHR-EHR-INSTRUCTION.medication_order.v3";

/// The entry archetype of the imaging project, the imaging examination
/// result its report is mapped from.
#[cfg(feature = "imaging")]
pub(super) const IMAGING_RESULT: &str = "openEHR-EHR-OBSERVATION.imaging_exam_result.v1";

/// The imaging project's left-out elements of the imaging examination result.
#[cfg(feature = "imaging")]
pub(super) const IMAGING_LEFT_OUT: &[LeftOut] = &[
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0055]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0071]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0061]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0014]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0008]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0056]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0063]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0057]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0058]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0059]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/data[at0003]/items[at0023]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/state[at0047]/items[at0048]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "data[at0001]/events[at0002]/state[at0047]/items[at0108]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0098]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0106]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[openEHR-EHR-CLUSTER.organisation.v1]/items[at0004]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[openEHR-EHR-CLUSTER.organisation.v1]/items[at0019]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0087]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0049]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0088]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0062]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0093]/items[at0094]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0093]/items[at0095]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0093]/items[at0096]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0093]/items[at0097]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0027]/items[at0031]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0027]/items[at0028]",
    },
    LeftOut {
        archetype: IMAGING_RESULT,
        path: "protocol[at0025]/items[at0027]/items[at0029]",
    },
];

/// The HDR project's left-out elements of its three entries.
#[cfg(feature = "discharge")]
pub(super) const HDR_LEFT_OUT: &[LeftOut] = &[
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "data[at0001]/items[at0079]",
    },
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "data[at0001]/items[at0009]",
    },
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "data[at0001]/items[at0078]",
    },
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "data[at0001]/items[at0080]",
    },
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "data[at0001]/items[at0072]",
    },
    LeftOut {
        archetype: HDR_DIAGNOSIS,
        path: "protocol[at0032]/items[at0070]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0049]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0070]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0072]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0065]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0058]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0071]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0069]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0066]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0073]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0060]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0061]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0074]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "description[at0001]/items[at0067]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "protocol[at0053]/items[at0054]",
    },
    LeftOut {
        archetype: HDR_PROCEDURE,
        path: "protocol[at0053]/items[at0056]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0092]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0094]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0047]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0173]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0174]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0064]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0178]/items[at0171]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0178]/items[at0162]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0051]/items[at0130]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0051]/items[at0053]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0150]/items[at0165]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0062]/items[at0150]/items[at0151]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0044]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0105]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0107]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0018]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0148]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0179]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0113]/items[at0013]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0113]/items[at0011]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0113]/items[at0016]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0113]/items[at0060]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0113]/items[at0050]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0106]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0132]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0154]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0139]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0155]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/description[at0002]/items[at0129]/items[at0161]",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "activities[at0001]/timing",
    },
    LeftOut {
        archetype: HDR_ORDER,
        path: "protocol[at0005]/items[at0004]",
    },
];

/// The `euvac` project's left-out elements of the medication action.
#[cfg(feature = "vaccination")]
pub(super) const EUVAC_LEFT_OUT: &[LeftOut] = &[
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0156]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0132]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0133]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0043]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0154]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0155]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0021]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0140]/items[at0141]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0140]/items[at0143]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0033]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "description[at0017]/items[at0149]",
    },
    LeftOut {
        archetype: VACCINATION_ENTRY,
        path: "protocol[at0030]/items[at0103]",
    },
];
