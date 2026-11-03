// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A mapping set loaded from text: the same set and the same refusals as
//! the set loaded from the same files on disk.

use core::error::Error;
use std::path::PathBuf;

use fhirconnect::model::load::load_set;
use fhirconnect::model::load::load_set_from_text;
use fhirconnect::model::semantic::StaticMappingCodes;

use crate::support::FIXTURES;

/// The files of the repeated-start project, every one of the directory.
fn project_files() -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(format!("{FIXTURES}/projects/ferrobridge/repeated_start"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "yml") {
            files.push(path);
        }
    }
    Ok(files)
}

#[test]
fn a_set_from_text_is_the_set_from_its_files() -> Result<(), Box<dyn Error>> {
    let files = project_files()?;
    let codes = StaticMappingCodes::default();
    let from_disk = load_set(&files, &codes).map_err(|diagnostics| format!("{diagnostics:?}"))?;
    let mut texts = Vec::new();
    for file in files.iter().rev() {
        texts.push((file.clone(), std::fs::read_to_string(file)?));
    }
    let from_text =
        load_set_from_text(texts, &codes).map_err(|diagnostics| format!("{diagnostics:?}"))?;
    assert_eq!(from_text, from_disk);
    Ok(())
}

#[test]
fn a_text_that_is_no_mapping_is_refused_under_its_name() {
    let refused = load_set_from_text(
        [("synthetic/broken.yml", "grammar: [")],
        &StaticMappingCodes::default(),
    );
    let Err(diagnostics) = refused else {
        panic!("a broken text should be refused");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.to_string().contains("synthetic/broken.yml")),
        "{diagnostics:?}"
    );
}
