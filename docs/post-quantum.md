<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-quantum inventory

This page lists every cryptographic use in FerroBRIDGE (the bridge, the
operator console, the library crates and the release workflows), the
algorithm each uses, and whether it resists a cryptographically relevant
quantum computer. The machine-readable copy is
`conformance/post-quantum.tsv`, one row per use, with five tab-separated
columns: `use`, `algorithm`, `safe` (`yes` or `no`), `pq_standard` (the
standard that defines the post-quantum form, or `no standard yet`) and
`location`. Every change that adds, removes or changes a cryptographic use
updates the tsv in the same pull request, and
`scripts/checks/post-quantum.sh --update` rewrites the table below and the
README badge from it. CI fails when the table, the badge or a location
disagrees with the tsv.

## The sources

- **NIST IR 8547 (final):** ECDSA, EdDSA and RSA signatures (§4.1.1) and
  ECDH, DH and RSA key establishment (§4.1.2) are deprecated after 2030 and
  disallowed after 2035. Symmetric primitives with at least 128 bits of
  classical security, AES-128 and up, SHA-256 and HMAC-SHA-256 among them,
  stay (§4.1.3). FIPS 203, 204 and 205 define ML-KEM, ML-DSA and SLH-DSA.
- **The EU coordinated implementation roadmap (NIS Cooperation Group,
  v1.1):** a use case is high-risk when its confidentiality must hold for
  10 years or more, which health records need, and high-risk use cases
  complete the transition by 31.12.2030 (§4.2). After 2030 they use no
  quantum-vulnerable public-key mechanism stand-alone, and standardised
  hybrid solutions are recommended (§4.1).
- **RFC 10024:** the TLS 1.3 hybrid group X25519MLKEM768 (codepoint
  0x11EC), within the hybrid design of RFC 9954.
- **RFC 9964:** ML-DSA for JOSE and COSE (key type `AKP`), the
  post-quantum form of the JWS rows. **RFC 9881:** ML-DSA in X.509.

A row is `safe` when every key it depends on is established or verified by
a post-quantum or hybrid mechanism, or when it is a symmetric primitive or
hash. A row whose post-quantum form has no standard yet counts as not safe,
whatever its `safe` column says.

## What FerroBRIDGE does

Every TLS 1.3 session FerroBRIDGE takes part in offers X25519MLKEM768 first,
then X25519, P-256 and P-384 for a peer that lacks the hybrid group:

- The HTTP listeners of the bridge and the console, and every outbound
  client built from an endpoint table (the CDR, the terminology server, the
  outbound FHIR server, the audit feed, the token issuers and their token
  endpoints), take the provider of `ferrohealth-server` 0.12.4, which orders
  the groups so.
- The rustls configurations FerroBRIDGE builds itself (the MLLP listener and
  the PostgreSQL clients of the CDM and the state store) and every client
  that takes rustls's default provider (reqwest's own configuration, the
  AMQP connector) take the same order from rustls's `prefer-post-quantum`
  feature, which the workspace enables.

The tests hold this for the HTTP listener, the MLLP listener in the crate and
in the bridge, the CDR, terminology and outbound FHIR clients and the
PostgreSQL pool: each agrees on X25519MLKEM768 with a peer that offers it and
on X25519 with a peer that offers only classical groups.

The key exchange protects what a session carries against an attacker who
records it now and decrypts it later, the risk the roadmap gives first
place. Three kinds of use stay classical:

- **TLS 1.2 and certificates.** The MLLP listener and the PostgreSQL and
  AMQP clients accept TLS 1.2, whose key exchange is ECDHE. The certificates
  that authenticate a peer are the operator's, and no TLS signature scheme
  for ML-DSA is published as an RFC yet.
- **JWS.** FerroBRIDGE pins `ferrohealth-smart` 0.34, whose access token
  verification, client assertions, DPoP proofs and ID token verification
  accept RSA, ECDSA and EdDSA only. `ferrohealth-smart` 0.35 adds ML-DSA in
  JWS (RFC 9964) as an option an operator turns on by configuration; these
  rows move when FerroBRIDGE takes that release and its tables offer the
  option.
- **Release signing.** crates.io Trusted Publishing and the Sigstore
  attestations use the keys GitHub and Sigstore issue.

FHIR R4 defines `Attachment.hash` as SHA-1, which FerroBRIDGE writes as the
specification asks. Its row counts as safe against a quantum computer, since
Grover's algorithm halves a hash's preimage strength and no more; SHA-1's
collision weakness is classical.

## The inventory

<!-- inventory:begin -->
| Use | Algorithm | Safe | Post-quantum standard | Location |
|---|---|---|---|---|
| TLS 1.3 key exchange of the HTTP listeners of the bridge and the console | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `Cargo.toml (ferrohealth-server =0.12.4), app/ferrobridge-server/src/lib.rs (serve_until)` |
| TLS 1.3 key exchange of the MLLP listener | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `crates/ferrobridge-hl7v2/src/mllp/tls.rs, app/ferrobridge-server/src/hl7v2/authorities.rs` |
| TLS 1.3 key exchange of the CDR, terminology, outbound FHIR, audit feed, issuer and token endpoint clients | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it (TlsClient::platform_config) | yes | RFC 10024 | `app/ferrobridge-server/src/config/resolve/mod.rs, app/ferrobridge-server/src/config/fhir_outbound.rs, app/ferrobridge-server/src/config/audit.rs, app/ferrobridge-server/src/config/auth.rs, app/ferrobridge-server/src/config/credentials.rs, crates/ferrobridge-term/src/client/mod.rs` |
| TLS 1.3 key exchange of the PostgreSQL clients of the CDM and the state store, writer and pool | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a server without it | yes | RFC 10024 | `crates/omop-cdm/src/connection/mod.rs, crates/omop-cdm/src/connection/relay.rs` |
| TLS 1.3 key exchange of the other HTTP clients: reference fetch, inbound recovery, usage report, console to bridge | X25519MLKEM768 first (rustls prefer-post-quantum, reqwest's default provider) | yes | RFC 10024 | `app/ferrobridge-server/src/facade/references/mod.rs, app/ferrobridge-server/src/inbound/recover.rs, app/ferrobridge-console/src/bridge/mod.rs, Cargo.toml (rustls features)` |
| TLS 1.3 key exchange of the AMQP events client | X25519MLKEM768 first (rustls prefer-post-quantum, through lapin's rustls connector) | yes | RFC 10024 | `app/ferrobridge-server/src/events/amqp.rs, Cargo.toml (lapin, rustls features)` |
| TLS 1.2 key exchange: the MLLP listener, the PostgreSQL and AMQP clients, and a listener or client whose min_version lowers the minimum | ECDHE over X25519, P-256 or P-384 | no | no standard yet | `crates/ferrobridge-hl7v2/src/mllp/tls.rs, app/ferrobridge-server/src/hl7v2/authorities.rs, crates/omop-cdm/src/connection/mod.rs` |
| TLS server and client certificate authentication | ECDSA, RSA or Ed25519, as the operator's certificates hold | no | no standard yet | `crates/ferrobridge-hl7v2/src/mllp/tls.rs, app/ferrobridge-server/src/hl7v2/authorities.rs, crates/omop-cdm/src/connection/mod.rs` |
| TLS record protection | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305 | yes | NIST IR 8547 4.1.3 | `Cargo.toml (rustls aws_lc_rs)` |
| Access token verification at the facade and the operator API | RS256, RS384, PS256, PS384, ES256, ES384 | no | RFC 9964 | `app/ferrobridge-server/src/smart.rs, app/ferrobridge-server/src/config/auth.rs` |
| Basic credentials of [[auth.basic.users]] | Argon2id PHC hashes | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/config/auth.rs` |
| Client assertions (private_key_jwt) the bridge signs for its own tokens | RS256 to PS512, ES256, ES384 or EdDSA, as the configured key holds | no | RFC 9964 | `app/ferrobridge-server/src/config/credentials.rs, app/ferrobridge-server/src/credentials.rs` |
| DPoP proofs the bridge signs for its own tokens | ES256 or ES384, as the configured key holds | no | RFC 9964 | `app/ferrobridge-server/src/config/credentials.rs` |
| ID token verification of the operator console's sign-in | RS256 to PS512, ES256, ES384, EdDSA | no | RFC 9964 | `app/ferrobridge-console/src/config/settings.rs` |
| Inbound webhook signatures, HMAC | HMAC-SHA-256 or HMAC-SHA-512 (hmac, Standard Webhooks v1) | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/inbound/verify.rs` |
| Inbound webhook signatures, public key | Ed25519 (Standard Webhooks v1a and v2a) | no | no standard yet | `app/ferrobridge-server/src/inbound/verify.rs` |
| State store sealing of stored bodies and lookup keys | AES-256-GCM, HMAC-SHA-256 | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/store/seal.rs` |
| SMART Health Links payload encryption | JWE dir with A256GCM | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/shl/seal.rs` |
| SMART Health Links passcodes | Argon2id | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/shl/passcode.rs` |
| SMART Health Links manifest ids, handles and tokens at rest | SHA-256 | yes | NIST IR 8547 4.1.3 | `app/ferrobridge-server/src/shl/store.rs` |
| OMOP person pseudonyms | HMAC-SHA-256 | yes | NIST IR 8547 4.1.3 | `crates/omop-cdm/src/writer/pseudonym.rs` |
| Derived identifiers, search cursor fingerprints, event dedupe keys and mapping file digests | SHA-256 | yes | NIST IR 8547 4.1.3 | `crates/fhirconnect/src/engine/identity.rs, crates/fhirconnect/src/engine/seam/identity.rs, crates/ferrobridge-hl7v2/src/map/run/resource.rs, app/ferrobridge-server/src/facade/search/cursor.rs, app/ferrobridge-server/src/inbound/event.rs, app/ferrobridge-server/src/mappings/files.rs` |
| FHIR Attachment.hash of documents | SHA-1, as FHIR R4 Attachment.hash defines | yes | NIST IR 8547 4.1.3 | `crates/eehrxf/src/document/metadata.rs, crates/fhirconnect/src/engine/lens/encapsulated.rs` |
| crates.io publishing | Trusted Publishing (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/release.yml, .github/workflows/publish-crates.yml` |
| Release provenance and SBOM attestations of the tarballs and images | Sigstore (ECDSA P-256) | no | no standard yet | `.github/workflows/release-build.yml, .github/workflows/release-image.yml, .github/workflows/release-console.yml` |
<!-- inventory:end -->
