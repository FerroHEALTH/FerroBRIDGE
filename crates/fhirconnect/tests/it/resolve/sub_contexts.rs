// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Sub-contexts: a `slotContext` enters a context and maps from its `start`
//! with that context's own extensions, into the parent composition
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`).

use core::error::Error;

use fhirconnect::resolve::program::Program;
use fhirconnect::resolve::program::mapping::Mapping;
use fhirconnect::resolve::program::mapping::Method;

use crate::resolve::borrow;
use crate::resolve::codes;
use crate::resolve::context;
use crate::resolve::extension;
use crate::resolve::model;
use crate::resolve::program_of;
use crate::resolve::refusals;
use crate::resolve::render;
use crate::resolve::set_of;
use crate::resolve::start_model;

/// The slot path of the problem qualifier cluster in the diagnosis template.
const QUALIFIER: &str = "$archetype/data[at0001]/items[openEHR-EHR-CLUSTER.problem_qualifier.v2]";

/// The start model of the parent context: one mapping that enters
/// `sub.context` at the qualifier slot, and one that slots the same model
/// with a `slotArchetype`.
fn parent_model() -> String {
    start_model(&format!(
        "mappings:\n  - name: \"entered\"\n    with:\n      fhir: \"$fhirRoot\"\n      openehr: \
         \"{QUALIFIER}\"\n    slotContext: \"sub.context\"\n  - name: \"slotted\"\n    with:\n      \
         fhir: \"$fhirRoot\"\n      openehr: \"{QUALIFIER}\"\n    slotArchetype: \
         \"CLUSTER.synthetic.v2\"\n"
    ))
}

/// The start model of the parent context with only the mapping that enters
/// `sub.context`.
fn entering_model() -> String {
    start_model(&format!(
        "mappings:\n  - name: \"entered\"\n    with:\n      fhir: \"$fhirRoot\"\n      openehr: \
         \"{QUALIFIER}\"\n    slotContext: \"sub.context\"\n"
    ))
}

/// The model the sub-context starts at: the problem qualifier cluster with
/// one mapping.
fn cluster_model() -> String {
    model(
        "CLUSTER.synthetic.v2",
        "openEHR-EHR-CLUSTER.problem_qualifier.v2",
        "mappings:\n  - name: \"code\"\n    with:\n      fhir: \"$resource.code\"\n      openehr: \
         \"$archetype/items[at0063]\"\n",
    )
}

/// The extension only the sub-context loads.
fn sub_extension() -> String {
    extension(
        "sub.extension",
        "CLUSTER.synthetic.v2",
        "mappings:\n  - name: \"status\"\n    extension: \"add\"\n    with:\n      fhir: \
         \"$resource.verificationStatus\"\n      openehr: \"$archetype/items[at0063]\"\n",
    )
}

/// The parent context body, listing `contexts` under its `contexts` key.
fn parent_context(contexts: &[&str]) -> String {
    let mut listed = String::new();
    if !contexts.is_empty() {
        listed.push_str("  contexts:\n");
        for name in contexts {
            listed.push_str("    - \"");
            listed.push_str(name);
            listed.push_str("\"\n");
        }
    }
    context(
        "parent.context",
        &format!(
            "  profile:\n    url: \"http://example.org/fhir/StructureDefinition/synthetic\"\n  \
             template:\n    id: \"ferrobridge.diagnose.v1\"\n  archetypes:\n    - \
             \"EVALUATION.synthetic.v1\"\n{listed}  start: \"EVALUATION.synthetic.v1\"\n"
        ),
    )
}

/// The sub-context body, `scope: sub` unless `scope` says otherwise, starting
/// at `start`.
fn sub_context(scope: &str, start: &str) -> String {
    context(
        "sub.context",
        &format!(
            "  scope: \"{scope}\"\n  profile:\n    url: \
             \"http://example.org/fhir/StructureDefinition/synthetic-sub\"\n  template:\n    id: \
             \"ferrobridge.diagnose.v1\"\n  archetypes:\n    - \"{start}\"\n  extensions:\n    - \
             \"sub.extension\"\n  start: \"{start}\"\n"
        ),
    )
}

/// The names of the mappings directly under the method of `mapping`.
fn inner_names(mapping: &Mapping) -> Vec<String> {
    let inner = match *mapping.method() {
        Method::Slot { ref mappings, .. } | Method::SlotContext { ref mappings, .. } => {
            mappings.as_slice()
        }
        _ => &[],
    };
    inner
        .iter()
        .map(|inner| String::from(inner.name()))
        .collect()
}

/// The top-level mapping of `program` called `name`.
fn top<'p>(program: &'p Program, name: &str) -> Result<&'p Mapping, Box<dyn Error>> {
    Ok(program
        .mappings()
        .iter()
        .find(|mapping| mapping.name() == name)
        .ok_or("the mapping compiled")?)
}

#[test]
fn a_sub_context_compiles_into_the_parent_with_its_own_extensions() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §`slotContext`: the engine "maps from its `start` with its own
    // `archetypes` and `extensions`"; §Example: the extension is "loaded by the sub-context alone".
    let files = [
        ("parent.yml", parent_model()),
        ("cluster.yml", cluster_model()),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&["sub.context"])),
        (
            "sub.context.yml",
            sub_context("sub", "CLUSTER.synthetic.v2"),
        ),
    ];
    let set = set_of(&borrow(&files))?;
    let program = program_of(&set, "parent.context").map_err(|diagnostics| render(&diagnostics))?;
    let entered = top(&program, "entered")?;
    let Method::SlotContext {
        ref context,
        ref model,
        ..
    } = *entered.method()
    else {
        return Err(format!("`entered` compiled to {:?}", entered.method()).into());
    };
    assert_eq!(
        (context.as_str(), model.as_str()),
        ("sub.context", "CLUSTER.synthetic.v2")
    );
    assert_eq!(inner_names(entered), ["code", "status"]);
    assert_eq!(
        inner_names(top(&program, "slotted")?),
        ["code"],
        "the parent's own slot of the model carries none of the sub-context's extensions"
    );
    Ok(())
}

#[test]
fn a_sub_context_compiled_as_a_root_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §`scope`: a `scope: sub` context "is never a root".
    let files = [
        ("parent.yml", parent_model()),
        ("cluster.yml", cluster_model()),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&["sub.context"])),
        (
            "sub.context.yml",
            sub_context("sub", "CLUSTER.synthetic.v2"),
        ),
    ];
    let diagnostics = refusals(&borrow(&files), "sub.context")?;
    assert_eq!(codes(&diagnostics), ["fc-sub-context-as-root"]);
    Ok(())
}

#[test]
fn a_slot_context_the_parent_does_not_list_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "The sub-context must be listed under `contexts`".
    let files = [
        ("parent.yml", parent_model()),
        ("cluster.yml", cluster_model()),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&[])),
        (
            "sub.context.yml",
            sub_context("sub", "CLUSTER.synthetic.v2"),
        ),
    ];
    let diagnostics = refusals(&borrow(&files), "parent.context")?;
    assert_eq!(codes(&diagnostics), ["fc-undeclared-sub-context"]);
    Ok(())
}

#[test]
fn a_slot_context_naming_no_context_is_refused() -> Result<(), Box<dyn Error>> {
    let files = [
        ("parent.yml", parent_model()),
        ("cluster.yml", cluster_model()),
        ("parent.context.yml", parent_context(&["sub.context"])),
    ];
    let diagnostics = refusals(&borrow(&files), "parent.context")?;
    assert_eq!(codes(&diagnostics), ["fc-unknown-context-reference"]);
    Ok(())
}

#[test]
fn a_sub_context_whose_start_differs_from_the_slot_archetype_is_refused()
-> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "The `start` archetype of the sub-context must match the
    // archetype in the slot's `openehr` path".
    let anatomical = model(
        "CLUSTER.anatomical.v1",
        "openEHR-EHR-CLUSTER.anatomical_location.v1",
        "mappings:\n  - name: \"site\"\n    with:\n      fhir: \"$resource.bodySite\"\n      \
         openehr: \"$archetype/items[at0001]\"\n",
    );
    let files = [
        ("parent.yml", parent_model()),
        ("cluster.yml", cluster_model()),
        ("anatomical.yml", anatomical),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&["sub.context"])),
        (
            "sub.context.yml",
            sub_context("sub", "CLUSTER.anatomical.v1"),
        ),
    ];
    let diagnostics = refusals(&borrow(&files), "parent.context")?;
    assert!(
        codes(&diagnostics).contains(&String::from("fc-archetype-mismatch")),
        "{}",
        render(&diagnostics)
    );
    Ok(())
}

/// A cluster model whose mappings also slot a composition model at
/// `$composition`, as the `start` of a root context does.
fn composing_cluster_model() -> String {
    model(
        "CLUSTER.synthetic.v2",
        "openEHR-EHR-CLUSTER.problem_qualifier.v2",
        "mappings:\n  - name: \"code\"\n    with:\n      fhir: \"$resource.code\"\n      openehr: \
         \"$archetype/items[at0063]\"\n  - name: \"composition\"\n    with:\n      fhir: \
         \"$resource\"\n      openehr: \"$composition\"\n    slotArchetype: \
         \"COMPOSITION.synthetic.v1\"\n",
    )
}

#[test]
fn a_sub_context_whose_start_slots_a_composition_is_refused() -> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Validation errors: "`scope: sub` whose `start` slots a composition
    // model" is "sub-context must not create a composition".
    let files = [
        ("parent.yml", entering_model()),
        ("cluster.yml", composing_cluster_model()),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&["sub.context"])),
        (
            "sub.context.yml",
            sub_context("sub", "CLUSTER.synthetic.v2"),
        ),
    ];
    let diagnostics = refusals(&borrow(&files), "parent.context")?;
    assert_eq!(codes(&diagnostics), ["fc-sub-context-composition"]);
    Ok(())
}

#[test]
fn a_root_context_entered_through_slot_context_skips_its_composition_slot()
-> Result<(), Box<dyn Error>> {
    // composition-element.adoc: a context "entered as a sub-context via `slotContext` ... maps
    // into the parent composition; if its `start` slots a composition model, that mapping is
    // skipped".
    let files = [
        ("parent.yml", entering_model()),
        ("cluster.yml", composing_cluster_model()),
        ("extension.yml", sub_extension()),
        ("parent.context.yml", parent_context(&["sub.context"])),
        (
            "sub.context.yml",
            sub_context("root", "CLUSTER.synthetic.v2"),
        ),
    ];
    let set = set_of(&borrow(&files))?;
    let program = program_of(&set, "parent.context").map_err(|diagnostics| render(&diagnostics))?;
    assert_eq!(inner_names(top(&program, "entered")?), ["code", "status"]);
    Ok(())
}
