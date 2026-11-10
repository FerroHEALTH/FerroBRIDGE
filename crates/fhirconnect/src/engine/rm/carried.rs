// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a tail below a template node reaches: the attribute it ends on and
//! the class that declares it.

use crate::resolve::derive;
use openehr_rm::v1_2::model;
use openehr_rm::v1_2::model::Container;
use openehr_rm::v1_2::paths::RmPath;
use openehr_rm::v1_2::typing::RmType;
use openehr_rm::v1_2::typing::base_name;

use super::tail_walk;
use super::tail_walk::concrete_forms;
use super::{ENTRY_TIMES, FAMILIES, PARTY_ROOTS};

/// What the attribute a tail ends on holds.
///
/// A tail below a template node names either one scalar attribute of the
/// node's data value or one data value nested in it; the whole value reaches
/// the wire under `|raw`, so every attribute the reference model gives the
/// value is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    /// A string attribute.
    Text,
    /// A real-valued attribute.
    Real,
    /// An integer attribute.
    Integer,
    /// A boolean attribute.
    Boolean,
    /// A data value of the tail's leaf class, read and written by its cell.
    Value,
    /// A data value of the tail's leaf class that is an attribute of a
    /// structural node, which FLAT writes as the `_`-prefixed family of the
    /// node (`_provider` on an `ENTRY`) and the engine writes through
    /// [`crate::engine::family::provider`].
    Family,
    /// A `DV_DATE_TIME` attribute of an entry the Web Template carries no
    /// node for, an `OBSERVATION`'s history origin or an `ACTION`'s time,
    /// which FLAT writes under the entry's node with the key
    /// [`super::entry_time_key`] answers.
    EntryTime,
}

/// Returns the class a tail below a node of `rm_type` is written as, and what
/// the tail's attribute holds, `None` when the engine cannot carry it.
///
/// The attributes come from `openehr_rm::v1_2::model`, looked up on the node's
/// class and on its concrete descendants, since a `DV_TEXT` node may hold a
/// `DV_CODED_TEXT` and a `PARTY_PROXY` node a `PARTY_IDENTIFIED`
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_coded_text_class>).
/// The class is the most general concrete one that declares the tail's first
/// attribute. A generic node (`DV_INTERVAL<DV_QUANTITY>`) is looked up by its
/// class, and a bound below it is its parameter
/// ([`derive::attribute_class`]). A tail may step through a `List` attribute,
/// and then names one element of the list ([`list_steps`]). A tail below a
/// structural node other than a [`FAMILIES`] or [`PARTY_ROOTS`] entry, one
/// that steps through a `Set` or `Hash`, whose members have no position, and
/// one that ends on a container attribute, which holds no single value, is
/// `None`. No specification governs the walk: our own design.
#[must_use]
pub fn carried(rm_type: &str, tail: &[&str]) -> Option<(&'static str, Carried)> {
    let wanted = tail.join("/");
    let class = base_name(rm_type);
    if let Some(&(owner, _)) = ENTRY_TIMES
        .iter()
        .find(|&&(owner, path)| path == wanted && model::is_a(class, owner))
    {
        return Some((owner, Carried::EntryTime));
    }
    if let Some(&(owner, _)) = FAMILIES
        .iter()
        .find(|&&(owner, path)| path == wanted && model::is_a(class, owner))
    {
        return Some((owner, Carried::Family));
    }
    if let Some(&(owner, _)) = PARTY_ROOTS
        .iter()
        .find(|&&(owner, path)| path == wanted && class == owner)
    {
        return Some((owner, Carried::Value));
    }
    let walked = walk(rm_type, tail)?;
    if walked.container != Container::None {
        return None;
    }
    Some((walked.owner, held(&walked.declared)?))
}

/// Returns the position in `tail` of each attribute that is a `List`, in
/// order, `None` when the engine cannot walk the tail at all.
///
/// An openEHR RM `List` attribute holds an ordered sequence, so one element
/// is addressed by its 1-based position (openEHR BASE Release 1.2.0 §Paths
/// and Locators, "Using Positional Predicates"); the engine gives each such
/// step its own occurrence axis inside the node's value. A tail that ends on
/// a `List` names the list as an axis whose elements the mapping's children
/// fill. No specification governs the axis: our own design.
#[must_use]
pub fn list_steps(rm_type: &str, tail: &[&str]) -> Option<Vec<usize>> {
    walk(rm_type, tail).map(|walked| walked.lists)
}

/// The attribute a tail ends on, reached by [`walk`].
struct Walked {
    /// The class that declares the tail's first attribute.
    owner: &'static str,
    /// The container shape of the last attribute the tail names.
    container: Container,
    /// The class one value of that attribute holds.
    declared: String,
    /// The position in the tail of each `List` attribute.
    lists: Vec<usize>,
}

/// Walks a tail below a data value with [`tail_walk::steps`].
///
/// A step through a `Set` or `Hash` is `None`, since neither orders its
/// members (openEHR BASE Release 1.2.0, foundation types §Container types),
/// and so is a walk below a structural node.
fn walk(rm_type: &str, tail: &[&str]) -> Option<Walked> {
    let class = rm_type.parse::<RmType>().ok()?;
    if model::is_a(class.name(), derive::STRUCTURAL) {
        return None;
    }
    let owner = owner_of(class.name(), tail.first()?)?;
    let path = tail.join("/").parse::<RmPath>().ok()?;
    // NOTE: a tail the model refuses or stops on is one the engine cannot carry, which `None` answers.
    let steps = tail_walk::steps(rm_type, &path).ok().flatten()?;
    let mut lists = Vec::new();
    for (position, step) in steps.iter().enumerate() {
        match step.container() {
            Container::None => {}
            Container::List => lists.push(position),
            Container::Set | Container::Hash => return None,
        }
    }
    let last = steps.last()?;
    Some(Walked {
        owner,
        container: last.container(),
        declared: String::from(last.item().name()),
        lists,
    })
}

/// Returns the most general concrete class at or below `rm_type` that
/// declares `attribute`, earliest in the model's order on a tie.
fn owner_of(rm_type: &str, attribute: &str) -> Option<&'static str> {
    let declaring: Vec<&'static str> = concrete_forms(rm_type)
        .into_iter()
        .filter(|form| model::class(form).is_some_and(|class| !class.is_abstract))
        .filter(|form| model::attribute(form, attribute).is_some())
        .collect();
    declaring.iter().copied().find(|&form| {
        !declaring
            .iter()
            .any(|&other| other != form && model::is_a(form, other))
    })
}

/// Returns what an attribute of `declared_type` holds, `None` for a type the
/// engine writes no scalar or value of.
///
/// The primitive names are the BASE foundation types the RM BMM declares
/// (<https://specifications.openehr.org/releases/BASE/Release-1.2.0/foundation_types.html>).
fn held(declared_type: &str) -> Option<Carried> {
    match declared_type {
        "String" | "Character" => Some(Carried::Text),
        "Real" => Some(Carried::Real),
        "Integer" | "Integer64" => Some(Carried::Integer),
        "Boolean" => Some(Carried::Boolean),
        other => model::class(other).map(|_| Carried::Value),
    }
}

#[cfg(test)]
mod tests {
    use super::{Carried, carried, list_steps};

    #[test]
    fn a_tail_reaches_every_attribute_the_model_gives_the_class() {
        assert_eq!(
            carried("DV_TEXT", &["hyperlink", "value"]),
            Some(("DV_TEXT", Carried::Text))
        );
        assert_eq!(
            carried("DV_TEXT", &["language"]),
            Some(("DV_TEXT", Carried::Value))
        );
        assert_eq!(
            carried("DV_DATE_TIME", &["normal_status", "code_string"]),
            Some(("DV_DATE_TIME", Carried::Text))
        );
        assert_eq!(
            carried("DV_PROPORTION", &["numerator"]),
            Some(("DV_PROPORTION", Carried::Real))
        );
        assert_eq!(
            carried("DV_INTERVAL", &["lower_included"]),
            Some(("DV_INTERVAL", Carried::Boolean))
        );
    }

    #[test]
    fn a_tail_below_a_generic_interval_reaches_its_parameter() {
        // RM 1.1.0 data_types §DV_INTERVAL: `lower` and `upper` are `T`.
        assert_eq!(
            carried("DV_INTERVAL<DV_QUANTITY>", &["lower"]),
            Some(("DV_INTERVAL", Carried::Value))
        );
        assert_eq!(
            carried("DV_INTERVAL<DV_QUANTITY>", &["upper", "magnitude"]),
            Some(("DV_INTERVAL", Carried::Real))
        );
        assert_eq!(
            carried("DV_INTERVAL<DV_COUNT>", &["lower", "magnitude"]),
            Some(("DV_INTERVAL", Carried::Integer))
        );
        assert_eq!(
            carried("DV_INTERVAL<DV_QUANTITY>", &["lower_included"]),
            Some(("DV_INTERVAL", Carried::Boolean))
        );
    }

    #[test]
    fn a_tail_the_interval_parameter_does_not_carry_is_not_carried() {
        assert_eq!(
            carried("DV_INTERVAL<DV_QUANTITY>", &["lower", "numerator"]),
            None
        );
        assert_eq!(carried("DV_INTERVAL<DV_QUANTITY>", &["middle"]), None);
    }

    #[test]
    fn a_tail_only_a_subtype_declares_writes_the_subtype() {
        assert_eq!(
            carried("DV_TEXT", &["defining_code", "code_string"]),
            Some(("DV_CODED_TEXT", Carried::Text))
        );
        assert_eq!(
            carried("PARTY_PROXY", &["name"]),
            Some(("PARTY_IDENTIFIED", Carried::Text))
        );
    }

    #[test]
    fn a_tail_that_ends_on_a_list_or_below_a_structure_is_not_carried() {
        // RM 1.1.0 data_types §DV_TEXT: `mappings` is a List<TERM_MAPPING>, no single value.
        assert_eq!(carried("DV_TEXT", &["mappings"]), None);
        assert_eq!(carried("DV_QUANTITY", &["other_reference_ranges"]), None);
        assert_eq!(carried("EVALUATION", &["other_participations"]), None);
        assert_eq!(carried("DV_TEXT", &["no_such_attribute"]), None);
    }

    #[test]
    fn a_tail_through_a_list_reaches_the_attributes_of_one_element() {
        // RM 1.1.0 data_types §DV_TEXT `mappings`, §DV_ORDERED `other_reference_ranges`.
        assert_eq!(
            carried("DV_TEXT", &["mappings", "match"]),
            Some(("DV_TEXT", Carried::Text))
        );
        assert_eq!(
            carried("DV_TEXT", &["mappings", "target", "code_string"]),
            Some(("DV_TEXT", Carried::Text))
        );
        assert_eq!(
            carried("DV_QUANTITY", &["other_reference_ranges", "meaning"]),
            Some(("DV_QUANTITY", Carried::Value))
        );
        assert_eq!(
            carried("DV_QUANTITY", &["other_reference_ranges", "range", "upper"]),
            Some(("DV_QUANTITY", Carried::Value))
        );
    }

    #[test]
    fn each_list_a_tail_steps_through_is_named_by_its_place_in_the_tail() {
        assert_eq!(list_steps("DV_TEXT", &["mappings"]), Some(vec![0]));
        assert_eq!(
            list_steps("DV_TEXT", &["mappings", "target", "code_string"]),
            Some(vec![0])
        );
        assert_eq!(
            list_steps("DV_QUANTITY", &["other_reference_ranges", "range", "lower"]),
            Some(vec![0])
        );
        assert_eq!(
            list_steps("DV_QUANTITY", &["normal_range", "upper"]),
            Some(Vec::new())
        );
        assert_eq!(list_steps("EVALUATION", &["other_participations"]), None);
    }
}
