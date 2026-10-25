// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every route under the FHIR base is classified, and one that is not fails
//! closed (#547).
//!
//! axum keeps no list of the routes a router mounts, so the walk reads every
//! `.route(` call of the server's sources: a route mounted under `/fhir`
//! without an entry in the guard's table fails here before it reaches a
//! deployment, where the guard would refuse it.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::routing::post;
use ferrobridge_server::facade::guard::CLASSIFIED;
use ferrobridge_server::facade::guard::layer;
use ferrobridge_server::smart::Smart;
use ferrohealth_smart::resource::configuration::SMART_CONFIGURATION;
use http::Method;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::mint;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// The server's source tree.
const SOURCES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");

/// The FHIR base every guarded route sits under.
const BASE: &str = "/fhir";

/// Returns every Rust file under `directory`.
fn rust_files(directory: &Path, found: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

/// Returns the text of the call that opens at `start`, up to its matching
/// parenthesis.
fn call_at(text: &str, start: usize) -> Option<&str> {
    let mut depth = 0_usize;
    for (offset, character) in text.get(start..)?.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return text.get(start..start + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the route template one `.route(` call names, with the constants it
/// spells out resolved, or `None` when it names no path under `/fhir`.
fn template_of(call: &str) -> Option<String> {
    let arguments = call.strip_prefix(".route(")?.trim_start();
    let raw = if let Some(rest) = arguments.strip_prefix("BASE_PATH") {
        rest.starts_with(',').then(|| String::from(BASE))?
    } else if let Some(rest) = arguments.strip_prefix("&format!(\"") {
        rest.split('"').next()?.to_owned()
    } else {
        arguments.strip_prefix('"')?.split('"').next()?.to_owned()
    };
    let resolved = raw
        .replace("{BASE_PATH}", BASE)
        .replace("{SMART_CONFIGURATION}", SMART_CONFIGURATION)
        .replace("{{", "{")
        .replace("}}", "}");
    (resolved == BASE || resolved.starts_with("/fhir/")).then_some(resolved)
}

/// Returns the methods one `.route(` call mounts.
fn methods_of(call: &str) -> Vec<Method> {
    let mut methods = Vec::new();
    for (name, method) in [
        ("get(", Method::GET),
        ("post(", Method::POST),
        ("put(", Method::PUT),
        ("delete(", Method::DELETE),
        ("patch(", Method::PATCH),
    ] {
        let mounted = call.match_indices(name).any(|(at, _)| {
            call.get(..at)
                .and_then(|before| before.chars().last())
                .is_some_and(|previous| !previous.is_alphanumeric() && previous != '_')
        });
        if mounted {
            methods.push(method);
        }
    }
    methods
}

/// Returns every `(method, template)` the server's sources mount under
/// `/fhir`.
fn mounted_routes() -> Result<BTreeSet<(String, String)>, Box<dyn Error>> {
    let mut files = Vec::new();
    rust_files(Path::new(SOURCES), &mut files)?;
    let mut routes = BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        for (start, _) in text.match_indices(".route(") {
            let call = call_at(&text, start).ok_or("a route call closes")?;
            let Some(template) = template_of(call) else {
                continue;
            };
            let methods = methods_of(call);
            assert!(!methods.is_empty(), "{}: {call}", file.display());
            for method in methods {
                routes.insert((method.to_string(), template.clone()));
            }
        }
    }
    Ok(routes)
}

#[test]
fn every_route_mounted_under_the_fhir_base_is_classified() -> Result<(), Box<dyn Error>> {
    let mounted = mounted_routes()?;
    let classified: BTreeSet<(String, String)> = CLASSIFIED
        .iter()
        .map(|(method, template, _)| (method.to_string(), format!("{BASE}{template}")))
        .collect();
    assert!(
        mounted.len() >= 14,
        "the walk found too few routes to be reading the router: {mounted:?}"
    );
    let unclassified: Vec<&(String, String)> = mounted.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "routes under /fhir the guard does not classify: {unclassified:?}"
    );
    let unmounted: Vec<&(String, String)> = classified.difference(&mounted).collect();
    assert!(
        unmounted.is_empty(),
        "the guard classifies routes no source mounts: {unmounted:?}"
    );
    Ok(())
}

#[tokio::test]
async fn every_classified_route_reaches_its_handler_under_a_full_scope()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    let token = mint(&key, &issuer.claims("system/*.cruds"));
    for (method, template, _) in CLASSIFIED {
        let path = format!("{BASE}{template}")
            .replace("{resource_type}", "Condition")
            .replace("{id}", "unknown-id")
            .replace("{vid}", "1");
        let request = Request::builder()
            .method(method.clone())
            .uri(&path)
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from("{}"))?;
        let response = raw(secured(&harness, &authority), bearing(request, &token)).await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
        assert_ne!(StatusCode::METHOD_NOT_ALLOWED, status, "{method} {path}");
        assert_ne!(StatusCode::FORBIDDEN, status, "{method} {path}");
        assert!(
            status != StatusCode::NOT_FOUND || !bytes.is_empty(),
            "{method} {path} met no route"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unclassified_route_is_refused_under_auth_and_open_without_it()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let authority = authority(issuer.settings()).await;
    let token = mint(&key, &issuer.claims("system/*.cruds"));
    let probe = |smart: Option<&Smart>| {
        Router::new()
            .route("/fhir/$probe", post(|| async { "reached" }))
            .layer(layer(smart))
    };
    let request = || Request::post("/fhir/$probe").body(Body::empty());
    let refused = raw(probe(Some(authority.as_ref())), bearing(request()?, &token)).await?;
    assert_eq!(StatusCode::FORBIDDEN, refused.status());
    let bytes = axum::body::to_bytes(refused.into_body(), 64 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(Some("OperationOutcome"), body["resourceType"].as_str());
    assert_eq!(Some("forbidden"), body["issue"][0]["code"].as_str());
    assert!(!String::from_utf8_lossy(&bytes).contains("reached"));

    let open = raw(probe(None), request()?).await?;
    assert_eq!(
        StatusCode::OK,
        open.status(),
        "with [auth] off the facade stays open"
    );
    Ok(())
}

#[test]
fn the_walk_reads_each_form_a_route_template_takes() {
    assert_eq!(
        Some(String::from("/fhir/{resource_type}/_search")),
        template_of(".route(\n &format!(\"{BASE_PATH}/{{resource_type}}/_search\"),\n post(x),\n)")
    );
    assert_eq!(
        Some(String::from("/fhir")),
        template_of(".route(BASE_PATH, post(handlers::transaction_route))")
    );
    assert_eq!(
        Some(String::from("/fhir/$tofhir")),
        template_of(".route(\"/fhir/$tofhir\", post(to_fhir))")
    );
    assert_eq!(
        None,
        template_of(".route(\"/health/liveness\", get(liveness))")
    );
    assert_eq!(
        vec![Method::GET, Method::PUT],
        methods_of(".route(p, get(handlers::read_route).put(handlers::update_route))")
    );
}
