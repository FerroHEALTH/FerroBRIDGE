// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token value (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.10).

use core::fmt;

use crate::escape::{EscapeError, split, unescape};

/// Why a token value is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenError {
    /// A part carries an illegal escape.
    #[error(transparent)]
    Escape(#[from] EscapeError),
    /// The value names neither a system nor a code: it is empty or `|`
    /// alone, which is none of the four forms.
    #[error("the token names neither a system nor a code")]
    Empty,
    /// The value carries more than one unescaped `|`; a `|` inside a system
    /// or a code is escaped (§3.1.1.4.19).
    #[error("the token carries more than one unescaped `|`")]
    Bars,
    /// An `:of-type` value is not three parts, `system|code|value`.
    #[error("an :of-type value is not system|code|value")]
    OfTypeParts,
    /// A part of an `:of-type` value is empty: "All 3 parts must be
    /// present" (§3.1.1.4.10).
    #[error("an :of-type value leaves a part empty")]
    OfTypeEmpty,
}

/// One token value in one of its four forms (§3.1.1.4.10).
///
/// `Debug` shows the form and the system and never the code, which may be
/// an identifier.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Token {
    /// `[code]`: the code, irrespective of the system.
    Code(String),
    /// `[system]|[code]`: the code in exactly this system.
    SystemCode {
        /// The system.
        system: String,
        /// The code.
        code: String,
    },
    /// `|[code]`: the code of a Coding or an Identifier with no system.
    NoSystem(String),
    /// `[system]|`: any code of the system.
    System(String),
}

/// What a token value asks of the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum System<'token> {
    /// Any system, or none: the `[code]` form.
    Any,
    /// No system: the `|[code]` form.
    Absent,
    /// Exactly this system.
    Is(&'token str),
}

impl Token {
    /// Reads one token value as [`split`] gave it, its escapes in place.
    ///
    /// The value splits at its one unescaped `|`, and each part is then
    /// unescaped, so `a\|b` is the code `a|b`.
    ///
    /// # Errors
    ///
    /// [`TokenError::Escape`] for an illegal escape, [`TokenError::Bars`]
    /// for more than one unescaped `|`, and [`TokenError::Empty`] for a value
    /// that names neither a system nor a code.
    pub fn parse(text: &str) -> Result<Self, TokenError> {
        let parts = split(text, '|');
        let token = match parts.as_slice() {
            [code] => Self::Code(unescape(code)?),
            [system, code] => {
                let system = unescape(system)?;
                let code = unescape(code)?;
                match (system.is_empty(), code.is_empty()) {
                    (true, true) => return Err(TokenError::Empty),
                    (true, false) => Self::NoSystem(code),
                    (false, true) => Self::System(system),
                    (false, false) => Self::SystemCode { system, code },
                }
            }
            _ => return Err(TokenError::Bars),
        };
        if matches!(&token, Self::Code(code) if code.is_empty()) {
            return Err(TokenError::Empty);
        }
        Ok(token)
    }

    /// What the value asks of the system.
    #[must_use]
    pub fn system(&self) -> System<'_> {
        match self {
            Self::Code(_) => System::Any,
            Self::NoSystem(_) => System::Absent,
            Self::SystemCode { system, .. } | Self::System(system) => System::Is(system),
        }
    }

    /// The code, absent in the `[system]|` form.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Code(code) | Self::NoSystem(code) | Self::SystemCode { code, .. } => Some(code),
            Self::System(_) => None,
        }
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(_) => f.debug_struct("Code").finish_non_exhaustive(),
            Self::SystemCode { system, .. } => f
                .debug_struct("SystemCode")
                .field("system", system)
                .finish_non_exhaustive(),
            Self::NoSystem(_) => f.debug_struct("NoSystem").finish_non_exhaustive(),
            Self::System(system) => f.debug_tuple("System").field(system).finish(),
        }
    }
}

/// One value of a token parameter under `:of-type`: `system|code|value`,
/// where the system and the code name an `Identifier.type` coding and the
/// value is the `Identifier.value` (§3.1.1.4.10).
///
/// `Debug` shows the type and never the value, which is an identifier.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OfType {
    system: String,
    code: String,
    value: String,
}

impl OfType {
    /// Reads one `:of-type` value as [`split`] gave it, its escapes in place.
    ///
    /// # Errors
    ///
    /// [`TokenError::Escape`] for an illegal escape,
    /// [`TokenError::OfTypeParts`] for other than three parts, and
    /// [`TokenError::OfTypeEmpty`] for an empty part.
    pub fn parse(text: &str) -> Result<Self, TokenError> {
        let parts = split(text, '|');
        let [system, code, value] = parts.as_slice() else {
            return Err(TokenError::OfTypeParts);
        };
        let of_type = Self {
            system: unescape(system)?,
            code: unescape(code)?,
            value: unescape(value)?,
        };
        if of_type.system.is_empty() || of_type.code.is_empty() || of_type.value.is_empty() {
            return Err(TokenError::OfTypeEmpty);
        }
        Ok(of_type)
    }

    /// The system of the `Identifier.type` coding.
    #[must_use]
    pub fn system(&self) -> &str {
        &self.system
    }

    /// The code of the `Identifier.type` coding.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The `Identifier.value`.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for OfType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfType")
            .field("system", &self.system)
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}
