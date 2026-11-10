// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The free text a `manual` entry writes at a coded text node.
//!
//! "Full FHIR or openEHR elements are created from all given paths"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/manual.adoc`),
//! so an entry that writes a `DV_CODED_TEXT` node's `value` and no
//! `defining_code`, which the class makes mandatory (openEHR RM Release 1.1.0,
//! data types), creates a `DV_TEXT`. A coded text leaf whose value set is open
//! holds one (Simplified Formats, §Open Value-Sets and the `|other` Suffix);
//! one whose value set is closed does not, and the entry is refused at load.

use openehr_mapping_core::diagnostic::ModelPath;
use openehr_rm::v1_2::model;

use crate::engine::rm::carried::Carried;
use crate::engine::rm::carried::carried;
use crate::model::ast::ManualEntry;
use crate::model::ast::ModelMappingFile;
use crate::model::ast::keyword::Direction;
use crate::resolve::compile::Compiler;
use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::diagnostic;
use crate::resolve::program::manual::ManualPath;
use crate::resolve::program::target::OpenehrTarget;
use crate::resolve::program::target::Target;

/// The class of a coded text value.
const CODED_TEXT: &str = "DV_CODED_TEXT";

/// The class of a free-text value.
const TEXT: &str = "DV_TEXT";

impl<'a> Compiler<'a> {
    /// Writes the `openehr` paths of one `manual` entry that create a
    /// `DV_TEXT` at a `DV_CODED_TEXT` node as a `DV_TEXT`, and refuses them
    /// where the node's value set is closed. An entry that runs only out of
    /// openEHR (`direction`) writes nothing there and is left as it is.
    pub(in crate::resolve::compile) fn free_text(
        &mut self,
        file: &'a ModelMappingFile,
        entry: &ManualEntry,
        path: &ModelPath,
        direction: Option<Direction>,
        openehr: Vec<ManualPath>,
    ) -> Vec<ManualPath> {
        if direction == Some(Direction::OpenehrToFhir) {
            return openehr;
        }
        let targets: Vec<&OpenehrTarget> = openehr
            .iter()
            .filter_map(|written| match *written.target() {
                Target::Openehr(ref target) => Some(target.as_ref()),
                Target::Fhir(_) => None,
            })
            .collect();
        let mut free: Vec<String> = Vec::new();
        for target in &targets {
            let node = target.node();
            let flat_id = node.flat_id().as_str();
            if node.rm_type() != CODED_TEXT
                || target.on_element()
                || free.iter().any(|seen| seen == flat_id)
            {
                continue;
            }
            let written: Vec<Vec<&str>> = targets
                .iter()
                .filter(|other| other.node().flat_id() == node.flat_id())
                .map(|other| written(other))
                .collect();
            if !creates_text(&written) {
                continue;
            }
            if !node.list_open() {
                self.diagnostics.push(diagnostic(
                    file.file(),
                    file.header().name().value(),
                    ResolveCode::ClosedValueSet,
                    entry.name.position(),
                    &path.field("openehr"),
                    format!(
                        "`{}` writes no `defining_code` at the DV_CODED_TEXT node `{}`, so it \
                         creates a DV_TEXT, and the node's value set is closed",
                        entry.name.value(),
                        node.aql_path().as_str()
                    ),
                ));
            }
            free.push(String::from(flat_id));
        }
        openehr
            .into_iter()
            .map(|written| match *written.target() {
                Target::Openehr(ref target)
                    if free
                        .iter()
                        .any(|flat_id| flat_id == target.node().flat_id().as_str()) =>
                {
                    let target = target.as_ref().clone().with_value_class(TEXT);
                    ManualPath::new(Target::Openehr(Box::new(target)), written.value().clone())
                }
                _ => written,
            })
            .collect()
    }
}

/// Returns the attributes one manual path writes below its node: its tail,
/// or the node's `value` when the tail is empty.
fn written(target: &OpenehrTarget) -> Vec<&str> {
    let segments: Vec<&str> = target
        .tail()
        .segments
        .iter()
        .map(|segment| segment.attribute.as_str())
        .collect();
    if segments.is_empty() {
        vec!["value"]
    } else {
        segments
    }
}

/// Whether the paths `written` at one coded text node create a `DV_TEXT`:
/// each opens with an attribute `DV_TEXT` itself declares, which leaves out
/// the `defining_code` of its descendant, a `DV_TEXT` takes each as text, and
/// they fill every attribute it makes mandatory.
fn creates_text(written: &[Vec<&str>]) -> bool {
    written.iter().all(|tail| {
        tail.first()
            .is_some_and(|first| model::attribute(TEXT, first).is_some())
            && carried(TEXT, tail).is_some_and(|(_, held)| held == Carried::Text)
    }) && model::attributes(TEXT)
        .filter(|attribute| attribute.is_mandatory)
        .all(|attribute| {
            written
                .iter()
                .any(|tail| tail.first() == Some(&attribute.name))
        })
}
