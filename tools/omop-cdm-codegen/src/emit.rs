// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `emit` command: the vendored definitions in, the generated `omop-cdm`
//! tree out, byte-deterministic.
//!
//! One run renders every table module into a scratch directory, formats it
//! with the pinned `rustfmt`, copies the four vendored DDL files beside them,
//! and then either replaces the tree in the crate or, in check mode, compares
//! it with what is on disk and reports every difference.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::definitions::{Definitions, LoadError};
use crate::dqd::{Catalogue, Dashboard, DqdError};
use crate::lower::{LowerError, Model};
use crate::render::{DDL_FILES, render_ddl, render_dqd, render_mod, render_table};

/// The directory of the generated modules, under the crate's `src/`.
pub const GENERATED_DIR: &str = "generated";

/// The directory the vendored DDL is copied into, under the crate root.
pub const DDL_DIR: &str = "ddl";

/// What to emit and where.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// The directory holding the two vendored definition CSV files.
    pub definitions_dir: PathBuf,
    /// The directory holding the four vendored PostgreSQL DDL files.
    pub ddl_dir: PathBuf,
    /// The directory holding the three vendored Data Quality Dashboard CSV
    /// files.
    pub dqd_dir: PathBuf,
    /// The generated crate directory (the one holding `Cargo.toml` and `src/`).
    pub crate_dir: PathBuf,
    /// The `rustfmt.toml` the output is formatted with, when it exists.
    pub rustfmt_config: PathBuf,
    /// Compare instead of writing.
    pub check: bool,
}

/// A failure while emitting.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// The definitions did not load.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// Lowering failed.
    #[error(transparent)]
    Lower(#[from] LowerError),
    /// The Data Quality Dashboard files did not load.
    #[error(transparent)]
    DqdLoad(#[from] crate::dqd::LoadError),
    /// The Data Quality Dashboard files did not lower.
    #[error(transparent)]
    Dqd(#[from] DqdError),
    /// Rendering to a string failed.
    #[error("rendering failed")]
    Render(#[from] std::fmt::Error),
    /// A file could not be read or written.
    #[error("cannot access {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// The generated tree could not be written, formatted or read back.
    #[error(transparent)]
    Tree(#[from] generated_tree::TreeError),
    /// Check mode found the tree out of date.
    #[error("the generated tree is out of date; {} file(s) differ: {}", .paths.len(), .paths.join(", "))]
    Drift {
        /// The files that differ, are missing, or are stale.
        paths: Vec<String>,
    },
}

/// What an emit run produced.
#[derive(Debug)]
pub struct EmitReport {
    /// The number of columns per table, in table-name order.
    pub columns: BTreeMap<String, usize>,
    /// The number of checks in the Data Quality Dashboard catalogue.
    pub checks: usize,
    /// The files written or checked, relative to the crate directory.
    pub files: Vec<String>,
}

/// Runs the pipeline per `options`.
///
/// # Errors
///
/// Returns [`EmitError`] for a load, lowering, I/O or `rustfmt` failure, and
/// [`EmitError::Drift`] in check mode when the tree differs from what the
/// emitter produces.
pub fn emit(options: &EmitOptions) -> Result<EmitReport, EmitError> {
    let definitions = Definitions::load(&options.definitions_dir)?;
    let model = Model::lower(&definitions)?;
    let catalogue = Catalogue::lower(&Dashboard::load(&options.dqd_dir)?, &model)?;

    let mut sources = BTreeMap::new();
    sources.insert(format!("{GENERATED_DIR}/mod.rs"), render_mod(&model)?);
    sources.insert(format!("{GENERATED_DIR}/ddl.rs"), render_ddl()?);
    sources.insert(format!("{GENERATED_DIR}/dqd.rs"), render_dqd(&catalogue)?);
    for table in &model.tables {
        sources.insert(
            format!("{GENERATED_DIR}/{}.rs", table.name),
            render_table(table)?,
        );
    }

    let scratch = tempfile::tempdir().map_err(|source| EmitError::Io {
        path: PathBuf::from("(temporary directory)"),
        source,
    })?;
    generated_tree::write(scratch.path(), &sources)?;
    generated_tree::rustfmt(
        sources.keys().map(|relative| scratch.path().join(relative)),
        &options.rustfmt_config,
    )?;
    let mut files = generated_tree::read(scratch.path(), sources.keys())?;
    for (name, content) in read_ddl(&options.ddl_dir)? {
        files.insert(name, content);
    }
    files.insert(format!("{DDL_DIR}/PROVENANCE.md"), provenance());

    let src = options.crate_dir.join("src");
    if options.check {
        let owned = [
            (src.join(GENERATED_DIR), GENERATED_DIR),
            (options.crate_dir.join(DDL_DIR), DDL_DIR),
        ];
        let drift = generated_tree::drift(
            &files,
            |relative| target_path(&options.crate_dir, &src, relative),
            &owned,
        )?;
        if !drift.is_empty() {
            return Err(EmitError::Drift { paths: drift });
        }
    } else {
        let generated = src.join(GENERATED_DIR);
        if generated.exists() {
            fs::remove_dir_all(&generated).map_err(|source| EmitError::Io {
                path: generated.clone(),
                source,
            })?;
        }
        let (modules, ddl): (BTreeMap<String, String>, BTreeMap<String, String>) = files
            .iter()
            .map(|(relative, content)| (relative.clone(), content.clone()))
            .partition(|(relative, _)| relative.starts_with(GENERATED_DIR));
        generated_tree::write(&src, &modules)?;
        generated_tree::write(&options.crate_dir, &ddl)?;
    }

    Ok(EmitReport {
        columns: model
            .tables
            .iter()
            .map(|table| (table.name.clone(), table.columns.len()))
            .collect(),
        checks: catalogue.checks.len(),
        files: files.keys().cloned().collect(),
    })
}

/// Returns where one rendered file belongs: a Rust module under `src/`, and
/// the DDL copies under the crate directory itself.
fn target_path(crate_dir: &Path, src: &Path, relative: &str) -> PathBuf {
    if relative.starts_with(GENERATED_DIR) {
        src.join(relative)
    } else {
        crate_dir.join(relative)
    }
}

/// Reads the four vendored DDL files, keyed by their path under `ddl/`.
fn read_ddl(ddl_dir: &Path) -> Result<BTreeMap<String, String>, EmitError> {
    let mut out = BTreeMap::new();
    for ddl in DDL_FILES {
        let path = ddl_dir.join(ddl.file);
        let content = fs::read_to_string(&path).map_err(|source| EmitError::Io {
            path: path.clone(),
            source,
        })?;
        out.insert(format!("{DDL_DIR}/{}", ddl.file), content);
    }
    Ok(out)
}

/// Returns the provenance note that travels with the copied DDL.
fn provenance() -> String {
    let mut out = String::from(
        "<!-- @generated by omop-cdm-codegen from OHDSI/CommonDataModel v5.5.0. DO NOT EDIT. -->\n\
         <!-- SPDX-FileCopyrightText: Cadasto B.V. -->\n\
         <!-- SPDX-License-Identifier: BUSL-1.1 -->\n\
         <!-- This file describes vendored third-party material; the bytes beside it\n\
         \x20    keep their upstream licence, not the licence of this repository. -->\n\
         \n\
         # Provenance: OHDSI's rendered PostgreSQL DDL for OMOP CDM v5.4\n\
         \n\
         Copied verbatim by `tools/omop-cdm-codegen` from\n\
         `docs/specs/omop-cdm/inst/ddl/5.4/postgresql/`, which\n\
         `scripts/vendor/omop-cdm.sh` vendors from\n\
         <https://github.com/OHDSI/CommonDataModel> at tag `v5.5.0`. The upstream\n\
         licence is `Apache License 2.0`; the source tree's\n\
         `docs/specs/omop-cdm/PROVENANCE.md` carries the commit and the digests.\n\
         \n\
         The copy lives inside the crate because `include_str!` has to reach the\n\
         files in a packaged crate, and `omop-cdm-codegen -- emit --check` fails\n\
         when a byte here stops matching the vendored tree.\n\
         \n\
         ## Files\n\
         \n",
    );
    for ddl in DDL_FILES {
        out.push_str("- `");
        out.push_str(ddl.file);
        out.push_str("`\n");
    }
    out
}
