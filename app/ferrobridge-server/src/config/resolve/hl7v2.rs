// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[hl7v2]` into the HL7 v2 face's listeners.
//!
//! The senders are MLLP sources in the state store, so the file holds only
//! the listeners: the TLS one with its material, the plain one, the timeouts
//! and the frame ceiling. No specification governs this configuration: our
//! own design.

use std::net::SocketAddr;
use std::time::Duration;

use crate::config::Config;
use crate::config::Hl7v2Settings;
use crate::config::error::Error;
use crate::config::hl7v2;
use crate::config::section::hl7v2::Hl7v2Tls;

/// Returns whether listeners at `one` and `other` would take one port: the
/// same port on the same address, or on an address one of them binds as
/// unspecified. Port 0 asks the system for a port of its own, so it shares
/// with nothing.
fn shared(one: SocketAddr, other: SocketAddr) -> bool {
    one.port() != 0
        && one.port() == other.port()
        && (one.ip() == other.ip() || one.ip().is_unspecified() || other.ip().is_unspecified())
}

/// Returns the TLS `tls` describes, `None` when it names no file: its chain
/// and key read once and the acceptor built over the client authorities
/// file, which the acceptor reads again when it changes.
///
/// # Errors
/// Returns [`Error::Needs`] for a table naming some files and not others,
/// [`Error::TlsFile`] for a file that cannot be read, and
/// [`Error::Hl7v2Tls`] for material the acceptor refuses.
fn tls_of(tls: &Hl7v2Tls) -> Result<Option<hl7v2::Tls>, Error> {
    if *tls == Hl7v2Tls::default() {
        return Ok(None);
    }
    let named = |key: &str, path: Option<&std::path::PathBuf>| {
        path.cloned()
            .ok_or_else(|| super::needed_by("hl7v2.tls", key))
    };
    let read = |key: &str, path: &std::path::Path| {
        std::fs::read(path).map_err(|source| Error::TlsFile {
            key: key.to_owned(),
            source,
        })
    };
    let chain = named("hl7v2.tls.certificate_file", tls.certificate_file.as_ref())?;
    let key = named("hl7v2.tls.key_file", tls.key_file.as_ref())?;
    let authorities = named("hl7v2.tls.client_ca_file", tls.client_ca_file.as_ref())?;
    let acceptor = crate::hl7v2::authorities::acceptor(
        &read("hl7v2.tls.certificate_file", &chain)?,
        &read("hl7v2.tls.key_file", &key)?,
        &authorities,
    )
    .map_err(|source| Error::Hl7v2Tls {
        source: Box::new(source),
    })?;
    Ok(Some(hl7v2::Tls { acceptor }))
}

/// Returns the timeout `span` states, `None` for `"0s"`.
fn timeout_of(span: ferrohealth_config::units::Duration) -> Option<Duration> {
    (!span.is_zero()).then(|| span.get())
}

/// Returns the HL7 v2 face `config` turns on, refused without the facade it
/// writes through, without the state store its senders are routed from, and
/// without a listener; `None` while it is off.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<Hl7v2Settings>, Error> {
    let hl7v2 = &config.hl7v2;
    if !hl7v2.enabled {
        return Ok(None);
    }
    super::needs(
        "hl7v2.enabled",
        &[
            (config.facade.enabled, "facade.enabled"),
            (config.state.is_some(), "[state]"),
        ],
    )?;
    let listen = hl7v2
        .listen
        .parse::<SocketAddr>()
        .map_err(|source| Error::Listen {
            key: String::from("hl7v2.listen"),
            source,
        })?;
    // NOTE: no specification governs this: our own design; a limit past the address
    // space bounds nothing more than the address space does, so it saturates.
    let frame_limit = usize::try_from(hl7v2.frame_limit.bytes()).unwrap_or(usize::MAX);
    if frame_limit == 0 {
        return Err(Error::FrameLimit {
            key: String::from("hl7v2.frame_limit"),
        });
    }
    let tls = tls_of(&hl7v2.tls)?;
    let plain = hl7v2
        .plain
        .as_ref()
        .map(|plain| {
            plain
                .bind
                .parse::<SocketAddr>()
                .map_err(|source| Error::Listen {
                    key: String::from("hl7v2.plain.bind"),
                    source,
                })
        })
        .transpose()?;
    if let Some(bind) = plain
        && tls.is_some()
        && shared(bind, listen)
    {
        return Err(Error::SharedListener {
            key: String::from("hl7v2.plain.bind"),
            value: bind.to_string(),
        });
    }
    if tls.is_none() && plain.is_none() {
        return Err(super::needed_by(
            "hl7v2.enabled",
            "[hl7v2.tls] or [hl7v2.plain]",
        ));
    }
    Ok(Some(Hl7v2Settings {
        listen,
        timeouts: ferrobridge_hl7v2::mllp::listener::Timeouts {
            idle: timeout_of(hl7v2.idle_timeout),
            frame: timeout_of(hl7v2.frame_timeout),
        },
        frame_limit,
        authority_max_age: hl7v2.authority_max_age.get(),
        tls,
        plain,
    }))
}
