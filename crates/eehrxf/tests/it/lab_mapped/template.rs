// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The template the laboratory project maps over: the published
//! `KDS_Laborbericht` with the composition's Comment added, and nothing else
//! changed.

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::mappings::LABORATORY;
use ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::TemplateSource;

use super::{LAB_TEMPLATE, LAB_TEMPLATE_ID};

/// Each node of the template `opt` as its path, RM type and occurrences.
fn constraints(opt: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let index = WebTemplateIndex::build(&TemplateSource::opt14(opt)?)?;
    Ok(index
        .nodes()
        .map(|node| {
            format!(
                "{} {} {:?}..{:?}",
                node.aql_path(),
                node.rm_type(),
                node.min(),
                node.max()
            )
        })
        .collect())
}

#[test]
fn the_template_keeps_every_kds_constraint_and_adds_only_the_comment() -> Result<(), Box<dyn Error>>
{
    let published = constraints(KDS_LABORBERICHT_OPT)?;
    let derived = constraints(LAB_TEMPLATE)?;
    let dropped: Vec<_> = published.difference(&derived).collect();
    assert!(dropped.is_empty(), "{dropped:#?}");
    let note = LABORATORY.note().ok_or("the project's comment")?;
    let added: Vec<_> = derived.difference(&published).collect();
    assert!(!added.is_empty(), "the comment is added");
    assert!(
        added
            .iter()
            .all(|node| node.starts_with(&note.element_path())),
        "{added:#?}"
    );
    Ok(())
}

#[test]
fn the_template_has_its_own_identity_and_the_comment_the_project_writes()
-> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&TemplateSource::opt14(LAB_TEMPLATE)?)?;
    assert_eq!(index.template_id(), LAB_TEMPLATE_ID);
    let note = LABORATORY.note().ok_or("the project's comment")?;
    assert_eq!(
        note.value_path(),
        "/context/other_context[at0001]/items[at0007]/value"
    );
    assert_eq!(
        index.node(&AqlPath::new(note.value_path()))?.rm_type(),
        "DV_TEXT"
    );
    let published = WebTemplateIndex::build(&TemplateSource::opt14(KDS_LABORBERICHT_OPT)?)?;
    assert!(
        published.node(&AqlPath::new(note.value_path())).is_err(),
        "the published template has no comment"
    );
    Ok(())
}
