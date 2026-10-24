// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferrobridge mapping check` and the checker module it shares with the
//! operator API, over the testkit template and synthetic mapping files.

use ferrobridge_server::check::{self, Inputs, Templates};
use ferrobridge_server::{EXIT_CHECK_UNAVAILABLE, EXIT_CONFIG};
use openehr_mapping_core::diagnostic::Severity;
use std::path::Path;
use wiremock::matchers;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The clean mapping set the facade suite runs: one context and one model
/// over the testkit's `ferrobridge.diagnose.v1` template.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The OMOCL corpus the laboratory run maps with.
const OMOCL_CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/specs/omocl");

/// The three laboratory files of the OMOCL corpus.
const LABORATORY_FILES: [&str; 3] = [
    "medical_data/observation/Laboratory_test_result_v1.yml",
    "medical_data/cluster/Laboratory_test_analyte_v1.yml",
    "medical_data/cluster/Specimen_v1.yml",
];

/// A model mapping whose openEHR path names no node of the template.
const UNKNOWN_NODE_MODEL: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_facade
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Condition

mappings:
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at9999]"
      type: "CODEABLECONCEPT"
"#;

/// The mapping tree, the template tree and the OMOCL tree of one case.
struct Tree {
    root: tempfile::TempDir,
}

impl Tree {
    /// Copies the clean fixtures, the testkit template and the laboratory
    /// OMOCL files into a fresh tree.
    fn clean() -> Result<Self, Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        for directory in ["mappings", "templates", "omocl"] {
            std::fs::create_dir_all(root.path().join(directory))?;
        }
        for name in ["ferrobridge_facade.context.yml", "ferrobridge_facade.yml"] {
            std::fs::copy(
                Path::new(FIXTURES).join(name),
                root.path().join("mappings").join(name),
            )?;
        }
        std::fs::write(
            root.path().join("templates/diagnose.opt"),
            ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
        )?;
        for file in LABORATORY_FILES {
            let from = Path::new(OMOCL_CORPUS).join(file);
            let name = from.file_name().ok_or("a corpus file name")?;
            std::fs::copy(&from, root.path().join("omocl").join(name))?;
        }
        Ok(Self { root })
    }

    /// Returns the path of `directory` inside the tree.
    fn path(&self, directory: &str) -> std::path::PathBuf {
        self.root.path().join(directory)
    }

    /// Returns the inputs over the whole tree with the template directory.
    fn inputs(&self) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        (
            self.path("mappings"),
            self.path("templates"),
            self.path("omocl"),
        )
    }

    /// Runs the binary's `mapping check` over the tree with `extra` arguments.
    fn run(&self, extra: &[&str]) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        Ok(
            std::process::Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
                .args(["mapping", "check"])
                .args(extra)
                .env_remove("FERROBRIDGE_CONFIG")
                .env_remove("RUST_LOG")
                .env("FERROBRIDGE__MAPPINGS__DIRECTORY", self.path("mappings"))
                .env("FERROBRIDGE__MAPPINGS__TEMPLATES", self.path("templates"))
                .env("FERROBRIDGE__MAPPINGS__OMOCL", self.path("omocl"))
                .output()?,
        )
    }
}

/// Checks the tree's three directories through the library.
async fn library_check(tree: &Tree) -> Result<check::Report, Box<dyn std::error::Error>> {
    let (mappings, templates, omocl) = tree.inputs();
    Ok(check::check(Inputs {
        fhirconnect: Some(&mappings),
        templates: Some(Templates::Directory(&templates)),
        omocl: Some(&omocl),
    })
    .await?)
}

#[tokio::test]
async fn a_clean_set_reports_no_diagnostic_and_counts_what_it_read() -> Result<(), Box<dyn std::error::Error>>
{
    let tree = Tree::clean()?;
    let report = library_check(&tree).await?;
    assert!(report.is_clean(), "{report}");
    assert_eq!(0, report.count(Severity::Error), "{report}");
    let counts = report.counts();
    assert_eq!(2, counts.fhirconnect_files);
    assert_eq!(1, counts.contexts);
    assert_eq!(1, counts.programs);
    assert_eq!(1, counts.templates);
    assert_eq!(3, counts.omocl_files);
    Ok(())
}

#[tokio::test]
async fn a_path_no_template_node_carries_is_a_compile_diagnostic() -> Result<(), Box<dyn std::error::Error>>
{
    let tree = Tree::clean()?;
    std::fs::write(
        tree.path("mappings/ferrobridge_facade.yml"),
        UNKNOWN_NODE_MODEL,
    )?;
    let report = library_check(&tree).await?;
    assert!(!report.is_clean(), "{report}");
    assert_eq!(0, report.counts().programs);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(
                |diagnostic| diagnostic.code().as_str() == "fc-unknown-template-node"
                    && diagnostic.file().ends_with("ferrobridge_facade.yml")
            ),
        "{report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_file_that_does_not_load_is_reported_with_every_other_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    std::fs::write(tree.path("mappings/broken.yml"), "grammar: [unclosed\n")?;
    std::fs::write(tree.path("omocl/broken.yml"), "grammar: [unclosed\n")?;
    let report = library_check(&tree).await?;
    let broken: Vec<&Path> = report
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(openehr_mapping_core::diagnostic::Diagnostic::file)
        .filter(|file| file.ends_with("broken.yml"))
        .collect();
    assert!(
        broken.iter().any(|file| file
            .parent()
            .is_some_and(|parent| parent.ends_with("mappings"))),
        "the FHIRconnect file is reported: {report}"
    );
    assert!(
        broken.iter().any(|file| file
            .parent()
            .is_some_and(|parent| parent.ends_with("omocl"))),
        "the OMOCL file is reported too: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_context_whose_template_no_directory_provides_is_reported()
-> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    std::fs::remove_file(tree.path("templates/diagnose.opt"))?;
    let report = library_check(&tree).await?;
    assert!(!report.is_clean(), "{report}");
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "fb-unknown-template")
        .ok_or("an unknown-template diagnostic")?;
    assert!(
        diagnostic
            .file()
            .ends_with("ferrobridge_facade.context.yml"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.message().contains("ferrobridge.diagnose.v1"),
        "{diagnostic}"
    );
    Ok(())
}

#[tokio::test]
async fn a_template_that_does_not_build_is_a_template_diagnostic() -> Result<(), Box<dyn std::error::Error>>
{
    let tree = Tree::clean()?;
    std::fs::write(tree.path("templates/broken.opt"), "<template>")?;
    let report = library_check(&tree).await?;
    assert!(
        report.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "template-build"
                && diagnostic.file().ends_with("broken.opt")
        }),
        "{report}"
    );
    assert_eq!(
        1,
        report.counts().programs,
        "the other template still serves"
    );
    Ok(())
}

/// Returns a CDR stub that answers the fixture template's ADL 1.4 route with
/// `status`, and the body the testkit template when it is `200`.
async fn cdr_answering(status: u16) -> MockServer {
    let cdr = MockServer::start().await;
    let response = if status == 200 {
        ResponseTemplate::new(200)
            .insert_header("Content-Type", "application/xml")
            .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT)
    } else {
        ResponseTemplate::new(status)
    };
    Mock::given(matchers::method("GET"))
        .and(matchers::path_regex("^/definition/template/"))
        .respond_with(response)
        .mount(&cdr)
        .await;
    cdr
}

/// Returns a CDR client over `cdr`, retrying nothing.
fn client(cdr: &MockServer) -> Result<ferrobridge_server::cdr::CdrClient, Box<dyn std::error::Error>> {
    let text = format!(
        "[cdr]\nbase_url = \"{}/\"\ntimeout_ms = 2000\n\n[cdr.retry]\nmax_attempts = 1\n",
        cdr.uri()
    );
    let settings = ferrobridge_server::config::Config::from_sources(
        Some(&text),
        &std::collections::BTreeMap::new(),
    )?
    .resolve()?;
    let config = settings.cdr.ok_or("a [cdr] section")?;
    Ok(ferrobridge_server::cdr::CdrClient::new(&config)?)
}

#[tokio::test]
async fn the_templates_come_from_the_cdr_without_a_template_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let cdr = cdr_answering(200).await;
    let client = client(&cdr)?;
    let fixtures = Path::new(FIXTURES);
    let report = check::check(Inputs {
        fhirconnect: Some(fixtures),
        templates: Some(Templates::Cdr(&client)),
        omocl: None,
    })
    .await?;
    assert!(report.is_clean(), "{report}");
    assert_eq!(1, report.counts().programs);
    Ok(())
}

#[tokio::test]
async fn a_template_the_cdr_does_not_hold_is_a_diagnostic_naming_the_status()
-> Result<(), Box<dyn std::error::Error>> {
    let cdr = cdr_answering(404).await;
    let client = client(&cdr)?;
    let report = check::check(Inputs {
        fhirconnect: Some(Path::new(FIXTURES)),
        templates: Some(Templates::Cdr(&client)),
        omocl: None,
    })
    .await?;
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "fb-unknown-template")
        .ok_or("an unknown-template diagnostic")?;
    assert!(diagnostic.message().contains("404"), "{diagnostic}");
    assert_eq!(
        1,
        report.count(Severity::Error),
        "one context, one diagnostic: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cdr_that_fails_the_fetch_is_an_error_carrying_the_upstream_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let cdr = cdr_answering(503).await;
    let client = client(&cdr)?;
    let error = check::check(Inputs {
        fhirconnect: Some(Path::new(FIXTURES)),
        templates: Some(Templates::Cdr(&client)),
        omocl: None,
    })
    .await
    .expect_err("an upstream failure is never a clean report");
    assert!(
        matches!(error, check::Error::Template { ref template, .. } if template == "ferrobridge.diagnose.v1"),
        "{error}"
    );
    Ok(())
}

#[test]
fn the_binary_exits_zero_on_a_clean_set_and_prints_the_summary() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    let output = tree.run(&[])?;
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(Some(0), output.status.code(), "{stdout}");
    assert_eq!(
        "checked 2 FHIRconnect files (1 contexts, 1 programs over 1 templates) and 3 OMOCL files: 0 errors, 0 warnings",
        stdout.trim_end()
    );
    Ok(())
}

#[test]
fn the_binary_exits_one_and_prints_every_diagnostic_on_a_broken_set()
-> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    std::fs::write(
        tree.path("mappings/ferrobridge_facade.yml"),
        UNKNOWN_NODE_MODEL,
    )?;
    std::fs::write(tree.path("omocl/broken.yml"), "grammar: [unclosed\n")?;
    let output = tree.run(&[])?;
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(Some(1), output.status.code(), "{stdout}");
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("ferrobridge_facade.yml:")
                && line.contains("error[fc-unknown-template-node]")),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("broken.yml") && line.contains("error[")),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn the_json_report_is_one_document_with_every_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    std::fs::write(
        tree.path("mappings/ferrobridge_facade.yml"),
        UNKNOWN_NODE_MODEL,
    )?;
    let output = tree.run(&["--format", "json"])?;
    assert_eq!(Some(1), output.status.code());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(serde_json::Value::Bool(false), report["clean"]);
    assert_eq!(3, report["counts"]["omocl_files"]);
    let diagnostics = report["diagnostics"]
        .as_array()
        .ok_or("a diagnostics array")?;
    let unknown = diagnostics
        .iter()
        .find(|diagnostic| diagnostic["code"] == "fc-unknown-template-node")
        .ok_or("the unknown node is reported")?;
    assert_eq!("error", unknown["severity"]);
    assert!(
        unknown["line"].as_u64().is_some_and(|line| line > 0),
        "{unknown}"
    );
    assert!(unknown["column"].as_u64().is_some(), "{unknown}");
    assert!(
        unknown["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("ferrobridge_facade.yml")),
        "{unknown}"
    );
    assert!(unknown["message"].as_str().is_some(), "{unknown}");
    Ok(())
}

#[test]
fn the_json_report_of_a_clean_set_says_so() -> Result<(), Box<dyn std::error::Error>> {
    let tree = Tree::clean()?;
    let output = tree.run(&["--format", "json"])?;
    assert_eq!(Some(0), output.status.code());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"]);
    assert_eq!(serde_json::json!([]), report["diagnostics"]);
    Ok(())
}

#[test]
fn nothing_to_check_exits_seventy_eight() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(["mapping", "check"])
        .env_remove("FERROBRIDGE_CONFIG")
        .output()?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("[mappings]"), "{stderr}");
    Ok(())
}

#[test]
fn an_unreadable_directory_exits_sixty_nine() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(["mapping", "check"])
        .env_remove("FERROBRIDGE_CONFIG")
        .env(
            "FERROBRIDGE__MAPPINGS__OMOCL",
            "/ferrobridge/no/such/directory",
        )
        .output()?;
    assert_eq!(
        Some(i32::from(EXIT_CHECK_UNAVAILABLE)),
        output.status.code()
    );
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("/ferrobridge/no/such/directory"),
        "{stderr}"
    );
    Ok(())
}
