// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The declarations at boot: what refuses the start, the required subject,
//! and two programs over one template.

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::programs::Programs;
use ferrobridge_server::facade::search;
use ferrobridge_server::facade::search::Limits;
use ferrobridge_server::facade::search::declaration::DeclarationError;
use http::StatusCode;
use std::error::Error as StdError;
use std::path::Path;
use std::sync::Arc;

use super::super::EHR_ID;
use super::super::FIXTURES;
use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::client;
use super::super::first_issue;
use super::super::handle;
use super::super::harness;
use super::super::settings;
use super::OTHER_CONTAINER;
use super::SUBJECT;
use super::get;
use super::matches;
use super::mount_rows;
use super::row;

/// The context of the fixture set.
const CONTEXT: &str = "ferrobridge_facade.context";

/// Writes `files` into a fresh directory and returns it.
fn directory_of(files: &[(&str, &str)]) -> Result<tempfile::TempDir, Box<dyn StdError>> {
    let directory = tempfile::tempdir()?;
    for (name, text) in files {
        std::fs::write(directory.path().join(name), text)?;
    }
    Ok(directory)
}

/// Loads the declarations `files` state against `programs`.
fn load(programs: &Programs, files: &[(&str, &str)]) -> Result<search::Catalog, DeclarationError> {
    let directory = directory_of(files).expect("a fixture directory");
    search::load(directory.path(), programs)
}

#[tokio::test]
async fn the_fixture_declaration_loads_and_the_mapping_loader_skips_it() {
    let harness = harness().await;
    let catalog = search::load(Path::new(FIXTURES), harness.facade.programs())
        .expect("the fixture declaration loads");
    assert_eq!(1, catalog.of_type("Condition").len());
    assert!(catalog.of_type("Observation").is_empty());
    let files = programs::mapping_files(Path::new(FIXTURES)).expect("the fixtures list");
    assert!(
        files
            .iter()
            .all(|file| !search::declaration::is_declaration(file)),
        "the FHIRconnect loader reads a search declaration: {files:?}"
    );
}

#[tokio::test]
async fn a_declaration_naming_no_loaded_context_refuses_the_start() {
    let harness = harness().await;
    let error = load(
        harness.facade.programs(),
        &[("x.search.yml", "context: no.such.context\n")],
    )
    .expect_err("the context is unknown");
    assert!(
        matches!(error, DeclarationError::UnknownContext { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_path_that_does_not_parse_refuses_the_start() {
    let harness = harness().await;
    for (path, name, placed) in [
        (
            "context/start_time/value[",
            "a path that does not parse",
            true,
        ),
        ("context/start_time/value, c/b", "two paths", true),
        ("nowhere/value", "a path the template does not hold", false),
    ] {
        let text = format!(
            "context: {CONTEXT}\nparameters:\n  when:\n    type: date\n    path: \"{path}\"\n"
        );
        let error = load(harness.facade.programs(), &[("x.search.yml", &text)])
            .expect_err("the path is refused");
        if placed {
            assert!(
                matches!(error, DeclarationError::Path { .. }),
                "{name}: {error:?}"
            );
        } else {
            assert!(
                matches!(error, DeclarationError::Template { .. }),
                "{name}: {error:?}"
            );
        }
    }
    let error = load(
        harness.facade.programs(),
        &[(
            "x.search.yml",
            &format!("context: {CONTEXT}\norder: [\"c/[\"]\n"),
        )],
    )
    .expect_err("an order path that does not parse is refused");
    assert!(matches!(error, DeclarationError::Path { .. }), "{error:?}");
}

#[tokio::test]
async fn a_token_whose_code_and_system_cannot_be_read_from_one_node_refuses_the_start() {
    // #526: the anatomical location cluster is unbounded in the fixture
    // template, so below the bound entry its coded value is one per cluster,
    // and a code and a system read through it could come from two clusters.
    let harness = harness().await;
    for node in [
        "    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }\n",
        "",
    ] {
        let text = format!(
            "context: {CONTEXT}\nparameters:\n  site:\n    type: token\n{node}    path: \"{}data[at0001]/items[openEHR-EHR-CLUSTER.anatomical_location.v1]/items[at0002]/value\"\n",
            if node.is_empty() {
                "content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/"
            } else {
                ""
            }
        );
        let error = load(harness.facade.programs(), &[("x.search.yml", &text)])
            .expect_err("the cluster repeats");
        assert!(
            matches!(error, DeclarationError::Template { .. }),
            "{node}: {error:?}"
        );
    }
    let bound = format!(
        "context: {CONTEXT}\nparameters:\n  site:\n    type: token\n    node: {{ class: CLUSTER, archetype: openEHR-EHR-CLUSTER.anatomical_location.v1 }}\n    path: items[at0002]/value\n"
    );
    load(harness.facade.programs(), &[("x.search.yml", &bound)])
        .expect("bound at the repeated cluster, code and system are one node's");
}

#[tokio::test]
async fn an_unknown_key_or_parameter_type_refuses_the_start() {
    let harness = harness().await;
    for text in [
        format!("context: {CONTEXT}\nfrom: EHR e\n"),
        format!("context: {CONTEXT}\nparameters:\n  value:\n    type: quantity\n    path: c/q\n"),
        format!(
            "context: {CONTEXT}\nparameters:\n  _lastUpdated:\n    type: date\n    path: c/q\n"
        ),
        format!("context: {CONTEXT}\npatient:\n  parameters: []\n"),
    ] {
        assert!(
            load(harness.facade.programs(), &[("x.search.yml", &text)]).is_err(),
            "{text}"
        );
    }
}

#[tokio::test]
async fn two_declarations_over_one_template_refuse_the_start() {
    let harness = harness().await;
    let text = format!("context: {CONTEXT}\n");
    let error = load(
        harness.facade.programs(),
        &[("a.search.yml", &text), ("b.search.yaml", &text)],
    )
    .expect_err("one template answers one declaration");
    assert!(
        matches!(error, DeclarationError::Duplicate { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_required_subject_refuses_a_search_that_names_none() -> Result<(), Box<dyn StdError>> {
    // The carry-over register (docs/architecture.md §12): a missing mandatory
    // scope is a 400.
    let base = harness().await;
    let catalog = load(
        base.facade.programs(),
        &[(
            "x.search.yml",
            &format!("context: {CONTEXT}\npatient:\n  parameters: [patient]\n  required: true\n"),
        )],
    )?;
    let set = programs::read_set(Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client(&base.cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::new());
    let facade = Facade::new(compiled, handle(&store), client(&base.cdr), settings())
        .with_search(catalog, Limits::default());
    let harness = Harness {
        cdr: base.cdr,
        store,
        facade: Arc::new(facade),
    };
    let (status, body) = get(&harness, "Condition").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(Some("required"), first_issue(&body)["code"].as_str());
    let (status, body) = get(&harness, "Condition?patient=nobody").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn two_programs_over_one_template_answer_one_entry_per_composition()
-> Result<(), Box<dyn StdError>> {
    // The carry-over register (docs/architecture.md §12) and R4 bundle.html
    // bdl-7: two programs over one template never double an entry.
    let base = harness().await;
    let model = std::fs::read_to_string(Path::new(FIXTURES).join("ferrobridge_facade.yml"))?;
    let context =
        std::fs::read_to_string(Path::new(FIXTURES).join("ferrobridge_facade.context.yml"))?;
    let twin = context
        .replace("ferrobridge_facade.context", "ferrobridge_facade.twin")
        .replace("ferrobridge-facade-diagnosis", "ferrobridge-facade-twin");
    let directory = directory_of(&[
        ("ferrobridge_facade.yml", &model),
        ("ferrobridge_facade.context.yml", &context),
        ("ferrobridge_facade.twin.yml", &twin),
        (
            "one.search.yml",
            &format!("context: {CONTEXT}\npatient:\n  parameters: [patient]\n"),
        ),
    ])?;
    let set = programs::read_set(directory.path())?;
    let templates = programs::fetch_templates(&set, &client(&base.cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    assert_eq!(
        2,
        compiled.len(),
        "both programs compile over the one template"
    );
    let catalog = search::load(directory.path(), &compiled)?;
    let store = Arc::new(MemoryStore::new());
    store.record_ehr(
        &PersonId::new("http://example.org/fhir", SUBJECT)?,
        &EhrId::new(EHR_ID)?,
    )?;
    let facade = Facade::new(compiled, handle(&store), client(&base.cdr), settings())
        .with_search(catalog, Limits::default());
    let harness = Harness {
        cdr: base.cdr,
        store,
        facade: Arc::new(facade),
    };
    let composition = harness.composition();
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_rows(
        &harness.cdr,
        &[row(VERSION_ONE, &composition), row(&second, &composition)],
    )
    .await;
    let (status, body) = get(&harness, "Condition?patient=p-1").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(2, found.len(), "{body}");
    assert_eq!(Some(2), body["total"].as_u64());
    let mut urls: Vec<&str> = found
        .iter()
        .filter_map(|entry| entry["fullUrl"].as_str())
        .collect();
    urls.dedup();
    assert_eq!(2, urls.len(), "two entries share a fullUrl: {body}");
    Ok(())
}
