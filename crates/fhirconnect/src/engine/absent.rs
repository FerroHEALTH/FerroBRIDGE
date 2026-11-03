// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An element a mapping writes absent: a `manual` entry that writes, out of
//! openEHR, the FHIR R4 data-absent-reason extension on an element the
//! content holds no value for.
//!
//! A mapping writes one for an element a profile requires and the template
//! cannot carry, so the resource conforms and says the value is missing
//! (<https://hl7.org/fhir/R4/extension-data-absent-reason.html>). The run
//! declares each as [`Loss::Absent`](crate::engine::outcome::Loss::Absent),
//! so the gap stays as visible as every other loss. A data-absent reason read
//! from a `NULL_FLAVOUR` carries recorded content and is no loss. No
//! specification governs declaring it: our own design.

/// The canonical URL of the FHIR R4 data-absent-reason extension.
pub const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// Returns the FHIRPath of the element whose extension the located `url`
/// path `path` belongs to: `Observation.performer[0]` for
/// `Observation.performer[0].extension[0].url`.
pub(crate) fn absent_element(path: &str) -> &str {
    path.strip_suffix(".url")
        .and_then(|extension| {
            extension
                .rfind(".extension")
                .map(|end| extension.get(..end))
        })
        .flatten()
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::absent_element;

    #[test]
    fn the_absent_element_is_the_one_that_holds_the_extension() {
        assert_eq!(
            absent_element("Observation.performer[0].extension[0].url"),
            "Observation.performer[0]"
        );
        assert_eq!(
            absent_element("Observation.performer.extension.url"),
            "Observation.performer"
        );
    }

    #[test]
    fn a_path_that_names_no_extension_url_is_kept() {
        assert_eq!(absent_element("Observation.status"), "Observation.status");
    }
}
