// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each verifier accepts a correct request and refuses a wrong one.

use std::fmt::Write as _;

use aws_lc_rs::hmac;
use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair as _};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrobridge_server::inbound::verify::{
    Encoding, Hmac, HmacAlgorithm, Key, Mtls, Refused, Request, Scheme, StandardWebhooks,
    StaticHeader, Verifier, WEBHOOK_ID, WEBHOOK_SIGNATURE, WEBHOOK_TIMESTAMP,
};
use http::{HeaderMap, HeaderValue};
use serde_json::json;

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(*name, HeaderValue::from_str(value).expect("a header value"));
    }
    map
}

fn request<'a>(headers: &'a HeaderMap, body: &'a [u8], now: i64) -> Request<'a> {
    Request {
        headers,
        body,
        peer: None,
        now,
    }
}

fn key(text: &str) -> Key {
    Key::new(text.as_bytes().to_vec())
}

fn standard(schemes: Vec<Scheme>, endpoint_id: Option<&str>) -> Verifier {
    Verifier::StandardWebhooks(StandardWebhooks {
        schemes,
        endpoint_id: endpoint_id.map(str::to_owned),
        ..StandardWebhooks::default()
    })
}

// The test vector of the Standard Webhooks reference library, tag v1.3.0
// (`libraries/rust/src/lib.rs`, `test_sign`).
const V1_SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";
const V1_ID: &str = "msg_27UH4WbU6Z5A5EzD8u03UvzRbpk";
const V1_TIMESTAMP: i64 = 1_649_367_553;
const V1_BODY: &[u8] = br#"{"email":"test@example.com","username":"test_user"}"#;
const V1_SIGNATURE: &str = "v1,tZ1I4/hDygAJgO5TYxiSd6Sd0kDW6hPenDe+bTa3Kkw=";

fn v1_headers(signature: &str) -> HeaderMap {
    headers(&[
        (WEBHOOK_ID, V1_ID),
        (WEBHOOK_TIMESTAMP, &V1_TIMESTAMP.to_string()),
        (WEBHOOK_SIGNATURE, signature),
    ])
}

#[test]
fn standard_webhooks_v1_accepts_the_reference_vector() {
    let map = v1_headers(V1_SIGNATURE);
    let verifier = standard(vec![Scheme::V1], None);
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, V1_BODY, V1_TIMESTAMP), &[key(V1_SECRET)])
    );
}

#[test]
fn standard_webhooks_v1_finds_its_signature_among_several_and_any_rotated_key() {
    let map = v1_headers(&format!("v1,Zm9v v2,abc {V1_SIGNATURE}"));
    let verifier = standard(vec![Scheme::V1], None);
    let keys = [
        key("whsec_bmV3ZXIga2V5IGZvciByb3RhdGlvbg=="),
        key(V1_SECRET),
    ];
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, V1_BODY, V1_TIMESTAMP + 10), &keys)
    );
}

#[test]
fn standard_webhooks_v1_refuses_a_changed_body_a_wrong_key_and_a_stale_timestamp() {
    let map = v1_headers(V1_SIGNATURE);
    let verifier = standard(vec![Scheme::V1], None);
    let changed = br#"{"email":"test@example.com","username":"other"}"#;
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(&request(&map, changed, V1_TIMESTAMP), &[key(V1_SECRET)])
    );
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(
            &request(&map, V1_BODY, V1_TIMESTAMP),
            &[key("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw")]
        )
    );
    assert_eq!(
        Err(Refused::Stale),
        verifier.verify(
            &request(&map, V1_BODY, V1_TIMESTAMP + 301),
            &[key(V1_SECRET)]
        )
    );
    assert_eq!(
        Err(Refused::Stale),
        verifier.verify(
            &request(&map, V1_BODY, V1_TIMESTAMP - 301),
            &[key(V1_SECRET)]
        )
    );
}

#[test]
fn standard_webhooks_refuses_a_scheme_the_source_does_not_list() {
    let map = v1_headers(V1_SIGNATURE);
    let verifier = standard(vec![Scheme::V2a], Some("ep_1"));
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(&request(&map, V1_BODY, V1_TIMESTAMP), &[key(V1_SECRET)])
    );
}

#[test]
fn standard_webhooks_refuses_a_missing_or_repeated_header() {
    let verifier = standard(vec![Scheme::V1], None);
    let missing = headers(&[(WEBHOOK_ID, V1_ID), (WEBHOOK_SIGNATURE, V1_SIGNATURE)]);
    assert_eq!(
        Err(Refused::MissingHeader),
        verifier.verify(&request(&missing, V1_BODY, V1_TIMESTAMP), &[key(V1_SECRET)])
    );
    let mut repeated = v1_headers(V1_SIGNATURE);
    repeated.append(WEBHOOK_ID, HeaderValue::from_static("msg_other"));
    assert_eq!(
        Err(Refused::MissingHeader),
        verifier.verify(
            &request(&repeated, V1_BODY, V1_TIMESTAMP),
            &[key(V1_SECRET)]
        )
    );
}

// The asymmetric example of Standard Webhooks at main 3d65b1824,
// §Asymmetric signatures.
const V2A_PUBLIC: &str = "whpk_uWKK13PfwqyOgrqT0OrWKcS2uVD7e89+oUKnkoPjx7w=";
const V2A_ENDPOINT: &str = "ep_2fVZdC8Kx9TR4ZoCqHWt3NnLgRb";
const V2A_ID: &str = "msg_2KWPBgLlAfxdpx2AI54pPJ85f4W";
const V2A_TIMESTAMP: i64 = 1_674_087_231;
const V2A_BODY: &[u8] = br#"{"type":"contact.created","timestamp":"2022-11-03T20:26:10.344522Z","data":{"id":"1f81eb52-5198-4599-803e-771906343485"}}"#;
const V2A_SIGNATURE: &str =
    "v2a,kG9XaxZjAEpKB8KeQQb5G1Xp73YH5Rf5gxoWe7cg1kYMiyspdtZXe9w+dAwKeOurAwxfqmlOG/QR4m9s37DzAg==";

fn v2a_headers() -> HeaderMap {
    headers(&[
        (WEBHOOK_ID, V2A_ID),
        (WEBHOOK_TIMESTAMP, &V2A_TIMESTAMP.to_string()),
        (WEBHOOK_SIGNATURE, V2A_SIGNATURE),
    ])
}

#[test]
fn standard_webhooks_v2a_accepts_the_specification_example() {
    let map = v2a_headers();
    let verifier = standard(vec![Scheme::V2a], Some(V2A_ENDPOINT));
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, V2A_BODY, V2A_TIMESTAMP), &[key(V2A_PUBLIC)])
    );
}

#[test]
fn standard_webhooks_v2a_refuses_another_endpoint_and_needs_a_declared_one() {
    let map = v2a_headers();
    let other = standard(vec![Scheme::V2a], Some("ep_another"));
    assert_eq!(
        Err(Refused::Mismatch),
        other.verify(&request(&map, V2A_BODY, V2A_TIMESTAMP), &[key(V2A_PUBLIC)])
    );
    let undeclared = standard(vec![Scheme::V2a], None);
    assert_eq!(
        Err(Refused::Declaration),
        undeclared.verify(&request(&map, V2A_BODY, V2A_TIMESTAMP), &[key(V2A_PUBLIC)])
    );
}

#[test]
fn standard_webhooks_v2a_refuses_a_key_of_the_symmetric_form() {
    let map = v2a_headers();
    let verifier = standard(vec![Scheme::V2a], Some(V2A_ENDPOINT));
    assert_eq!(
        Err(Refused::NoKey),
        verifier.verify(&request(&map, V2A_BODY, V2A_TIMESTAMP), &[key(V1_SECRET)])
    );
}

#[test]
fn standard_webhooks_v1a_signs_the_content_itself() {
    let pair = Ed25519KeyPair::generate().expect("a key pair");
    let public = format!("whpk_{}", STANDARD.encode(pair.public_key().as_ref()));
    let content = format!("{V1_ID}.{V1_TIMESTAMP}.").into_bytes();
    let signed = [content.as_slice(), V1_BODY].concat();
    let signature = format!("v1a,{}", STANDARD.encode(pair.sign(&signed).as_ref()));
    let map = v1_headers(&signature);
    let verifier = standard(vec![Scheme::V1a], None);
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, V1_BODY, V1_TIMESTAMP), &[key(&public)])
    );
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(&request(&map, b"{}", V1_TIMESTAMP), &[key(&public)])
    );
}

fn hmac_sha512_hex(secret: &str, body: &[u8]) -> String {
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA512, secret.as_bytes()), body);
    tag.as_ref().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("writing to a string");
        text
    })
}

fn sha512_hex_verifier() -> Verifier {
    Verifier::Hmac(Hmac {
        algorithm: HmacAlgorithm::Sha512,
        header: String::from("X-Signature-SHA512"),
        encoding: Encoding::Hex,
        prefix: String::new(),
    })
}

#[test]
fn hmac_sha512_hex_accepts_the_body_signed_with_a_listed_key() {
    let body = br#"{"customerCode":"t1","modelType":"client","eventType":"UPDATE","id":42}"#;
    let signature = hmac_sha512_hex("connector-secret", body);
    let map = headers(&[("x-signature-sha512", &signature)]);
    let verifier = sha512_hex_verifier();
    assert_eq!(
        Ok(()),
        verifier.verify(
            &request(&map, body, 0),
            &[key("rotated-secret"), key("connector-secret")]
        )
    );
    let upper = headers(&[("x-signature-sha512", &signature.to_uppercase())]);
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&upper, body, 0), &[key("connector-secret")])
    );
}

#[test]
fn hmac_refuses_a_wrong_key_a_changed_body_and_a_malformed_signature() {
    let body = br#"{"id":42}"#;
    let signature = hmac_sha512_hex("connector-secret", body);
    let map = headers(&[("x-signature-sha512", &signature)]);
    let verifier = sha512_hex_verifier();
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(&request(&map, body, 0), &[key("other-secret")])
    );
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(
            &request(&map, br#"{"id":43}"#, 0),
            &[key("connector-secret")]
        )
    );
    let malformed = headers(&[("x-signature-sha512", "not-hex")]);
    assert_eq!(
        Err(Refused::Malformed),
        verifier.verify(&request(&malformed, body, 0), &[key("connector-secret")])
    );
    assert_eq!(
        Err(Refused::MissingHeader),
        verifier.verify(
            &request(&HeaderMap::new(), body, 0),
            &[key("connector-secret")]
        )
    );
    assert_eq!(
        Err(Refused::NoKey),
        verifier.verify(&request(&map, body, 0), &[])
    );
}

#[test]
fn hmac_sha256_base64_reads_its_prefix() {
    let body = b"payload";
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, b"s3cret"), body);
    let value = format!("sha256={}", STANDARD.encode(tag.as_ref()));
    let map = headers(&[("x-hub-signature-256", &value)]);
    let verifier = Verifier::Hmac(Hmac {
        algorithm: HmacAlgorithm::Sha256,
        header: String::from("X-Hub-Signature-256"),
        encoding: Encoding::Base64,
        prefix: String::from("sha256="),
    });
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, body, 0), &[key("s3cret")])
    );
    let unprefixed = headers(&[("x-hub-signature-256", &STANDARD.encode(tag.as_ref()))]);
    assert_eq!(
        Err(Refused::Malformed),
        verifier.verify(&request(&unprefixed, body, 0), &[key("s3cret")])
    );
}

#[test]
fn a_static_header_admits_only_a_listed_value() {
    let verifier = Verifier::StaticHeader(StaticHeader {
        header: String::from("Authorization"),
    });
    let map = headers(&[("authorization", "Bearer shared")]);
    assert_eq!(
        Ok(()),
        verifier.verify(&request(&map, b"", 0), &[key("Bearer shared")])
    );
    assert_eq!(
        Err(Refused::Mismatch),
        verifier.verify(&request(&map, b"", 0), &[key("Bearer other")])
    );
}

#[test]
fn mtls_admits_only_a_certificate_carrying_a_listed_name() {
    let key = rcgen::KeyPair::generate().expect("a key");
    let certificate = rcgen::CertificateParams::new(vec![String::from("sender.example")])
        .expect("parameters")
        .self_signed(&key)
        .expect("a certificate");
    let der = certificate.der().clone();
    let listed = Verifier::Mtls(Mtls {
        subjects: vec![String::from("sender.example")],
    });
    let other = Verifier::Mtls(Mtls {
        subjects: vec![String::from("someone.example")],
    });
    let map = HeaderMap::new();
    let mut presented = request(&map, b"", 0);
    presented.peer = Some(&der);
    assert_eq!(Ok(()), listed.verify(&presented, &[]));
    assert_eq!(Err(Refused::Mismatch), other.verify(&presented, &[]));
    assert_eq!(
        Err(Refused::Mismatch),
        listed.verify(&request(&map, b"", 0), &[])
    );
}

#[test]
fn a_verifier_reads_from_its_document_form() {
    let verifier: Verifier = serde_json::from_value(json!({
        "kind": "standard-webhooks", "schemes": ["v1", "v2a"], "tolerance": 120, "endpoint_id": "ep_1"
    }))
    .expect("a verifier");
    assert_eq!(
        Verifier::StandardWebhooks(StandardWebhooks {
            schemes: vec![Scheme::V1, Scheme::V2a],
            tolerance: std::time::Duration::from_secs(120),
            endpoint_id: Some(String::from("ep_1")),
        }),
        verifier
    );
    let hmac: Verifier = serde_json::from_value(json!({
        "kind": "hmac", "algorithm": "sha512", "header": "X-Signature-SHA512"
    }))
    .expect("an hmac verifier");
    assert_eq!(sha512_hex_verifier(), hmac);
    for refused in [
        json!({"kind": "hmac", "header": "X", "extra": 1}),
        json!({"kind": "hmac", "algorithm": "md5", "header": "X"}),
        json!({"kind": "standard-webhooks", "schemes": ["v3"]}),
        json!({"kind": "unknown"}),
    ] {
        assert!(
            serde_json::from_value::<Verifier>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn a_key_never_shows_in_debug_output() {
    assert_eq!("Key(..)", format!("{:?}", key("connector-secret")));
}
