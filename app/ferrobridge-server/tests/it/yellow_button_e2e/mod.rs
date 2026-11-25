// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Yellow Button API journey behind the `FERROBRIDGE_E2E` gate, the
//! xShare Bronze evidence (#2018): a patient signs in through FerroSMART
//! with a standalone patient launch (SMART App Launch 2.2.0), downloads
//! their patient summary (`Patient/[id]/$summary`, HL7 IPS 2.0.1), shares it
//! as a SMART Health Link with a passcode, a receiver opens it once
//! (SMART Health Cards and Links 1.0.0), the patient revokes it, and each
//! step leaves its BALP `AuditEvent` (IHE BALP 1.1.4) at the Audit Record
//! Repository. The bridge serves in process on a real listener, because
//! FerroSMART reads the patient's own record at the bridge during the launch.

mod login;
mod record;
mod stack;

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::boot::serve_settings;
use ferrobridge_server::retry::Backoff;
use ferrobridge_testkit::containers;
use http::StatusCode;
use stack::Onward;
use stack::Stack;

use crate::facade::shl::decode;
use crate::facade::shl::kinds;
use crate::facade::shl::open;
use crate::facade::summary::emit;

/// The bridge's usage report, off in a test.
const USAGE_REPORT_OFF: &str = "[usage_report]\nenabled = false\n";

/// How long the bridge may take to serve after its start.
const READINESS: Duration = Duration::from_secs(180);

/// The passcode the patient protects the link with.
const PASSCODE: &str = "journey passcode";

/// The recipient the receiver names itself as.
const RECIPIENT: &str = "Dr. Receiver";

#[tokio::test(flavor = "multi_thread")]
async fn a_patient_downloads_shares_and_revokes_their_summary_through_ferrosmart()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    Box::pin(run(Onward::Off)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_journey_holds_with_the_cdr_read_under_the_patient_s_exchanged_token()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    Box::pin(run(Onward::On)).await
}

/// Starts the platform with `onward`, serves the bridge and walks the
/// journey, then stops the bridge.
async fn run(onward: Onward) -> Result<(), Box<dyn Error>> {
    let mut stack = Stack::start(onward).await?;
    let listener = stack.listener.take().ok_or("the bridge's listener")?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = Box::pin(serve_settings(
        &stack.settings,
        move || async move { Ok(listener) },
        Backoff::default(),
        async move {
            let _closed = stopped.await.is_err();
        },
    ));
    let walking = async {
        let walked = journey(&stack).await;
        drop(stop);
        walked
    };
    let (served, walked) = tokio::join!(serving, walking);
    if let Err(error) = walked {
        return Err(format!(
            "{error}\nthe bridge: {:?}\nFerroSMART's log:\n{}",
            served.err().map(|error| format!("{error:#}")),
            stack.smart.log().await
        )
        .into());
    }
    served.map_err(|error| format!("{error:#}"))?;
    Ok(())
}

/// One HTTP answer: the status and the JSON body, `Null` when it is none.
type Answer = (StatusCode, serde_json::Value);

/// Sends `request` and returns its answer.
async fn send(request: reqwest::RequestBuilder) -> Result<Answer, Box<dyn Error>> {
    let response = request.send().await?;
    let status = StatusCode::from_u16(response.status().as_u16())?;
    let text = response.text().await?;
    Ok((
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
    ))
}

/// Waits until the bridge's readiness answers `200`.
async fn ready(client: &reqwest::Client, origin: &str) -> Result<(), Box<dyn Error>> {
    let deadline = tokio::time::Instant::now() + READINESS;
    loop {
        let answer = client
            .get(format!("{origin}/health/readiness"))
            .timeout(Duration::from_secs(2))
            .send()
            .await;
        if let Ok(response) = answer
            && response.status() == StatusCode::OK
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the bridge was not ready within {}s", READINESS.as_secs()).into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Asserts that `answer` is a refusal of `status` carrying an
/// `OperationOutcome`.
fn refused(what: &str, status: StatusCode, (got, body): &Answer) {
    assert_eq!(status, *got, "{what}: {body}");
    assert_eq!("OperationOutcome", body["resourceType"], "{what}: {body}");
}

/// The journey's view of the bridge: its client, its routes and the two
/// patients' tokens.
struct Walk {
    /// The client.
    client: reqwest::Client,
    /// The first patient's summary route.
    summary: String,
    /// The links' route.
    links: String,
    /// The first patient's access token.
    bearer: String,
    /// The second patient's access token.
    their_bearer: String,
}

/// Walks the journey over `stack`, whose bridge is starting.
async fn journey(stack: &Stack) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    ready(&client, &stack.origin).await?;
    let facade = stack.facade();
    let patient = &stack.patients.patient;
    let summary = format!("{facade}/Patient/{patient}/$summary");
    let links = format!("{}/shl", stack.origin);

    // NOTE: SMART App Launch 2.2.0, a request without a token is refused.
    for (what, request) in [
        ("the summary", client.get(&summary)),
        (
            "a link's create",
            client.post(&links).json(&serde_json::json!({})),
        ),
        ("the links' list", client.get(&links)),
        ("a link's revoke", client.delete(format!("{links}/unknown"))),
    ] {
        refused(what, StatusCode::UNAUTHORIZED, &send(request).await?);
    }

    let token = login::launch(
        stack.smart.issuer(),
        &stack.upstream,
        &facade,
        patient,
        "journey-patient",
    )
    .await?;
    assert_eq!(serde_json::json!(patient), token["patient"], "{token}");
    let theirs = login::launch(
        stack.smart.issuer(),
        &stack.upstream,
        &facade,
        &stack.patients.other,
        "journey-other",
    )
    .await?;
    let walk = Walk {
        client,
        summary,
        links,
        bearer: token["access_token"]
            .as_str()
            .ok_or("an access token")?
            .to_owned(),
        their_bearer: theirs["access_token"]
            .as_str()
            .ok_or("an access token")?
            .to_owned(),
    };
    let served = walk.download().await?;
    let shared = walk.share(&served).await?;
    let revoked = walk.revoke().await?;
    audited(stack, &shared, &revoked).await
}

impl Walk {
    /// Downloads the summary in both forms, sees another patient's token
    /// refused, and returns the IPS document.
    async fn download(&self) -> Result<serde_json::Value, Box<dyn Error>> {
        let client = &self.client;
        let (status, served) = send(client.get(&self.summary).bearer_auth(&self.bearer)).await?;
        assert_eq!(StatusCode::OK, status, "{served}");
        assert_eq!("document", served["type"], "{served}");
        emit("journey-summary-ips", &served)?;
        let (status, european) = send(
            client
                .get(format!(
                    "{}?profile={}",
                    self.summary,
                    eehrxf::crosswalk::patient_summary::EPS_COMPOSITION
                ))
                .bearer_auth(&self.bearer),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{european}");
        emit("journey-summary-eps", &european)?;
        refused(
            "another patient's token",
            StatusCode::NOT_FOUND,
            &send(client.get(&self.summary).bearer_auth(&self.their_bearer)).await?,
        );
        Ok(served)
    }

    /// Shares the summary as a link with a passcode, opens it once as a
    /// receiver, checks the file is the document `served`, sees a second use
    /// refused, and returns the link's handle.
    async fn share(&self, served: &serde_json::Value) -> Result<String, Box<dyn Error>> {
        let client = &self.client;
        let (status, created) = send(
            client
                .post(&self.links)
                .bearer_auth(&self.bearer)
                .json(&serde_json::json!({"passcode": PASSCODE, "label": "For my doctor"})),
        )
        .await?;
        assert_eq!(StatusCode::CREATED, status, "{created}");
        let payload = decode(created["shlink"].as_str().ok_or("the shlink")?)?;
        let url = payload["url"].as_str().ok_or("the url")?;
        let receiver = serde_json::json!({"recipient": RECIPIENT, "passcode": PASSCODE});
        let (status, manifest) = send(client.post(url).json(&receiver)).await?;
        assert_eq!(StatusCode::OK, status, "{manifest}");
        let location = manifest["files"][0]["location"]
            .as_str()
            .ok_or_else(|| format!("a file location: {manifest}"))?;
        let jwe = client.get(location).send().await?.text().await?;
        let (_, document) = open(payload["key"].as_str().ok_or("the key")?, &jwe)?;
        assert_eq!(
            served["meta"]["profile"], document["meta"]["profile"],
            "the file is the document the download serves"
        );
        assert_eq!(kinds(served), kinds(&document), "the same entries");
        assert_eq!(served["entry"][1], document["entry"][1], "the same patient");
        emit("journey-shl-ips", &document)?;
        refused(
            "a second use of the link",
            StatusCode::NOT_FOUND,
            &send(client.post(url).json(&receiver)).await?,
        );
        Ok(created["handle"].as_str().ok_or("the handle")?.to_owned())
    }

    /// Creates a second link, sees another patient's revoke refused, revokes
    /// it, sees its manifest answer `404`, and returns its handle.
    async fn revoke(&self) -> Result<String, Box<dyn Error>> {
        let client = &self.client;
        let (status, second) = send(
            client
                .post(&self.links)
                .bearer_auth(&self.bearer)
                .json(&serde_json::json!({"passcode": PASSCODE, "answers": 2})),
        )
        .await?;
        assert_eq!(StatusCode::CREATED, status, "{second}");
        let handle = second["handle"].as_str().ok_or("the handle")?;
        let revoke = format!("{}/{handle}", self.links);
        refused(
            "another patient's revoke",
            StatusCode::NOT_FOUND,
            &send(client.delete(&revoke).bearer_auth(&self.their_bearer)).await?,
        );
        let (status, body) = send(client.delete(&revoke).bearer_auth(&self.bearer)).await?;
        assert_eq!(StatusCode::NO_CONTENT, status, "{body}");
        let revoked = decode(second["shlink"].as_str().ok_or("the shlink")?)?;
        let receiver = serde_json::json!({"recipient": RECIPIENT, "passcode": PASSCODE});
        refused(
            "a revoked link's manifest",
            StatusCode::NOT_FOUND,
            &send(
                client
                    .post(revoked["url"].as_str().ok_or("the url")?)
                    .json(&receiver),
            )
            .await?,
        );
        Ok(handle.to_owned())
    }
}

/// Checks the BALP records of the journey at the repository: the summary's
/// read naming the patient, their EHR and the category, the link's create, the
/// receiver's answer naming the recipient, and the revoke.
async fn audited(stack: &Stack, shared: &str, revoked: &str) -> Result<(), Box<dyn Error>> {
    let patient = stack.patients.patient.as_str();
    let ehr_id = stack.patients.ehr_id.as_str();
    let wanted = |records: &[serde_json::Value]| {
        let text = |record: &serde_json::Value| record.to_string();
        let has = |action: &str, named: &[&str]| {
            records.iter().any(|record| {
                record["action"] == action && named.iter().all(|name| text(record).contains(name))
            })
        };
        has("R", &[patient, ehr_id, "Patient-Summaries"])
            && has("C", &[shared])
            && has("R", &[shared, RECIPIENT])
            && has("D", &[revoked])
    };
    let mut records = Vec::new();
    for _ in 0..240 {
        records = stack
            .arr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/AuditEvent")
            .filter_map(|request| serde_json::from_slice(&request.body).ok())
            .collect();
        if wanted(&records) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "the repository lacks a record of the journey: {}",
        serde_json::to_string(&records)?
    )
    .into())
}
