// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The openEHR ports of a PROGRAMMED mapping, resolved against the template.

use core::fmt;

use crate::resolve::program::target::OpenehrTarget;

/// One openEHR place a registered function reads and writes.
///
/// A port names a node relative to the mapping's `with.openehr`. The Web
/// Template gives each data type a polymorphic `ELEMENT` admits a value node
/// of its own, so a port holds one target per class it admits, and the class
/// of the value a function writes picks the node. No specification governs
/// the shape of a function's interface: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    name: &'static str,
    values: Vec<(String, OpenehrTarget)>,
}

impl Port {
    /// Creates a port with the value node of each class it admits.
    #[must_use]
    pub const fn new(name: &'static str, values: Vec<(String, OpenehrTarget)>) -> Self {
        Self { name, values }
    }

    /// Returns the name the function's signature gives the port.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns each class the port admits with its value node, in template
    /// order.
    pub fn values(&self) -> impl Iterator<Item = (&str, &OpenehrTarget)> {
        self.values
            .iter()
            .map(|(class, target)| (class.as_str(), target))
    }

    /// Returns the value node that holds a value of `class`, `None` when the
    /// template admits no such value at the port.
    #[must_use]
    pub fn value_of(&self, class: &str) -> Option<&OpenehrTarget> {
        self.values
            .iter()
            .find(|(admitted, _)| admitted == class)
            .map(|(_, target)| target)
    }
}

/// Renders the ports of a PROGRAMMED mapping as the program rendering
/// writes them after its `mappingCode`, empty for none.
pub(crate) fn render_ports(ports: &[Port]) -> String {
    let mut rendered = String::new();
    for port in ports {
        rendered.push_str(" port ");
        rendered.push_str(&port.to_string());
    }
    rendered
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = if self.name.is_empty() { "." } else { self.name };
        write!(f, "{name} [")?;
        for (index, (class, _)) in self.values.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{class}")?;
        }
        write!(f, "]")
    }
}
