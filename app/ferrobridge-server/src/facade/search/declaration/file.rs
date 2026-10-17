// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The written form of one declared parameter, and the parser that reads a
//! declaration from its text.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::facade::search::declaration::DeclarationError;
use crate::facade::search::declaration::DeclarationFile;

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

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::facade::search::declaration::DeclarationError;
    use crate::facade::search::declaration::is_declaration;
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
}
