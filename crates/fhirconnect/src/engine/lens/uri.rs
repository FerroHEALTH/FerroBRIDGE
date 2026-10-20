// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_URI` against FHIR `url`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_URI.adoc`:
//! the `url` is the `value`. Both sides hold the reference as text, so it
//! moves byte for byte. A `DV_EHR_URI` held where a `DV_URI` is written keeps
//! its class going back, since the reference model admits it as a subtype
//! (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_ehr_uri_class>).

use fhir_types::r4::primitives::Url;
use openehr_rm::v1_2::data_types::uri::dv_ehr_uri::DvEhrUri;
use openehr_rm::v1_2::data_types::uri::dv_uri::DvUri;
use openehr_rm::v1_2::data_types::uri::dv_uri::DvUriData;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `url` cell carries in a diagnostic.
pub const CELL: &str = "DV_URI against url";

/// `DV_URI` against `url`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UriLens;

impl Lens<DvUri, Url> for UriLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvUri) -> Result<Url, LensError> {
        let value = value_of(source);
        if value.is_empty() {
            return Err(LensError::Missing {
                cell: CELL,
                attribute: "url",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(Url::from(value))
    }

    fn put(&self, view: &Url, existing: Option<&DvUri>) -> Result<DvUri, LensError> {
        let value = String::from(text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: CELL,
            attribute: "DV_URI.value",
            direction: Direction::FhirToOpenehr,
        })?);
        Ok(match existing {
            Some(DvUri::DvEhrUri(_)) => DvUri::DvEhrUri(DvEhrUri { value }),
            Some(DvUri::DvUri(_)) | None => DvUri::DvUri(DvUriData { value }),
        })
    }

    fn fallbacks(&self, _view: &Url) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the reference a `DV_URI` of either class holds.
fn value_of(uri: &DvUri) -> &str {
    match *uri {
        DvUri::DvEhrUri(ref ehr) => &ehr.value,
        DvUri::DvUri(ref plain) => &plain.value,
    }
}

#[cfg(test)]
mod tests {
    use super::UriLens;
    use crate::engine::lens::Lens;
    use fhir_types::r4::primitives::Url;
    use openehr_rm::v1_2::data_types::uri::dv_ehr_uri::DvEhrUri;
    use openehr_rm::v1_2::data_types::uri::dv_uri::DvUri;
    use openehr_rm::v1_2::data_types::uri::dv_uri::DvUriData;
    use proptest::proptest;

    proptest! {
        #[test]
        fn get_put(path in "[a-z0-9/]{1,24}", ehr in proptest::bool::ANY) {
            let source = if ehr {
                DvUri::DvEhrUri(DvEhrUri { value: format!("ehr:/{path}") })
            } else {
                DvUri::DvUri(DvUriData { value: format!("https://example.org/{path}") })
            };
            let view = UriLens.get(&source).expect("the reference travels");
            let back = UriLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(path in "[a-z0-9/]{1,24}") {
            let view = Url::from(format!("https://example.org/{path}").as_str());
            let back = UriLens.put(&view, None).expect("the view is complete");
            let again = UriLens.get(&back).expect("the reference travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn an_empty_reference_is_refused_both_ways() {
        assert!(
            UriLens
                .get(&DvUri::DvUri(DvUriData {
                    value: String::new()
                }))
                .is_err()
        );
        assert!(UriLens.put(&Url::default(), None).is_err());
    }
}
