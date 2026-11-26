// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The language a document is written in.
//!
//! openEHR RM 1.1.0 records it as two mandatory attributes of `COMPOSITION`:
//! `language`, an ISO 639-1 code, and `territory`, an ISO 3166-1 alpha-2
//! code. [`DocumentLanguage`] joins the two into the language-region tag of
//! RFC 5646 §2.1 (`en` and `GB` give `en-GB`) and takes no bare language, so
//! a document carries the region its record states. A form whose
//! `Composition` profile binds `Composition.language` with strength
//! `required` ([`Language::binding`]) takes only a [`HeldLanguage`], which
//! [`DocumentLanguage::held`] gives once the terminology server answers that
//! the bound value set holds the tag: `ValueSet/$validate-code`, with no value
//! set copied into the crate.

use fhir_types::r4::coding::Coding;

use crate::terminology::{Candidate, Terminology, TerminologyError};

/// The code system of a BCP 47 language tag
/// (<https://hl7.org/fhir/R4/valueset-languages.html>).
pub const BCP_47: &str = "urn:ietf:bcp:47";

/// The MyHealth@EU value set eHDSILanguage.
///
/// The MyHealth@EU `Composition` profiles bind `Composition.language` to it
/// with strength `required` (`composition-hdr-myhealtheu`,
/// `composition-euvac-myhealtheu`).
pub const EHDSI_LANGUAGE: &str = "http://terminology.ehdsi.eu/ValueSet/eHDSILanguage";

/// What a form's profiles ask of the document's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// Whether the profiles require `Bundle.language` or
    /// `Composition.language`.
    pub required: bool,
    /// The value set the `Composition` profile binds `Composition.language`
    /// to with strength `required`, a canonical URL, when it binds it so.
    pub binding: Option<&'static str>,
}

impl Language {
    /// Profiles that require no language and bind none.
    pub const OPTIONAL: Self = Self {
        required: false,
        binding: None,
    };

    /// Profiles that require a language bound no stronger than `preferred`.
    pub const REQUIRED: Self = Self {
        required: true,
        binding: None,
    };
}

/// Why a language cannot be the document's.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LanguageError {
    /// The language is no ISO 639-1 code.
    #[error("the language {language:?} is no ISO 639-1 code of two letters")]
    Language {
        /// The language as given.
        language: String,
    },
    /// The territory is no ISO 3166-1 alpha-2 code.
    #[error("the territory {territory:?} is no ISO 3166-1 alpha-2 code of two letters")]
    Territory {
        /// The territory as given.
        territory: String,
    },
    /// The composition records no language or no territory.
    #[error("the composition records no {attribute}, which the openEHR RM requires")]
    Unrecorded {
        /// `language` or `territory`.
        attribute: &'static str,
    },
    /// The bound value set does not hold the tag.
    #[error("the value set {value_set} does not hold the language {tag}")]
    Unheld {
        /// The language tag.
        tag: String,
        /// The bound value set.
        value_set: &'static str,
    },
    /// The terminology server gave no answer on the tag.
    #[error("the terminology server gave no answer on the language {tag}")]
    Terminology {
        /// The language tag.
        tag: String,
        /// The terminology failure.
        #[source]
        source: TerminologyError,
    },
}

/// A language-region tag of RFC 5646 §2.1, built from an openEHR
/// composition's `language` and `territory`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLanguage {
    tag: String,
}

impl DocumentLanguage {
    /// Joins `language`, an ISO 639-1 code, and `territory`, an ISO 3166-1
    /// alpha-2 code, into a language-region tag, the language in lower case
    /// and the region in upper case (RFC 5646 §2.1.1).
    ///
    /// # Errors
    ///
    /// [`LanguageError::Language`] and [`LanguageError::Territory`] for a
    /// code that is not two ASCII letters.
    pub fn new(language: &str, territory: &str) -> Result<Self, LanguageError> {
        if !two_letters(language) {
            return Err(LanguageError::Language {
                language: language.to_owned(),
            });
        }
        if !two_letters(territory) {
            return Err(LanguageError::Territory {
                territory: territory.to_owned(),
            });
        }
        Ok(Self {
            tag: format!(
                "{}-{}",
                language.to_ascii_lowercase(),
                territory.to_ascii_uppercase()
            ),
        })
    }

    /// Reads the language of `composition`, an openEHR composition in its
    /// canonical JSON form: `language.code_string` and
    /// `territory.code_string` (openEHR RM 1.1.0 `COMPOSITION`).
    ///
    /// # Errors
    ///
    /// [`LanguageError::Unrecorded`] for a composition with no language or
    /// no territory, and the errors of [`DocumentLanguage::new`].
    pub fn of_composition(composition: &serde_json::Value) -> Result<Self, LanguageError> {
        let code = |attribute: &'static str| {
            composition
                .pointer(&format!("/{attribute}/code_string"))
                .and_then(serde_json::Value::as_str)
                .ok_or(LanguageError::Unrecorded { attribute })
        };
        Self::new(code("language")?, code("territory")?)
    }

    /// Returns the tag, such as `en-GB`.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// Returns the tag, consuming the language.
    pub(super) fn into_tag(self) -> String {
        self.tag
    }

    /// Asks the terminology server whether `value_set` holds the tag, and
    /// gives the language a form bound to that value set takes.
    ///
    /// # Errors
    ///
    /// [`LanguageError::Unheld`] when the server answers that the value set
    /// does not hold the tag, and [`LanguageError::Terminology`] when it
    /// gives no answer, no server configured among them.
    pub async fn held(
        self,
        terminology: &Terminology,
        value_set: &'static str,
    ) -> Result<HeldLanguage, LanguageError> {
        let candidate = Candidate::Coding(Box::new(Coding {
            system: Some(BCP_47.into()),
            code: Some(self.tag.as_str().into()),
            ..Coding::default()
        }));
        match terminology.validate(value_set, &candidate).await {
            Ok(true) => Ok(HeldLanguage {
                language: self,
                value_set,
            }),
            Ok(false) => Err(LanguageError::Unheld {
                tag: self.tag,
                value_set,
            }),
            Err(source) => Err(LanguageError::Terminology {
                tag: self.tag,
                source,
            }),
        }
    }
}

/// A [`DocumentLanguage`] the terminology server answered a value set holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldLanguage {
    language: DocumentLanguage,
    value_set: &'static str,
}

impl HeldLanguage {
    /// Returns the language.
    #[must_use]
    pub const fn language(&self) -> &DocumentLanguage {
        &self.language
    }

    /// Returns the tag, consuming the language.
    pub(super) fn into_tag(self) -> String {
        self.language.tag
    }

    /// Returns the value set that holds it.
    #[must_use]
    pub const fn value_set(&self) -> &'static str {
        self.value_set
    }
}

/// Whether `code` is two ASCII letters.
fn two_letters(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic())
}
