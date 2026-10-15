// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR search over AQL projections declared beside the context mappings.
//!
//! FHIRconnect "does not focus specifically on AQL and `FHIRsearch`", so the
//! whole surface is FerroBRIDGE's own design (`docs/architecture.md` §4.6).
//! A context answers search only when a [`declaration`] file names it; the
//! declaration is checked and every query it can build is parsed with
//! `openehr-query` when the server boots, so a declaration that does not
//! parse refuses the start. A request is read into [`criteria`], run as one
//! AQL query per declaration over `POST /query/aql` with the CDR's own paging,
//! and each row is mapped back through the context's compiled program.
//!
//! One resource type answers one parameter set: every declaration of a type
//! states the same parameters, and the `CapabilityStatement` names exactly
//! those. Two declarations of one type over one template are refused, so a
//! composition is never found twice and a searchset carries one entry per
//! composition.

pub mod aql;
pub mod bundle;
pub mod criteria;
pub mod cursor;
pub mod declaration;
pub mod plan;
pub mod template;
pub mod value;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use crate::facade::programs::Programs;
use crate::facade::search::aql::Conditions;
use crate::facade::search::aql::OrderKey;
use crate::facade::search::declaration::DeclarationError;
use crate::facade::search::declaration::Parameter;
use crate::facade::search::declaration::ParameterType;

/// The page sizes a deployment configured.
///
/// "Servers SHALL NOT return more resources than requested … but may return
/// less than the client requested" (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.5.3), so a `_count` above `max` is answered with `max` and the page
/// links name the size used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The page size of a search that names no `_count`.
    pub default_count: u32,
    /// The largest page one search answers.
    pub max_count: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            default_count: 20,
            max_count: 100,
        }
    }
}

/// The reference parameters that scope a search to the EHR of one subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientScope {
    /// The parameter names, in declaration order.
    names: Vec<String>,
    /// Whether a search without one of them is refused.
    required: bool,
}

impl PatientScope {
    /// Returns the parameter names, in declaration order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Whether a search without one of them is refused.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }
}

/// One checked declaration, bound to the program it searches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The file it was read from.
    file: PathBuf,
    /// The `metadata.name` of the context it searches.
    context: String,
    /// The resource type that context maps.
    resource_type: String,
    /// The template that context maps.
    template_id: String,
    /// The subject parameters, when any.
    patient: Option<PatientScope>,
    /// The total order every page is read in.
    order: Vec<String>,
    /// The declared parameters, by name.
    parameters: BTreeMap<String, Parameter>,
}

impl Declared {
    /// Returns the file it was read from.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Returns the `metadata.name` of the context it searches.
    #[must_use]
    pub fn context(&self) -> &str {
        &self.context
    }

    /// Returns the resource type it answers.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the template it reads.
    #[must_use]
    pub fn template_id(&self) -> &str {
        &self.template_id
    }

    /// Returns the subject parameters, when any.
    #[must_use]
    pub const fn patient(&self) -> Option<&PatientScope> {
        self.patient.as_ref()
    }

    /// Returns the total order, first key first.
    #[must_use]
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// Returns the declared parameters, by name.
    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<String, Parameter> {
        &self.parameters
    }

    /// Returns the parameter names with their types, the patient section
    /// included, which the `CapabilityStatement` declares.
    #[must_use]
    pub fn surface(&self) -> BTreeMap<String, ParameterType> {
        let mut surface: BTreeMap<String, ParameterType> = self
            .parameters
            .iter()
            .map(|(name, parameter)| (name.clone(), parameter.kind()))
            .collect();
        for name in self.patient.iter().flat_map(|patient| patient.names.iter()) {
            surface.insert(name.clone(), ParameterType::Reference);
        }
        surface
    }
}

/// Every declaration this deployment loaded, by resource type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The declarations of each type, in context-name order.
    by_type: BTreeMap<String, Vec<Declared>>,
}

impl Catalog {
    /// Returns the declarations of `resource_type`, in context-name order.
    #[must_use]
    pub fn of_type(&self, resource_type: &str) -> &[Declared] {
        self.by_type.get(resource_type).map_or(&[], Vec::as_slice)
    }

    /// Whether no declaration was loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }

    /// Returns the parameter set `resource_type` answers, when it answers
    /// search.
    #[must_use]
    pub fn surface(&self, resource_type: &str) -> Option<BTreeMap<String, ParameterType>> {
        self.of_type(resource_type).first().map(Declared::surface)
    }
}

/// Reads every declaration under `directory` and binds each to its program.
///
/// # Errors
///
/// Returns the [`DeclarationError`] naming the first file refused: one that
/// does not read or parse, names an unknown context, declares a parameter or
/// a path the checks refuse, builds a query that does not parse, repeats a
/// type and template another declaration answers, or states another parameter
/// set than its type's first declaration.
pub fn load(directory: &Path, programs: &Programs) -> Result<Catalog, DeclarationError> {
    let files = crate::walk::files(directory, declaration::is_declaration).map_err(
        |crate::walk::Error { path, source }| DeclarationError::Read { file: path, source },
    )?;
    let mut catalog = Catalog::default();
    for file in files {
        let written = declaration::read(&file)?;
        let declared = bind(&file, written, programs)?;
        insert(&mut catalog, declared)?;
    }
    tracing::info!(
        lane = "facade",
        types = catalog
            .by_type
            .keys()
            .cloned()
            .collect::<Vec<String>>()
            .join(","),
        "the FHIR facade searches these resource types"
    );
    Ok(catalog)
}

/// Checks one parsed declaration and binds it to its program.
///
/// # Errors
///
/// Returns the [`DeclarationError`] naming the first fault.
pub fn bind(
    file: &Path,
    written: declaration::DeclarationFile,
    programs: &Programs,
) -> Result<Declared, DeclarationError> {
    let loaded =
        programs
            .by_context(&written.context)
            .ok_or_else(|| DeclarationError::UnknownContext {
                file: file.to_path_buf(),
                context: written.context.clone(),
            })?;
    let patient = match written.patient {
        Some(section) => Some(check_patient(file, section)?),
        None => None,
    };
    let patient_names = patient
        .as_ref()
        .map(|scope| scope.names.clone())
        .unwrap_or_default();
    let parameters =
        declaration::check_parameters(file, &written.parameters, &patient_names, loaded.index())?;
    let order = written.order.unwrap_or_else(|| {
        aql::DEFAULT_ORDER
            .iter()
            .map(|path| (*path).to_owned())
            .collect()
    });
    if order.is_empty() {
        return Err(DeclarationError::Path {
            file: file.to_path_buf(),
            name: String::from("order"),
            path: String::new(),
            reason: String::from("a declared order names at least one path"),
        });
    }
    let declared = Declared {
        file: file.to_path_buf(),
        context: written.context,
        resource_type: String::from(loaded.program().resource().as_str()),
        template_id: String::from(loaded.program().template().id().as_str()),
        patient,
        order,
        parameters,
    };
    check_paths(&declared)?;
    Ok(declared)
}

/// Checks the `patient` section.
fn check_patient(
    file: &Path,
    section: declaration::PatientFile,
) -> Result<PatientScope, DeclarationError> {
    if section.parameters.is_empty() {
        return Err(DeclarationError::Name {
            file: file.to_path_buf(),
            name: String::from("patient"),
            reason: "the patient section names at least one parameter",
        });
    }
    let mut seen: Vec<&String> = Vec::new();
    for name in &section.parameters {
        declaration::check_name(file, name)?;
        if seen.contains(&name) {
            return Err(DeclarationError::Name {
                file: file.to_path_buf(),
                name: name.clone(),
                reason: "the patient section names it twice",
            });
        }
        seen.push(name);
    }
    Ok(PatientScope {
        names: section.parameters,
        required: section.required,
    })
}

/// Checks every path one declaration names, alone and inside the widest
/// query it can build.
fn check_paths(declared: &Declared) -> Result<(), DeclarationError> {
    let refuse = |name: &str, path: &str, reason: String| DeclarationError::Path {
        file: declared.file.clone(),
        name: name.to_owned(),
        path: path.to_owned(),
        reason,
    };
    for path in &declared.order {
        aql::check_path(path).map_err(|reason| refuse("order", path, reason))?;
    }
    let mut conditions = Conditions::new(&declared.template_id);
    let mut order: Vec<OrderKey> = Vec::new();
    for (name, parameter) in &declared.parameters {
        let probe = if parameter.node().is_some() {
            aql::NODE_ALIAS
        } else {
            "c"
        };
        for path in parameter.paths(probe) {
            aql::check_path(&path).map_err(|reason| refuse(name, &path, reason))?;
        }
        let alias = conditions.alias(parameter.node());
        for path in parameter.paths(&alias) {
            let bound = conditions.bind("probe");
            conditions.push(format!("{path} = {bound}"));
            conditions.push(format!("NOT EXISTS {path}"));
        }
        if parameter.sortable() {
            order.push(OrderKey {
                path: parameter.value_path("c"),
                descending: true,
            });
        }
    }
    order.extend(declared.order.iter().map(|path| OrderKey {
        path: path.clone(),
        descending: false,
    }));
    for text in [
        aql::rows_text(&conditions, &order),
        aql::count_text(&conditions),
    ] {
        aql::parses(&text).map_err(|source| DeclarationError::Query {
            file: declared.file.clone(),
            source,
        })?;
    }
    Ok(())
}

/// Files `declared` under its type, refusing a second declaration over one
/// template and a parameter set that differs from the type's first.
fn insert(catalog: &mut Catalog, declared: Declared) -> Result<(), DeclarationError> {
    let entries = catalog
        .by_type
        .entry(declared.resource_type.clone())
        .or_default();
    if let Some(twin) = entries
        .iter()
        .find(|entry| entry.template_id == declared.template_id)
    {
        return Err(DeclarationError::Duplicate {
            first: twin.file.clone(),
            second: declared.file,
            resource_type: declared.resource_type,
            template: declared.template_id,
        });
    }
    if let Some(first) = entries.first()
        && (first.surface() != declared.surface()
            || sortability(first) != sortability(&declared)
            || first.patient.as_ref().map(PatientScope::required)
                != declared.patient.as_ref().map(PatientScope::required))
    {
        return Err(DeclarationError::Inconsistent {
            first: first.file.clone(),
            second: declared.file,
            resource_type: declared.resource_type,
        });
    }
    entries.push(declared);
    entries.sort_by(|left, right| left.context.cmp(&right.context));
    Ok(())
}

/// Returns which declared parameters `_sort` may order by, by name.
fn sortability(declared: &Declared) -> BTreeMap<&str, bool> {
    declared
        .parameters
        .iter()
        .map(|(name, parameter)| (name.as_str(), parameter.sortable()))
        .collect()
}
