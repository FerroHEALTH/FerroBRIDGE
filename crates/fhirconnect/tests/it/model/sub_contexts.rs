// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sub-context keys through the whole loader: the strict schemas and the
//! semantic rules of
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`.

use core::error::Error;

use fhirconnect::model::ast::keyword::ContextScope;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;

/// A model mapping file named `name` with the mappings `body`.
fn model(name: &str, body: &str) -> String {
    format!(
        "grammar: FHIRConnect/v1.0.0\ntype: model\nmetadata:\n  name: {name}\n  version: \
         1.0.0\nspec:\n  system: FHIR\n  version: R4\n  openEhrConfig:\n    archetype: \
         openEHR-EHR-CLUSTER.problem_qualifier.v2\n  fhirConfig:\n    structureDefinition: \
         http://hl7.org/fhir/StructureDefinition/Condition\n{body}"
    )
}

/// A context mapping file named `name` with the extra `context` keys `keys`.
fn context(name: &str, keys: &str) -> String {
    format!(
        "grammar: FHIRConnect/v1.0.0\ntype: context\nmetadata:\n  name: {name}\n  version: \
         1.0.0\nspec:\n  system: FHIR\n  version: R4\ncontext:\n  profile:\n    url: \
         \"http://example.org/fhir/StructureDefinition/{name}\"\n  template:\n    id: \
         \"ferrobridge.diagnose.v1\"\n  archetypes:\n    - \"CLUSTER.a.v1\"\n{keys}  start: \
         \"CLUSTER.a.v1\"\n"
    )
}

/// One mapping entering the context `entered` at `openehr`.
fn entering(entered: &str, openehr: &str) -> String {
    format!(
        "mappings:\n  - name: \"entered\"\n    with:\n      fhir: \"$fhirRoot\"\n      openehr: \
         \"{openehr}\"\n    slotContext: \"{entered}\"\n"
    )
}

/// Loads the named files through the whole loader.
fn load(files: &[(&str, String)]) -> Result<Result<MappingSet, Vec<Diagnostic>>, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let mut paths = Vec::new();
    for (name, text) in files {
        let path = directory.path().join(name);
        std::fs::write(&path, text)?;
        paths.push(path);
    }
    Ok(load_set(paths, &StaticMappingCodes::default()))
}

/// The codes of the refusals a load raised, sorted.
fn refused(files: &[(&str, String)]) -> Result<Vec<String>, Box<dyn Error>> {
    let Err(diagnostics) = load(files)? else {
        return Err("the set loaded".into());
    };
    let mut codes: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    codes.sort();
    Ok(codes)
}

#[test]
fn scope_and_contexts_read_into_the_context_block() -> Result<(), Box<dyn Error>> {
    let files = [
        ("a.yml", model("CLUSTER.a.v1", "mappings: []\n")),
        (
            "parent.yml",
            context("parent.context", "  contexts:\n    - \"sub.context\"\n"),
        ),
        ("sub.yml", context("sub.context", "  scope: \"sub\"\n")),
    ];
    let set = load(&files)?.map_err(|diagnostics| format!("{diagnostics:?}"))?;
    let parent = set
        .context(&MappingName::new("parent.context")?)
        .ok_or("the parent loaded")?;
    let sub = set
        .context(&MappingName::new("sub.context")?)
        .ok_or("the sub-context loaded")?;
    assert_eq!(
        parent.context().scope(),
        ContextScope::Root,
        "root by default"
    );
    assert_eq!(sub.context().scope(), ContextScope::Sub);
    let listed: Vec<&str> = parent
        .context()
        .contexts
        .iter()
        .map(|name| name.value().as_str())
        .collect();
    assert_eq!(listed, ["sub.context"]);
    Ok(())
}

#[test]
fn slot_archetype_and_slot_context_on_one_mapping_are_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §`slotContext`: "The two keys are mutually exclusive on one mapping".
    let both = "mappings:\n  - name: \"both\"\n    with:\n      fhir: \"$fhirRoot\"\n      openehr: \
                \"$archetype\"\n    slotArchetype: \"CLUSTER.a.v1\"\n    slotContext: \
                \"sub.context\"\n";
    let files = [
        ("a.yml", model("CLUSTER.a.v1", both)),
        (
            "parent.yml",
            context("parent.context", "  contexts:\n    - \"sub.context\"\n"),
        ),
        ("sub.yml", context("sub.context", "  scope: \"sub\"\n")),
    ];
    // The refused model leaves the set, so the contexts naming it are refused too.
    let codes = refused(&files)?;
    assert_eq!(
        codes.first().map(String::as_str),
        Some("fc-schema-violation"),
        "{codes:?}"
    );
    Ok(())
}

#[test]
fn a_scope_outside_root_and_sub_is_refused() -> Result<(), Box<dyn Error>> {
    let files = [
        ("a.yml", model("CLUSTER.a.v1", "mappings: []\n")),
        ("sub.yml", context("sub.context", "  scope: \"leaf\"\n")),
    ];
    assert_eq!(refused(&files)?, ["fc-schema-violation"]);
    Ok(())
}

#[test]
fn a_slot_context_naming_no_loaded_context_is_refused() -> Result<(), Box<dyn Error>> {
    let files = [
        (
            "a.yml",
            model("CLUSTER.a.v1", &entering("missing.context", "$archetype")),
        ),
        ("parent.yml", context("parent.context", "")),
    ];
    assert_eq!(refused(&files)?, ["fc-unknown-mapping-reference"]);
    Ok(())
}

#[test]
fn a_slot_context_at_the_reference_variable_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "A `slotContext` mapping with `openehr: $reference` is a
    // validation error".
    let files = [
        (
            "a.yml",
            model("CLUSTER.a.v1", &entering("sub.context", "$reference")),
        ),
        (
            "parent.yml",
            context("parent.context", "  contexts:\n    - \"sub.context\"\n"),
        ),
        ("sub.yml", context("sub.context", "  scope: \"sub\"\n")),
    ];
    assert_eq!(refused(&files)?, ["fc-sub-context-reference-variable"]);
    Ok(())
}

#[test]
fn a_sub_context_no_context_lists_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "Every `scope: sub` context must be listed under `contexts` of
    // at least one context".
    let files = [
        ("a.yml", model("CLUSTER.a.v1", "mappings: []\n")),
        ("sub.yml", context("sub.context", "  scope: \"sub\"\n")),
    ];
    assert_eq!(refused(&files)?, ["fc-unreferenced-sub-context"]);
    Ok(())
}

#[test]
fn a_cycle_through_contexts_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "References must be acyclic".
    let files = [
        ("a.yml", model("CLUSTER.a.v1", "mappings: []\n")),
        (
            "one.yml",
            context("one.context", "  contexts:\n    - \"two.context\"\n"),
        ),
        (
            "two.yml",
            context("two.context", "  contexts:\n    - \"one.context\"\n"),
        ),
    ];
    assert_eq!(
        refused(&files)?,
        ["fc-cyclic-context-reference", "fc-cyclic-context-reference"]
    );
    Ok(())
}

#[test]
fn a_contexts_entry_naming_no_loaded_context_is_refused() -> Result<(), Box<dyn Error>> {
    let files = [
        ("a.yml", model("CLUSTER.a.v1", "mappings: []\n")),
        (
            "parent.yml",
            context("parent.context", "  contexts:\n    - \"missing.context\"\n"),
        ),
    ];
    assert_eq!(refused(&files)?, ["fc-unknown-mapping-reference"]);
    Ok(())
}
