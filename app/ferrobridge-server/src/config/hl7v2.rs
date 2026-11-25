// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings of the HL7 v2 face that `[hl7v2]` resolves into, and the
//! sending facility an MLLP route names.

/// One sending facility, matched against MSH-4.
///
/// MSH-4 is an `HD`: a namespace id in HD.1, or a universal id in HD.2 with
/// its type in HD.3 (HL7 v2.5.1 chapter 2, `HD` data type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    /// A facility named by its namespace id, HD.1.
    Namespace(String),
    /// A facility named by its universal id, HD.2, of the type HD.3.
    Universal {
        /// HD.2.
        id: String,
        /// HD.3, such as `ISO`.
        kind: String,
    },
}

/// The TLS the MLLP listener serves: its certificate and key, and the
/// authorities a client certificate must chain to.
#[derive(Clone)]
pub struct Tls {
    /// The acceptor, which requires a client certificate.
    pub acceptor: tokio_rustls::TlsAcceptor,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").finish_non_exhaustive()
    }
}

impl Sender {
    /// Returns the facility `text` names, as a route writes it: a namespace
    /// id, or `^<universal id>^<type>`.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let mut parts = text.split('^');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(""), Some(id), Some(kind)) => Self::Universal {
                id: id.to_owned(),
                kind: kind.to_owned(),
            },
            _ => Self::Namespace(text.to_owned()),
        }
    }

    /// Returns the facility as MSH-4 writes it: the namespace id, or
    /// `^<universal id>^<type>` (HL7 v2.5.1 chapter 2, `HD` data type).
    #[must_use]
    pub fn facility(&self) -> String {
        match self {
            Self::Namespace(namespace) => namespace.clone(),
            Self::Universal { id, kind } => format!("^{id}^{kind}"),
        }
    }
}
