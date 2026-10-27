// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A secret of the configuration: set inline under its key, or read from the
//! file its `<key>_file` sibling names, never both, through
//! `ferrohealth-secret`.
//!
//! A secret file is the shape a container runtime mounts a secret in, so a
//! value never needs to sit in the configuration file or the environment.
//! The file's trailing line ending is removed, an empty file is refused, and
//! the value is held in a `SecretString` that never renders it. No
//! specification governs this: our own design.

use std::path::Path;

use ferrohealth_secret::file::read_secret;
use ferrohealth_secret::secret::Secret;
use secrecy::SecretString;

use crate::config::error::Error;

/// Returns the secret set inline or in the file its `_file` sibling names.
///
/// `key` is the dotted key of the inline value, so a refusal names it and
/// `<key>_file`, never the value.
///
/// # Errors
/// Returns [`Error::SecretFile`] when both are set, when the file cannot be
/// read, and when it holds nothing.
pub fn resolve(
    key: &str,
    inline: Option<&Secret>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    read_secret(key, inline.map(Secret::to_secret_string), file)
        .map_err(|source| Error::SecretFile { source })
}
