// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `\` escape of a search value (<https://hl7.org/fhir/R4/search.html>,
//! §3.1.1.4.19).
//!
//! "When any of these characters appear in an actual parameter value, they
//! must be prepended by the character `\`, which also must be used to
//! prepend itself": the characters are `$`, `,` and `|`. The escape applies
//! to the value after its percent-decoding.

/// The characters a search value gives a meaning to, so a literal one is
/// escaped: the `\` itself, the `,` between alternatives, the `$` of a
/// composite and the `|` of a token or a quantity.
pub const SPECIAL: [char; 4] = ['\\', ',', '$', '|'];

/// Why a value's escapes are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EscapeError {
    /// A `\` before a character other than `\`, `,`, `$` and `|`: "The
    /// parameter value `xx\xx` is illegal" (§3.1.1.4.19).
    #[error("a `\\` at byte {at} escapes none of `\\`, `,`, `$` and `|`")]
    Illegal {
        /// The byte offset of the `\` in the value.
        at: usize,
    },
    /// A `\` that ends the value and so escapes nothing.
    #[error("the value ends in a `\\` that escapes nothing")]
    Trailing,
}

/// Splits `text` at every `separator` no `\` escapes, keeping every escape
/// for [`unescape`].
///
/// A `\` escapes the character after it whatever that is, so `a\\,b` splits
/// after the escaped `\` and `a\,b` does not split. Splitting at `,` gives
/// the alternatives of an `OR` (§3.1.1.4.19: `code=a,b` asks for `a` or
/// `b`), at `|` the parts of a token, and at `$` the parts of a composite.
/// The parts keep their text as written: an empty part is the reader's to
/// refuse, and an illegal escape is [`unescape`]'s.
#[must_use]
pub fn split(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            escaped = false;
            current.push(character);
        } else if character == '\\' {
            escaped = true;
            current.push(character);
        } else if character == separator {
            parts.push(core::mem::take(&mut current));
        } else {
            current.push(character);
        }
    }
    parts.push(current);
    parts
}

/// Removes the `\` escapes of one value.
///
/// # Errors
///
/// [`EscapeError::Illegal`] for a `\` before any character other than `\`,
/// `,`, `$` and `|`, and [`EscapeError::Trailing`] for a `\` that ends the
/// value.
pub fn unescape(text: &str) -> Result<String, EscapeError> {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.char_indices();
    while let Some((at, character)) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some((_, escaped)) if SPECIAL.contains(&escaped) => plain.push(escaped),
            Some(_) => return Err(EscapeError::Illegal { at }),
            None => return Err(EscapeError::Trailing),
        }
    }
    Ok(plain)
}

/// Escapes every `\`, `,`, `$` and `|` of a literal value, the inverse of
/// [`unescape`], so a client writes a value a server reads back unchanged.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if SPECIAL.contains(&character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
