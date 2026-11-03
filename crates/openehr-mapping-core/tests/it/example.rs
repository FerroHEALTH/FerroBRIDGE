// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic example composition of every composition template the
//! testkit carries, each one the builder accepts and validates.

use std::error::Error;
use std::path::{Path, PathBuf};

use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::{PathError, TemplateSource};

/// The testkit's operational templates.
const OPTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt"
);

/// The timestamp the builder fills the `ctx/time` default from.
const NOW: &str = "2026-11-03T09:00:00Z";

/// Returns every `.opt` file under `directory`, sorted.
fn opts(directory: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(opts(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "opt") {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// Returns the index of the template at `path`.
fn index(path: &Path) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(path)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

#[test]
fn every_composition_template_has_an_example_the_builder_accepts() -> Result<(), Box<dyn Error>> {
    let mut built = 0;
    for path in opts(Path::new(OPTS))? {
        let index = index(&path)?;
        if index.web_template().tree.rm_type != "COMPOSITION" {
            assert!(
                matches!(
                    index.example_composition(NOW),
                    Err(PathError::NotComposition { .. })
                ),
                "{}",
                path.display()
            );
            continue;
        }
        let example = index
            .example_composition(NOW)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        assert_eq!(index.template_id(), example.template_id());
        let reread = index.accept(example.into_value());
        assert!(reread.is_ok(), "{}: {reread:?}", path.display());
        built += 1;
    }
    assert!(built >= 10, "only {built} composition templates were read");
    Ok(())
}

#[test]
fn an_example_holds_synthetic_content_beside_its_context() -> Result<(), Box<dyn Error>> {
    let index = index(&Path::new(OPTS).join("diagnose.opt"))?;
    let example = index.example_composition(NOW)?;
    let flat = index.flatten(&example)?;
    assert!(
        flat.keys().any(|key| key.contains('/')
            && !key.starts_with("ctx")
            && !key.ends_with("/language|code")
            && !key.contains("composer")),
        "{flat:?}"
    );
    Ok(())
}
