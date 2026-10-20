// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The throwaway TLS material reads the same whichever `openssl` made it:
//! every certificate is signed with SHA-256 and carries its key on the named
//! curve P-256. `LibreSSL`, the `openssl` of macOS, otherwise signs with SHA-1,
//! which PostgreSQL's OpenSSL refuses, and writes the curve's parameters out
//! in full, which the bridge's TLS stack refuses.

use std::error::Error;
use std::io::Write as _;
use std::process::{Command, Stdio};

use ferrobridge_testkit::tls::{TlsError, TlsMaterial};

/// What `openssl x509 -text` prints for the PEM certificate `pem`.
fn described(pem: &[u8]) -> Result<String, Box<dyn Error>> {
    let mut child = Command::new("openssl")
        .args(["x509", "-noout", "-text"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or("no stdin")?.write_all(pem)?;
    let output = child.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

/// The trimmed line of `text` that starts with `label`.
fn line<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(label))
}

#[test]
fn every_certificate_is_signed_with_sha_256_over_a_named_p_256_key() -> Result<(), Box<dyn Error>> {
    let material = match TlsMaterial::generate() {
        Ok(material) => material,
        Err(TlsError::MissingOpenssl) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for (name, pem) in [
        ("the CA", material.ca()),
        ("the stranger CA", material.stranger_ca()),
        ("the server certificate", material.server_certificate()),
    ] {
        let text = described(pem)?;
        assert_eq!(
            Some("Signature Algorithm: ecdsa-with-SHA256"),
            line(&text, "Signature Algorithm:"),
            "{name}"
        );
        assert_eq!(
            Some("ASN1 OID: prime256v1"),
            line(&text, "ASN1 OID:"),
            "{name} names its curve"
        );
    }
    Ok(())
}
