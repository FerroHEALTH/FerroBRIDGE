// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Consent` on the facade and the IHE BPPC and APPC operations, over a
//! `wiremock` CDR that serves the `EHR_ACCESS` it was last sent.
//!
//! Each vendored document goes in through the facade, the CDR receives the
//! `EHR_ACCESS` version, the stub then serves that version, and the read,
//! the search or the export answers the directives back; a document the
//! mapping refuses answers `422` at the element the library names.

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use base64::Engine as _;
use ferrobridge_server::consent::exchange::Settings as ExchangeSettings;
use ferrobridge_server::consent::fhir::Release;
use ferrobridge_server::consent::fhir::import::ImportContext;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs::Programs;
use http::Request;
use http::StatusCode;
use http::header;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_rm::draft::access_control::access_control::access_directive::AccessDirective;
use openehr_rm::draft::access_control::access_control::access_directive_settings::AccessDirectiveSettings;
use openehr_rm::draft::access_control::access_control::access_policy::AccessPolicy;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use serde_json::Value;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::app_over;
use super::call;
use super::client;
use super::handle;
use super::settings;
use super::stub::mount_ehr;

/// The CDR's extensions root, under the stub, as the reference CDR serves it.
const EXTENSIONS: &str = "/ferroehr/rest/extensions/v1";

/// The first `EHR_ACCESS` version the stub serves.
const VERSION_1: &str = "0a1b2c3d-0000-4000-8000-00000000000b::ferrobridge.test::1";

/// The version the stub serves once a commit came in.
const VERSION_2: &str = "0a1b2c3d-0000-4000-8000-00000000000b::ferrobridge.test::2";

/// The contribution every commit answers.
const CONTRIBUTION: &str = "6c7e5d4b-0000-4000-8000-000000000006";

/// An `EHR_ACCESS` with no settings, the state of a new EHR.
const EMPTY_ACCESS: &str = r#"{
  "_type": "EHR_ACCESS",
  "name": {"_type": "DV_TEXT", "value": "EHR access"},
  "archetype_node_id": "openEHR-EHR-EHR_ACCESS.access_directives.v1.0.0"
}"#;

/// One case: the stub CDR and the application over it.
struct Exchange {
    /// The stub CDR.
    cdr: MockServer,
    /// The application under test.
    app: Router,
    /// The catalogue the stub's `ACCESS_POLICY` holds.
    catalogue: Vec<AccessDirective>,
}

/// Returns the synthetic `ACCESS_POLICY` holding `catalogue`.
fn policy(catalogue: &[AccessDirective]) -> Result<Value, Box<dyn Error>> {
    let policy = AccessPolicy {
        name: ferrobridge_server::consent::rm::dv_text("access policy"),
        archetype_node_id: String::from("openEHR-EHR-ACCESS_POLICY.access_policy.v1.0.0"),
        uid: None,
        links: None,
        archetype_details: None,
        feeder_audit: None,
        controller: PartyRef {
            namespace: String::from("demographic"),
            r#type: String::from("ORGANISATION"),
            id: ObjectId::HierObjectId(HierObjectId::new("5c6e4d3a-0000-4000-8000-000000000005")?),
        },
        classifications: None,
        care_relationship_required: false,
        relationship_grace: None,
        purpose_required: false,
        catalogue: ferrobridge_server::consent::rm::list(catalogue.to_vec()),
        log_restrictions: None,
        details: None,
    };
    Ok(openehr_its::json::to_canonical_value(&policy))
}

impl Exchange {
    /// Returns a case whose `ACCESS_POLICY` holds `catalogue`.
    async fn new(catalogue: Vec<AccessDirective>) -> Result<Self, Box<dyn Error>> {
        let cdr = MockServer::start().await;
        let store = Arc::new(MemoryStore::default());
        let facade = Facade::new(
            Programs::default(),
            handle(&store),
            client(&cdr),
            settings(),
        )
        .with_consent(Some(ExchangeSettings {
            extensions_root: url::Url::parse(&format!("{}{EXTENSIONS}", cdr.uri()))?,
            default_access: AccessDefault::Restricted,
        }));
        let exchange = Self {
            app: app_over(&Arc::new(facade)),
            cdr,
            catalogue,
        };
        exchange
            .serve(&serde_json::from_str(EMPTY_ACCESS)?, VERSION_1)
            .await?;
        Ok(exchange)
    }

    /// Mounts the CDR serving `access` as `version`, with the EHR, the
    /// policy and the commit.
    async fn serve(&self, access: &Value, version: &str) -> Result<(), Box<dyn Error>> {
        self.cdr.reset().await;
        mount_ehr(&self.cdr).await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!(
                "{EXTENSIONS}/ehr/{EHR_ID}/ehr_access"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", format!("\"{version}\"").as_str())
                    .set_body_raw(access.to_string(), "application/json"),
            )
            .mount(&self.cdr)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("{EXTENSIONS}/admin/access_policy")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(policy(&self.catalogue)?.to_string(), "application/json"),
            )
            .mount(&self.cdr)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path(format!("/ehr/{EHR_ID}/contribution")))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("ETag", format!("\"{CONTRIBUTION}\"").as_str()),
            )
            .mount(&self.cdr)
            .await;
        Ok(())
    }

    /// Returns the `EHR_ACCESS` the last commit carried, its preceding
    /// version checked, and serves it as the next version.
    async fn committed(&self) -> Result<Vec<AccessDirective>, Box<dyn Error>> {
        let requests = self.cdr.received_requests().await.ok_or("recorded")?;
        let sent = requests
            .iter()
            .rev()
            .find(|request| request.method.as_str() == "POST")
            .ok_or("a commit reached the CDR")?;
        let contribution: Value = serde_json::from_slice(&sent.body)?;
        assert_eq!(
            contribution.pointer("/versions/0/preceding_version_uid/value"),
            Some(&Value::from(VERSION_1)),
            "the commit names the version it read"
        );
        let access = contribution
            .pointer("/versions/0/data")
            .ok_or("the version carries the EHR_ACCESS")?
            .clone();
        self.serve(&access, VERSION_2).await?;
        let access: openehr_rm::v1_2::ehr::ehr_access::EhrAccess =
            openehr_its::json::from_canonical_json(&access.to_string())?;
        let settings = access
            .settings
            .as_ref()
            .map(AccessDirectiveSettings::try_from)
            .transpose()?
            .ok_or("the EHR_ACCESS carries settings")?;
        Ok(settings.directives.map(Vec::from).unwrap_or_default())
    }

    /// Sends `request` and reads the answer.
    async fn send(&self, request: Request<Body>) -> Result<(StatusCode, Value), Box<dyn Error>> {
        call(self.app.clone(), request).await
    }
}

/// The media type of a `Consent` of `release`.
fn media(release: Release) -> &'static str {
    match release {
        Release::R4 => "application/fhir+json",
        Release::R5 => "application/fhir+json; fhirVersion=5.0",
    }
}

/// Returns the vendored examples of `release`.
fn examples(release: Release) -> Result<Vec<String>, Box<dyn Error>> {
    let dir = match release {
        Release::R4 => "fhir-consent-r4",
        Release::R5 => "fhir-consent-r5",
    };
    let root = format!("{}/../../docs/specs/{dir}", env!("CARGO_MANIFEST_DIR"));
    let mut names = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            name.starts_with("consent-example")
                && std::path::Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

/// The verdict of the library on `body`, with no terminology server.
fn verdict(release: Release, body: &[u8]) -> Result<usize, String> {
    let context = ImportContext {
        catalogue: &[],
        default_access: AccessDefault::Restricted,
        operations: &|_| None,
    };
    ferrobridge_server::consent::fhir::read(release, body)
        .and_then(|document| {
            ferrobridge_server::consent::fhir::import::import(release, &document, &context)
        })
        .map(|directives| directives.len())
        .map_err(|refused| refused.element)
}

/// Every directive of `directives` without the `uid` the exchange stamps
/// and the `instructed` a BPPC document carries as its own time.
///
/// An exported acknowledgement is authored when it is exported (ITI TF-3
/// 5.1.2.2, `effectiveTime` and `author/time`), so its time is the export's.
fn unstamped(directives: &[AccessDirective]) -> Vec<AccessDirective> {
    directives
        .iter()
        .cloned()
        .map(|mut directive| {
            directive.uid = None;
            directive.instructed = None;
            directive
        })
        .collect()
}

/// Posts every vendored example of `release`, and checks each against the
/// library's verdict and, when created, its read back through the CDR.
async fn round_trip(release: Release) -> Result<(), Box<dyn Error>> {
    let mut created = 0_usize;
    for name in examples(release)? {
        let body = crate::consent::fhir::vendored(release, &name);
        let expected = verdict(release, &body);
        let exchange = Exchange::new(Vec::new()).await?;
        let (status, answer) = exchange
            .send(
                Request::post("/fhir/Consent")
                    .header(header::CONTENT_TYPE, media(release))
                    .header(header::ACCEPT, media(release))
                    .header("prefer", "return=OperationOutcome")
                    .body(Body::from(body.clone()))?,
            )
            .await?;
        match expected {
            Err(element) => {
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {answer}");
                let diagnostics = answer["issue"][0]["diagnostics"]
                    .as_str()
                    .unwrap_or_default();
                assert!(diagnostics.contains(&element), "{name}: {diagnostics}");
            }
            Ok(count) => {
                assert_eq!(status, StatusCode::CREATED, "{name}: {answer}");
                created += 1;
                let committed = exchange.committed().await?;
                assert_eq!(committed.len(), count, "{name}");
                let patient = serde_json::from_slice::<Value>(&body)?
                    .pointer(match release {
                        Release::R4 => "/patient/reference",
                        Release::R5 => "/subject/reference",
                    })
                    .and_then(Value::as_str)
                    .ok_or("the example names its patient")?
                    .to_owned();
                let (status, bundle) = exchange
                    .send(
                        Request::get(format!("/fhir/Consent?patient={patient}"))
                            .header(header::ACCEPT, media(release))
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(status, StatusCode::OK, "{name}: {bundle}");
                assert_eq!(bundle["total"], Value::from(count), "{name}");
                let mut back = Vec::new();
                for entry in bundle["entry"].as_array().ok_or("entries")? {
                    let consent = serde_json::to_vec(&entry["resource"])?;
                    let document = ferrobridge_server::consent::fhir::read(release, &consent)?;
                    back.extend(ferrobridge_server::consent::fhir::import::import(
                        release,
                        &document,
                        &ImportContext {
                            catalogue: &[],
                            default_access: AccessDefault::Restricted,
                            operations: &|_| None,
                        },
                    )?);
                }
                assert_eq!(back, committed, "{name} reads back as committed");
            }
        }
    }
    assert_ne!(created, 0, "at least one {release:?} example is created");
    Ok(())
}

#[tokio::test]
async fn every_vendored_r4_example_is_created_and_reads_back_or_is_refused_at_its_element()
-> Result<(), Box<dyn Error>> {
    round_trip(Release::R4).await
}

#[tokio::test]
async fn every_vendored_r5_example_is_created_and_reads_back_or_is_refused_at_its_element()
-> Result<(), Box<dyn Error>> {
    round_trip(Release::R5).await
}

#[tokio::test]
async fn a_created_consent_is_read_by_the_id_its_location_names() -> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    let body = crate::consent::fhir::vendored(Release::R4, "consent-example-notTime.json");
    let response = super::raw(
        exchange.app.clone(),
        Request::post("/fhir/Consent")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(body))?,
    )
    .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or("a Location")?
        .to_owned();
    let id = location.rsplit('/').next().ok_or("an id")?.to_owned();
    assert_eq!(id.len(), 64);
    exchange.committed().await?;
    let (status, consent) = exchange
        .send(Request::get(format!("/fhir/Consent/{id}")).body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::OK, "{consent}");
    assert_eq!(consent["id"], Value::from(id.as_str()));
    assert_eq!(consent["resourceType"], Value::from("Consent"));
    let other = format!("{}{}", id.get(..32).ok_or("an EHR part")?, "0".repeat(32));
    let (status, _) = exchange
        .send(Request::get(format!("/fhir/Consent/{other}")).body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn a_consent_naming_no_patient_is_refused() -> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    let mut body: Value = serde_json::from_slice(&crate::consent::fhir::vendored(
        Release::R4,
        "consent-example-notTime.json",
    ))?;
    body.as_object_mut().ok_or("an object")?.remove("patient");
    let (status, answer) = exchange
        .send(
            Request::post("/fhir/Consent")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(body.to_string()))?,
        )
        .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    assert_eq!(answer["issue"][0]["code"], Value::from("required"));
    Ok(())
}

#[tokio::test]
async fn a_stale_ehr_access_is_a_conflict() -> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    exchange.cdr.reset().await;
    mount_ehr(&exchange.cdr).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "{EXTENSIONS}/ehr/{EHR_ID}/ehr_access"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{VERSION_1}\"").as_str())
                .set_body_raw(EMPTY_ACCESS, "application/json"),
        )
        .mount(&exchange.cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("{EXTENSIONS}/admin/access_policy")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&exchange.cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/contribution")))
        .respond_with(ResponseTemplate::new(409))
        .mount(&exchange.cdr)
        .await;
    let (status, answer) = exchange
        .send(
            Request::post("/fhir/Consent")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(crate::consent::fhir::vendored(
                    Release::R4,
                    "consent-example-notTime.json",
                )))?,
        )
        .await?;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_search_takes_one_patient_and_nothing_else() -> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    for query in ["", "?patient=Patient/f001&status=active"] {
        let (status, answer) = exchange
            .send(Request::get(format!("/fhir/Consent{query}")).body(Body::empty())?)
            .await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {answer}");
    }
    let (status, bundle) = exchange
        .send(Request::get("/fhir/Consent?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::OK, "{bundle}");
    assert_eq!(bundle["total"], Value::from(0));
    Ok(())
}

/// Returns the documents of an export Bundle, decoded.
fn documents(bundle: &Value) -> Result<Vec<String>, Box<dyn Error>> {
    let mut out = Vec::new();
    for entry in bundle["entry"].as_array().ok_or("entries")? {
        if entry["resource"]["resourceType"] == "Binary" {
            assert_eq!(
                entry["resource"]["contentType"],
                Value::from("text/xml"),
                "a document is XML"
            );
            let data = entry["resource"]["data"].as_str().ok_or("data")?;
            out.push(String::from_utf8(
                base64::engine::general_purpose::STANDARD.decode(data)?,
            )?);
        }
    }
    Ok(out)
}

#[tokio::test]
async fn the_vendored_bppc_sample_round_trips_through_the_cdr() -> Result<(), Box<dyn Error>> {
    let catalogue = crate::consent::bppc::catalogue();
    let exchange = Exchange::new(catalogue.clone()).await?;
    let evidence = crate::consent::bppc::EVIDENCE;
    let (status, answer) = exchange
        .send(
            Request::post(format!(
                "/fhir/Consent/$bppc?patient=Patient/f001&evidence={evidence}"
            ))
            .header(header::CONTENT_TYPE, "text/xml")
            .body(Body::from(crate::consent::bppc::sample()))?,
        )
        .await?;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    let committed = exchange.committed().await?;
    assert_eq!(committed.len(), 1);
    let (status, bundle) = exchange
        .send(Request::get("/fhir/Consent/$bppc?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::OK, "{bundle}");
    let documents = documents(&bundle)?;
    assert_eq!(documents.len(), 1);
    let back = ferrobridge_server::consent::bppc::import(
        documents.first().ok_or("one document")?,
        &ferrobridge_server::consent::bppc::ImportContext {
            catalogue: &catalogue,
            evidence,
        },
    )?;
    assert_eq!(unstamped(&back), unstamped(&committed));
    Ok(())
}

#[tokio::test]
async fn a_bppc_acknowledgement_of_a_policy_outside_the_catalogue_is_refused()
-> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    let (status, answer) = exchange
        .send(
            Request::post(format!(
                "/fhir/Consent/$bppc?patient=Patient/f001&evidence={}",
                crate::consent::bppc::EVIDENCE
            ))
            .header(header::CONTENT_TYPE, "text/xml")
            .body(Body::from(crate::consent::bppc::sample()))?,
        )
        .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
    let diagnostics = answer["issue"][0]["diagnostics"]
        .as_str()
        .unwrap_or_default();
    assert!(diagnostics.contains("catalogue"), "{diagnostics}");
    Ok(())
}

#[tokio::test]
async fn the_appc_example_round_trips_through_the_cdr() -> Result<(), Box<dyn Error>> {
    let catalogue = crate::consent::appc::catalogue();
    let exchange = Exchange::new(catalogue.clone()).await?;
    let (status, answer) = exchange
        .send(
            Request::post("/fhir/Consent/$appc?patient=Patient/f001")
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(crate::consent::appc::EXAMPLE))?,
        )
        .await?;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    let committed = exchange.committed().await?;
    let (status, bundle) = exchange
        .send(Request::get("/fhir/Consent/$appc?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::OK, "{bundle}");
    let documents = documents(&bundle)?;
    let back = ferrobridge_server::consent::appc::import(
        documents.first().ok_or("one document")?,
        &ferrobridge_server::consent::appc::ImportContext {
            catalogue: &catalogue,
        },
    )?;
    assert_eq!(back, committed);
    Ok(())
}

#[tokio::test]
async fn an_ihe_document_in_another_media_type_is_refused() -> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    let (status, _) = exchange
        .send(
            Request::post("/fhir/Consent/$appc?patient=Patient/f001")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(crate::consent::appc::EXAMPLE))?,
        )
        .await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_lists_consent_only_when_it_is_served()
-> Result<(), Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    let (status, statement) = exchange
        .send(Request::get("/fhir/metadata").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::OK);
    let consent = statement["rest"][0]["resource"]
        .as_array()
        .ok_or("resources")?
        .iter()
        .find(|entry| entry["type"] == "Consent")
        .ok_or("Consent is listed")?
        .clone();
    let interactions = consent["interaction"]
        .as_array()
        .ok_or("interactions")?
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(interactions, ["create", "read", "search-type"]);
    let operations = consent["operation"]
        .as_array()
        .ok_or("operations")?
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(operations, ["bppc", "appc"]);
    assert_eq!(consent["searchParam"][0]["name"], Value::from("patient"));
    let cdr = MockServer::start().await;
    let store = Arc::new(MemoryStore::default());
    let plain = Facade::new(
        Programs::default(),
        handle(&store),
        client(&cdr),
        settings(),
    );
    let (_, statement) = call(
        app_over(&Arc::new(plain)),
        Request::get("/fhir/metadata").body(Body::empty())?,
    )
    .await?;
    let listed = statement["rest"][0]["resource"]
        .as_array()
        .is_some_and(|all| all.iter().any(|entry| entry["type"] == "Consent"));
    assert!(!listed, "a facade without the exchange lists no Consent");
    Ok(())
}

/// Returns a case whose CDR answers the `EHR_ACCESS` read with `status`.
async fn refusing(status: u16) -> Result<Exchange, Box<dyn Error>> {
    let exchange = Exchange::new(Vec::new()).await?;
    exchange.cdr.reset().await;
    mount_ehr(&exchange.cdr).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "{EXTENSIONS}/ehr/{EHR_ID}/ehr_access"
        )))
        .respond_with(ResponseTemplate::new(status))
        .mount(&exchange.cdr)
        .await;
    Ok(exchange)
}

#[tokio::test]
async fn a_cdr_404_for_the_caller_is_a_404_and_never_an_empty_searchset()
-> Result<(), Box<dyn Error>> {
    let exchange = refusing(404).await?;
    let (status, answer) = exchange
        .send(Request::get("/fhir/Consent?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
    assert_eq!(answer["resourceType"], Value::from("OperationOutcome"));
    let (status, _) = exchange
        .send(Request::get("/fhir/Consent/$bppc?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn a_cdr_403_for_the_caller_is_a_403() -> Result<(), Box<dyn Error>> {
    let exchange = refusing(403).await?;
    let (status, answer) = exchange
        .send(Request::get("/fhir/Consent?patient=Patient/f001").body(Body::empty())?)
        .await?;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    let (status, _) = exchange
        .send(
            Request::post("/fhir/Consent")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(crate::consent::fhir::vendored(
                    Release::R4,
                    "consent-example-notTime.json",
                )))?,
        )
        .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    Ok(())
}
