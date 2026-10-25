// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The null flavour an `ELEMENT` holds in place of its value.
//!
//! `null_flavour` and `null_reason` are optional attributes of `ELEMENT`
//! (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_structures.html#_element_class>).

use openehr_its::json::JsonParseError;
use openehr_its::json::from_canonical_value;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;
use serde_json::Value;

use crate::engine::lens::null_flavour::NullFlavour;
use crate::engine::rm::RmError;

/// The `ELEMENT` attribute that holds its null flavour.
pub const NULL_FLAVOUR: &str = "null_flavour";

/// The `ELEMENT` attribute that holds why its value is absent.
pub const NULL_REASON: &str = "null_reason";

/// Reads the null flavour and the reason an `ELEMENT`'s canonical JSON
/// holds, `None` when it holds no null flavour.
///
/// Both are optional attributes of `ELEMENT`
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_structures.html#_element_class>).
///
/// # Errors
///
/// Returns [`RmError::Decode`] when either attribute does not read as its
/// class.
pub fn null_flavour_of(element: &Value, node: &str) -> Result<Option<NullFlavour>, RmError> {
    let refuse = |rm_type: &str| {
        let node = String::from(node);
        let rm_type = String::from(rm_type);
        move |source: JsonParseError| RmError::Decode {
            node,
            rm_type,
            source,
        }
    };
    let Some(flavour) = element.get(NULL_FLAVOUR) else {
        return Ok(None);
    };
    let flavour = from_canonical_value::<DvCodedText>(flavour).map_err(refuse("DV_CODED_TEXT"))?;
    let reason = element
        .get(NULL_REASON)
        .map(|reason| from_canonical_value::<DvText>(reason).map_err(refuse("DV_TEXT")))
        .transpose()?;
    Ok(Some(NullFlavour { flavour, reason }))
}
