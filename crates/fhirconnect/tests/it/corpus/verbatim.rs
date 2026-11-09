// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a library file a chain reaches runs verbatim.
//!
//! A library file counts only when it runs as published, so a file loses
//! the chain when a stand-in applied after it overwrites a method it defines
//! or appends to (coordinator ruling on #1546, 2026-11-09).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use fhirconnect::model::ast::ModelMappingFile;
use fhirconnect::model::ast::keyword::ExtensionMethod;
use fhirconnect::model::load::load_model_file;
use fhirconnect::resolve::program::Program;

use super::CORPUS;
use super::collect;
use crate::support::FIXTURES;

/// Returns every model and extension file under the test fixtures, by
/// `metadata.name`.
///
/// A chain's stand-ins are fixtures, so this is where an extension a program
/// applied and no corpus file of the chain supplies is read from.
pub(super) fn fixture_models() -> Result<BTreeMap<String, Vec<ModelMappingFile>>, Box<dyn Error>> {
    let root = PathBuf::from(FIXTURES);
    let mut paths = Vec::new();
    collect(&root, &root, &mut paths)?;
    paths.sort();
    let mut by_name: BTreeMap<String, Vec<ModelMappingFile>> = BTreeMap::new();
    for relative in paths {
        // NOTE: a fixture that does not load as a model file (a context, or a
        // refusal a test pins) is no extension a compiled program applied.
        let Ok(file) = load_model_file(root.join(&relative)) else {
            continue;
        };
        by_name
            .entry(file.header().name().value().as_str().to_owned())
            .or_default()
            .push(file);
    }
    Ok(by_name)
}

/// Whether `file` defines a method one of `overwritten` names: a top-level
/// mapping of that name, or an `append` onto it.
fn defines_any(file: &ModelMappingFile, overwritten: &BTreeSet<&str>) -> bool {
    file.mappings().iter().any(|mapping| {
        let appended_to = mapping
            .append_to
            .as_ref()
            .filter(|_| {
                mapping
                    .extension
                    .as_ref()
                    .is_some_and(|method| *method.value() == ExtensionMethod::Append)
            })
            .and_then(|path| path.value().split('.').next());
        overwritten.contains(mapping.name.value().as_str())
            || appended_to.is_some_and(|name| overwritten.contains(name))
    })
}

/// Returns the corpus files of a chain whose methods a stand-in replaces.
///
/// A library file counts only when it runs verbatim, so a file loses the
/// chain when a fixture extension applied after it overwrites a method it
/// defines or appends to (coordinator ruling on #1546, 2026-11-09).
pub(super) fn replaced_files(
    files: &[String],
    program: &Program,
    fixtures: &BTreeMap<String, Vec<ModelMappingFile>>,
) -> BTreeSet<String> {
    let mut published: BTreeMap<String, (String, ModelMappingFile)> = BTreeMap::new();
    for relative in files {
        // NOTE: a context file of the chain defines no method to replace.
        let Ok(file) = load_model_file(PathBuf::from(CORPUS).join(relative)) else {
            continue;
        };
        let name = file.header().name().value().as_str().to_owned();
        published.insert(name, (relative.clone(), file));
    }
    let mut replaced = BTreeSet::new();
    for binding in program.models() {
        let applied = binding.extensions();
        for (index, extension) in applied.iter().enumerate() {
            if published.contains_key(extension.as_str()) {
                continue;
            }
            for stand_in in fixtures.get(extension.as_str()).into_iter().flatten() {
                let overwritten: BTreeSet<&str> = stand_in
                    .mappings()
                    .iter()
                    .filter(|mapping| {
                        mapping
                            .extension
                            .as_ref()
                            .is_some_and(|method| *method.value() == ExtensionMethod::Overwrite)
                    })
                    .map(|mapping| mapping.name.value().as_str())
                    .collect();
                let targets = core::iter::once(binding.name()).chain(applied.iter().take(index));
                for target in targets {
                    if let Some((relative, file)) = published.get(target.as_str())
                        && defines_any(file, &overwritten)
                    {
                        replaced.insert(relative.clone());
                    }
                }
            }
        }
    }
    replaced
}

/// Returns the files of a chain a stand-in replaces a method of.
fn replaced_in(chain: crate::support::Chain) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let (files, program) = chain;
    let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
    Ok(replaced_files(&files, &program, &fixture_models()?))
}

#[test]
fn a_file_a_stand_in_replaces_a_method_of_does_not_count() -> Result<(), Box<dyn Error>> {
    let expected = |files: &[&str]| -> BTreeSet<String> {
        files.iter().map(|file| (*file).to_owned()).collect()
    };
    // `ferrobridge_procedure_name` overwrites `name`, which `bfarm` appends to.
    assert_eq!(
        replaced_in(crate::prozedur::kds_prozedur_chain()?)?,
        expected(&["projects/org.highmed/KDS/procedure/KDS_procedure.v1.yml"])
    );
    // `ferrobridge_problem_qualifier_terminology_id` overwrites `statusCoded`.
    assert_eq!(
        replaced_in(crate::todesursache::kds_todesursache_chain()?)?,
        expected(&["model/cluster/org.openehr/problem_qualifier.v2.yml"])
    );
    assert_eq!(
        replaced_in(crate::person::kds_person_chain()?)?,
        expected(&[
            "model/admin_entry/org.highmed/person_data.v0.yml",
            "model/cluster/org.openehr/structured_name.v1.yml",
        ])
    );
    Ok(())
}
