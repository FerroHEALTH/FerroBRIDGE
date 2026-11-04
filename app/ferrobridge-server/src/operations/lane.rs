// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operations lane: the compiled mapping set, the settings of each run,
//! and why the lane is off when it is.

use std::sync::Arc;

use fhirconnect::engine::demographics::Demographics;
use fhirconnect::engine::demographics::NoDemographics;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::programs::ProgramSet;
use fhirconnect::operations::run;

use crate::terminology::Translator;

/// The compiled mapping set and what every operation run needs beside it.
///
/// The run timestamp is read per request, because it is the
/// `Provenance.recorded` instant of that run and the composition context start
/// time the engine defaults to; `fhirconnect` reads no clock, so this is where
/// the clock lives.
#[derive(Debug)]
pub struct OperationsLane {
    programs: std::sync::RwLock<Arc<ProgramSet>>,
    device: String,
    composer: String,
    language: Option<String>,
    territory: Option<String>,
    translator: Translator,
    demographics: Arc<dyn Demographics>,
}

impl OperationsLane {
    /// Pairs a compiled set with the `Device` reference of this deployment.
    #[must_use]
    pub fn new(programs: ProgramSet, device: impl Into<String>) -> Self {
        Self {
            programs: std::sync::RwLock::new(Arc::new(programs)),
            device: device.into(),
            composer: String::from(DEFAULT_COMPOSER),
            language: None,
            territory: None,
            translator: Translator::default(),
            demographics: Arc::new(NoDemographics),
        }
    }

    /// Returns this lane translating every `conceptmap` code through
    /// `translator`.
    ///
    /// The default translates through no server, so a `conceptmap` refuses.
    #[must_use]
    pub fn with_translator(mut self, translator: Translator) -> Self {
        self.translator = translator;
        self
    }

    /// Returns this lane resolving every party reference through
    /// `demographics`.
    ///
    /// The default resolves none, so a party reference refuses the run.
    #[must_use]
    pub fn with_demographics(mut self, demographics: Arc<dyn Demographics>) -> Self {
        self.demographics = demographics;
        self
    }

    /// Returns the demographics endpoint the runs resolve party references
    /// through.
    #[must_use]
    pub fn demographics(&self) -> &dyn Demographics {
        self.demographics.as_ref()
    }

    /// Returns the terminology server the runs translate through.
    #[must_use]
    pub const fn translator(&self) -> &Translator {
        &self.translator
    }

    /// Returns this lane with the composition defaults an inbound run applies.
    ///
    /// `engine/defaults-for-fields.adoc` puts the composer on the engine and
    /// the composition language and territory on "the project performing the
    /// mapping", so they are configured rather than invented.
    #[must_use]
    pub fn with_composition_defaults(
        mut self,
        composer: impl Into<String>,
        language: Option<String>,
        territory: Option<String>,
    ) -> Self {
        self.composer = composer.into();
        self.language = language;
        self.territory = territory;
        self
    }

    /// Returns the compiled programs one call selects from, taken once per
    /// request so a reload never changes them under it.
    #[must_use]
    pub fn programs(&self) -> Arc<ProgramSet> {
        // NOTE: no specification governs this: our own design; the lock only
        // guards an `Arc` swap, so a poisoned lock still holds a whole set.
        Arc::clone(
            &self
                .programs
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Replaces the programs every later call selects from; a call in flight
    /// finishes on the set it took.
    pub fn replace(&self, programs: ProgramSet) {
        *self
            .programs
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(programs);
    }

    /// Returns the settings of one run, stamped with the current time.
    #[must_use]
    pub fn settings(&self) -> run::Settings {
        run_settings(
            &self.device,
            &self.composer,
            self.language.as_deref(),
            self.territory.as_deref(),
            &jiff::Timestamp::now().to_string(),
        )
    }
}

/// Returns the settings of one FHIRconnect run at `now`: the engine's
/// `device`, and the composer, language and territory the composition
/// defaults to when no mapping writes them (`engine/defaults-for-fields.adoc`).
pub(crate) fn run_settings(
    device: &str,
    composer: &str,
    language: Option<&str>,
    territory: Option<&str>,
    now: &str,
) -> run::Settings {
    let mut defaults = Defaults::at(now).with_composer(composer);
    if let Some(language) = language {
        defaults = defaults.with_language(language);
    }
    if let Some(territory) = territory {
        defaults = defaults.with_territory(territory);
    }
    run::Settings::new(device, now).with_defaults(defaults)
}

/// The composer the engine fills in when no mapping does.
///
/// "the `composer` can be defaulted with a value such as `FHIRconnect` party"
/// (`engine/defaults-for-fields.adoc`).
const DEFAULT_COMPOSER: &str = "FHIRconnect";

/// Why the FHIRconnect operations are not served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperationsOff {
    /// No `[mappings] directory` is set, so there is no mapping set to run.
    #[default]
    NoMappingSet,
    /// `[operations] enabled = false` turned the lane off.
    Disabled,
}
