// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `emit` command: packages in, generated crate out, byte-deterministic.
//!
//! One run emits every FHIR version in [`EmitOptions::versions`]: each
//! package lowers to its own module, and one `lib.rs` declares them all. One
//! tree holds the union of the declared root sets, and each emitted item
//! carries the `cfg` of the narrowest feature that selects it. The
//! pipeline renders every file into a scratch directory, formats it with the
//! pinned `rustfmt`, and then either replaces the generated tree or, in check
//! mode, compares it with the tree on disk and reports every difference.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use generated_tree::Formatting;

use crate::closure::{ClosureError, TypeClosure};
use crate::lower::{LowerError, VersionModule};
use crate::operations::{
    OperationContract, OperationError, render_descriptor_module, render_operation,
    render_operations_mod,
};
use crate::package::{LoadError, Package};
use crate::render::{render_lib, render_module, render_version_mod};
use crate::roots::{MissingRoot, RootScope, RootSet};
use crate::search::Registry;

/// One FHIR version to emit: its module name and its vendored package.
#[derive(Debug, Clone)]
pub struct VersionInput {
    /// The module name, for example `r4b`.
    pub module: String,
    /// The vendored package directory (the one holding `package/`).
    pub package_dir: PathBuf,
}

/// What to emit and where.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// The versions to emit, in module-name order.
    pub versions: Vec<VersionInput>,
    /// The generated crate directory (the one holding `Cargo.toml` and `src/`).
    pub crate_dir: PathBuf,
    /// Compare instead of writing.
    pub check: bool,
}

/// A failure while emitting.
#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    /// No version was given.
    #[error("no FHIR version to emit")]
    NoVersions,
    /// The package did not load.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// A root resource is missing.
    #[error(transparent)]
    MissingRoot(#[from] MissingRoot),
    /// The closure did not compute.
    #[error(transparent)]
    Closure(#[from] ClosureError),
    /// Lowering failed.
    #[error(transparent)]
    Lower(#[from] LowerError),
    /// Rendering to a string failed.
    #[error("rendering failed")]
    Render(#[from] std::fmt::Error),
    /// An operation contract failed to lower.
    #[error(transparent)]
    Operation(#[from] OperationError),
    /// A search parameter registry failed to lower.
    #[error(transparent)]
    Search(#[from] crate::search::SearchError),
    /// The generated tree could not be written, formatted or read back.
    #[error(transparent)]
    Tree(#[from] generated_tree::TreeError),
}

/// What an emit run produced.
#[derive(Debug)]
pub struct EmitReport {
    /// The number of types per version module.
    pub types: BTreeMap<String, usize>,
    /// The files written or checked, relative to `src/`.
    pub files: Vec<String>,
}

/// Runs the pipeline per `options`.
///
/// # Errors
///
/// Returns [`EmitError`] for a load, closure, lowering, I/O, or `rustfmt`
/// failure, and [`generated_tree::TreeError::Drift`] in check mode when the tree differs.
pub fn emit(options: &EmitOptions) -> Result<EmitReport, EmitError> {
    if options.versions.is_empty() {
        return Err(EmitError::NoVersions);
    }
    let mut versions: Vec<&VersionInput> = options.versions.iter().collect();
    versions.sort_by(|a, b| a.module.cmp(&b.module));

    let mut packages = Vec::with_capacity(versions.len());
    for version in &versions {
        packages.push(Package::open(&version.package_dir)?);
    }
    let models = lower_models(&versions, &packages)?;
    let mut registries = Vec::with_capacity(models.len());
    for (package, model) in packages.iter().zip(&models) {
        registries.push(Registry::lower(package, model)?);
    }
    let files = render_files(&models, &registries)?;

    let formatted = formatted(&files, &options.crate_dir)?;

    let module_names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    let target_src = options.crate_dir.join("src");
    if options.check {
        let owned: Vec<(PathBuf, &str)> = module_names
            .iter()
            .map(|module| (target_src.join(module), *module))
            .collect();
        generated_tree::check(&formatted, |relative| target_src.join(relative), &owned)?;
    } else {
        replace_tree(&target_src, &formatted, &module_names)?;
    }
    Ok(EmitReport {
        types: models
            .iter()
            .map(|m| (m.name.clone(), m.types.len()))
            .collect(),
        files: formatted.keys().cloned().collect(),
    })
}

/// Renders every file of the generated crate, keyed by its path under `src/`.
///
/// The shared files carry the crate banner over the two runtime templates; the
/// rest are rendered per version module.
fn render_files(
    models: &[VersionModule],
    registries: &[Registry],
) -> Result<BTreeMap<String, String>, EmitError> {
    let crate_banner = crate::render::crate_banner(models);
    let mut files = BTreeMap::new();
    files.insert(String::from("lib.rs"), render_lib(models)?);
    files.insert(
        String::from("operation.rs"),
        render_descriptor_module(&crate_banner),
    );
    files.insert(
        String::from("codec.rs"),
        format!("{crate_banner}{}", include_str!("templates/codec.rs")),
    );
    files.insert(
        String::from("schema.rs"),
        format!("{crate_banner}{}", include_str!("templates/schema.rs")),
    );
    files.insert(
        String::from("definition.rs"),
        format!("{crate_banner}{}", include_str!("templates/definition.rs")),
    );
    files.insert(
        String::from("xml.rs"),
        format!("{crate_banner}{}", include_str!("templates/xml.rs")),
    );
    let codes: BTreeSet<&str> = registries.iter().flat_map(Registry::type_codes).collect();
    files.insert(
        String::from("search.rs"),
        crate::search::render_shared(&crate_banner, &codes)?,
    );
    for (model, registry) in models.iter().zip(registries) {
        render_version_files(model, &mut files)?;
        files.insert(
            format!("{}/search_parameters.rs", model.name),
            crate::search::render(model, registry)?,
        );
    }
    Ok(files)
}

/// Renders one version module's files into `files`.
fn render_version_files(
    model: &VersionModule,
    files: &mut BTreeMap<String, String>,
) -> Result<(), EmitError> {
    files.insert(format!("{}/mod.rs", model.name), render_version_mod(model)?);
    files.insert(
        format!("{}/schema.rs", model.name),
        crate::render_schema::render_schema(model)?,
    );
    if crate::render_definition::defines_element_definition(model) {
        files.insert(
            format!("{}/definition.rs", model.name),
            crate::render_definition::render_definition(model)?,
        );
    }
    for (module, types) in model.modules() {
        files.insert(
            format!("{}/{module}.rs", model.name),
            render_module(model, module, &types)?,
        );
    }
    let banner = crate::render::banner(model);
    files.insert(
        format!("{}/operations/mod.rs", model.name),
        render_operations_mod(&banner, &model.name.to_uppercase(), &model.operations)?,
    );
    for contract in &model.operations {
        files.insert(
            format!("{}/operations/{}.rs", model.name, contract.module),
            render_operation(&banner, contract)?,
        );
    }
    Ok(())
}

/// Removes the stale version directories under `target_src`, then writes `files`.
fn replace_tree(
    target_src: &Path,
    files: &BTreeMap<String, String>,
    version_modules: &[&str],
) -> Result<(), EmitError> {
    for module in version_modules {
        generated_tree::clear(&target_src.join(module))?;
    }
    Ok(generated_tree::write(target_src, files)?)
}

/// Lowers every version's module and operation contracts, the terminology
/// ecosystem overlay applied.
fn lower_models(
    versions: &[&VersionInput],
    packages: &[Package],
) -> Result<Vec<VersionModule>, EmitError> {
    let mut root_sets = Vec::with_capacity(packages.len());
    for package in packages {
        root_sets.push(RootSet::select(package, RootScope::Resources)?);
    }
    // NOTE: the overlay pre-adopts R6 parameters into every earlier version
    // (`crate::ecosystem`); the R6 module is the source, named `r6`.
    let r6 = versions
        .iter()
        .zip(&root_sets)
        .find(|(version, _)| version.module == "r6")
        .map(|(_, roots)| roots);
    let mut models = Vec::with_capacity(versions.len());
    for ((version, package), roots) in versions.iter().zip(packages).zip(&root_sets) {
        let closure = TypeClosure::compute(package, roots)?;
        let mut model = VersionModule::lower(
            &closure,
            &version.module,
            &package.manifest().name,
            &package.manifest().version,
        )?;
        let mut contracts = Vec::new();
        // NOTE: no specification governs this: our own design; the overlay serves the
        // R4 to R5 servers of the terminology ecosystem, and none speaks STU3 through it.
        let overlaid = !package.manifest().is_stu3();
        for (url, operation) in &roots.operations {
            let source = r6.and_then(|roots| roots.operations.get(url).copied());
            for resource in &operation.resource {
                contracts.push(if overlaid {
                    OperationContract::lower_overlaid(operation, resource, &model, source)?
                } else {
                    OperationContract::lower(operation, resource, &model)?
                });
            }
        }
        contracts.sort_by(|a, b| a.module.cmp(&b.module));
        model.operations = contracts;
        models.push(model);
    }
    Ok(models)
}

/// Returns the rendered tree as the pinned `rustfmt` formats it.
///
/// `rustfmt` formats a file together with every out-of-line module it
/// declares, so the tree is formatted once, from its crate root, which every
/// rendered file is reachable from. Naming the other files as well would
/// format each `mod.rs` subtree again (#433).
pub(crate) fn formatted(
    files: &BTreeMap<String, String>,
    crate_dir: &Path,
) -> Result<BTreeMap<String, String>, generated_tree::TreeError> {
    generated_tree::formatted(
        files,
        Formatting::FromRoot("lib.rs"),
        &crate_dir.join("../../rustfmt.toml"),
    )
}
