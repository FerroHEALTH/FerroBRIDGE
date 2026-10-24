// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ordinal options an ADL 1.4 operational template constrains at each
//! `DV_ORDINAL` node.
//!
//! An OPT 1.4 `C_DV_ORDINAL` lists the `DV_ORDINAL` values a node admits,
//! each a `value` and a coded `symbol` (the openEHR OPT 1.4 XML schema,
//! `C_DV_ORDINAL`; openEHR RM Release 1.1.0, data types, the `DV_ORDINAL`
//! class). The Web Template keeps the codes and the ordinals of that list but
//! not the terminology of each symbol, so the index reads the list from the
//! operational template and keys it by the `aqlPath` the Web Template builder
//! gives the node, the key the embedded-template map uses.

use std::collections::BTreeMap;

use openehr_its::opt14::types::CAttribute;
use openehr_its::opt14::types::CDvOrdinal;
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::OperationalTemplate;

use crate::index::embedded::fixed_name;
use crate::index::embedded::step;
use crate::index::paths::AqlPath;

/// One option of a `C_DV_ORDINAL` constraint: the ordinal value and the code
/// of its symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalOption {
    /// The ordinal value, `DV_ORDINAL.value`.
    value: i32,
    /// The code of the symbol, `DV_ORDINAL.symbol.defining_code.code_string`.
    code: String,
    /// The terminology of the symbol, the `terminology_id` of its defining
    /// code (`local` for an archetype's own term set).
    terminology: String,
}

impl OrdinalOption {
    /// Creates an option of `value` with the symbol `code` in `terminology`.
    #[must_use]
    pub fn new(value: i32, code: &str, terminology: &str) -> Self {
        Self {
            value,
            code: code.to_owned(),
            terminology: terminology.to_owned(),
        }
    }

    /// Returns the ordinal value.
    #[must_use]
    pub const fn value(&self) -> i32 {
        self.value
    }

    /// Returns the code of the symbol.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the terminology of the symbol.
    #[must_use]
    pub fn terminology(&self) -> &str {
        &self.terminology
    }
}

/// Reads the ordinal options of every `C_DV_ORDINAL` node of `template`,
/// keyed by the `aqlPath` of the node.
pub(super) fn ordinal_options(
    template: &OperationalTemplate,
) -> BTreeMap<AqlPath, Vec<OrdinalOption>> {
    let mut found = BTreeMap::new();
    walk(&template.definition.attributes, "", &mut found);
    found
}

/// Returns the options of one constraint, in template order.
fn options(ordinal: &CDvOrdinal) -> Vec<OrdinalOption> {
    ordinal
        .list
        .iter()
        .map(|entry| {
            let code = &entry.symbol.defining_code;
            OrdinalOption::new(entry.value, &code.code_string, &code.terminology_id.value)
        })
        .collect()
}

/// Records the ordinal options under `attributes`, whose owner sits at
/// `parent`.
fn walk(
    attributes: &[CAttribute],
    parent: &str,
    found: &mut BTreeMap<AqlPath, Vec<OrdinalOption>>,
) {
    for attribute in attributes {
        let (name, children) = match attribute {
            CAttribute::CMultipleAttribute(multiple) => {
                (multiple.rm_attribute_name.as_str(), &multiple.children)
            }
            CAttribute::CSingleAttribute(single) => {
                (single.rm_attribute_name.as_str(), &single.children)
            }
        };
        if name == "name" {
            continue;
        }
        for child in children {
            match child {
                CObject::CArchetypeRoot(root) => {
                    let path = step(
                        parent,
                        name,
                        root.archetype_id.value.as_str(),
                        fixed_name(&root.attributes),
                    );
                    walk(&root.attributes, &path, found);
                }
                CObject::CComplexObject(object) => {
                    let path = step(
                        parent,
                        name,
                        object.node_id.as_str(),
                        fixed_name(&object.attributes),
                    );
                    walk(&object.attributes, &path, found);
                }
                CObject::CDvOrdinal(ordinal) => {
                    let path = step(parent, name, ordinal.node_id.as_str(), None);
                    found.insert(AqlPath::new(path), options(ordinal));
                }
                _ => {}
            }
        }
    }
}
