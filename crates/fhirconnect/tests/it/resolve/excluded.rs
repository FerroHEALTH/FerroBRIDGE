// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A model mapping that names a node the template excludes.
//!
//! Model mappings "contain some mappings that are not part of the
//! corresponding Profile/Templates, as they have (0..0) occurrences. These
//! are ignored and not executed"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/model-mappings.adoc`,
//! §Mapping 0..0 fields). The template under test is the testkit's diagnosis
//! template with one more term in the `problem_diagnosis` root, `at0009`, for
//! which it carries no node: the OPT 1.4 `C_ARCHETYPE_ROOT` lists its
//! archetype's term definitions whole, so that is a node the template
//! excludes.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::resolve::PROBLEM;
use crate::resolve::codes;
use crate::resolve::context;
use crate::resolve::extension;
use crate::resolve::set_of;
use crate::resolve::start_context;
use crate::resolve::start_model;

/// The last term the diagnosis template's `problem_diagnosis` root defines,
/// as the fixture spells it.
const LAST_TERM: &str = "<items id=\"description\">The synthetic date of onset of the problem</items>\n        </term_definitions>";

/// A method naming the excluded node, with a child under it.
const EXCLUDED: &str = "  - name: \"certainty\"\n    with:\n      fhir: \"$resource\"\n      \
                        openehr: \"$archetype/data[at0001]/items[at0009]\"\n      type: \"NONE\"\n    \
                        followedBy:\n      mappings:\n        - name: \"note\"\n          with:\n            \
                        fhir: \"note.text\"\n            openehr: \"$openehrRoot\"\n";

/// Builds the index over the diagnosis template with `at0009` excluded.
fn excluding_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let xml = ferrobridge_testkit::fixtures::DIAGNOSE_OPT;
    if !xml.contains(LAST_TERM) {
        return Err("the diagnosis template no longer spells its last term as expected".into());
    }
    let patched = xml.replacen(
        LAST_TERM,
        &format!(
            "{LAST_TERM}\n        <term_definitions code=\"at0009\">\n          <items \
             id=\"text\">Excluded</items>\n          <items id=\"description\">A node the \
             template excludes</items>\n        </term_definitions>"
        ),
        1,
    );
    let opt = openehr_its::opt14::from_xml(&patched)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles one synthetic model, plus any extension files, against the
/// template with `at0009` excluded.
fn compiled(
    body: &str,
    extensions: &[(&str, String)],
) -> Result<Result<Arc<Program>, Vec<Diagnostic>>, Box<dyn Error>> {
    let names: Vec<&str> = extensions
        .iter()
        .map(|(name, _)| name.trim_end_matches(".yml"))
        .collect();
    let model = start_model(body);
    let context = context("synthetic.context", &start_context(&names));
    let mut files: Vec<(&str, &str)> = vec![
        ("model.yml", model.as_str()),
        ("context.yml", context.as_str()),
    ];
    files.extend(extensions.iter().map(|(name, text)| (*name, text.as_str())));
    let set = set_of(&files)?;
    Ok(compile(
        &set,
        &MappingName::new("synthetic.context")?,
        &excluding_template()?,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    ))
}

#[test]
fn a_model_method_naming_an_excluded_node_is_ignored_with_everything_under_it()
-> Result<(), Box<dyn Error>> {
    let program = compiled(&format!("{PROBLEM}{EXCLUDED}"), &[])?
        .map_err(|diagnostics| format!("{:?}", codes(&diagnostics)))?;
    assert!(program.mapping_named("problem").is_some());
    assert!(program.mapping_named("certainty").is_none());
    assert!(program.mapping_named("certainty.note").is_none());
    assert_eq!(codes(program.warnings()), vec!["fc-excluded-template-node"]);
    let message = program
        .warnings()
        .first()
        .map(Diagnostic::message)
        .ok_or("a warning")?;
    assert!(
        message.contains("at0009") && message.contains("ferrobridge.diagnose.v1"),
        "{message}"
    );
    Ok(())
}

/// A method with no child naming `at0009`, so a refusal of it stands alone.
const LEAF: &str = "  - name: \"certainty\"\n    with:\n      fhir: \"$resource.note.text\"\n      \
                    openehr: \"$archetype/data[at0001]/items[at0009]\"\n";

#[test]
fn a_code_the_archetype_does_not_define_is_still_refused() -> Result<(), Box<dyn Error>> {
    let body = format!("{PROBLEM}{}", LEAF.replace("at0009", "at0099"));
    let diagnostics = compiled(&body, &[])?
        .err()
        .ok_or("a code the archetype does not define compiled")?;
    assert_eq!(codes(&diagnostics), vec!["fc-unknown-template-node"]);
    Ok(())
}

/// `EVALUATION.data` holds one `ITEM_STRUCTURE`, and the template keeps
/// `data[at0001]` there, so `data[at0009]` names the wrong place.
#[test]
fn a_defined_code_in_a_single_valued_attribute_the_template_fills_is_still_refused()
-> Result<(), Box<dyn Error>> {
    let body = format!(
        "{PROBLEM}{}",
        LEAF.replace("data[at0001]/items[at0009]", "data[at0009]")
    );
    let diagnostics = compiled(&body, &[])?
        .err()
        .ok_or("a defined code in a filled single-valued attribute compiled")?;
    assert_eq!(codes(&diagnostics), vec!["fc-unknown-template-node"]);
    Ok(())
}

/// An extension is written for one template, so the model mapping's rule does
/// not cover a method it adds.
#[test]
fn an_extension_method_naming_an_excluded_node_is_still_refused() -> Result<(), Box<dyn Error>> {
    let added = extension(
        "synthetic.extension",
        "EVALUATION.synthetic.v1",
        &format!(
            "mappings:\n{}",
            LEAF.replacen("    with:", "    extension: \"add\"\n    with:", 1)
        ),
    );
    let diagnostics = compiled(PROBLEM, &[("synthetic.extension.yml", added)])?
        .err()
        .ok_or("an extension method naming an excluded node compiled")?;
    assert_eq!(codes(&diagnostics), vec!["fc-unknown-template-node"]);
    Ok(())
}
