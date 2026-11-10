// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composition fields the engine fills when no mapping does
//! (`engine/defaults-for-fields.adoc`).

use openehr_term::codes as term;

use crate::engine::origin::ItemId;
use crate::engine::origin::Origin;
use crate::engine::origin::Party;

/// The composition fields the engine fills when no mapping does.
///
/// `engine/defaults-for-fields.adoc` puts the composer and the context start
/// time on the engine and the rest on "the project performing the mapping", so
/// only those two carry a value here unless the caller sets more. Every field
/// the engine fills is recorded as a
/// [`Loss::Defaulted`](crate::engine::outcome::Loss::Defaulted) warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Defaults {
    pub(super) composer: String,
    pub(super) start_time: String,
    pub(super) setting: Setting,
    pub(super) language: Option<String>,
    pub(super) territory: Option<String>,
    origin: Option<Origin>,
    pub(super) facility: Option<Party>,
    pub(super) composer_id: Option<ComposerId>,
    pub(super) composer_identifiers: Vec<ItemId>,
}

/// The identifier of a default composer: the `external_ref` the Simplified
/// Formats builder writes from `ctx/composer_id`, `ctx/id_namespace` and
/// `ctx/id_scheme` (master06 §Composer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerId {
    /// The identifier, `PARTY_REF.id.value`.
    pub id: String,
    /// Its namespace, `PARTY_REF.namespace`.
    pub namespace: String,
    /// Its scheme, `GENERIC_ID.scheme`.
    pub scheme: String,
}

impl Defaults {
    /// Creates the defaults, with `now` as the context start time.
    ///
    /// "For the `context_start_time`, the default mapping time can be used",
    /// and the composer is defaulted "with a value such as `FHIRconnect`
    /// party" (`engine/defaults-for-fields.adoc`). This crate reads no clock,
    /// so the caller passes the time.
    #[must_use]
    pub fn at(now: impl Into<String>) -> Self {
        Self {
            composer: String::from("FHIRconnect"),
            start_time: now.into(),
            setting: Setting {
                code: String::from(term::Setting::OtherCare.code()),
                value: String::from(term::Setting::OtherCare.rubric()),
            },
            language: None,
            territory: None,
            origin: None,
            facility: None,
            composer_id: None,
            composer_identifiers: Vec::new(),
        }
    }

    /// Returns these defaults with `name` as the composer.
    #[must_use]
    pub fn with_composer(mut self, name: impl Into<String>) -> Self {
        self.composer = name.into();
        self
    }

    /// Returns these defaults with the `EVENT_CONTEXT.setting` the project
    /// performing the mapping defines.
    ///
    /// `code` and `value` are one concept of the openEHR terminology's
    /// `setting` group, which "should be defined by the project performing the
    /// mapping" and "can be defaulted to one of the valid values"
    /// (`engine/defaults-for-fields.adoc`, which links the group in
    /// `openehr_terminology.xml`). The default is `238` "other care".
    #[must_use]
    pub fn with_setting(mut self, code: impl Into<String>, value: impl Into<String>) -> Self {
        self.setting = Setting {
            code: code.into(),
            value: value.into(),
        };
        self
    }

    /// Returns these defaults with the composition language `code`.
    ///
    /// The code set is ISO 639-1
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_composition_class>).
    #[must_use]
    pub fn with_language(mut self, code: impl Into<String>) -> Self {
        self.language = Some(code.into());
        self
    }

    /// Returns these defaults with the composition territory `code`.
    ///
    /// The code set is ISO 3166-1
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_composition_class>).
    #[must_use]
    pub fn with_territory(mut self, code: impl Into<String>) -> Self {
        self.territory = Some(code.into());
        self
    }

    /// Returns these defaults with `origin` recorded in the composition's
    /// `FEEDER_AUDIT`.
    ///
    /// The system and the source resource travel as `originating_system_audit`
    /// and `originating_system_item_ids`, and every field the engine defaults
    /// travels as one `feeder_system_item_ids` entry in the order it was
    /// defaulted, so a reader of the stored composition sees what the bridge
    /// supplied
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
    /// Which attribute carries which fact is our own design: no specification
    /// governs the allocation.
    #[must_use]
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Returns these defaults naming the default composer by `id` beside its
    /// name.
    #[must_use]
    pub fn with_composer_id(mut self, id: ComposerId) -> Self {
        self.composer_id = Some(id);
        self
    }

    /// Returns these defaults naming the default composer by `identifiers`, its
    /// formal identifiers (RM Common IM, `PARTY_IDENTIFIED.identifiers`), beside
    /// its name and its `external_ref`.
    ///
    /// The `ctx/` keys of the composer carry no identifiers (Simplified Formats
    /// master06 §Composer), so a composer with identifiers travels whole as
    /// `composer|raw` (master04 §Raw canonical JSON).
    #[must_use]
    pub fn with_composer_identifiers(mut self, identifiers: Vec<ItemId>) -> Self {
        self.composer_identifiers = identifiers;
        self
    }

    /// Returns these defaults naming `facility` as the health care facility of a
    /// composition no mapping gave one (RM EHR IM,
    /// `EVENT_CONTEXT.health_care_facility`). A composition with no
    /// `EVENT_CONTEXT` names it as the originating system's location in its
    /// `FEEDER_AUDIT` instead, when the run records an origin.
    #[must_use]
    pub fn with_facility(mut self, facility: Party) -> Self {
        self.facility = Some(facility);
        self
    }

    /// Returns the origin the run records, when the caller set one.
    #[must_use]
    pub const fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
}

/// One concept of the openEHR terminology's `setting` group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Setting {
    pub(super) code: String,
    pub(super) value: String,
}
