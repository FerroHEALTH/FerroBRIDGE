// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

/// A refused configuration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration could not be read: a file that does not read, TOML
    /// that does not parse, an unknown key or variable, or a value of the
    /// wrong type, each named by its key and never by its value.
    #[error("the configuration does not read")]
    Load {
        /// What the family's loader reported.
        #[source]
        source: ferrohealth_config::error::ConfigError,
    },
    /// A socket address that does not parse.
    #[error("`{key}` is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A locale that is no BCP 47 language tag.
    #[error("`{key}` is not a language tag")]
    Locale {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: ferrohealth_ui::i18n::UnknownLanguage,
    },
    /// A URL that does not parse.
    #[error("`{key}` is not a URL")]
    Url {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: url::ParseError,
    },
    /// A URL its key does not admit.
    #[error("`{key}` {reason}")]
    UrlShape {
        /// The key.
        key: String,
        /// What the key requires.
        reason: &'static str,
    },
    /// A value that must be positive is zero.
    #[error("`{key}` must be greater than zero")]
    Zero {
        /// The key.
        key: String,
    },
    /// A required value is absent.
    #[error("`{key}` is required")]
    Missing {
        /// The key.
        key: String,
    },
    /// A scope list names a value its key does not admit.
    #[error("`{key}` {reason}")]
    Scope {
        /// The key.
        key: String,
        /// What the key requires.
        reason: &'static str,
    },
    /// A refusal of `[oidc]` or `[session]` the keys above do not name, such
    /// as a sealed store with no key or an absolute lifetime shorter than the
    /// idle one.
    #[error(transparent)]
    Session(Box<ferrohealth_session::config::ConfigError>),
    /// Cookies without `Secure` on a console that is not plain HTTP on
    /// loopback, where they would travel in the clear.
    #[error("`{key}` may be false only for a console whose redirect_uri is http on loopback")]
    InsecureCookie {
        /// The key.
        key: String,
    },
    /// A secret could not be read from its key or its `_file` sibling: both
    /// set, a file that cannot be read, or one that holds nothing.
    #[error("a secret could not be read")]
    SecretFile {
        /// What `ferrohealth-secret` reported, with the key.
        #[source]
        source: ferrohealth_secret::file::SecretError,
    },
}

impl From<ferrohealth_session::config::ConfigError> for Error {
    /// The refusal of a `[oidc]` or `[session]` key, by its key as the
    /// console's own refusals name one.
    fn from(error: ferrohealth_session::config::ConfigError) -> Self {
        use ferrohealth_session::config::ConfigError;
        match error {
            ConfigError::Secret(source) => Self::SecretFile { source },
            ConfigError::Missing { key } => Self::Missing {
                key: key.to_owned(),
            },
            ConfigError::Url { key, source } => Self::Url {
                key: key.to_owned(),
                source,
            },
            ConfigError::Zero { key } => Self::Zero {
                key: key.to_owned(),
            },
            ConfigError::UrlShape { key, reason } => Self::UrlShape {
                key: key.to_owned(),
                reason,
            },
            ConfigError::NoOpenId => Self::Missing {
                key: String::from("oidc.scopes (openid)"),
            },
            ConfigError::OfflineAccessScope => Self::Scope {
                key: String::from("oidc.scopes"),
                reason: "must not list offline_access; set oidc.offline_access = true",
            },
            other => Self::Session(Box::new(other)),
        }
    }
}
