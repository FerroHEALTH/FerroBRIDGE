// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two HL7 v2 senders committing under the one bridge client into the
//! reference CDR as a node that checks facilities.
//!
//! The CDR declares the bridge as its relay client, with a provider, two
//! facilities and the face's template. Each composition must name one of
//! those facilities by an identifier with its issuer
//! (`EVENT_CONTEXT.health_care_facility`), and its composer must name the
//! professional by an identifier (RM EHR IM, `COMPOSITION.composer`, "The
//! person primarily responsible for the content of the Composition"), or
//! the CDR refuses the commit with `422`. The bridge reaches it with its own
//! client credentials token (RFC 6749 §4.4), signed by the test issuer the
//! CDR trusts. Every message, identifier and name is synthetic.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::config::hl7v2::{Sender, Subject};
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::containers::Cdr;
use ferrobridge_testkit::facility_cdr::{Facility, RelayClient, relay};
use ferrobridge_testkit::mllp_tls::ClientIdentity;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use ferrohealth_smart::client::{ClientCredentials, Endpoint};
use secrecy::SecretString;
use tokio::io::AsyncWriteExt;

use super::{FIXTURES, NORTHLAB, NORTHLAB_SUBJECT, PKI, Serving, face_settings, frame, read_ack};
use crate::facade::settings;

/// The issuer of the bridge's client credentials tokens.
const MACHINES: &str = "https://machines.ferrobridge.test";

/// The audience the CDR accepts the bridge's tokens for.
const AUDIENCE: &str = "ferroehr-machines";

/// The bridge's client id.
const CLIENT: &str = "ferrobridge";

/// The issuer of the facility identifiers the CDR checks.
const FACILITIES: &str = "urn:oid:2.999.1";

/// The scopes the bridge's token carries: every composition, the templates
/// and every query (SMART on openEHR, `system/` resource scopes).
const SCOPES: &str = "system/composition-*.cruds system/template-*.r system/aql-*.s";

/// The DNS name the sending facility SOUTHLAB's client certificate carries.
const SOUTHLAB_SUBJECT: &str = "southlab.example.org";

/// A result from `sender` for its patient `subject`, at the visit location
/// whose facility is `facility`, ordered by `clinician`, whose assigning
/// authority is the sender.
fn result(
    sender: &str,
    control_id: &str,
    subject: &str,
    facility: &str,
    clinician: &str,
) -> Vec<u8> {
    let segments = [
        format!(
            "MSH|^~\\&|LAB|{sender}|EHR|SOUTHCLINIC|20260925143000+0200||ORU^R01^ORU_R01|{control_id}|P|2.5.1"
        ),
        format!("PID|1||{subject}^^^{sender}^MR||Synthetic^Sam^^^^^L||19800101|M"),
        format!("PV1|1|O|ROOM-1^^^{facility}"),
        String::from("ORC|RE|PLC-1|FIL-1"),
        format!(
            "OBR|1|PLC-1|FIL-1|2345-7^Glucose^LN|||20260925120000+0200|||||||||{clinician}^Doctor^Dana^^^^^^{sender}"
        ),
        String::from(
            "OBX|1|NM|2345-7^Glucose^LN^^^^^^Synthetic glucose||5.4|mmol/L^mmol/L^UCUM|3.9-5.8|N|||F|||20260925130000+0200",
        ),
    ];
    let mut bytes = Vec::new();
    for segment in segments {
        bytes.extend_from_slice(segment.as_bytes());
        bytes.push(b'\r');
    }
    bytes
}

/// Sends `message` to `serving` on a new connection as `identity` and reads
/// the answer.
async fn send_as(
    serving: &Serving,
    identity: &ClientIdentity,
    message: &[u8],
) -> Result<String, Box<dyn Error>> {
    let connector = PKI.connector(Some(identity))?;
    let mut stream = ferrobridge_testkit::mllp_tls::connect(serving.address, &connector).await?;
    stream.write_all(&frame(message)).await?;
    read_ack(&mut stream).await
}

/// The MSA segment of `ack`.
fn msa(ack: &str) -> Option<&str> {
    ack.split('\r').find(|segment| segment.starts_with("MSA|"))
}

/// Signs the bridge's client credentials token: an RFC 9068 access token
/// whose subject is the client, which marks it app-only.
fn token(provider: &Provider) -> Result<String, Box<dyn Error>> {
    let now = jiff::Timestamp::now().as_second();
    let mut header = Provider::header();
    header.typ = Some(String::from("at+jwt"));
    Ok(provider.sign(
        &header,
        &serde_json::json!({
            "iss": MACHINES,
            "aud": AUDIENCE,
            "sub": CLIENT,
            "client_id": CLIENT,
            "iat": now,
            "exp": now.saturating_add(3600),
            "jti": "synthetic-bridge-access",
            "scope": SCOPES,
        }),
    )?)
}

/// Returns the facility name of every composition the CDR holds, sorted.
async fn stored_facilities(base_url: &str, token: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let answer: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/query/aql"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "q": "SELECT c/context/health_care_facility/name FROM EHR e CONTAINS COMPOSITION c"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut names: Vec<String> = answer["rows"]
        .as_array()
        .ok_or("the result set carries rows")?
        .iter()
        .filter_map(|row| row[0].as_str().map(str::to_owned))
        .collect();
    names.sort();
    Ok(names)
}

/// Starts the CDR as a node that declares the bridge as its relay client
/// for the facilities `NORTHWARD` and `SOUTHWARD`, trusting `provider`'s key.
async fn node(provider: &Provider) -> Result<Cdr, Box<dyn Error>> {
    let facility = |id: &str| Facility {
        issuer: String::from(FACILITIES),
        id: String::from(id),
    };
    Ok(relay(&RelayClient {
        issuer: String::from(MACHINES),
        audience: String::from(AUDIENCE),
        keys: provider.jwks(),
        client_id: String::from(CLIENT),
        provider: Facility {
            issuer: String::from("urn:oid:2.999.7"),
            id: String::from("clinic"),
        },
        facilities: vec![facility("NORTHWARD"), facility("SOUTHWARD")],
        templates: vec![(
            String::from("ferrobridge.diagnose.v1"),
            ferrobridge_testkit::fixtures::DIAGNOSE_OPT
                .as_bytes()
                .to_vec(),
        )],
    })
    .await?)
}

/// Starts the face over `cdr`, reached with the client credentials grant at
/// `provider`, accepting NORTHLAB and SOUTHLAB on their own certificates.
async fn face(cdr: &Cdr, provider: &Provider) -> Result<Serving, Box<dyn Error>> {
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?).with_oauth(
        ClientCredentials::new(
            Endpoint::Token(provider.token_endpoint().parse()?),
            String::from(CLIENT),
            SecretString::from("synthetic-secret"),
        ),
    ))?;
    let set = programs::read_set(Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let mut facade_settings = settings();
    facade_settings.ehr_policy = Policy::CreateOnFirstWrite;
    let facade = Arc::new(Facade::new(
        compiled,
        Arc::new(MemoryStore::default()),
        client,
        facade_settings,
    ));
    let mut lane = face_settings(true);
    lane.facility_issuer = Some(String::from(FACILITIES));
    lane.senders = ["NORTHLAB", "SOUTHLAB", "NORTHWARD", "SOUTHWARD", "EASTWARD"]
        .into_iter()
        .map(|name| Sender::Namespace(String::from(name)))
        .collect();
    lane.subjects = vec![
        Subject {
            sender: Sender::Namespace(String::from("NORTHLAB")),
            name: String::from(NORTHLAB_SUBJECT),
        },
        Subject {
            sender: Sender::Namespace(String::from("SOUTHLAB")),
            name: String::from(SOUTHLAB_SUBJECT),
        },
    ];
    Serving::start(facade, lane, None).await
}

#[tokio::test]
async fn two_senders_commit_each_its_facility_under_the_one_bridge_client()
-> Result<(), Box<dyn Error>> {
    if !ferrobridge_testkit::containers::e2e_enabled() {
        return Ok(());
    }
    let provider = Provider::start().await?;
    let access = token(&provider)?;
    provider
        .answer_grant(
            "grant_type=client_credentials",
            &TokenAnswer {
                expires_in: Some(3600),
                ..TokenAnswer::bearer(&access)
            },
        )
        .await?;
    let cdr = node(&provider).await?;
    let serving = face(&cdr, &provider).await?;
    let southlab = PKI.client(SOUTHLAB_SUBJECT)?;

    let north = send_as(
        &serving,
        &NORTHLAB,
        &result(
            "NORTHLAB",
            "MSG-RELAY-N1",
            "synthetic-north-0001",
            "NORTHWARD",
            "DR-N",
        ),
    )
    .await?;
    assert_eq!(Some("MSA|AA|MSG-RELAY-N1"), msa(&north), "{north}");
    let south = send_as(
        &serving,
        &southlab,
        &result(
            "SOUTHLAB",
            "MSG-RELAY-S1",
            "synthetic-south-0001",
            "SOUTHWARD",
            "DR-S",
        ),
    )
    .await?;
    assert_eq!(Some("MSA|AA|MSG-RELAY-S1"), msa(&south), "{south}");
    assert_eq!(
        vec![String::from("NORTHWARD"), String::from("SOUTHWARD")],
        stored_facilities(cdr.base_url(), &access).await?,
        "each sender's composition names its own facility"
    );

    let east = send_as(
        &serving,
        &NORTHLAB,
        &result(
            "NORTHLAB",
            "MSG-RELAY-E1",
            "synthetic-north-0001",
            "EASTWARD",
            "DR-N",
        ),
    )
    .await?;
    assert_ne!(
        Some("MSA|AA|MSG-RELAY-E1"),
        msa(&east),
        "the CDR refuses a facility its relay client does not commit for: {east}"
    );
    assert!(
        east.contains("422") && east.contains("health_care_facility"),
        "the refusal names the facility check: {east}"
    );
    assert_eq!(
        vec![String::from("NORTHWARD"), String::from("SOUTHWARD")],
        stored_facilities(cdr.base_url(), &access).await?,
        "nothing is committed for the refused facility"
    );
    serving.stop().await
}
