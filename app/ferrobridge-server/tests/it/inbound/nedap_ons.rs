// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nedap Ons source template against Nedap's published examples
//! (<https://ons-api.nl/english/technical/Webhooks.html>): the Java HMAC
//! sample verifies, the notification model reads, the NOP pair is a probe,
//! and the template's map renders a client and its BSN as an R4 `Patient`.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::event::{Answered, Change, Reading};
use ferrobridge_server::inbound::verify::{Key, Refused, Request};
use ferrobridge_server::sources::document::{Document, Webhook};
use ferrobridge_server::structuremap::Set;
use ferrobridge_server::structuremap::engine::Engine;
use ferrobridge_server::structuremap::engine::translate::Terminology;
use ferrobridge_server::structuremap::model::Known;
use ferrobridge_server::structuremap::parse::parse;
use ferrobridge_testkit::stubs::terminology;
use fhir_types::codec::Value;
use http::HeaderMap;
use serde_json::json;
use wiremock::{Mock, MockServer, matchers};

/// The repository root.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The body of Nedap's Java sample.
const SAMPLE: &str = r#"{"customerCode":"TE1000","modelType":"client","eventType":"CREATE","id":1,"timestamp":"2024-08-22T10:08:11+02:00","amountOfRetries":0}"#;

/// The signature the Java sample prints for [`SAMPLE`] under `SuperSecret`.
const SAMPLE_SIGNATURE: &str = "a89bf4503874ce3069409bc195c003623fc660eefe8aed0106caba59d78fa1f160c006475b015767cd713b4fcd738c219a684155087fa77d5cb55d482a2525b4";

/// Reads the file `path` below the repository root.
fn read(path: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!("{ROOT}/{path}"))?)
}

/// Returns the template as an operator API write.
pub(crate) fn template() -> Result<Write, Box<dyn Error>> {
    Ok(serde_json::from_str(&read(
        "inbound-templates/nedap-ons.json",
    )?)?)
}

/// Returns the template's webhook document with the tenant `TE1000` below
/// the production base.
fn webhook() -> Result<Webhook, Box<dyn Error>> {
    let mut write = template()?;
    write.document["tenants"] = json!([{
        "tenant": "TE1000",
        "identifier_system": "urn:oid:2.16.840.1.113883.2.4.3.11.999.1",
        "base": "https://api.ons.io",
        "credentials": {"secret": "nedap-ons-te1000-certificate"}
    }]);
    match Document::read(write.kind, &write.document)? {
        Document::Webhook(webhook) => Ok(*webhook),
        _ => Err("the template is a webhook".into()),
    }
}

/// Returns the headers of a notification signed with `signature`.
fn signed(signature: &str) -> Result<HeaderMap, Box<dyn Error>> {
    let mut headers = HeaderMap::new();
    headers.insert("X-Signature-SHA512", signature.parse()?);
    Ok(headers)
}

#[test]
fn the_template_is_an_inert_webhook_that_declares_its_resolution() -> Result<(), Box<dyn Error>> {
    let write = template()?;
    assert_eq!(Kind::Webhook, write.kind);
    assert!(
        !write.enabled,
        "the template is inert until an operator enables it"
    );
    let webhook = webhook()?;
    let resolve = webhook.resolve.as_ref().ok_or("a resolution")?;
    assert_eq!(Some(404), resolve.removed_on);
    assert_eq!(4, webhook.limits.concurrency);
    let document = Document::Webhook(Box::new(webhook.clone()));
    assert_eq!(
        vec![
            "nedap-ons-connector-secret",
            "nedap-ons-environment-certificate",
            "nedap-ons-te1000-certificate"
        ],
        document.secrets().into_iter().collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn the_java_sample_verifies_and_a_wrong_signature_does_not() -> Result<(), Box<dyn Error>> {
    let webhook = webhook()?;
    let keys = [Key::new(b"SuperSecret".to_vec())];
    let headers = signed(SAMPLE_SIGNATURE)?;
    let request = Request {
        headers: &headers,
        body: SAMPLE.as_bytes(),
        peer: None,
        now: 0,
    };
    webhook.verify.verify(&request, &keys)?;
    let wrong = signed(&SAMPLE_SIGNATURE.replace('a', "b"))?;
    assert_eq!(
        Err(Refused::Mismatch),
        webhook.verify.verify(
            &Request {
                headers: &wrong,
                ..request
            },
            &keys
        )
    );
    Ok(())
}

#[test]
fn the_notification_model_reads_and_the_nop_pair_is_a_probe() -> Result<(), Box<dyn Error>> {
    let webhook = webhook()?;
    let headers = HeaderMap::new();
    let notification = |model: &str, event: &str| {
        json!({
            "customerCode": "TE1002", "modelType": model, "eventType": event,
            "id": 1234, "timestamp": "2024-08-22T10:08:11+02:00", "amountOfRetries": 0
        })
    };
    for (event, change) in [
        ("CREATE", Change::Upsert),
        ("UPDATE", Change::Upsert),
        ("DELETE", Change::Remove),
    ] {
        let Reading::Store(read) = webhook
            .events
            .read(&headers, &notification("client", event))?
        else {
            return Err(format!("a client {event} is stored").into());
        };
        assert_eq!(change, read.change);
        assert_eq!(Some("TE1002"), read.tenant.as_deref());
        assert_eq!("1234", read.entity);
    }
    assert_eq!(
        Reading::Answer(Answered::Probe),
        webhook
            .events
            .read(&headers, &notification("client", "NOP"))?
    );
    for (model, event) in [
        ("client_address", "UPDATE"),
        ("care_tech_order_created", "CUSTOM"),
    ] {
        assert_eq!(
            Reading::Answer(Answered::Ignored),
            webhook.events.read(&headers, &notification(model, event))?
        );
    }
    let update = webhook
        .events
        .read(&headers, &notification("client", "UPDATE"))?;
    let create = webhook
        .events
        .read(&headers, &notification("client", "CREATE"))?;
    let (Reading::Store(update), Reading::Store(create)) = (update, create) else {
        return Err("both are stored".into());
    };
    assert_ne!(
        update.dedupe_key, create.dedupe_key,
        "an UPDATE before its CREATE is a second event of the same work item"
    );
    assert_eq!(update.entity, create.entity);
    Ok(())
}

/// Returns the template's map and logical models.
pub(crate) fn set() -> Result<Set, Box<dyn Error>> {
    let mut set = Set::default();
    set.maps.push(parse(&read(
        "inbound-templates/nedap-ons/client-patient.map",
    )?)?);
    for model in ["client", "bsn"] {
        let json: serde_json::Value =
            serde_json::from_str(&read(&format!("inbound-templates/nedap-ons/{model}.json"))?)?;
        set.logical.add(&Value::from_serde_json(json))?;
    }
    Ok(set)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_template_map_renders_a_client_and_its_bsn() -> Result<(), Box<dyn Error>> {
    let concept_map: serde_json::Value = serde_json::from_str(&read(
        "terminology/nedap-ons-values/ConceptMap-gender.json",
    )?)?;
    let gender_map = concept_map["url"].as_str().ok_or("a ConceptMap URL")?;
    assert_eq!(
        json!([{"code": "female", "equivalence": "equivalent"}]),
        concept_map["group"][0]["element"][1]["target"]
    );
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .and(matchers::body_string_contains(gender_map))
        .respond_with(terminology::translate(
            "equivalent",
            "http://hl7.org/fhir/administrative-gender",
            "female",
            "Female",
        ))
        .mount(&server)
        .await;
    let config = ferrobridge_term::config::Config::new(
        format!("{}/r4", server.uri()).parse()?,
        ferrobridge_term::config::WireVersion::R4,
    )
    .with_timeout(Duration::from_secs(5));
    let translator = Terminology::new(
        ferrobridge_term::client::Client::new(config)?,
        tokio::runtime::Handle::current(),
    );
    let set = set()?;
    let url = webhook()?.resolve.ok_or("a resolution")?.map;
    let engine = Arc::new(
        Engine::new(
            set.maps,
            Known {
                logical: set.logical,
            },
        )
        .with_translator(Arc::new(translator)),
    );
    let client = Value::from_serde_json(json!({
        "id": 4711, "firstName": "Testa", "lastName": "Voorbeeld",
        "dateOfBirth": "1950-02-03", "gender": "F"
    }));
    let bsn = Value::from_serde_json(json!({"number": "999990019", "idVerified": true}));
    let output = engine
        .transform_blocking(url, vec![client, bsn], String::from("Patient"))
        .await?;
    assert_eq!(
        json!({
            "resourceType": "Patient",
            "identifier": [{"system": "http://fhir.nl/fhir/NamingSystem/bsn", "value": "999990019"}],
            "name": [{"family": "Voorbeeld", "given": ["Testa"]}],
            "birthDate": "1950-02-03",
            "gender": "female"
        }),
        serde_json::to_value(&output.target)?
    );
    Ok(())
}
