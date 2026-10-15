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

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use openehr_mapping_core::index::WebTemplateIndex;
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
    /// The repeated node the path is read under, bound in the query's
    /// `FROM`; absent, the path is read under the composition.
    pub node: Option<NodeFile>,
    /// The AQL path compared, relative to the node: for a `token`, the coded
    /// text or code phrase whose code and system it compares.
    pub path: String,
    /// The FHIR system URI to openEHR terminology id table of a `token`
    /// parameter.
    #[serde(default)]
    pub systems: BTreeMap<String, String>,
}

/// The node a parameter is read under, as written: an RM class and the
/// archetype id of its root.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeFile {
    /// The RM class of the archetype root, `EVALUATION` or `CLUSTER`.
    pub class: String,
    /// The archetype id of the root.
    pub archetype: String,
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
        checked.insert(name.clone(), check_parameter(file, name, written, index)?);
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

/// Checks one parameter's keys against its type and its path against the
/// template.
fn check_parameter(
    file: &Path,
    name: &str,
    written: &ParameterFile,
    index: &WebTemplateIndex,
) -> Result<Parameter, DeclarationError> {
    let kind = match written.kind.as_str() {
        "token" => ParameterType::Token,
        "string" => ParameterType::String,
        "date" => ParameterType::Date,
        other => {
            return Err(DeclarationError::UnsupportedType {
                file: file.to_path_buf(),
                name: name.to_owned(),
                kind: other.to_owned(),
            });
        }
    };
    if kind != ParameterType::Token && !written.systems.is_empty() {
        return Err(DeclarationError::Shape {
            file: file.to_path_buf(),
            name: name.to_owned(),
            kind: kind.as_str(),
            reason: "only a token translates code systems through `systems`",
        });
    }
    let node = written.node.as_ref().map(|node| Node {
        class: node.class.clone(),
        archetype: node.archetype.clone(),
    });
    let coded = crate::facade::search::template::locate(index, node.as_ref(), &written.path, kind)
        .map_err(|reason| DeclarationError::Template {
            file: file.to_path_buf(),
            name: name.to_owned(),
            reason,
        })?;
    Ok(Parameter {
        kind,
        node,
        path: written.path.clone(),
        coded,
        systems: written.systems.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Coded, DeclarationError, ParameterType, check_parameters, is_declaration, parse};
    use openehr_mapping_core::index::WebTemplateIndex;
    use openehr_mapping_core::template::TemplateSource;
    use std::path::Path;

    const FILE: &str = "condition.search.yml";

    /// The node every token case below is read under.
    const NODE: &str =
        "    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }\n";

    fn index() -> WebTemplateIndex {
        let source = TemplateSource::opt14(ferrobridge_testkit::fixtures::DIAGNOSE_OPT)
            .expect("the fixture template parses");
        WebTemplateIndex::build(&source).expect("the fixture template indexes")
    }

    fn check(text: &str) -> Result<super::Parameter, DeclarationError> {
        let declared = parse(Path::new(FILE), text).expect("the declaration parses");
        check_parameters(Path::new(FILE), &declared.parameters, &[], &index())
            .map(|mut checked| checked.pop_first().expect("one parameter").1)
    }

    #[test]
    fn only_the_search_suffixes_name_a_declaration() {
        assert!(is_declaration(Path::new("a/condition.search.yml")));
        assert!(is_declaration(Path::new("Condition.SEARCH.YAML")));
        assert!(!is_declaration(Path::new("condition.yml")));
        assert!(!is_declaration(Path::new("search.yml")));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        for text in [
            "context: c\nfrom: EHR e\n",
            "context: c\nparameters:\n  code:\n    type: token\n    code: c/x\n    path: c/x\n",
        ] {
            let error = parse(Path::new(FILE), text).expect_err("the key is refused");
            assert!(matches!(error, DeclarationError::Yaml { .. }), "{error:?}");
        }
    }

    #[test]
    fn a_duplicate_key_is_refused() {
        let error =
            parse(Path::new(FILE), "context: a\ncontext: b\n").expect_err("a key twice is refused");
        assert!(matches!(error, DeclarationError::Yaml { .. }), "{error:?}");
    }

    #[test]
    fn a_token_reads_code_and_system_under_one_bound_node() {
        let token = check(&format!(
            "context: c\nparameters:\n  code:\n    type: token\n{NODE}    path: data[at0001]/items[at0002]/value\n"
        ))
        .expect("the token checks");
        assert_eq!(ParameterType::Token, token.kind());
        assert_eq!(Some(Coded::Text), token.coded);
        assert_eq!(
            "n0/data[at0001]/items[at0002]/value/defining_code/code_string",
            token.value_path("n0")
        );
        assert_eq!(
            Some(String::from(
                "n0/data[at0001]/items[at0002]/value/defining_code/terminology_id/value"
            )),
            token.system_path("n0")
        );
        assert!(!token.sortable());
    }

    #[test]
    fn a_composition_level_date_checks_and_sorts() {
        let date = check("context: c\nparameters:\n  recorded:\n    type: date\n    path: context/start_time/value\n")
            .expect("a composition-level date checks");
        assert!(date.sortable());
        assert_eq!("c/context/start_time/value", date.value_path("c"));
    }

    #[test]
    fn a_path_crossing_a_repeated_node_is_refused() {
        // The anatomical location cluster is unbounded in the fixture
        // template, so a path through it has one value per cluster.
        let error = check(&format!(
            "context: c\nparameters:\n  site:\n    type: token\n{NODE}    path: data[at0001]/items[openEHR-EHR-CLUSTER.anatomical_location.v1]/items[at0002]/value\n"
        ))
        .expect_err("the cluster repeats");
        assert!(
            matches!(error, DeclarationError::Template { ref reason, .. } if reason.contains("more than once")),
            "{error:?}"
        );
    }

    #[test]
    fn a_token_over_no_coded_value_is_refused() {
        let error = check(&format!(
            "context: c\nparameters:\n  onset:\n    type: token\n{NODE}    path: data[at0001]/items[at0077]/value\n"
        ))
        .expect_err("a date-time carries no code");
        assert!(
            matches!(error, DeclarationError::Template { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_node_the_template_does_not_hold_is_refused() {
        let error = check(
            "context: c\nparameters:\n  code:\n    type: token\n    node: { class: OBSERVATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }\n    path: data[at0001]/items[at0002]/value\n",
        )
        .expect_err("the class does not match the archetype root");
        assert!(
            matches!(error, DeclarationError::Template { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_type_outside_the_declarable_set_is_refused_by_name() {
        let error = check("context: c\nparameters:\n  value:\n    type: quantity\n    path: c/q\n")
            .expect_err("quantity is not declarable");
        assert!(
            matches!(error, DeclarationError::UnsupportedType { ref kind, .. } if kind == "quantity"),
            "{error:?}"
        );
    }

    #[test]
    fn a_reserved_or_malformed_name_is_refused() {
        for name in ["_lastUpdated", "code:exact", "patient.identifier", "9code"] {
            let text = format!(
                "context: c\nparameters:\n  {name}:\n    type: date\n    path: context/start_time/value\n"
            );
            let error = check(&text).expect_err("the name is refused");
            assert!(
                matches!(error, DeclarationError::Name { .. }),
                "{name}: {error:?}"
            );
        }
    }

    #[test]
    fn systems_on_a_parameter_other_than_a_token_are_refused() {
        let error = check(
            "context: c\nparameters:\n  recorded:\n    type: date\n    path: context/start_time/value\n    systems: { a: b }\n",
        )
        .expect_err("only a token translates systems");
        assert!(matches!(error, DeclarationError::Shape { .. }), "{error:?}");
    }

    #[test]
    fn a_parameter_repeating_a_patient_name_is_refused() {
        let declared = parse(
            Path::new(FILE),
            "context: c\nparameters:\n  patient:\n    type: date\n    path: context/start_time/value\n",
        )
        .expect("the declaration parses");
        let error = check_parameters(
            Path::new(FILE),
            &declared.parameters,
            &[String::from("patient")],
            &index(),
        )
        .expect_err("the patient section takes the name");
        assert!(matches!(error, DeclarationError::Name { .. }), "{error:?}");
    }
}
