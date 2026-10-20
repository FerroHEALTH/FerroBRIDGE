// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ELEMENT.null_flavour` against the code of the FHIR `data-absent-reason`
//! extension.
//!
//! The cell is FHIRconnect `types-of-mappings/data-type/NULL_FLAVOUR.adoc`
//! §Data absent: the openEHR null flavour is mapped to the R4
//! `data-absent-reason` value set, and `null_reason` carries the FHIR code
//! "along with the `null_flavour`", read back when it matches a code of the
//! value set.
//!
//! The page's table is no function. Its `openEHR` column names the concept
//! and its `openEHR code` column the code, and in two rows they disagree with
//! the openEHR terminology (group `null_flavours`: 271 "no information", 253
//! "unknown", 272 "masked", 273 "not applicable"): `unknown` is paired with
//! "unknown" and 271, and the last row pairs `unknown` with "no information"
//! and 253. The cell reads the concept column as the intent and takes the code
//! from the terminology, so those rows are `unknown` against 253 and `unknown`
//! against 271, both the default going out to FHIR. Every other row names
//! "unknown" with 253, which agrees with the terminology. Going into openEHR,
//! `unknown` is 253, `masked` 272, `not-applicable` 273 and every other code
//! 253 with the FHIR code kept in `null_reason`; 271 is never written, and a
//! held value that reads back as the view is kept as it is. A FHIR code
//! outside the value set and an openEHR code outside the group are refused,
//! and a `null_reason` that names no code is text FHIR has no field for, which
//! [`uncarried_reason`] names so the engine declares it.

use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name this cell carries in a diagnostic.
pub const CELL: &str = "NULL_FLAVOUR against data-absent-reason";

/// The terminology every null flavour is coded in.
pub const OPENEHR: &str = "openehr";

/// The extension that carries a FHIR data-absent reason
/// (<https://hl7.org/fhir/R4/extension-data-absent-reason.html>).
pub const EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// The null flavours of the openEHR terminology, with their rubrics and the
/// FHIR code each reads back as when no `null_reason` names one.
///
/// The codes and rubrics are the openEHR terminology's group `null_flavours`;
/// the FHIR codes are `NULL_FLAVOUR.adoc`'s rows marked "Default for
/// openEHR->FHIR", with the two rows read as the module docs say.
pub const FLAVOURS: &[(&str, &str, &str)] = &[
    ("253", "unknown", "unknown"),
    ("271", "no information", "unknown"),
    ("272", "masked", "masked"),
    ("273", "not applicable", "not-applicable"),
];

/// The R4 `data-absent-reason` codes with the null flavour each is written
/// as, in the order of `NULL_FLAVOUR.adoc`'s table.
///
/// The codes are the whole R4 value set
/// (<https://hl7.org/fhir/R4/valueset-data-absent-reason.html>).
pub const BRIDGE: &[(&str, &str)] = &[
    ("unknown", "253"),
    ("asked-unknown", "253"),
    ("temp-unknown", "253"),
    ("not-asked", "253"),
    ("asked-declined", "253"),
    ("masked", "272"),
    ("not-applicable", "273"),
    ("unsupported", "253"),
    ("as-text", "253"),
    ("error", "253"),
    ("not-a-number", "253"),
    ("negative-infinity", "253"),
    ("positive-infinity", "253"),
    ("not-performed", "253"),
    ("not-permitted", "253"),
];

/// An `ELEMENT`'s null flavour with the reason beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct NullFlavour {
    /// `ELEMENT.null_flavour`.
    pub flavour: DvCodedText,
    /// `ELEMENT.null_reason`.
    pub reason: Option<DvText>,
}

/// `ELEMENT.null_flavour` and `null_reason` against the extension's `code`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NullFlavourLens;

impl Lens<NullFlavour, fhir_types::r4::primitives::String> for NullFlavourLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &NullFlavour) -> Result<fhir_types::r4::primitives::String, LensError> {
        let code = &source.flavour.defining_code;
        let default = (code.terminology_id.value == OPENEHR)
            .then(|| flavour(&code.code_string))
            .flatten()
            .map(|&(_, _, fhir)| fhir)
            .ok_or_else(|| LensError::Shape {
                cell: CELL,
                attribute: "ELEMENT.null_flavour",
                found: format!("{}::{}", code.terminology_id.value, code.code_string),
                reason: "the openEHR terminology's null flavours are 253, 271, 272 and 273",
                direction: Direction::OpenehrToFhir,
            })?;
        let named = reason_text(source.reason.as_ref()).filter(|&text| bridged(text).is_some());
        Ok(text(named.unwrap_or(default)))
    }

    fn put(
        &self,
        view: &fhir_types::r4::primitives::String,
        existing: Option<&NullFlavour>,
    ) -> Result<NullFlavour, LensError> {
        let code = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: CELL,
            attribute: "ELEMENT.null_flavour",
            direction: Direction::FhirToOpenehr,
        })?;
        let flavour_code = bridged(code).ok_or_else(|| LensError::Shape {
            cell: CELL,
            attribute: "ELEMENT.null_flavour",
            found: String::from(code),
            reason: "the code is outside the R4 data-absent-reason value set",
            direction: Direction::FhirToOpenehr,
        })?;
        if let Some(held) = existing
            && self.get(held).is_ok_and(|read| read == *view)
        {
            return Ok(held.clone());
        }
        let Some(&(_, rubric, default)) = flavour(flavour_code) else {
            return Err(LensError::Shape {
                cell: CELL,
                attribute: "ELEMENT.null_flavour",
                found: String::from(flavour_code),
                reason: "the bridge names a null flavour the openEHR terminology lacks",
                direction: Direction::FhirToOpenehr,
            });
        };
        Ok(NullFlavour {
            flavour: coded(flavour_code, rubric),
            reason: (code != default).then(|| reason(code)),
        })
    }

    fn fallbacks(&self, _view: &fhir_types::r4::primitives::String) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the `null_reason` text going out to FHIR leaves behind: a reason
/// that names no `data-absent-reason` code, which the extension has no field
/// for.
#[must_use]
pub fn uncarried_reason(source: &NullFlavour) -> Option<&str> {
    reason_text(source.reason.as_ref()).filter(|&text| bridged(text).is_none())
}

/// Returns the row of the null flavour `code`.
fn flavour(code: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    FLAVOURS.iter().find(|&&(flavour, _, _)| flavour == code)
}

/// Returns the null flavour a FHIR code is written as, `None` outside the
/// value set.
fn bridged(code: &str) -> Option<&'static str> {
    BRIDGE
        .iter()
        .find(|&&(fhir, _)| fhir == code)
        .map(|&(_, flavour)| flavour)
}

/// Returns the text of a `null_reason`.
fn reason_text(reason: Option<&DvText>) -> Option<&str> {
    match reason? {
        DvText::DvCodedText(coded) => Some(coded.value.as_str()),
        DvText::DvText(plain) => Some(plain.value.as_str()),
    }
}

/// Returns the FHIR `code` primitive holding `code`.
fn text(code: &str) -> fhir_types::r4::primitives::String {
    fhir_types::r4::primitives::String {
        value: Some(String::from(code)),
        ..fhir_types::r4::primitives::String::default()
    }
}

/// Returns the null flavour `code` with its rubric.
fn coded(code: &str, rubric: &str) -> DvCodedText {
    DvCodedText {
        value: String::from(rubric),
        hyperlink: None,
        formatting: None,
        mappings: None,
        language: None,
        encoding: None,
        defining_code: CodePhrase {
            terminology_id: TerminologyId {
                value: String::from(OPENEHR),
            },
            code_string: String::from(code),
            preferred_term: None,
        },
    }
}

/// Returns the `null_reason` that keeps a FHIR code.
fn reason(code: &str) -> DvText {
    DvText::DvText(DvTextData {
        value: String::from(code),
        hyperlink: None,
        formatting: None,
        mappings: None,
        language: None,
        encoding: None,
    })
}

#[cfg(test)]
mod tests {
    use super::BRIDGE;
    use super::FLAVOURS;
    use super::NullFlavour;
    use super::NullFlavourLens;
    use super::coded;
    use super::reason;
    use super::text;
    use super::uncarried_reason;
    use crate::engine::lens::Lens;
    use proptest::prop_oneof;
    use proptest::proptest;
    use proptest::sample::select;
    use proptest::strategy::Just;
    use proptest::strategy::Strategy;

    /// Returns the null flavour `code` with no reason.
    fn flavour(code: &str) -> NullFlavour {
        let rubric = FLAVOURS
            .iter()
            .find(|&&(flavour, _, _)| flavour == code)
            .map_or("", |&(_, rubric, _)| rubric);
        NullFlavour {
            flavour: coded(code, rubric),
            reason: None,
        }
    }

    /// Generates a null flavour of the group, with no reason, a reason that
    /// names a FHIR code, or a free-text reason.
    fn held() -> impl Strategy<Value = NullFlavour> {
        let codes: Vec<&str> = FLAVOURS.iter().map(|&(code, _, _)| code).collect();
        let fhir: Vec<&str> = BRIDGE.iter().map(|&(code, _)| code).collect();
        let reasons = prop_oneof![
            Just(None),
            select(fhir).prop_map(|code| Some(reason(code))),
            "[a-z ]{1,12}".prop_map(|text| Some(reason(&text))),
        ];
        (select(codes), reasons).prop_map(|(code, reason)| NullFlavour {
            reason,
            ..flavour(code)
        })
    }

    proptest! {
        #[test]
        fn get_put(source in held()) {
            let view = NullFlavourLens.get(&source).expect("a null flavour of the group travels");
            let back = NullFlavourLens.put(&view, Some(&source)).expect("the view is a code of the set");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(code in select(BRIDGE.iter().map(|&(code, _)| code).collect::<Vec<&str>>())) {
            let view = text(code);
            let source = NullFlavourLens.put(&view, None).expect("the code is in the set");
            let back = NullFlavourLens.get(&source).expect("the written flavour is of the group");
            assert_eq!(back, view, "PutGet failed for {code}");
        }
    }

    #[test]
    fn every_bridge_entry_writes_its_flavour() {
        // NULL_FLAVOUR.adoc §Data absent, one row per R4 code.
        for &(code, flavour_code) in BRIDGE {
            let written = NullFlavourLens
                .put(&text(code), None)
                .expect("every row writes a flavour");
            assert_eq!(
                written.flavour.defining_code.code_string, flavour_code,
                "{code} is written as {flavour_code}"
            );
            assert_eq!(
                written.flavour.defining_code.terminology_id.value,
                "openehr"
            );
            let read = NullFlavourLens.get(&written).expect("the row reads back");
            assert_eq!(read, text(code), "{code} reads back as itself");
        }
    }

    #[test]
    fn the_three_defaults_write_no_reason() {
        for code in ["unknown", "masked", "not-applicable"] {
            let written = NullFlavourLens
                .put(&text(code), None)
                .expect("a default code");
            assert_eq!(written.reason, None, "{code} needs no reason");
        }
    }

    #[test]
    fn a_code_without_a_flavour_of_its_own_keeps_it_in_the_reason() {
        // NULL_FLAVOUR.adoc: "The `null_reason` is to mapped from code along
        // with the `null_flavour`."
        let written = NullFlavourLens
            .put(&text("asked-declined"), None)
            .expect("the code is in the set");
        assert_eq!(written.flavour.defining_code.code_string, "253");
        assert_eq!(written.reason, Some(reason("asked-declined")));
    }

    #[test]
    fn no_information_reads_back_as_unknown() {
        // openEHR terminology null_flavours: 271 is "no information".
        let read = NullFlavourLens
            .get(&flavour("271"))
            .expect("271 is of the group");
        assert_eq!(read, text("unknown"));
    }

    #[test]
    fn a_code_outside_the_value_set_is_refused() {
        let error = NullFlavourLens
            .put(&text("not-a-reason"), None)
            .expect_err("the bridge is closed");
        assert!(error.to_string().contains("not-a-reason"), "{error}");
    }

    #[test]
    fn a_flavour_outside_the_group_is_refused() {
        let error = NullFlavourLens
            .get(&flavour("999"))
            .expect_err("999 is no null flavour");
        assert!(error.to_string().contains("999"), "{error}");
    }

    #[test]
    fn a_flavour_of_another_terminology_is_refused() {
        let mut source = flavour("253");
        source.flavour.defining_code.terminology_id.value = String::from("local");
        assert!(NullFlavourLens.get(&source).is_err());
    }

    #[test]
    fn a_code_with_no_value_is_refused() {
        assert!(
            NullFlavourLens
                .put(&fhir_types::r4::primitives::String::default(), None)
                .is_err()
        );
    }

    #[test]
    fn a_free_text_reason_is_named_as_uncarried() {
        let source = NullFlavour {
            reason: Some(reason("the patient was asleep")),
            ..flavour("253")
        };
        assert_eq!(uncarried_reason(&source), Some("the patient was asleep"));
        let coded_reason = NullFlavour {
            reason: Some(reason("asked-declined")),
            ..flavour("253")
        };
        assert_eq!(uncarried_reason(&coded_reason), None);
        assert_eq!(
            NullFlavourLens
                .get(&coded_reason)
                .expect("253 is of the group"),
            text("asked-declined"),
            "a reason that names a code is read back"
        );
    }
}
