// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a coded leaf of a template admits a free-text value.
//!
//! "When an archetype constrains a leaf to `DV_CODED_TEXT` with an _open_
//! value-set ... the same leaf may at runtime carry either a `DV_CODED_TEXT`
//! (chosen from the list) or a `DV_TEXT` (free-text value)", which the Web
//! Template states as `listOpen: true` on the leaf's input (Simplified
//! Formats master04 §Open Value-Sets and the `|other` Suffix).

use openehr_sdt::flat::webtemplate::model::WebTemplateNode;

use crate::index::ResolvedNode;
use crate::index::WebTemplateIndex;
use crate::template::PathError;

/// The class a free-text value stands in for.
const CODED_TEXT: &str = "DV_CODED_TEXT";

impl WebTemplateIndex {
    /// Returns whether a `DV_TEXT` stands at `node` in place of the
    /// `DV_CODED_TEXT` the template constrains it to.
    ///
    /// An open value set (`listOpen: true`) admits one; a closed one, and a
    /// node of any other class, does not.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::UnknownNode`] when the node belongs to another
    /// template.
    pub fn admits_free_text(&self, node: &ResolvedNode) -> Result<bool, PathError> {
        self.entry(node.flat_id())?;
        if node.rm_type() != CODED_TEXT {
            return Ok(false);
        }
        Ok(self.web_template_node(node).is_some_and(|leaf| {
            leaf.inputs
                .iter()
                .any(|input| input.list_open == Some(true))
        }))
    }

    /// Returns the Web Template node `node` was indexed from, walked down
    /// the identifiers of its flat id.
    fn web_template_node(&self, node: &ResolvedNode) -> Option<&WebTemplateNode> {
        let mut steps = node.flat_id().as_str().split('/');
        let root = &self.web_template.tree;
        if steps.next() != Some(root.id.as_str()) {
            return None;
        }
        let mut reached = root;
        let mut rest = steps.peekable();
        while let Some(id) = rest.next() {
            let last = rest.peek().is_none();
            reached = reached
                .children
                .iter()
                .find(|child| child.id == id && (!last || child.rm_type == node.rm_type()))?;
        }
        Some(reached)
    }
}
