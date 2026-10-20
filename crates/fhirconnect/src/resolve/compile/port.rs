// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The openEHR ports of a PROGRAMMED mapping, resolved at load.

use openehr_mapping_core::diagnostic::ModelPath;
use openehr_mapping_core::position::Located;

use crate::resolve::error::ResolveCode;
use crate::resolve::extensions::Node;
use crate::resolve::extensions::diagnostic;
use crate::resolve::program::mapping::Method;
use crate::resolve::program::port::Port;
use crate::resolve::program::target::OpenehrTarget;

use crate::resolve::compile::Compiler;
use crate::resolve::compile::Scope;

/// The class of the node that holds one data value of its own.
const ELEMENT: &str = "ELEMENT";

impl<'a> Compiler<'a> {
    /// Resolves the ports the registered function of a PROGRAMMED mapping
    /// declares, below the mapping's own openEHR target.
    ///
    /// A port that does not resolve against the template refuses the context
    /// at load, the same as any other path, so the engine never meets a port
    /// the template lacks. A method of any other kind is returned unchanged.
    pub(super) fn ports(
        &mut self,
        node: &Node<'a>,
        method: Method,
        anchor: Option<&OpenehrTarget>,
        inner: &Scope,
        path: &ModelPath,
    ) -> Method {
        let Method::Programmed { code, .. } = method else {
            return method;
        };
        let declared = self.codes.ports(&code).unwrap_or(&[]);
        let file = node.origin;
        let owner = file.header().name().value();
        let position = node
            .mapping
            .mapping_code
            .as_ref()
            .map_or(node.mapping.position, Located::position);
        let at = path.field("mappingCode");
        let mut ports = Vec::with_capacity(declared.len());
        if declared.is_empty() {
            return Method::Programmed { code, ports };
        }
        let Some(anchor) = anchor else {
            self.diagnostics.push(diagnostic(
                file.file(),
                owner,
                ResolveCode::UnanchoredFunctionPort,
                position,
                &at,
                format!(
                    "`{code}` reads and writes openEHR ports, and the mapping writes no \
                     `with.openehr` to anchor them"
                ),
            ));
            return Method::Programmed { code, ports };
        };
        for &name in declared {
            let target = if name.is_empty() {
                Some(anchor.clone())
            } else {
                self.openehr_target(
                    file.file(),
                    owner,
                    &Located::new(position, String::from(name)),
                    inner,
                    &at,
                )
            };
            let Some(target) = target else {
                continue;
            };
            if !target.tail().segments.is_empty() {
                self.diagnostics.push(diagnostic(
                    file.file(),
                    owner,
                    ResolveCode::UnknownTemplateNode,
                    position,
                    &at,
                    format!("the port `{name}` of `{code}` names no node of the template"),
                ));
                continue;
            }
            if target.occurrences().len() != anchor.occurrences().len() {
                self.diagnostics.push(diagnostic(
                    file.file(),
                    owner,
                    ResolveCode::RepeatingFunctionPort,
                    position,
                    &at,
                    format!(
                        "the port `{name}` of `{code}` repeats below `{}`, and a port holds one \
                         value",
                        anchor.path()
                    ),
                ));
                continue;
            }
            ports.push(Port::new(name, self.port_values(&target)));
        }
        Method::Programmed { code, ports }
    }

    /// Returns the value node of each class a port's node admits.
    ///
    /// The Web Template gives each data type a polymorphic `ELEMENT` admits a
    /// child node of its own; any other node holds a value of its own class.
    fn port_values(&self, target: &OpenehrTarget) -> Vec<(String, OpenehrTarget)> {
        let node = target.node();
        if node.rm_type() != ELEMENT {
            return vec![(String::from(node.rm_type()), target.clone())];
        }
        node.children()
            .iter()
            .filter_map(|flat_id| self.template.node_by_flat_id(flat_id).ok())
            .map(|child| {
                // NOTE: openEHR RM data_structures §ELEMENT: `value` holds one data value, so
                // the value node repeats exactly where the `ELEMENT` does and takes its axes.
                let value = OpenehrTarget::new(
                    child.rm_path().clone(),
                    child.clone(),
                    target.tail().clone(),
                    target.occurrences().to_vec(),
                );
                (String::from(child.rm_type()), value)
            })
            .collect()
    }
}
