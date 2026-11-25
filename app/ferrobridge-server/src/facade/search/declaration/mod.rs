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
use openehr_query::ast::IdentifiedPath;
use openehr_query::ast::ObjectPath;
use serde::Deserialize;

use crate::facade::search::aql::below;
use crate::facade::search::declaration::file::ParameterFile;

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
    /// The standard reference parameters of the type a search may name in
    /// `_include`.
    #[serde(default)]
    pub include: Vec<String>,
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
    /// The node inside the archetype the parameter is read under, when it is
    /// not the root itself.
    pub step: Option<Step>,
}

/// A node inside an archetype, bound below its root as
/// `CONTAINS <class> <alias>[<node id>]` (QUERY AQL 1.1.0 §3.13.2.2), so a
/// parameter read under a repeated node matches when any instance of it
/// matches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Step {
    /// The RM class of the node, `EVENT` for any event.
    pub class: String,
    /// The node's at-code in its archetype.
    pub node_id: String,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// The FHIR type.
    kind: ParameterType,
    /// Where its value is read.
    source: Source,
    /// The FHIR system URI to openEHR terminology id table of a `token`.
    systems: BTreeMap<String, String>,
}

/// Where a parameter's value is read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Source {
    /// A path in the composition.
    Path {
        /// The node the path is read under, `None` for the composition.
        node: Option<Node>,
        /// The path, relative to the node or to the composition, parsed
        /// when the declaration loaded.
        path: ObjectPath,
        /// The coded value's type, for a `token`.
        coded: Option<Coded>,
    },
    /// The one coding the mapping writes on every resource of the context.
    Fixed(FixedToken),
}

/// The coding a `fixed` token parameter compares.
///
/// A mapping that writes a constant, as the category of an observation, has
/// no openEHR path to search, and every resource the context answers carries
/// the coding, so a value either matches them all or none (no specification
/// governs this: our own design).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedToken {
    /// The code system URI.
    pub system: String,
    /// The code.
    pub code: String,
}

impl FixedToken {
    /// Whether the search token `token` matches this coding
    /// (<https://hl7.org/fhir/R4/search.html#token>).
    #[must_use]
    pub fn matches(&self, token: &fhir_search::token::Token) -> bool {
        use fhir_search::token::System;
        let system = match token.system() {
            System::Any => true,
            System::Absent => false,
            System::Is(uri) => uri == self.system,
        };
        system && token.code().is_none_or(|code| code == self.code)
    }
}

impl Parameter {
    /// Returns the FHIR type of this parameter.
    #[must_use]
    pub const fn kind(&self) -> ParameterType {
        self.kind
    }

    /// Returns the node the paths are read under, `None` for the
    /// composition and for a fixed token.
    #[must_use]
    pub const fn node(&self) -> Option<&Node> {
        match &self.source {
            Source::Path { node, .. } => node.as_ref(),
            Source::Fixed(_) => None,
        }
    }

    /// Returns the coding of a fixed token.
    #[must_use]
    pub const fn fixed(&self) -> Option<&FixedToken> {
        match &self.source {
            Source::Fixed(fixed) => Some(fixed),
            Source::Path { .. } => None,
        }
    }

    /// Returns the FHIR system URI to openEHR terminology id table.
    #[must_use]
    pub const fn systems(&self) -> &BTreeMap<String, String> {
        &self.systems
    }

    /// Whether `_sort` may order by this parameter.
    ///
    /// A parameter read under a repeated node has a value per node, so it
    /// gives a composition no single position in an order, and a fixed token
    /// gives every composition the same one.
    #[must_use]
    pub const fn sortable(&self) -> bool {
        matches!(self.source, Source::Path { node: None, .. })
    }

    /// Returns the compared path relative to `alias`: the one path of a
    /// `string` or a `date`, the code path of a `token`; `None` for a fixed
    /// token.
    #[must_use]
    pub fn value_path(&self, alias: &str) -> Option<IdentifiedPath> {
        let Source::Path { path, coded, .. } = &self.source else {
            return None;
        };
        Some(match coded {
            Some(Coded::Text) => below(alias, path, &["defining_code", "code_string"]),
            Some(Coded::Phrase) => below(alias, path, &["code_string"]),
            None => below(alias, path, &[]),
        })
    }

    /// Returns the system path of a `token` relative to `alias`.
    #[must_use]
    pub fn system_path(&self, alias: &str) -> Option<IdentifiedPath> {
        let Source::Path {
            path,
            coded: Some(coded),
            ..
        } = &self.source
        else {
            return None;
        };
        Some(match coded {
            Coded::Text => below(alias, path, &["defining_code", "terminology_id", "value"]),
            Coded::Phrase => below(alias, path, &["terminology_id", "value"]),
        })
    }

    /// Returns every path this parameter compares, relative to `alias`.
    #[must_use]
    pub fn paths(&self, alias: &str) -> Vec<IdentifiedPath> {
        self.value_path(alias)
            .into_iter()
            .chain(self.system_path(alias))
            .collect()
    }
}

/// One reference parameter a search may name in `_include`: the standard R4
/// parameter of its code, whose expression selects the references followed
/// (<https://hl7.org/fhir/R4/search.html#include>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Include {
    /// The parameter code.
    pub code: String,
    /// The branches of the standard parameter's FHIRPath expression that
    /// select from the type.
    pub expression: String,
}

/// Returns the union branches of a standard parameter's `expression` that
/// start from `resource_type`, joined again by `|`.
///
/// A parameter several types share writes one branch per type, and a branch
/// naming another type is an error on this one's instances (FHIRPath N1
/// §4.1).
fn own_branches(expression: &str, resource_type: &str) -> String {
    let mut branches = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (at, character) in expression.char_indices() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                branches.push(expression.get(start..at).unwrap_or_default());
                start = at.saturating_add(1);
            }
            _ => {}
        }
    }
    branches.push(expression.get(start..).unwrap_or_default());
    branches
        .into_iter()
        .map(str::trim)
        .filter(|branch| {
            branch
                .trim_start_matches('(')
                .strip_prefix(resource_type)
                .is_some_and(|rest| rest.starts_with('.'))
        })
        .collect::<Vec<&str>>()
        .join(" | ")
}

/// Checks the `include` list of one declaration against the standard R4
/// reference parameters of `resource_type`.
///
/// # Errors
///
/// Returns [`DeclarationError::Include`] for a code R4 defines no reference
/// parameter with on the type and for a code named twice, and
/// [`DeclarationError::IncludeExpression`] for a standard expression the
/// evaluator does not parse.
pub fn check_includes(
    file: &Path,
    resource_type: &str,
    codes: &[String],
) -> Result<BTreeMap<String, Include>, DeclarationError> {
    let mut checked = BTreeMap::new();
    for code in codes {
        let refuse = |reason: &'static str| DeclarationError::Include {
            file: file.to_path_buf(),
            code: code.clone(),
            reason,
        };
        let standard = standard(resource_type, code)
            .filter(|parameter| parameter.r#type.code() == ParameterType::Reference.as_str())
            .ok_or_else(|| refuse("R4 defines no reference parameter of this code on the type"))?;
        let expression = standard
            .expression
            .map(|expression| own_branches(expression, resource_type))
            .filter(|expression| !expression.is_empty())
            .ok_or_else(|| refuse("the standard parameter states no expression for the type"))?;
        ferrobridge_fhirpath::expression::Expression::parse(&expression).map_err(|source| {
            DeclarationError::IncludeExpression {
                file: file.to_path_buf(),
                code: code.clone(),
                source,
            }
        })?;
        let include = Include {
            code: code.clone(),
            expression,
        };
        if checked.insert(code.clone(), include).is_some() {
            return Err(refuse("the include list names it twice"));
        }
    }
    Ok(checked)
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
    /// Two files declare search for one resource type over one template
    /// and one entry.
    #[error(
        "the search declarations {} and {} both answer {resource_type} from one entry of the template {template}",
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
    /// An `include` code that names no standard reference parameter.
    #[error("the search declaration {} includes `{code}`: {reason}", file.display())]
    Include {
        /// The file.
        file: PathBuf,
        /// The code.
        code: String,
        /// Why it is refused.
        reason: &'static str,
    },
    /// The expression of an `include` code does not parse as FHIRPath.
    #[error("the search declaration {} includes `{code}`, whose standard expression does not parse", file.display())]
    IncludeExpression {
        /// The file.
        file: PathBuf,
        /// The code.
        code: String,
        /// What the parser reported.
        #[source]
        source: ferrobridge_fhirpath::parse::ParseError,
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

impl DeclarationError {
    /// Returns the file the refusal names: the second file of a pair.
    #[must_use]
    pub fn file(&self) -> &Path {
        match self {
            Self::Read { file, .. }
            | Self::Yaml { file, .. }
            | Self::UnknownContext { file, .. }
            | Self::Name { file, .. }
            | Self::UnsupportedType { file, .. }
            | Self::Standard { file, .. }
            | Self::Shape { file, .. }
            | Self::Template { file, .. }
            | Self::Path { file, .. }
            | Self::Include { file, .. }
            | Self::IncludeExpression { file, .. }
            | Self::Query { file, .. } => file,
            Self::Duplicate { second, .. } | Self::Inconsistent { second, .. } => second,
        }
    }
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
