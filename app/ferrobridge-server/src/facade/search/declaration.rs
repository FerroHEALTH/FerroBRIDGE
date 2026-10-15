// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search declaration file, read beside the context mapping it serves.
//!
//! FHIRconnect "does not focus specifically on AQL and `FHIRsearch`", so no
//! specification governs this file: our own design. One file per context,
//! named `<anything>.search.yml` (or `.search.yaml`) anywhere in the mapping
//! tree, states which search parameters the context's resource type answers
//! and the AQL path each one compares. Every key is checked when the file is
//! read: an unknown key, a parameter type outside [`ParameterType`], a path
//! that is not one AQL identified path rooted at `e`, `v` or `c`, or a context
//! no loaded program compiled is a boot refusal naming the file.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;

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

/// One declared parameter, as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterFile {
    /// The FHIR search parameter type.
    #[serde(rename = "type")]
    pub kind: String,
    /// The AQL path a `string` or `date` parameter compares.
    pub path: Option<String>,
    /// The AQL path of a `token` parameter's code.
    pub code: Option<String>,
    /// The AQL path of a `token` parameter's code system.
    pub system: Option<String>,
    /// The FHIR system URI to openEHR terminology id table of a `token`
    /// parameter.
    #[serde(default)]
    pub systems: BTreeMap<String, String>,
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

/// One checked parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parameter {
    /// A `token` over a code path and an optional system path.
    Token {
        /// The AQL path of the code.
        code: String,
        /// The AQL path of the code system.
        system: Option<String>,
        /// The FHIR system URI to openEHR terminology id table.
        systems: BTreeMap<String, String>,
    },
    /// A `string` over one path.
    String {
        /// The AQL path compared.
        path: String,
    },
    /// A `date` over one path.
    Date {
        /// The AQL path compared.
        path: String,
    },
}

impl Parameter {
    /// Returns the FHIR type of this parameter.
    #[must_use]
    pub const fn kind(&self) -> ParameterType {
        match *self {
            Self::Token { .. } => ParameterType::Token,
            Self::String { .. } => ParameterType::String,
            Self::Date { .. } => ParameterType::Date,
        }
    }

    /// Returns the path `_sort` orders this parameter by.
    #[must_use]
    pub fn sort_path(&self) -> &str {
        match *self {
            Self::Token { ref code, .. } => code,
            Self::String { ref path } | Self::Date { ref path } => path,
        }
    }

    /// Returns every AQL path this parameter compares.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        match *self {
            Self::Token {
                ref code,
                ref system,
                ..
            } => std::iter::once(code.as_str())
                .chain(system.as_deref())
                .collect(),
            Self::String { ref path } | Self::Date { ref path } => vec![path.as_str()],
        }
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

/// The names R4 reserves for the parameters every resource and every search
/// shares, which a declaration may not take.
///
/// Every one of them starts with `_` (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.4.1 and the search result parameters), and `_id` is the facade's
/// own, resolved through the identity map.
const RESERVED_PREFIX: char = '_';

/// Whether `name` is a search parameter code: a letter, then letters, digits
/// and hyphens.
///
/// R4 gives `SearchParameter.code` the `code` type and every published code
/// takes this shape (no specification fixes a narrower grammar: our own
/// design), and the shape keeps `:` and `.` free for modifiers and chains.
fn is_parameter_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '-')
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
    parse(file, &text)
}

/// Parses one declaration from `text`, naming `file` in a refusal.
///
/// Duplicate keys are refused and an unrecognised tag is refused, as the
/// mapping files are read.
///
/// # Errors
///
/// Returns [`DeclarationError::Yaml`] when `text` is not the declaration
/// shape.
pub fn parse(file: &Path, text: &str) -> Result<DeclarationFile, DeclarationError> {
    let options = serde_saphyr::options! {
        with_snippet: false,
        reject_unsupported_tags: true,
    };
    serde_saphyr::from_str_with_options::<DeclarationFile>(text, options).map_err(|source| {
        DeclarationError::Yaml {
            file: file.to_path_buf(),
            source: Box::new(source),
        }
    })
}

/// Checks the parameter section of one declaration.
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
        checked.insert(name.clone(), check_parameter(file, name, written)?);
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
    let reason = if name.starts_with(RESERVED_PREFIX) {
        "R4 reserves the `_` prefix for the parameters every search shares"
    } else if is_parameter_name(name) {
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

/// Checks one parameter's keys against its type.
fn check_parameter(
    file: &Path,
    name: &str,
    written: &ParameterFile,
) -> Result<Parameter, DeclarationError> {
    let shape = |kind: &'static str, reason: &'static str| DeclarationError::Shape {
        file: file.to_path_buf(),
        name: name.to_owned(),
        kind,
        reason,
    };
    match written.kind.as_str() {
        "token" => {
            if written.path.is_some() {
                return Err(shape(
                    "token",
                    "a token compares `code` and `system`, never `path`",
                ));
            }
            let code = written
                .code
                .clone()
                .ok_or_else(|| shape("token", "a token needs the `code` path"))?;
            if written.system.is_none() && !written.systems.is_empty() {
                return Err(shape(
                    "token",
                    "`systems` translates the `system` path, which is absent",
                ));
            }
            Ok(Parameter::Token {
                code,
                system: written.system.clone(),
                systems: written.systems.clone(),
            })
        }
        kind @ ("string" | "date") => {
            let label = if kind == "string" { "string" } else { "date" };
            if written.code.is_some() || written.system.is_some() || !written.systems.is_empty() {
                return Err(shape(
                    label,
                    "only a token takes `code`, `system` and `systems`",
                ));
            }
            let path = written
                .path
                .clone()
                .ok_or_else(|| shape(label, "the parameter needs its `path`"))?;
            Ok(if kind == "string" {
                Parameter::String { path }
            } else {
                Parameter::Date { path }
            })
        }
        other => Err(DeclarationError::UnsupportedType {
            file: file.to_path_buf(),
            name: name.to_owned(),
            kind: other.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{DeclarationError, Parameter, check_parameters, is_declaration, parse};
    use std::path::Path;

    const FILE: &str = "condition.search.yml";

    #[test]
    fn only_the_search_suffixes_name_a_declaration() {
        assert!(is_declaration(Path::new("a/condition.search.yml")));
        assert!(is_declaration(Path::new("Condition.SEARCH.YAML")));
        assert!(!is_declaration(Path::new("condition.yml")));
        assert!(!is_declaration(Path::new("search.yml")));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let error = parse(Path::new(FILE), "context: c\nfrom: EHR e\n")
            .expect_err("`from` is no declaration key");
        assert!(matches!(error, DeclarationError::Yaml { .. }), "{error:?}");
    }

    #[test]
    fn a_duplicate_key_is_refused() {
        let error =
            parse(Path::new(FILE), "context: a\ncontext: b\n").expect_err("a key twice is refused");
        assert!(matches!(error, DeclarationError::Yaml { .. }), "{error:?}");
    }

    #[test]
    fn the_three_declarable_types_check() {
        let declared = parse(
            Path::new(FILE),
            "context: c\nparameters:\n  code:\n    type: token\n    code: c/x/code_string\n    system: c/x/terminology_id/value\n    systems:\n      http://snomed.info/sct: SNOMED-CT\n  note:\n    type: string\n    path: c/y/value\n  onset:\n    type: date\n    path: c/z/value\n",
        )
        .expect("the declaration parses");
        let checked = check_parameters(Path::new(FILE), &declared.parameters, &[])
            .expect("the parameters check");
        assert!(matches!(checked.get("code"), Some(Parameter::Token { .. })));
        assert!(matches!(
            checked.get("note"),
            Some(Parameter::String { .. })
        ));
        assert!(matches!(checked.get("onset"), Some(Parameter::Date { .. })));
    }

    #[test]
    fn a_type_outside_the_declarable_set_is_refused_by_name() {
        let declared = parse(
            Path::new(FILE),
            "context: c\nparameters:\n  value:\n    type: quantity\n    path: c/q\n",
        )
        .expect("the declaration parses");
        let error = check_parameters(Path::new(FILE), &declared.parameters, &[])
            .expect_err("quantity is not declarable");
        assert!(
            matches!(error, DeclarationError::UnsupportedType { ref kind, .. } if kind == "quantity"),
            "{error:?}"
        );
    }

    #[test]
    fn a_reserved_or_malformed_name_is_refused() {
        for name in ["_lastUpdated", "code:exact", "patient.identifier", "9code"] {
            let text =
                format!("context: c\nparameters:\n  {name}:\n    type: string\n    path: c/p\n");
            let declared = parse(Path::new(FILE), &text).expect("the declaration parses");
            let error = check_parameters(Path::new(FILE), &declared.parameters, &[])
                .expect_err("the name is refused");
            assert!(
                matches!(error, DeclarationError::Name { .. }),
                "{name}: {error:?}"
            );
        }
    }

    #[test]
    fn a_token_without_its_code_path_is_refused() {
        let declared = parse(
            Path::new(FILE),
            "context: c\nparameters:\n  code:\n    type: token\n    path: c/p\n",
        )
        .expect("the declaration parses");
        let error = check_parameters(Path::new(FILE), &declared.parameters, &[])
            .expect_err("a token compares code and system");
        assert!(matches!(error, DeclarationError::Shape { .. }), "{error:?}");
    }

    #[test]
    fn a_parameter_repeating_a_patient_name_is_refused() {
        let declared = parse(
            Path::new(FILE),
            "context: c\nparameters:\n  patient:\n    type: string\n    path: c/p\n",
        )
        .expect("the declaration parses");
        let error = check_parameters(
            Path::new(FILE),
            &declared.parameters,
            &[String::from("patient")],
        )
        .expect_err("the patient section takes the name");
        assert!(matches!(error, DeclarationError::Name { .. }), "{error:?}");
    }
}
