// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A link's passcode, kept as an Argon2id hash (RFC 9106) and never as
//! text.

use argon2::Argon2;
use argon2::password_hash::Error;
use argon2::password_hash::PasswordHasher as _;
use argon2::password_hash::PasswordVerifier as _;
use secrecy::ExposeSecret as _;
use secrecy::SecretString;

/// The bytes of a passcode hash's salt (RFC 9106 §3.1 recommends 16).
const SALT_BYTES: usize = 16;

/// Why a passcode could not be hashed or checked.
#[derive(Debug, thiserror::Error)]
pub enum PasscodeError {
    /// The system random source failed.
    #[error("the system random source failed")]
    Random,
    /// The hash could not be computed, or a stored hash does not read.
    #[error("the passcode hash could not be computed or read")]
    Hash(#[source] Error),
}

/// Returns the PHC string of the Argon2id hash of `passcode` under a fresh
/// salt.
///
/// # Errors
///
/// Returns [`PasscodeError`] when no salt can be drawn or the hash fails.
pub fn hash(passcode: &SecretString) -> Result<String, PasscodeError> {
    let mut salt = [0_u8; SALT_BYTES];
    aws_lc_rs::rand::fill(&mut salt).map_err(|_unspecified| PasscodeError::Random)?;
    Argon2::default()
        .hash_password_with_salt(passcode.expose_secret().as_bytes(), &salt)
        .map(|hashed| hashed.to_string())
        .map_err(PasscodeError::Hash)
}

/// Returns whether `passcode` is the one `stored` hashes.
///
/// # Errors
///
/// Returns [`PasscodeError::Hash`] when `stored` is no Argon2 PHC string.
pub fn matches(passcode: &str, stored: &str) -> Result<bool, PasscodeError> {
    match Argon2::default().verify_password(passcode.as_bytes(), stored) {
        Ok(()) => Ok(true),
        Err(Error::PasswordInvalid) => Ok(false),
        Err(error) => Err(PasscodeError::Hash(error)),
    }
}
