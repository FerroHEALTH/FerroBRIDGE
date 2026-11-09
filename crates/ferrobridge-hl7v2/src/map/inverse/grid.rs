// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One v2 value as the inverse builds it: components, each its
//! subcomponents, by position from 1.

use crate::map::condition::Probe;
use crate::write::value::{Component, Repetition};

/// One v2 value as built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Grid {
    components: Vec<Vec<String>>,
}

impl Grid {
    /// Writes `text` at `path`: the first component for an empty path, a
    /// component's first subcomponent, or a subcomponent.
    pub(super) fn set(&mut self, path: &[usize], text: String) {
        let (component, subcomponent) = position(path);
        let slot = self.slot(component, subcomponent);
        if let Some(slot) = slot {
            *slot = text;
        }
    }

    /// Writes `inner`, the value of a data type, at `path`: as this value for
    /// an empty path, and as the subcomponents of a component otherwise.
    pub(super) fn nest(&mut self, path: &[usize], inner: Self) {
        match path.first() {
            None => *self = inner,
            Some(&component) => {
                for (offset, parts) in inner.components.into_iter().enumerate() {
                    if let Some(text) = parts.into_iter().next() {
                        self.set(&[component, offset.saturating_add(1)], text);
                    }
                }
            }
        }
    }

    /// Returns the text at `path`, `None` when it is empty.
    pub(super) fn get(&self, path: &[usize]) -> Option<&str> {
        let (component, subcomponent) = position(path);
        self.components
            .get(component.checked_sub(1)?)?
            .get(subcomponent.checked_sub(1)?)
            .map(String::as_str)
            .filter(|text| !text.is_empty())
    }

    /// Returns what a condition reads at `path`.
    pub(super) fn probe(&self, path: &[usize]) -> Probe {
        let text = self.get(path).map(String::from);
        Probe {
            valued: text.is_some(),
            count: u64::from(text.is_some()),
            text,
        }
    }

    /// Returns whether no subcomponent holds text.
    pub(super) fn is_empty(&self) -> bool {
        self.components
            .iter()
            .all(|parts| parts.iter().all(String::is_empty))
    }

    /// Returns the value as one repetition to write.
    pub(super) fn into_repetition(self) -> Repetition {
        self.components
            .into_iter()
            .map(Component::from_iter)
            .collect()
    }

    /// Returns the text slot at `component` and `subcomponent`, growing the
    /// value to reach it; `None` for a position 0.
    fn slot(&mut self, component: usize, subcomponent: usize) -> Option<&mut String> {
        let component = component.checked_sub(1)?;
        let subcomponent = subcomponent.checked_sub(1)?;
        if self.components.len() <= component {
            self.components
                .resize(component.saturating_add(1), Vec::new());
        }
        let parts = self.components.get_mut(component)?;
        if parts.len() <= subcomponent {
            parts.resize(subcomponent.saturating_add(1), String::new());
        }
        parts.get_mut(subcomponent)
    }
}

/// The component and subcomponent a path names: the first component for an
/// empty path, and a component's first subcomponent for one position.
const fn position(path: &[usize]) -> (usize, usize) {
    match path {
        [] => (1, 1),
        [component] => (*component, 1),
        [component, subcomponent, ..] => (*component, *subcomponent),
    }
}
