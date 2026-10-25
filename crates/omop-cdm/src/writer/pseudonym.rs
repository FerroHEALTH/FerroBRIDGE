// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pseudonymisation of a row before it reaches the CDM.
//!
//! A pseudonymised write carries a keyed pseudonym for every column that
//! links a row back to its source, erases the columns that name a provider
//! or an address, and moves every date of a person by the person's shift.
//!
//! The CDM defines no pseudonymisation. The columns are the ones its field
//! definitions describe as a link back to the source or as a person's
//! identity (`OMOP_CDMv5.4_Field_Level.csv`: `person_source_value` "Use this
//! field to link back to persons in the source data", `provider_source_value`,
//! `care_site_source_value`, `specimen_source_id`, `visit_source_value`,
//! `visit_detail_source_value`, `family_source_value`, the provider's `npi`,
//! `dea` and `provider_name`, and the address of a `LOCATION`). No
//! specification governs the choice or the derivation: our own design.
//!
//! A pseudonym is HMAC-SHA-256 under the configured key (RFC 2104,
//! `aws_lc_rs::hmac`), so one value under one key gives one pseudonym on every
//! run and an incremental load joins. The date shift of a person is a whole
//! number of days in `[-N, N]` from the same HMAC over the person's EHR id, so
//! every row of a person moves by the same offset and the intervals between
//! them stay as they were.

use std::fmt;
use std::fmt::Write as _;

use aws_lc_rs::hmac;

use crate::meta::ColumnMeta;
use crate::meta::TableMeta;
use crate::writer::stage::Staged;

/// The shortest key the pseudonymiser accepts, in bytes: the output length of
/// SHA-256, the length RFC 2104 §3 recommends as the minimum.
pub const MIN_KEY_BYTES: usize = 32;

/// The largest date shift the pseudonymiser accepts, in days: ten years.
pub const MAX_SHIFT_DAYS: u16 = 3650;

/// The bytes of a pseudonym's HMAC tag kept, written as 40 hex characters,
/// which fit every `varchar(50)` source value column.
const PSEUDONYM_BYTES: usize = 20;

/// Why a pseudonymiser was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PseudonymError {
    /// The key is shorter than [`MIN_KEY_BYTES`].
    #[error("the pseudonymisation key holds {length} bytes; it needs at least {MIN_KEY_BYTES}")]
    ShortKey {
        /// The key's length in bytes.
        length: usize,
    },
    /// The date shift exceeds [`MAX_SHIFT_DAYS`].
    #[error("a date shift of {days} days exceeds the {MAX_SHIFT_DAYS} days allowed")]
    ShiftTooLarge {
        /// The shift asked for.
        days: u16,
    },
}

/// What the writer does with one column of a pseudonymised write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    /// The value is replaced by its keyed pseudonym.
    Pseudonym,
    /// The value is written as NULL.
    Erase,
}

/// Returns what a pseudonymised write does with `column` of `table`, or
/// `None` when the column is written as mapped.
#[must_use]
pub fn treatment(table: &str, column: &str) -> Option<Treatment> {
    match (table, column) {
        ("person", "person_source_value")
        | ("provider", "provider_source_value")
        | ("care_site", "care_site_source_value")
        | ("location", "location_source_value")
        | ("specimen", "specimen_source_id")
        | ("visit_occurrence", "visit_source_value")
        | ("visit_detail", "visit_detail_source_value")
        | ("payer_plan_period", "family_source_value") => Some(Treatment::Pseudonym),
        ("provider", "provider_name" | "npi" | "dea")
        | ("location", "address_1" | "address_2" | "zip" | "latitude" | "longitude") => {
            Some(Treatment::Erase)
        }
        _ => None,
    }
}

/// The keyed pseudonym and the date shift a writer applies.
#[derive(Clone)]
pub struct Pseudonymiser {
    key: hmac::Key,
    shift_days: u16,
}

impl fmt::Debug for Pseudonymiser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pseudonymiser")
            .field("shift_days", &self.shift_days)
            .finish_non_exhaustive()
    }
}

impl Pseudonymiser {
    /// Creates a pseudonymiser over `key` that shifts every date of a person
    /// by up to `shift_days` days either way, or leaves dates alone when it is
    /// 0.
    ///
    /// # Errors
    ///
    /// Returns [`PseudonymError::ShortKey`] when `key` is shorter than
    /// [`MIN_KEY_BYTES`] and [`PseudonymError::ShiftTooLarge`] when
    /// `shift_days` exceeds [`MAX_SHIFT_DAYS`].
    pub fn new(key: &[u8], shift_days: u16) -> Result<Self, PseudonymError> {
        if key.len() < MIN_KEY_BYTES {
            return Err(PseudonymError::ShortKey { length: key.len() });
        }
        if shift_days > MAX_SHIFT_DAYS {
            return Err(PseudonymError::ShiftTooLarge { days: shift_days });
        }
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, key),
            shift_days,
        })
    }

    /// Returns the HMAC tag of `value` under the key, in the domain `purpose`
    /// names, so a pseudonym and a shift never share a tag.
    fn tag(&self, purpose: &[u8], value: &str) -> hmac::Tag {
        let mut context = hmac::Context::with_key(&self.key);
        context.update(purpose);
        context.update(&[0]);
        context.update(value.as_bytes());
        context.sign()
    }

    /// Returns the pseudonym of `value`: 40 lowercase hex characters.
    ///
    /// # Panics
    ///
    /// Never: writing to a `String` does not fail.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "`impl fmt::Write for String` never returns an error"
    )]
    pub fn pseudonym(&self, value: &str) -> String {
        let tag = self.tag(b"ferrobridge pseudonym", value);
        let mut pseudonym = String::with_capacity(2 * PSEUDONYM_BYTES);
        for byte in tag.as_ref().iter().take(PSEUDONYM_BYTES) {
            write!(pseudonym, "{byte:02x}").expect("a String should accept every write");
        }
        pseudonym
    }

    /// Returns the date shift of the person of `ehr_id`, in days.
    ///
    /// # Panics
    ///
    /// Never: the offset is below `2 * MAX_SHIFT_DAYS + 1`, which fits an `i32`.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "the offset is below 2 * MAX_SHIFT_DAYS + 1, which fits an i32"
    )]
    pub fn shift(&self, ehr_id: &str) -> i32 {
        if self.shift_days == 0 {
            return 0;
        }
        let tag = self.tag(b"ferrobridge date shift", ehr_id);
        let mut word = [0_u8; 8];
        for (slot, byte) in word.iter_mut().zip(tag.as_ref()) {
            *slot = *byte;
        }
        let span = 2 * u64::from(self.shift_days) + 1;
        let offset = u64::from_be_bytes(word) % span;
        i32::try_from(offset).expect("the offset should be below 7301") - i32::from(self.shift_days)
    }

    /// Returns `value` of `column` of `table` as a pseudonymised write stages
    /// it.
    pub(super) fn staged(&self, table: &TableMeta, column: &ColumnMeta, value: Staged) -> Staged {
        match (treatment(table.name, column.name), value) {
            (Some(Treatment::Pseudonym), Staged::Text(Some(text))) => {
                Staged::Text(Some(self.pseudonym(&text)))
            }
            (Some(Treatment::Erase), Staged::Integer(_)) => Staged::Integer(None),
            (Some(Treatment::Erase), Staged::Text(_)) => Staged::Text(None),
            (_, value) => value,
        }
    }
}
