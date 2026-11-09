// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The v2-to-FHIR half of `emit`: the guide, the overrides and the value
//! package in, `ferrobridge-hl7v2`'s `src/map/generated/` out.
//!
//! Before rendering, the inputs are held to the split the crate relies on:
//! the overrides hold structure maps only and the value package table maps
//! only. Each names FerroBRIDGE in its `title`, and its `description` opens
//! with `Overrides ConceptMap/<id> of <package>.` when the guide carries the
//! id, else with `Adds`, and names the tracker issue (`FerroBRIDGE #`). A
//! structure override keeps the guide map's url, since the guide's rows name
//! it; every other map FerroBRIDGE writes takes a url under
//! [`CANONICAL_BASE`], so the terminology server holds the guide's map and
//! FerroBRIDGE's beside each other. No specification governs this: our own
//! design, under the guide's leave to add a mapping locally
//! (`mapping_guidelines.md` §General Format/Approach).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use generated_tree::Formatting;

use crate::v2maps::read::{self, Kind, Map, ReadError};
use crate::v2maps::render::{Sources, render};

/// The canonical base of a map FerroBRIDGE writes under its own url.
pub const CANONICAL_BASE: &str = "https://ferrobridge.eu/fhir/v2mappings/ConceptMap/";

/// What to emit and where.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// The guide package directory (the one holding `package/`).
    pub guide: PathBuf,
    /// The directory of FerroBRIDGE's structure overrides.
    pub overrides: PathBuf,
    /// The directory of FerroBRIDGE's value package.
    pub values: PathBuf,
    /// The crate directory (the one holding `Cargo.toml` and `src/`).
    pub crate_dir: PathBuf,
    /// Compare instead of writing.
    pub check: bool,
}

/// What an emit run produced.
#[derive(Debug)]
pub struct EmitReport {
    /// The guide's structure maps.
    pub guide_maps: usize,
    /// The guide's table maps.
    pub guide_tables: usize,
    /// The structure overrides.
    pub overrides: usize,
    /// The value package's table maps.
    pub values: usize,
    /// The files written or checked, relative to `src/map/generated/`.
    pub files: Vec<String>,
}

/// A failure while emitting the maps.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// A directory of maps did not read.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The guide's `package.json` did not read.
    #[error("{path} names no package name and version")]
    Manifest {
        /// The path.
        path: PathBuf,
    },
    /// A map FerroBRIDGE writes breaks the rules the module states.
    #[error("{id}: {reason}")]
    Input {
        /// The map id.
        id: String,
        /// The rule it breaks.
        reason: String,
    },
    /// Rendering failed.
    #[error("rendering failed")]
    Render(#[from] std::fmt::Error),
    /// Writing, formatting, or checking the tree failed.
    #[error(transparent)]
    Tree(#[from] generated_tree::TreeError),
}

/// Runs the pipeline per `options`.
///
/// # Errors
///
/// Returns [`EmitError`] for a read, input, render, I/O, or `rustfmt`
/// failure, and a [`generated_tree::TreeError::Drift`] in check mode when the
/// tree differs.
pub fn emit(options: &EmitOptions) -> Result<EmitReport, EmitError> {
    let package_dir = options.guide.join("package");
    let package = manifest(&package_dir)?;
    let guide = read::directory(&package_dir)?;
    let overrides = read::directory(&options.overrides)?;
    let values = read::directory(&options.values)?;
    for map in overrides.values() {
        check(map, &guide, &package, false)?;
    }
    for map in values.values() {
        check(map, &guide, &package, true)?;
    }
    let files = render(&Sources {
        package: &package,
        guide: &guide,
        overrides: &overrides,
        values: &values,
    })?;
    let formatted = generated_tree::formatted(
        &files,
        Formatting::FromRoot("mod.rs"),
        &options.crate_dir.join("../../rustfmt.toml"),
    )?;
    let target = options.crate_dir.join("src/map/generated");
    if options.check {
        let owned = [(target.clone(), "")];
        generated_tree::check(&formatted, |relative| target.join(relative), &owned)?;
    } else {
        generated_tree::clear(&target)?;
        generated_tree::write(&target, &formatted)?;
    }
    let tables = guide.values().filter(|map| map.kind == Kind::Table).count();
    Ok(EmitReport {
        guide_maps: guide.len().saturating_sub(tables),
        guide_tables: tables,
        overrides: overrides.len(),
        values: values.len(),
        files: formatted.keys().cloned().collect(),
    })
}

/// The guide package's `<name> <version>`, from its `package.json`.
fn manifest(package_dir: &Path) -> Result<String, EmitError> {
    let path = package_dir.join("package.json");
    let refused = || EmitError::Manifest { path: path.clone() };
    let text = std::fs::read_to_string(&path).map_err(|source| ReadError::Io {
        path: path.clone(),
        source,
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| ReadError::Json {
            path: path.clone(),
            source,
        })?;
    let field = |name: &str| value.get(name).and_then(serde_json::Value::as_str);
    match (field("name"), field("version")) {
        (Some(name), Some(version)) => Ok(format!("{name} {version}")),
        _ => Err(refused()),
    }
}

/// Holds one map FerroBRIDGE writes to the rules of the module, a table map
/// of the value package when `table` holds, else a structure override.
fn check(
    map: &Map,
    guide: &BTreeMap<String, Map>,
    package: &str,
    table: bool,
) -> Result<(), EmitError> {
    let refused = |reason: String| EmitError::Input {
        id: map.id.clone(),
        reason,
    };
    if (map.kind == Kind::Table) != table {
        return Err(refused(String::from(if table {
            "the value package holds table maps only"
        } else {
            "the overrides hold structure maps only"
        })));
    }
    if !map
        .title
        .as_deref()
        .is_some_and(|title| title.starts_with("FerroBRIDGE supplement: "))
    {
        return Err(refused(String::from(
            "the title does not open with `FerroBRIDGE supplement: `",
        )));
    }
    let description = map.description.as_deref().unwrap_or_default();
    if !description.contains("FerroBRIDGE #") {
        return Err(refused(String::from(
            "the description names no tracker issue",
        )));
    }
    let own = format!("{CANONICAL_BASE}{}", map.id);
    if let Some(replaced) = guide.get(&map.id) {
        let opening = format!("Overrides ConceptMap/{} of {package}.", map.id);
        if !description.starts_with(&opening) {
            return Err(refused(format!(
                "the description does not open with `{opening}`"
            )));
        }
        let url = if table {
            own.as_str()
        } else {
            replaced.url.as_str()
        };
        if map.url != url {
            return Err(refused(format!("the url is not {url}")));
        }
    } else {
        if !description.starts_with("Adds ") {
            return Err(refused(String::from(
                "the description does not open with `Adds `",
            )));
        }
        if map.url != own {
            return Err(refused(format!("the url is not {own}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{CANONICAL_BASE, check};
    use crate::v2maps::read::{Kind, Map};

    fn map(id: &str, url: &str, kind: Kind, description: &str) -> Map {
        Map {
            id: id.to_owned(),
            url: url.to_owned(),
            title: Some(String::from("FerroBRIDGE supplement: a map")),
            description: Some(description.to_owned()),
            kind,
            rows: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn guide() -> BTreeMap<String, Map> {
        let id = "table-hl70202-to-contact-point-system";
        let url = format!("http://hl7.org/fhir/uv/v2mappings/ConceptMap/{id}");
        BTreeMap::from([(id.to_owned(), map(id, &url, Kind::Table, ""))])
    }

    const PACKAGE: &str = "hl7.fhir.uv.v2mappings 1.0.0";

    #[test]
    fn a_value_override_takes_ferrobridges_url() {
        let id = "table-hl70202-to-contact-point-system";
        let description =
            format!("Overrides ConceptMap/{id} of {PACKAGE}. Because. FerroBRIDGE #1762.");
        let own = map(
            id,
            &format!("{CANONICAL_BASE}{id}"),
            Kind::Table,
            &description,
        );
        assert!(check(&own, &guide(), PACKAGE, true).is_ok());
        let guides = map(
            id,
            &format!("http://hl7.org/fhir/uv/v2mappings/ConceptMap/{id}"),
            Kind::Table,
            &description,
        );
        assert!(check(&guides, &guide(), PACKAGE, true).is_err());
    }

    #[test]
    fn a_structure_map_in_the_value_package_is_refused() {
        let id = "segment-pid-to-patient";
        let refused = map(
            id,
            &format!("{CANONICAL_BASE}{id}"),
            Kind::Segment,
            "Adds a map. FerroBRIDGE #1.",
        );
        assert!(check(&refused, &guide(), PACKAGE, true).is_err());
        assert!(check(&refused, &guide(), PACKAGE, false).is_ok());
    }

    #[test]
    fn a_map_naming_no_issue_is_refused() {
        let id = "table-a-to-b";
        let refused = map(
            id,
            &format!("{CANONICAL_BASE}{id}"),
            Kind::Table,
            "Adds a map.",
        );
        assert!(check(&refused, &guide(), PACKAGE, true).is_err());
    }
}
