// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks one declared parameter passes: its name against the grammar,
//! its keys against its type and its path against the template.

use std::path::Path;

use openehr_mapping_core::index::WebTemplateIndex;

use crate::facade::search::declaration::DeclarationError;
use crate::facade::search::declaration::FixedToken;
use crate::facade::search::declaration::Node;
use crate::facade::search::declaration::Parameter;
use crate::facade::search::declaration::ParameterType;
use crate::facade::search::declaration::Source;
use crate::facade::search::declaration::file::ParameterFile;

/// The names R4 reserves for the parameters every resource and every search
/// shares, which a declaration may not take.
///
/// Every one of them starts with `_` (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.4.1 and the search result parameters), and `_id` is the facade's
/// own, resolved through the identity map.
pub(super) const RESERVED_PREFIX: char = '_';

/// Whether `name` is a search parameter code: a letter, then letters, digits
/// and hyphens.
///
/// R4 gives `SearchParameter.code` the `code` type and every published code
/// takes this shape (no specification fixes a narrower grammar: our own
/// design), and the shape keeps `:` and `.` free for modifiers and chains.
pub(super) fn is_parameter_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '-')
}

/// Checks one parameter's keys against its type and its path against the
/// template.
pub(super) fn check_parameter(
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
    let source = match (&written.fixed, &written.path) {
        (Some(fixed), None) if kind == ParameterType::Token => {
            if written.node.is_some() || !written.systems.is_empty() {
                return Err(shape(
                    file,
                    name,
                    kind,
                    "a fixed token is read under no node and translates no system",
                ));
            }
            Source::Fixed(FixedToken {
                system: fixed.system.clone(),
                code: fixed.code.clone(),
            })
        }
        (Some(_), None) => {
            return Err(shape(file, name, kind, "only a token takes a fixed coding"));
        }
        (Some(_), Some(_)) => {
            return Err(shape(
                file,
                name,
                kind,
                "a parameter takes either a path or a fixed coding",
            ));
        }
        (None, None) => {
            return Err(shape(file, name, kind, "a parameter names its path"));
        }
        (None, Some(written_path)) => read_path(file, name, kind, written, written_path, index)?,
    };
    Ok(Parameter {
        kind,
        source,
        systems: written.systems.clone(),
    })
}

/// Returns a [`DeclarationError::Shape`] for `name`.
fn shape(file: &Path, name: &str, kind: ParameterType, reason: &'static str) -> DeclarationError {
    DeclarationError::Shape {
        file: file.to_path_buf(),
        name: name.to_owned(),
        kind: kind.as_str(),
        reason,
    }
}

/// Places the path of one parameter in the template and parses it.
fn read_path(
    file: &Path,
    name: &str,
    kind: ParameterType,
    written: &ParameterFile,
    written_path: &str,
    index: &WebTemplateIndex,
) -> Result<Source, DeclarationError> {
    let node = written.node.as_ref().map(|node| Node {
        class: node.class.clone(),
        archetype: node.archetype.clone(),
    });
    let coded = crate::facade::search::template::locate(index, node.as_ref(), written_path, kind)
        .map_err(|reason| DeclarationError::Template {
        file: file.to_path_buf(),
        name: name.to_owned(),
        reason,
    })?;
    let refuse = |reason: String| DeclarationError::Path {
        file: file.to_path_buf(),
        name: name.to_owned(),
        path: written_path.to_owned(),
        reason,
    };
    let path = crate::facade::search::aql::check_path(&format!("c/{written_path}"))
        .map_err(refuse)?
        .path
        .ok_or_else(|| refuse(String::from("it names no attribute below its node")))?;
    Ok(Source::Path { node, path, coded })
}

#[cfg(test)]
mod tests {
    use crate::facade::search::declaration::Coded;
    use crate::facade::search::declaration::DeclarationError;
    use crate::facade::search::declaration::Parameter;
    use crate::facade::search::declaration::ParameterType;
    use crate::facade::search::declaration::Source;
    use crate::facade::search::declaration::check_parameters;
    use crate::facade::search::declaration::check_standard;
    use crate::facade::search::declaration::file::parse;
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

    fn check(text: &str) -> Result<Parameter, DeclarationError> {
        let declared = parse(Path::new(FILE), text).expect("the declaration parses");
        check_parameters(Path::new(FILE), &declared.parameters, &[], &index())
            .map(|mut checked| checked.pop_first().expect("one parameter").1)
    }

    #[test]
    fn a_token_reads_code_and_system_under_one_bound_node() {
        let token = check(&format!(
            "context: c\nparameters:\n  code:\n    type: token\n{NODE}    path: data[at0001]/items[at0002]/value\n"
        ))
        .expect("the token checks");
        assert_eq!(ParameterType::Token, token.kind());
        assert!(
            matches!(
                token.source,
                Source::Path {
                    coded: Some(Coded::Text),
                    ..
                }
            ),
            "{token:?}"
        );
        assert_eq!(
            Some(String::from(
                "n0/data[at0001]/items[at0002]/value/defining_code/code_string"
            )),
            token
                .value_path("n0")
                .map(|path| path.column_path_with_variable())
        );
        assert_eq!(
            Some(String::from(
                "n0/data[at0001]/items[at0002]/value/defining_code/terminology_id/value"
            )),
            token
                .system_path("n0")
                .map(|path| path.column_path_with_variable())
        );
        assert!(!token.sortable());
    }

    #[test]
    fn a_composition_level_date_checks_and_sorts() {
        let date = check("context: c\nparameters:\n  recorded:\n    type: date\n    path: context/start_time/value\n")
            .expect("a composition-level date checks");
        assert!(date.sortable());
        assert_eq!(
            Some(String::from("c/context/start_time/value")),
            date.value_path("c")
                .map(|path| path.column_path_with_variable())
        );
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

    #[test]
    fn a_standard_code_declared_with_another_type_is_refused_naming_the_standard() {
        // R4 capabilitystatement-definitions.html: "standard parameters cannot
        // be redefined"; Condition answers `code` as the token clinical-code.
        let error = check_standard(Path::new(FILE), "Condition", "code", ParameterType::String)
            .expect_err("code is a token on Condition");
        let DeclarationError::Standard {
            standard,
            definition,
            ..
        } = error
        else {
            panic!("a Standard refusal: {error:?}");
        };
        assert_eq!("token", standard);
        assert_eq!(
            "http://hl7.org/fhir/SearchParameter/clinical-code",
            definition
        );
        assert!(
            check_standard(Path::new(FILE), "Condition", "patient", ParameterType::Date).is_err(),
            "patient is a reference on Condition"
        );
    }

    #[test]
    fn a_standard_code_with_its_own_type_and_an_implementation_code_pass() {
        for (name, kind) in [
            ("code", ParameterType::Token),
            ("onset-date", ParameterType::Date),
            ("subject", ParameterType::Reference),
            ("diagnosis-name", ParameterType::String),
        ] {
            assert!(
                check_standard(Path::new(FILE), "Condition", name, kind).is_ok(),
                "{name}"
            );
        }
    }

    #[test]
    fn a_fixed_token_reads_no_path_and_matches_its_coding() {
        let token = check(
            "context: c\nparameters:\n  category:\n    type: token\n    fixed: { system: 'http://terminology.hl7.org/CodeSystem/observation-category', code: vital-signs }\n",
        )
        .expect("the fixed token checks");
        let fixed = token.fixed().expect("a fixed coding");
        assert_eq!(
            Vec::<openehr_query::ast::IdentifiedPath>::new(),
            token.paths("n0")
        );
        assert!(token.node().is_none());
        assert!(!token.sortable());
        for (text, matches) in [
            ("vital-signs", true),
            (
                "http://terminology.hl7.org/CodeSystem/observation-category|vital-signs",
                true,
            ),
            (
                "http://terminology.hl7.org/CodeSystem/observation-category|",
                true,
            ),
            ("laboratory", false),
            ("|vital-signs", false),
            ("http://example.org|vital-signs", false),
        ] {
            let value = fhir_search::token::Token::parse(text).expect("a token value");
            assert_eq!(matches, fixed.matches(&value), "{text}");
        }
    }

    #[test]
    fn a_fixed_coding_with_a_path_a_node_or_another_type_is_refused() {
        for text in [
            "context: c\nparameters:\n  category:\n    type: token\n    path: context/start_time/value\n    fixed: { system: s, code: c }\n".to_owned(),
            format!("context: c\nparameters:\n  category:\n    type: token\n{NODE}    fixed: {{ system: s, code: c }}\n"),
            "context: c\nparameters:\n  category:\n    type: string\n    fixed: { system: s, code: c }\n".to_owned(),
            "context: c\nparameters:\n  category:\n    type: token\n".to_owned(),
        ] {
            let error = check(&text).expect_err("the shape is refused");
            assert!(matches!(error, DeclarationError::Shape { .. }), "{error:?}");
        }
    }

    #[test]
    fn an_include_keeps_the_branches_of_its_own_type() {
        let includes = crate::facade::search::declaration::check_includes(
            Path::new(FILE),
            "MedicationStatement",
            &[String::from("medication")],
        )
        .expect("the include checks");
        assert_eq!(
            Some("(MedicationStatement.medication as Reference)"),
            includes
                .get("medication")
                .map(|include| include.expression.as_str())
        );
        for (resource_type, code) in [("Condition", "code"), ("Condition", "nothing")] {
            let error = crate::facade::search::declaration::check_includes(
                Path::new(FILE),
                resource_type,
                &[String::from(code)],
            )
            .expect_err("no reference parameter of that code");
            assert!(
                matches!(error, DeclarationError::Include { .. }),
                "{error:?}"
            );
        }
    }
}
