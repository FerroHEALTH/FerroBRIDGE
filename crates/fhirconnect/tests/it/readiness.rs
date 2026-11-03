// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The EHDS readiness matrix, generated from `docs/ehds/readiness.toml`.
//!
//! Every template file, context and round-trip test a row names must exist,
//! and a claimed profile must be named in the context's project directory, so
//! a row whose evidence is gone fails here rather than being skipped. The
//! matrix is rendered into the book page between its markers, and a page that
//! differs fails; `FERROBRIDGE_UPDATE_READINESS=1` writes the page instead.
//! No specification governs the matrix: our own design.

use core::error::Error;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;

/// The workspace root, relative to this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The metadata the matrix is generated from.
const METADATA: &str = "docs/ehds/readiness.toml";

/// The book page the matrix is rendered into.
const PAGE: &str = "website/book/src/evaluate/ehds-readiness.md";

/// The marker the generated matrix starts after.
const BEGIN: &str =
    "<!-- readiness:begin (generated from docs/ehds/readiness.toml; do not edit) -->";

/// The marker the generated matrix ends before.
const END: &str = "<!-- readiness:end -->";

/// The metadata file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Readiness {
    /// The priority categories, in Article 14(1) order.
    category: Vec<Category>,
}

/// One priority category.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Category {
    /// The point of Article 14(1).
    article: String,
    /// The category's name.
    name: String,
    /// What the matrix says beside the category's rows.
    note: Option<String>,
    /// The target profiles.
    row: Vec<Row>,
}

/// One target profile.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    /// What the matrix shows as the target.
    target: String,
    /// The canonical the context's project directory names.
    profile: Option<String>,
    /// The implementation guide package and version.
    package: String,
    /// The committed template, by name.
    template: Option<String>,
    /// The committed template file.
    template_file: Option<String>,
    /// The context that maps the profile.
    context: Option<String>,
    /// The round-trip test, `<file>::<function>`.
    test: Option<String>,
    /// Whether the produced resource claims the profile.
    claimed: Option<bool>,
    /// Why no template exists.
    gap: Option<String>,
    /// What the matrix says beside the row's target.
    remark: Option<String>,
}

/// Returns `relative` under the workspace root.
fn rooted(relative: &str) -> PathBuf {
    Path::new(ROOT).join(relative)
}

/// Returns the defects of one row: every named file or test that is gone.
fn defects(row: &Row) -> Vec<String> {
    let mut found = Vec::new();
    if row.gap.is_some() {
        if row.template.is_some() || row.context.is_some() || row.test.is_some() {
            found.push(format!("{} names a gap and evidence both", row.target));
        }
        return found;
    }
    let (Some(_), Some(template_file), Some(context), Some(test)) = (
        row.template.as_ref(),
        row.template_file.as_ref(),
        row.context.as_ref(),
        row.test.as_ref(),
    ) else {
        found.push(format!(
            "{} names neither a gap nor a template, a context and a test",
            row.target
        ));
        return found;
    };
    if !rooted(template_file).is_file() {
        found.push(format!(
            "{}: the template {template_file} is gone",
            row.target
        ));
    }
    let context_path = rooted(context);
    if !context_path.is_file() {
        found.push(format!("{}: the context {context} is gone", row.target));
    }
    if row.claimed != Some(false) {
        match (row.profile.as_ref(), context_path.parent()) {
            (Some(profile), Some(directory)) => {
                let named = std::fs::read_dir(directory)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
                    .any(|text| text.contains(profile.as_str()));
                if !named {
                    found.push(format!(
                        "{}: no file beside {context} names {profile}",
                        row.target
                    ));
                }
            }
            (None, _) => found.push(format!("{} claims no profile canonical", row.target)),
            (_, None) => found.push(format!("{}: {context} has no directory", row.target)),
        }
    }
    match test.split_once("::") {
        Some((file, function)) => {
            let source = std::fs::read_to_string(rooted(file)).unwrap_or_default();
            if !source.contains(&format!("fn {function}(")) {
                found.push(format!("{}: the test {test} is gone", row.target));
            }
        }
        None => found.push(format!("{}: {test} is not <file>::<function>", row.target)),
    }
    found
}

/// Returns the last segment of a `<file>::<function>` test, as the matrix
/// names it.
fn test_name(test: &str) -> &str {
    test.rsplit("::").next().unwrap_or(test)
}

/// Renders the matrix as the Markdown the book page carries between its
/// markers.
fn render(readiness: &Readiness) -> Result<String, std::fmt::Error> {
    let mut out = String::new();
    for category in &readiness.category {
        writeln!(
            out,
            "\n### Article {}: {}\n",
            category.article, category.name
        )?;
        if let Some(ref note) = category.note {
            writeln!(out, "{note}\n")?;
        }
        writeln!(
            out,
            "| Target profile | Package | Committed template | Transform proven |"
        )?;
        writeln!(out, "|---|---|---|---|")?;
        for row in &category.row {
            let mut target = match row.claimed {
                Some(false) => format!("`{}` (produced, not claimed)", row.target),
                _ => format!("`{}`", row.target),
            };
            if let Some(ref remark) = row.remark {
                write!(target, "; {remark}")?;
            }
            let (template, proven) = match row.gap {
                Some(ref gap) => (format!("none: {gap}"), String::from("no")),
                None => (
                    row.template.clone().unwrap_or_default(),
                    row.test
                        .as_deref()
                        .map(|test| format!("`{}`", test_name(test)))
                        .unwrap_or_default(),
                ),
            };
            writeln!(
                out,
                "| {target} | {} | {template} | {proven} |",
                row.package
            )?;
        }
    }
    Ok(out)
}

#[test]
fn the_readiness_matrix_names_only_evidence_that_exists() -> Result<(), Box<dyn Error>> {
    let readiness: Readiness = toml::from_str(&std::fs::read_to_string(rooted(METADATA))?)?;
    assert_eq!(
        6,
        readiness.category.len(),
        "the six priority categories of Article 14(1)"
    );
    let defects: Vec<String> = readiness
        .category
        .iter()
        .flat_map(|category| category.row.iter())
        .flat_map(defects)
        .collect();
    assert!(defects.is_empty(), "{defects:#?}");
    Ok(())
}

#[test]
fn the_book_carries_the_readiness_matrix_the_metadata_generates() -> Result<(), Box<dyn Error>> {
    let readiness: Readiness = toml::from_str(&std::fs::read_to_string(rooted(METADATA))?)?;
    let page_path = rooted(PAGE);
    let page = std::fs::read_to_string(&page_path)?;
    let (head, rest) = page
        .split_once(BEGIN)
        .ok_or("the book page carries the begin marker")?;
    let (_, tail) = rest
        .split_once(END)
        .ok_or("the book page carries the end marker")?;
    let generated = format!("{head}{BEGIN}\n{}\n{END}{tail}", render(&readiness)?);
    if std::env::var_os("FERROBRIDGE_UPDATE_READINESS").is_some() {
        std::fs::write(&page_path, generated)?;
        return Ok(());
    }
    assert!(
        page == generated,
        "{PAGE} differs from the matrix {METADATA} generates; run the test with FERROBRIDGE_UPDATE_READINESS=1 to rewrite it"
    );
    Ok(())
}
