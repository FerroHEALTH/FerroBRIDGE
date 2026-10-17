// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search declaration file, read beside the context mapping it serves.
//!
//! FHIRconnect "does not focus specifically on AQL and `FHIRsearch`", so no
//! specification governs this file: our own design. One file per context,
//! named `<anything>.search.yml` (or `.search.yaml`) anywhere in the mapping
//! tree, states which search parameters the context's resource type answers
//! and the AQL path each one compares. A parameter read under a repeated
//! node names that node, an RM class and an archetype id, and the query binds
//! it in its `FROM`, so every path of one parameter is read from one node: a
//! token's code and system are always one coded value's two parts. Every key
//! is checked when the file is read, and each path is placed in the context's
//! template: an unknown key, a parameter type outside [`ParameterType`], a
//! node the template does not hold, a path crossing a node that may occur
//! more than once below the bound one, a token over no coded value, or a
//! context no loaded program compiled is a boot refusal naming the file.

pub mod file;

mod check;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use fhir_types::r4::search_parameters::SEARCH_PARAMETERS;
use fhir_types::search::SearchParameter;
use openehr_mapping_core::index::WebTemplateIndex;
use serde::Deserialize;

use crate::facade::search::declaration::file::ParameterFile;

/// The suffixes a search declaration's file name ends in.
///
/// The FHIRconnect loader reads every `.yml` and `.yaml` file of the mapping
/// tree, so a declaration carries a second extension both walkers skip.
pub const DECLARATION_SUFFIXES: [&str; 2] = [".search.yml", ".search.yaml"];

/// Whether `path` names a search declaration rather than a mapping file.
#[must_use]
pub fn is_declaration(path: &Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| {
            DECLARATION_SUFFIXES
                .iter()
                .any(|suffix| name.ends_with(suffix))
        })
}

/// One search declaration file, as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationFile {
    /// The `metadata.name` of the context mapping this file declares search
    /// for.
    pub context: String,
    /// The parameters that address the subject of the EHR, when any do.
    pub patient: Option<PatientFile>,
    /// The AQL paths that give every page a stable total order, first key
    /// first.
    pub order: Option<Vec<String>>,
    /// The declared search parameters, by name.
    #[serde(default)]
    pub parameters: BTreeMap<String, ParameterFile>,
}

/// The `patient` section: the reference parameters that scope a search to
/// one EHR.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatientFile {
    /// The parameter names that address the subject, `patient` or `subject`.
    pub parameters: Vec<String>,
    /// Whether a search without one of them is refused.
    #[serde(default)]
    pub required: bool,
}

/// The FHIR search parameter types a declaration may name.
///
/// The codes are of `http://hl7.org/fhir/ValueSet/search-param-type`
/// (<https://hl7.org/fhir/R4/search.html#ptypes>); `reference` is the
/// `patient` section's alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParameterType {
    /// A code, optionally with its system.
    Token,
    /// A string, matched from its start unless a modifier says otherwise.
    String,
    /// A date or date-time, compared as a range.
    Date,
    /// A reference to the subject of the EHR.
    Reference,
}

impl ParameterType {
    /// Returns the code as the value set spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::String => "string",
            Self::Date => "date",
            Self::Reference => "reference",
        }
    }
}

/// The parameter types a declared parameter may carry, as the error names
/// them.
const DECLARABLE: &str = "token, string, date";

/// A node one parameter is read under, checked against the template.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Node {
    /// The RM class of the archetype root.
    pub class: String,
    /// The archetype id of the root.
    pub archetype: String,
}

/// The RM type a token's coded value carries, which places its code and its
/// system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coded {
    /// A `DV_TEXT` or `DV_CODED_TEXT`: `defining_code/code_string` and
    /// `defining_code/terminology_id/value`.
    Text,
    /// A `CODE_PHRASE`: `code_string` and `terminology_id/value`.
    Phrase,
}

/// One checked parameter: where it is read and what it compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// The FHIR type.
    kind: ParameterType,
    /// The node the path is read under, `None` for the composition.
    node: Option<Node>,
    /// The path, relative to the node or to the composition.
    path: String,
    /// The coded value's type, for a `token`.
    coded: Option<Coded>,
    /// The FHIR system URI to openEHR terminology id table of a `token`.
    systems: BTreeMap<String, String>,
}

impl Parameter {
    /// Returns the FHIR type of this parameter.
    #[must_use]
    pub const fn kind(&self) -> ParameterType {
        self.kind
    }

    /// Returns the node the paths are read under, `None` for the
    /// composition.
    #[must_use]
    pub const fn node(&self) -> Option<&Node> {
        self.node.as_ref()
    }

    /// Returns the FHIR system URI to openEHR terminology id table.
    #[must_use]
    pub const fn systems(&self) -> &BTreeMap<String, String> {
        &self.systems
    }

    /// Whether `_sort` may order by this parameter.
    ///
    /// A parameter read under a repeated node has a value per node, so it
    /// gives a composition no single position in an order.
    #[must_use]
    pub const fn sortable(&self) -> bool {
        self.node.is_none()
    }

    /// Returns the compared path relative to `alias`: the one path of a
    /// `string` or a `date`, the code path of a `token`.
    #[must_use]
    pub fn value_path(&self, alias: &str) -> String {
        match self.coded {
            Some(Coded::Text) => format!("{alias}/{}/defining_code/code_string", self.path),
            Some(Coded::Phrase) => format!("{alias}/{}/code_string", self.path),
            None => format!("{alias}/{}", self.path),
        }
    }

    /// Returns the system path of a `token` relative to `alias`.
    #[must_use]
    pub fn system_path(&self, alias: &str) -> Option<String> {
        match self.coded? {
            Coded::Text => Some(format!(
                "{alias}/{}/defining_code/terminology_id/value",
                self.path
            )),
            Coded::Phrase => Some(format!("{alias}/{}/terminology_id/value", self.path)),
        }
    }

    /// Returns every path this parameter compares, relative to `alias`.
    #[must_use]
    pub fn paths(&self, alias: &str) -> Vec<String> {
        std::iter::once(self.value_path(alias))
            .chain(self.system_path(alias))
            .collect()
    }
}

/// Why a declaration file is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeclarationError {
    /// The file could not be read.
    #[error("the search declaration {} could not be read", file.display())]
    Read {
        /// The file.
        file: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The file is not the declaration shape.
    #[error("the search declaration {} is not valid", file.display())]
    Yaml {
        /// The file.
        file: PathBuf,
        /// What the parser reported, an unknown key included.
        #[source]
        source: Box<serde_saphyr::Error>,
    },
    /// The file names a context no loaded program compiled.
    #[error("the search declaration {} names the context {context}, which no loaded mapping compiles", file.display())]
    UnknownContext {
        /// The file.
        file: PathBuf,
        /// The context it names.
        context: String,
    },
    /// Two files declare search for one resource type over one template.
    #[error(
        "the search declarations {} and {} both answer {resource_type} over the template {template}",
        first.display(),
        second.display()
    )]
    Duplicate {
        /// The file read first.
        first: PathBuf,
        /// The file read second.
        second: PathBuf,
        /// The resource type.
        resource_type: String,
        /// The template.
        template: String,
    },
    /// Two declarations of one resource type state different parameters.
    #[error(
        "the search declarations {} and {} state different parameters for {resource_type}; one type answers one parameter set",
        first.display(),
        second.display()
    )]
    Inconsistent {
        /// The file read first.
        first: PathBuf,
        /// The file read second.
        second: PathBuf,
        /// The resource type.
        resource_type: String,
    },
    /// A parameter name outside the grammar, reserved, or declared twice.
    #[error("the search declaration {} declares the parameter `{name}`: {reason}", file.display())]
    Name {
        /// The file.
        file: PathBuf,
        /// The name.
        name: String,
        /// Why it is refused.
        reason: &'static str,
    },
    /// A parameter type this facade does not search on.
    #[error(
        "the search declaration {} gives `{name}` the type `{kind}`; a declared parameter is one of {DECLARABLE}",
        file.display()
    )]
    UnsupportedType {
        /// The file.
        file: PathBuf,
        /// The parameter.
        name: String,
        /// The type it was given.
        kind: String,
    },
    /// A parameter that takes the code of a standard parameter of its
    /// resource type with another type.
    #[error(
        "the search declaration {} gives `{name}` the type `{kind}`, and the standard parameter {definition} of that code is a {standard}",
        file.display()
    )]
    Standard {
        /// The file.
        file: PathBuf,
        /// The parameter.
        name: String,
        /// The type it was given.
        kind: &'static str,
        /// The type the standard parameter states.
        standard: &'static str,
        /// The canonical of the standard parameter.
        definition: &'static str,
    },
    /// A parameter's keys do not fit its type.
    #[error("the search declaration {} declares `{name}` as a {kind}: {reason}", file.display())]
    Shape {
        /// The file.
        file: PathBuf,
        /// The parameter.
        name: String,
        /// Its type.
        kind: &'static str,
        /// What is missing or out of place.
        reason: &'static str,
    },
    /// A path the template does not hold, or holds under a repeated node.
    #[error("the search declaration {} cannot place `{name}` in the template: {reason}", file.display())]
    Template {
        /// The file.
        file: PathBuf,
        /// The parameter.
        name: String,
        /// Why it cannot be placed.
        reason: String,
    },
    /// A path that is not one AQL identified path over the query's aliases.
    #[error("the search declaration {} gives `{name}` the path `{path}`: {reason}", file.display())]
    Path {
        /// The file.
        file: PathBuf,
        /// The parameter, or `order`.
        name: String,
        /// The path as written.
        path: String,
        /// Why it is refused.
        reason: String,
    },
    /// The query the declaration builds does not parse as AQL.
    #[error("the search declaration {} builds a query that does not parse as AQL", file.display())]
    Query {
        /// The file.
        file: PathBuf,
        /// What the parser reported.
        #[source]
        source: openehr_query::parser::ParseError,
    },
}

/// Reads one declaration file.
///
/// # Errors
///
/// Returns [`DeclarationError::Read`] when the file cannot be read and
/// [`DeclarationError::Yaml`] when it is not the declaration shape.
pub fn read(file: &Path) -> Result<DeclarationFile, DeclarationError> {
    let text = std::fs::read_to_string(file).map_err(|source| DeclarationError::Read {
        file: file.to_path_buf(),
        source,
    })?;
    file::parse(file, &text)
}

/// Checks the parameter section of one declaration against the template
/// `index` its context maps.
///
/// `patient_names` are the names the `patient` section takes, which no
/// declared parameter may repeat.
///
/// # Errors
///
/// Returns the [`DeclarationError`] naming the first fault.
pub fn check_parameters(
    file: &Path,
    parameters: &BTreeMap<String, ParameterFile>,
    patient_names: &[String],
    index: &WebTemplateIndex,
) -> Result<BTreeMap<String, Parameter>, DeclarationError> {
    let mut checked = BTreeMap::new();
    for (name, written) in parameters {
        check_name(file, name)?;
        if patient_names.contains(name) {
            return Err(DeclarationError::Name {
                file: file.to_path_buf(),
                name: name.clone(),
                reason: "the patient section already takes this name",
            });
        }
        checked.insert(
            name.clone(),
            check::check_parameter(file, name, written, index)?,
        );
    }
    Ok(checked)
}

/// Checks one parameter name against the grammar and the reserved prefix.
///
/// # Errors
///
/// Returns [`DeclarationError::Name`] for a name outside the grammar or under
/// the reserved prefix.
pub fn check_name(file: &Path, name: &str) -> Result<(), DeclarationError> {
    let reason = if name.starts_with(check::RESERVED_PREFIX) {
        "R4 reserves the `_` prefix for the parameters every search shares"
    } else if check::is_parameter_name(name) {
        return Ok(());
    } else {
        "a parameter name is a letter followed by letters, digits and hyphens"
    };
    Err(DeclarationError::Name {
        file: file.to_path_buf(),
        name: name.to_owned(),
        reason,
    })
}

/// Returns the standard R4 search parameter `name` of `resource_type`, its own
/// or one a type it specializes defines, when R4 defines one.
#[must_use]
pub fn standard(resource_type: &str, name: &str) -> Option<&'static SearchParameter> {
    SEARCH_PARAMETERS.of(resource_type, name)
}

/// Checks a declared parameter against the standard R4 parameter of its code,
/// when R4 defines one for `resource_type`.
///
/// "Parameter names cannot overlap with standard parameter names, and
/// standard parameters cannot be redefined", and the type a statement names
/// "SHALL be the same as the type in the search parameter definition"
/// (<https://hl7.org/fhir/R4/capabilitystatement-definitions.html#CapabilityStatement.rest.resource.searchParam.name>),
/// so a declaration that takes a standard code takes its type too.
///
/// # Errors
///
/// Returns [`DeclarationError::Standard`] when the standard parameter of that
/// code has another type.
pub fn check_standard(
    file: &Path,
    resource_type: &str,
    name: &str,
    kind: ParameterType,
) -> Result<(), DeclarationError> {
    match standard(resource_type, name) {
        Some(parameter) if parameter.r#type.code() != kind.as_str() => {
            Err(DeclarationError::Standard {
                file: file.to_path_buf(),
                name: name.to_owned(),
                kind: kind.as_str(),
                standard: parameter.r#type.code(),
                definition: parameter.url,
            })
        }
        _ => Ok(()),
    }
}
