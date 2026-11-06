// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document held to its profiles in full by
//! [`ReceivedDocument::validate`], for each guide whose package closure
//! `scripts/vendor/profile-packages.sh` pins: its document examples pass, an
//! example that breaks R4 is refused for that defect, and one planted profile
//! violation is reported. The set is read from the
//! trees that script fetches into the ignored `docs/specs/fhir-packages/`; a
//! test returns early when they are absent, and fails under CI.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use eehrxf::category::Category;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::{Finding, FindingKind};
use eehrxf::receive::validation::{Validated, ValidationError};
use ferrobridge_term::client::Client;
use ferrobridge_validator::release::R4;
use ferrobridge_validator::set::PackageSet;
use flate2::read::GzDecoder;

/// The package set of R4 core and every fetched profile package, read once
/// per test binary: `None` when the trees are absent outside CI, an error
/// when they are absent under CI or do not load.
fn profile_set() -> Result<Option<&'static PackageSet>, Box<dyn Error>> {
    static SET: OnceLock<Result<Option<PackageSet>, String>> = OnceLock::new();
    SET.get_or_init(|| load().map_err(|error| error.to_string()))
        .as_ref()
        .map(Option::as_ref)
        .map_err(|error| error.clone().into())
}

/// Reads the package set [`profile_set`] holds.
fn load() -> Result<Option<PackageSet>, Box<dyn Error>> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join("docs/specs/fhir-packages");
    if !root
        .join("hl7.fhir.eu.eps#1.0.0-ballot/package/package.json")
        .is_file()
    {
        if std::env::var("CI").is_ok_and(|value| value == "true") {
            return Err(format!(
                "the profile packages are absent from {} under CI; run scripts/vendor/profile-packages.sh before the tests",
                root.display()
            )
            .into());
        }
        return Ok(None);
    }
    let mut paths: Vec<PathBuf> =
        vec![workspace.join("tools/fhir-codegen/vendor/hl7.fhir.r4.core")];
    for entry in std::fs::read_dir(&root)? {
        let path = entry?.path();
        if path.join("package/package.json").is_file() {
            paths.push(path);
        }
    }
    Ok(Some(PackageSet::<R4>::load(paths)?))
}

/// Returns every document `Bundle` example of the vendored guide `tarball`
/// under `package/example/`, by member path, with its text.
fn examples(tarball: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(tarball);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    let mut found = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let member = entry.path()?.to_string_lossy().into_owned();
        if member.starts_with("package/example/Bundle-") {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            let tree: serde_json::Value = serde_json::from_str(&text)?;
            if tree.get("type").and_then(serde_json::Value::as_str) == Some("document") {
                found.push((member, text));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Reads `text` as a received document of `category` and validates it
/// against the category's `Bundle` profile over `set`, with no terminology
/// server.
async fn validated(
    set: &PackageSet,
    category: Category,
    text: &str,
) -> Result<Result<Validated, ValidationError>, Box<dyn Error>> {
    let document = ReceivedDocument::read(text)?;
    assert_eq!(document.category()?, category);
    let profile = category
        .profiles()
        .ok_or("the category is received")?
        .bundle;
    Ok(document.validate(set, profile, None::<&Client>).await)
}

/// Validates every document example of `tarball` as a document of
/// `category`, but the members `refused`, which their own tests hold, failing
/// naming every one that does not pass; returns how many passed.
async fn every_example_passes(
    set: &PackageSet,
    category: Category,
    tarball: &str,
    refused: &[&str],
) -> Result<usize, Box<dyn Error>> {
    let examples = examples(tarball)?;
    for member in refused {
        assert!(
            examples.iter().any(|(found, _)| found == member),
            "{tarball} has no document example {member}"
        );
    }
    let mut passed = 0;
    let mut failed = Vec::new();
    for (member, text) in &examples {
        if refused.contains(&member.as_str()) {
            continue;
        }
        match validated(set, category, text).await {
            Ok(Ok(_)) => passed += 1,
            Ok(Err(error)) => failed.push(format!("{member}: {error}: {error:?}")),
            Err(error) => failed.push(format!("{member}: {error}")),
        }
    }
    if failed.is_empty() {
        Ok(passed)
    } else {
        Err(failed.join("\n").into())
    }
}

/// Returns the document example `member` of `tarball` with the JSON pointer
/// `pointer` removed, failing when the example has no such member.
fn planted(tarball: &str, member: &str, pointer: &str) -> Result<String, Box<dyn Error>> {
    let (_, text) = examples(tarball)?
        .into_iter()
        .find(|(found, _)| found == member)
        .ok_or_else(|| format!("{tarball} has no document example {member}"))?;
    let mut tree: serde_json::Value = serde_json::from_str(&text)?;
    let (parent, key) = pointer.rsplit_once('/').ok_or("a pointer below the root")?;
    tree.pointer_mut(parent)
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|object| object.remove(key))
        .ok_or_else(|| format!("{member} has no {pointer}"))?;
    Ok(serde_json::to_string(&tree)?)
}

/// Returns the findings `outcome` refuses the document with, failing when it
/// passed or could not be validated.
fn refusals(outcome: Result<Validated, ValidationError>) -> Result<Vec<Finding>, Box<dyn Error>> {
    match outcome {
        Err(ValidationError::NonConformant { findings }) => Ok(findings),
        other => Err(format!("the document was not refused: {other:?}").into()),
    }
}

/// Returns whether `findings` hold one at `location` whose message says
/// `needle`.
fn reported(findings: &[Finding], location: &str, needle: &str) -> bool {
    findings.iter().any(|finding| {
        finding.location == location
            && matches!(&finding.kind, FindingKind::Issue { message, .. } if message.contains(needle))
    })
}

#[cfg(feature = "patient-summary")]
mod patient_summary {
    use std::error::Error;

    use eehrxf::category::Category;
    use eehrxf::receive::conform::FindingKind;

    use super::{every_example_passes, planted, profile_set, refusals, reported, validated};

    /// The vendored HL7 Europe Patient Summary 1.0.0-ballot package.
    const EPS: &str = "eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz";

    /// The example that breaks its profiles, held by its own test.
    const BROKEN: &str =
        "package/example/Bundle-Instance-Bundle-1a24e60d-9b12-4109-a50a-07249a4f21c3.json";

    /// The example the planted violation is made in.
    const PLANTED: &str =
        "package/example/Bundle-EPSExampleBundle01NoProblemsMedicationAllergies.json";

    #[tokio::test]
    async fn every_conformant_patient_summary_example_validates() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let passed = every_example_passes(set, Category::PatientSummary, EPS, &[BROKEN]).await?;
        assert_eq!(passed, 1, "the guide's conformant document examples");
        Ok(())
    }

    // NOTE: FHIR R4 documents.html §3.3.1 ("The document bundle SHALL include only" what the
    // Composition references) and ele-1 (elementdefinition.html); the example breaks both.
    #[tokio::test]
    async fn the_patient_summary_example_that_breaks_r4_is_refused() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let (_, text) = super::examples(EPS)?
            .into_iter()
            .find(|(member, _)| member == BROKEN)
            .ok_or("the example")?;
        let findings = refusals(validated(set, Category::PatientSummary, &text).await?)?;
        for entry in 11..=15 {
            assert!(
                reported(
                    &findings,
                    &format!("Bundle.entry[{entry}]"),
                    "the Composition does not reference"
                ),
                "entry {entry}, a Condition nothing references: {findings:?}"
            );
        }
        for entry in [81, 83] {
            assert!(
                reported(
                    &findings,
                    &format!("Bundle.entry[{entry}].resource.udiCarrier[0]"),
                    "ele-1"
                ),
                "entry {entry}, a Device with an empty udiCarrier: {findings:?}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_patient_summary_without_its_sections_is_refused() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let text = planted(EPS, PLANTED, "/entry/0/resource/section")?;
        let findings = refusals(validated(set, Category::PatientSummary, &text).await?)?;
        assert!(
            findings.iter().any(|finding| matches!(
                &finding.kind,
                FindingKind::Issue { message, .. }
                    if message.contains("Composition.section:sectionAllergies occurs 0 times")
            )),
            "{findings:?}"
        );
        Ok(())
    }
}

#[cfg(feature = "laboratory")]
mod laboratory {
    use std::error::Error;

    use eehrxf::category::Category;
    use eehrxf::receive::conform::FindingKind;

    use super::{every_example_passes, planted, profile_set, refusals, reported, validated};

    /// The vendored HL7 Europe Laboratory Report 2.0.0 package.
    const LAB: &str = "eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.0.0.tgz";

    /// The example that breaks R4, held by its own test.
    const BROKEN: &str = "package/example/Bundle-BundleLabResultReportPOC.json";

    /// The example the planted violation is made in.
    const PLANTED: &str = "package/example/Bundle-BundleHepatitisPanel.json";

    #[tokio::test]
    async fn every_conformant_laboratory_report_example_validates() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let passed = every_example_passes(set, Category::Laboratory, LAB, &[BROKEN]).await?;
        assert_eq!(passed, 4, "the guide's conformant document examples");
        Ok(())
    }

    // NOTE: FHIR R4 documents.html §3.3.1, "The document bundle SHALL include only" what the
    // Composition references; the example's ServiceRequest, and the Specimen only it names, are not.
    #[tokio::test]
    async fn the_laboratory_report_example_that_breaks_r4_is_refused() -> Result<(), Box<dyn Error>>
    {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let (_, text) = super::examples(LAB)?
            .into_iter()
            .find(|(member, _)| member == BROKEN)
            .ok_or("the example")?;
        let findings = refusals(validated(set, Category::Laboratory, &text).await?)?;
        for (entry, kind) in [(4, "ServiceRequest"), (6, "Specimen")] {
            assert!(
                reported(
                    &findings,
                    &format!("Bundle.entry[{entry}]"),
                    &format!("holds a {kind} the Composition does not reference")
                ),
                "entry {entry}: {findings:?}"
            );
        }
        assert_eq!(findings.len(), 2, "{findings:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_laboratory_report_without_its_section_is_refused() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let text = planted(LAB, PLANTED, "/entry/0/resource/section")?;
        let findings = refusals(validated(set, Category::Laboratory, &text).await?)?;
        assert!(
            findings.iter().any(|finding| matches!(
                &finding.kind,
                FindingKind::Issue { message, .. }
                    if message.contains("Composition.section occurs 0 times")
            )),
            "{findings:?}"
        );
        Ok(())
    }
}

#[cfg(feature = "imaging")]
mod imaging {
    use std::error::Error;

    use eehrxf::category::Category;
    use eehrxf::receive::conform::FindingKind;

    use super::{examples, planted, profile_set, refusals, reported, validated};

    /// The vendored HL7 Europe Imaging Report 1.0.0-ballot package.
    const IMAGING: &str = "eu-hl7-imaging/hl7.fhir.eu.imaging-1.0.0-ballot.tgz";

    /// The structured report, which breaks R4 and is held by its own test.
    const STRUCTURED: &str = "package/example/Bundle-bundle-report-structured.json";

    // NOTE: FHIR R4 documents.html §3.3.1, "The document bundle SHALL include only" what the
    // Composition references; nothing in the example references its two key image DocumentReferences.
    #[tokio::test]
    async fn the_structured_imaging_report_example_is_refused() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let (_, text) = examples(IMAGING)?
            .into_iter()
            .find(|(member, _)| member == STRUCTURED)
            .ok_or("the example")?;
        let findings = refusals(validated(set, Category::Imaging, &text).await?)?;
        for entry in [46, 47] {
            assert!(
                reported(
                    &findings,
                    &format!("Bundle.entry[{entry}]"),
                    "holds a DocumentReference the Composition does not reference"
                ),
                "entry {entry}: {findings:?}"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn an_imaging_report_without_its_identifier_is_refused() -> Result<(), Box<dyn Error>> {
        let Some(set) = profile_set()? else {
            return Ok(());
        };
        let text = planted(IMAGING, STRUCTURED, "/entry/0/resource/identifier")?;
        let findings = refusals(validated(set, Category::Imaging, &text).await?)?;
        assert!(
            findings.iter().any(|finding| matches!(
                &finding.kind,
                FindingKind::Issue { message, .. }
                    if message.contains("Composition.identifier occurs 0 times")
            )),
            "{findings:?}"
        );
        Ok(())
    }
}
