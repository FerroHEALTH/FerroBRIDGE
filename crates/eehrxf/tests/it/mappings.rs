// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shipped FHIRconnect projects: each embeds exactly the files of its
//! directory under `mappings/`, as they stand, and writes them out again.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use eehrxf::mappings::Project;

/// The directory of the shipped project `project`.
fn directory(project: &Project) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("mappings")
        .join(project.directory())
}

/// Asserts that `project` embeds every mapping file of its directory, and
/// only those, each with the text on disk.
fn embeds_its_directory(project: &Project) -> Result<(), Box<dyn Error>> {
    let on_disk: BTreeSet<String> = std::fs::read_dir(directory(project))?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .filter(|name| {
            name.as_ref().map_or(true, |name| {
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension == "yml")
            })
        })
        .collect::<Result<_, _>>()?;
    let embedded: BTreeSet<String> = project
        .files()
        .iter()
        .map(|file| file.name.to_owned())
        .collect();
    assert_eq!(embedded, on_disk, "{}", project.directory());
    assert_eq!(
        embedded.len(),
        project.files().len(),
        "no file is listed twice"
    );
    for file in project.files() {
        let text = std::fs::read_to_string(directory(project).join(file.name))?;
        assert_eq!(file.text, text, "{}", file.name);
    }
    Ok(())
}

/// Asserts that `project` writes its files where it is told to, in order.
fn writes_its_files(project: &Project) -> Result<(), Box<dyn Error>> {
    let target = std::env::temp_dir().join(format!(
        "eehrxf-shipped-{}-{}",
        project.directory(),
        std::process::id()
    ));
    std::fs::create_dir_all(&target)?;
    let written = project.write_to(&target)?;
    let expected: Vec<PathBuf> = project
        .files()
        .iter()
        .map(|file| target.join(file.name))
        .collect();
    assert_eq!(written, expected);
    for (path, file) in written.iter().zip(project.files()) {
        assert_eq!(std::fs::read_to_string(path)?, file.text);
    }
    std::fs::remove_dir_all(&target)?;
    Ok(())
}

#[cfg(any(feature = "prescription", feature = "dispensation"))]
#[test]
fn the_mpd_project_embeds_its_directory() -> Result<(), Box<dyn Error>> {
    embeds_its_directory(&eehrxf::mappings::MPD)?;
    writes_its_files(&eehrxf::mappings::MPD)
}

#[cfg(feature = "laboratory")]
#[test]
fn the_laboratory_project_embeds_its_directory() -> Result<(), Box<dyn Error>> {
    embeds_its_directory(&eehrxf::mappings::LABORATORY)?;
    writes_its_files(&eehrxf::mappings::LABORATORY)
}

#[cfg(feature = "patient-summary")]
#[test]
fn the_eps_project_embeds_its_directory() -> Result<(), Box<dyn Error>> {
    embeds_its_directory(&eehrxf::mappings::EPS)?;
    writes_its_files(&eehrxf::mappings::EPS)
}
