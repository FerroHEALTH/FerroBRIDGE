// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value a target already holds, read as the class a cell writes.
//!
//! Each reader returns the held value when the target holds one of its class,
//! so a cell carries over the attributes its table gives no FHIR counterpart.

use openehr_rm::v1_2::data_types::encapsulated::dv_multimedia::DvMultimedia;
use openehr_rm::v1_2::data_types::encapsulated::dv_parsable::DvParsable;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_time::DvTime;
use openehr_rm::v1_2::data_types::quantity::dv_count::DvCount;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;
use openehr_rm::v1_2::data_types::uri::dv_uri::DvUri;

use crate::engine::rm::RmValue;

/// Returns the held `DV_TEXT`, when the target holds one.
pub(super) fn held_text(existing: Option<&RmValue>) -> Option<&DvTextData> {
    match existing {
        Some(RmValue::Text(text)) => Some(text),
        _other => None,
    }
}

/// Returns the held `DV_CODED_TEXT`, when the target holds one.
pub(super) fn held_coded(existing: Option<&RmValue>) -> Option<&DvCodedText> {
    match existing {
        Some(RmValue::CodedText(text)) => Some(text.as_ref()),
        _other => None,
    }
}

/// Returns the held `CODE_PHRASE`, when the target holds one.
pub(super) fn held_code_phrase(existing: Option<&RmValue>) -> Option<&CodePhrase> {
    match existing {
        Some(RmValue::CodePhrase(code)) => Some(code),
        _other => None,
    }
}

/// Returns the held `DV_DATE`, when the target holds one.
pub(super) fn held_date(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::date_time::dv_date::DvDate> {
    match existing {
        Some(RmValue::Date(date)) => Some(date.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_BOOLEAN`, when the target holds one.
pub(super) fn held_boolean(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::basic::dv_boolean::DvBoolean> {
    match existing {
        Some(RmValue::Boolean(boolean)) => Some(boolean),
        _other => None,
    }
}

/// Returns the held `DV_DATE_TIME`, when the target holds one.
pub(super) fn held_date_time(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime> {
    match existing {
        Some(RmValue::DateTime(date)) => Some(date.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_INTERVAL<DV_DATE_TIME>`, when the target holds one.
pub(super) fn held_interval(
    existing: Option<&RmValue>,
) -> Option<&DvInterval<openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime>>
{
    match existing {
        Some(RmValue::DateTimeInterval(interval)) => Some(interval.as_ref()),
        _other => None,
    }
}

/// Returns the held `PARTY_IDENTIFIED`, when the target holds one.
pub(super) fn held_party(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData> {
    match existing {
        Some(RmValue::Party(party)) => Some(party),
        _other => None,
    }
}

/// Returns the held `DV_IDENTIFIER`, when the target holds one.
pub(super) fn held_identifier(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier> {
    match existing {
        Some(RmValue::Identifier(identifier)) => Some(identifier),
        _other => None,
    }
}

/// Returns the held `DV_QUANTITY`, when the target holds one.
pub(super) fn held_quantity(existing: Option<&RmValue>) -> Option<&DvQuantity> {
    match existing {
        Some(RmValue::Quantity(quantity)) => Some(quantity.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_INTERVAL<DV_QUANTITY>`, when the target holds one.
pub(super) fn held_quantity_interval(
    existing: Option<&RmValue>,
) -> Option<&DvInterval<DvQuantity>> {
    match existing {
        Some(RmValue::QuantityInterval(interval)) => Some(interval.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_URI`, when the target holds one.
pub(super) fn held_uri(existing: Option<&RmValue>) -> Option<&DvUri> {
    match existing {
        Some(RmValue::Uri(uri)) => Some(uri),
        _other => None,
    }
}

/// Returns the held `DV_MULTIMEDIA`, when the target holds one.
pub(super) fn held_multimedia(existing: Option<&RmValue>) -> Option<&DvMultimedia> {
    match existing {
        Some(RmValue::Multimedia(multimedia)) => Some(multimedia.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_PARSABLE`, when the target holds one.
pub(super) fn held_parsable(existing: Option<&RmValue>) -> Option<&DvParsable> {
    match existing {
        Some(RmValue::Parsable(parsable)) => Some(parsable.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_TIME`, when the target holds one.
pub(super) fn held_time(existing: Option<&RmValue>) -> Option<&DvTime> {
    match existing {
        Some(RmValue::Time(time)) => Some(time.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_DURATION`, when the target holds one.
pub(super) fn held_duration(existing: Option<&RmValue>) -> Option<&DvDuration> {
    match existing {
        Some(RmValue::Duration(duration)) => Some(duration.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_COUNT`, when the target holds one.
pub(super) fn held_count(existing: Option<&RmValue>) -> Option<&DvCount> {
    match existing {
        Some(RmValue::Count(count)) => Some(count.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_PROPORTION`, when the target holds one.
pub(super) fn held_proportion(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::dv_proportion::DvProportion> {
    match existing {
        Some(RmValue::Proportion(proportion)) => Some(proportion.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_ORDINAL`, when the target holds one.
pub(super) fn held_ordinal(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::dv_ordinal::DvOrdinal> {
    match existing {
        Some(RmValue::Ordinal(ordinal)) => Some(ordinal.as_ref()),
        _other => None,
    }
}
