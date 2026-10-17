// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Slicing (<https://hl7.org/fhir/R4/profiling.html#slicing>, §5.1.0.10 to
//! §5.1.0.14): each item of a sliced element matched to its slice by the
//! discriminators, the rules `closed`, `open` and `openAtEnd`, the order,
//! the `@default` slice, each slice's cardinality, and re-slicing.
//!
//! A matched item is then held to its slice's element and walked against
//! the slice's sub-tree of the snapshot ([`Walk::apply`]).

mod definition;
mod discriminate;
mod instance;
mod path;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::TypeSchema;

use crate::issue::Issue;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::Reason;
use crate::validate::ValidateError;
use crate::validate::slicing::instance::Node;
use crate::validate::slicing::path::Step;
use crate::validate::structure::Structure;
use crate::validate::walk::Cursor;
use crate::validate::walk::Form;
use crate::validate::walk::Held;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;

/// The member being walked: its type and field in the element table, the
/// cursor of its parent, its element id and definition, and the parent's
/// location.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Site<'a> {
    /// The parent's type.
    pub(crate) schema: &'static TypeSchema,
    /// The member's field.
    pub(crate) field: &'static FieldSchema,
    /// The cursor of the parent.
    pub(crate) cursor: &'a Cursor,
    /// The member's element id, the slicing entry.
    pub(crate) id: &'a str,
    /// The member's element definition.
    pub(crate) element: Option<&'a ElementDefinition>,
    /// The parent's location.
    pub(crate) location: &'a str,
}

/// One item of a sliced element: an occurrence and its form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Item<'a, 'i> {
    /// The form the occurrence takes.
    pub(crate) form: &'a Form,
    /// The occurrence.
    pub(crate) occurrence: &'a Occurrence<'i>,
}

/// One discriminator of a slicing entry, its path parsed once.
#[derive(Debug)]
pub(crate) struct Discriminator {
    /// The type: `value`, `exists`, `pattern`, `type` or `profile`.
    pub(crate) kind: String,
    /// The path as written.
    pub(crate) path: String,
    /// The steps, `None` for a path outside the simple subset.
    pub(crate) steps: Option<Vec<Step>>,
}

impl Discriminator {
    /// The constraint an unevaluated discriminator is listed as.
    pub(crate) fn constraint(&self) -> Constraint {
        Constraint::Discriminator {
            kind: self.kind.clone(),
            path: self.path.clone(),
        }
    }
}

/// Why a discriminator could not be evaluated for one slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unknown {
    /// The constraint listed as unevaluated.
    pub(crate) constraint: Constraint,
    /// Why.
    pub(crate) reason: Reason,
}

/// Whether an item belongs to a slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Every discriminator matches.
    Match,
    /// A discriminator does not match.
    NoMatch,
    /// The item fails the validation that decides the slice, with the errors
    /// it raised.
    Rejected(Vec<Issue>),
    /// No discriminator fails, and these could not be evaluated.
    Unknown(Vec<Unknown>),
}

/// Where the slicing places one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// In the slice at this index.
    Slice(usize),
    /// In no slice.
    Unmatched,
    /// Not decided: a discriminator could not be evaluated.
    Undecided,
}

/// One level of slicing: the entry, its slices in snapshot order, its
/// discriminators, the index of its `@default` slice and of the slice an
/// item in no other slice falls to.
#[derive(Debug)]
struct Plan<'p> {
    structure: &'p Arc<Structure>,
    entry_id: &'p str,
    slices: &'p [(&'p str, &'p ElementDefinition)],
    discriminators: &'p [Discriminator],
    default: Option<usize>,
    fallback: Option<usize>,
    rules: &'p str,
    ordered: bool,
}

/// The nodes one discriminator path selects from one item, or why it
/// could not be followed.
type Selected<'i> = Result<Vec<Node<'i>>, Reason>;

/// The items a validation kept out of each slice: by the slice's index,
/// each item's index with the errors it raised.
type Rejections = BTreeMap<usize, Vec<(usize, Vec<Issue>)>>;

impl<'i> Walk<'_, '_, 'i> {
    /// Evaluates the slicing `entry` declares over `items`, and returns for
    /// each item the ids of the slices it belongs to, outermost first.
    pub(crate) fn slicing(
        &mut self,
        site: &Site<'_>,
        entry: &ElementDefinition,
        items: &[Item<'_, 'i>],
    ) -> Result<Vec<Vec<String>>, ValidateError> {
        let mut chains = vec![Vec::new(); items.len()];
        let all: Vec<usize> = (0..items.len()).collect();
        self.level(site, site.id, entry, items, &all, &mut chains)?;
        Ok(chains)
    }

    /// Evaluates one level of slicing over the items at `members`.
    fn level(
        &mut self,
        site: &Site<'_>,
        entry_id: &str,
        entry: &ElementDefinition,
        items: &[Item<'_, 'i>],
        members: &[usize],
        chains: &mut [Vec<String>],
    ) -> Result<(), ValidateError> {
        let Some(slicing) = &entry.slicing else {
            return Ok(());
        };
        let structure = Arc::clone(&site.cursor.structure);
        let slices = structure.slices(entry_id);
        let rules = slicing.rules.value.as_deref().unwrap_or("open");
        let ordered = slicing
            .ordered
            .as_ref()
            .and_then(|flag| flag.value)
            .unwrap_or(false);
        let discriminators: Vec<Discriminator> = slicing
            .discriminator
            .iter()
            .map(|discriminator| {
                let path = discriminator.path.value.clone().unwrap_or_default();
                Discriminator {
                    kind: discriminator.r#type.value.clone().unwrap_or_default(),
                    steps: path::parse(&path),
                    path,
                }
            })
            .collect();
        let default = slices
            .iter()
            .position(|(id, _)| id.ends_with(":@default") || id.ends_with("/@default"));
        let fallback = match default {
            // NOTE: profiling.html §5.1.0.13: "Default slices are only allowed when the slicing
            // rule = closed", so under other rules the profile's @default slice is not used.
            Some(_) if rules != "closed" => {
                self.out().unevaluated(
                    structure.url(),
                    entry_id,
                    Constraint::Slicing,
                    site.location,
                    Reason::DefaultSliceNotClosed,
                );
                None
            }
            found => found,
        };
        let plan = Plan {
            structure: &structure,
            entry_id,
            slices: &slices,
            discriminators: &discriminators,
            default,
            fallback,
            rules,
            ordered,
        };
        let mut placements = Vec::with_capacity(members.len());
        let mut undecided: BTreeSet<usize> = BTreeSet::new();
        let mut rejections = Rejections::new();
        for &member in members {
            let Some(item) = items.get(member) else {
                continue;
            };
            let (placement, unknown) = self.place(site, &plan, *item, (member, &mut rejections))?;
            undecided.extend(unknown);
            placements.push((member, placement));
        }
        self.rules(&plan, items, &placements);
        for (index, (slice_id, slice)) in slices.iter().enumerate() {
            let inside: Vec<usize> = placements
                .iter()
                .filter(|(_, placement)| *placement == Placement::Slice(index))
                .map(|(member, _)| *member)
                .collect();
            if !undecided.contains(&index)
                && self.slice_cardinality(site, slice_id, slice, inside.len())
            {
                self.rejection(slice_id, items, rejections.get(&index));
            }
            for &member in &inside {
                if let Some(chain) = chains.get_mut(member) {
                    chain.push((*slice_id).to_owned());
                }
            }
            if slice.slicing.is_some() {
                self.level(site, slice_id, slice, items, &inside, chains)?;
            }
        }
        Ok(())
    }

    /// Places one item: in the one slice it matches, in the `@default` slice
    /// or none when it matches no slice, or undecided when a discriminator
    /// could not be evaluated, with the slices that could not be decided.
    fn place(
        &mut self,
        site: &Site<'_>,
        plan: &Plan<'_>,
        item: Item<'_, 'i>,
        (member, rejections): (usize, &mut Rejections),
    ) -> Result<(Placement, Vec<usize>), ValidateError> {
        let location = &item.occurrence.location;
        let node = Node::item(site.field, item.form, item.occurrence);
        let selected: Vec<Option<Selected<'i>>> = plan
            .discriminators
            .iter()
            .map(|discriminator| {
                let steps = discriminator.steps.as_ref()?;
                let node = node?;
                Some(self.follow(node, steps, &discriminator.path, location))
            })
            .collect();
        let mut matched = Vec::new();
        let mut unknown = Vec::new();
        for (index, (slice_id, _)) in plan.slices.iter().enumerate() {
            if Some(index) == plan.default {
                continue;
            }
            let verdict = if plan.discriminators.is_empty() {
                self.speculate(site, slice_id, item)?
            } else {
                let found = (plan.structure, *slice_id);
                self.verdict(found, plan.discriminators, &selected, location)?
            };
            match verdict {
                Verdict::Match => matched.push(index),
                Verdict::NoMatch => {}
                Verdict::Rejected(errors) => {
                    rejections.entry(index).or_default().push((member, errors));
                }
                Verdict::Unknown(reasons) => {
                    for reason in reasons {
                        self.out().unevaluated(
                            plan.structure.url(),
                            plan.entry_id,
                            reason.constraint,
                            location,
                            reason.reason,
                        );
                    }
                    unknown.push(index);
                }
            }
        }
        let placement = match matched.as_slice() {
            [first, second, ..] => {
                let name = |index: &usize| plan.slices.get(*index).map_or("", |(id, _)| *id);
                // NOTE: profiling.html §5.1.0.11: a profile "SHALL ensure that the possible values
                // for each slice are different and non-overlapping"; our own design reports it.
                self.out().issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::OverlappingSlices,
                    location,
                    format!(
                        "the item matches the slices {} and {}, which overlap",
                        name(first),
                        name(second)
                    ),
                );
                Placement::Slice(*first)
            }
            [only] => Placement::Slice(*only),
            [] if !unknown.is_empty() => return Ok((Placement::Undecided, unknown)),
            [] => plan.fallback.map_or(Placement::Unmatched, Placement::Slice),
        };
        Ok((placement, Vec::new()))
    }

    /// Reports the items the rules `closed` and `openAtEnd` and an ordered
    /// slicing refuse.
    fn rules(
        &mut self,
        plan: &Plan<'_>,
        items: &[Item<'_, 'i>],
        placements: &[(usize, Placement)],
    ) {
        let entry_id = plan.entry_id;
        let name = |index: usize| plan.slices.get(index).map_or("", |(id, _)| *id);
        let mut seen_unmatched = false;
        let mut highest: Option<usize> = None;
        for &(member, placement) in placements {
            let Some(item) = items.get(member) else {
                continue;
            };
            let location = &item.occurrence.location;
            match placement {
                Placement::Unmatched => {
                    if plan.rules == "closed" {
                        self.out().issue(
                            Severity::Error,
                            IssueType::Structure,
                            MessageId::UnmatchedSliceItem,
                            location,
                            format!(
                                "the item matches no slice of {entry_id}, whose slicing is closed"
                            ),
                        );
                    }
                    seen_unmatched = true;
                }
                Placement::Slice(index) => {
                    if plan.rules == "openAtEnd" && seen_unmatched {
                        self.out().issue(
                            Severity::Error,
                            IssueType::Structure,
                            MessageId::OpenAtEnd,
                            location,
                            format!("the item of the slice {} follows an item in no slice, and {} is open only at the end", name(index), entry_id),
                        );
                    }
                    // NOTE: profiling.html §5.1.0.10 is silent on items in no slice between ordered
                    // slices; no specification governs this: our own design, `open` lets them be anywhere.
                    if plan.ordered
                        && let Some(previous) = highest
                        && index < previous
                    {
                        self.out().issue(
                            Severity::Error,
                            IssueType::Structure,
                            MessageId::SliceOrder,
                            location,
                            format!("the item of the slice {} follows an item of the slice {}, and the slicing is ordered", name(index), name(previous)),
                        );
                    }
                    highest = Some(highest.map_or(index, |previous| previous.max(index)));
                }
                Placement::Undecided => {}
            }
        }
    }

    /// Names, as information, the errors that kept out of the slice
    /// `slice_id` the item of `rejected` that raised the fewest, the first
    /// of them on a tie.
    fn rejection(
        &mut self,
        slice_id: &str,
        items: &[Item<'_, 'i>],
        rejected: Option<&Vec<(usize, Vec<Issue>)>>,
    ) {
        let Some((member, errors)) = rejected
            .into_iter()
            .flatten()
            .min_by_key(|(_, errors)| errors.len())
        else {
            return;
        };
        let at = items
            .get(*member)
            .map_or("", |item| item.occurrence.location.as_str());
        for error in errors {
            self.out().issue(
                Severity::Information,
                IssueType::Informational,
                MessageId::SliceRejection,
                &error.expression,
                format!("the item at {at} is not in {slice_id}: {}", error.message),
            );
        }
    }

    /// Checks how many items one slice holds against its own cardinality
    /// (profiling.html §5.1.0.12), answering whether it holds fewer than its
    /// minimum.
    fn slice_cardinality(
        &mut self,
        site: &Site<'_>,
        slice_id: &str,
        slice: &ElementDefinition,
        count: usize,
    ) -> bool {
        let min = slice.min.as_ref().and_then(|min| min.value).unwrap_or(0);
        let max = match slice.max.as_ref().and_then(|max| max.value.as_deref()) {
            Some("*") | None => None,
            Some(max) => max.parse::<u32>().ok(),
        };
        let short = usize::try_from(min).is_ok_and(|min| count < min);
        if short {
            self.out().issue(
                Severity::Error,
                IssueType::Required,
                MessageId::MinCardinality,
                site.location,
                format!("{slice_id} occurs {count} times, below its minimum of {min}"),
            );
        }
        if let Some(max) = max
            && usize::try_from(max).is_ok_and(|max| count > max)
        {
            self.out().issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::MaxCardinality,
                &format!("{}.{}", site.location, site.field.name),
                format!("{slice_id} occurs {count} times, above its maximum of {max}"),
            );
        }
        short
    }

    /// Answers whether `item` validates against the slice `slice_id`
    /// without an error, in a walk of its own: how an item is matched when
    /// the slicing names no discriminator.
    fn speculate(
        &mut self,
        site: &Site<'_>,
        slice_id: &str,
        item: Item<'_, 'i>,
    ) -> Result<Verdict, ValidateError> {
        let mut fork = self.fork();
        let slices = [slice_id.to_owned()];
        let held = Held {
            id: site.id,
            element: site.element,
            slices: &slices,
        };
        fork.apply(
            site.schema,
            site.field,
            item.form,
            item.occurrence,
            site.cursor,
            held,
        )?;
        let errors = fork.out().errors();
        Ok(if errors.is_empty() {
            Verdict::Match
        } else {
            Verdict::Rejected(errors)
        })
    }

    /// Combines the discriminators of one slice: a match when every one
    /// matches ("composite values ... are unique", profiling.html §5.1.0.11).
    fn verdict(
        &mut self,
        (structure, slice_id): (&Arc<Structure>, &str),
        discriminators: &[Discriminator],
        selected: &[Option<Selected<'i>>],
        location: &str,
    ) -> Result<Verdict, ValidateError> {
        let mut unknown = Vec::new();
        for (discriminator, nodes) in discriminators.iter().zip(selected) {
            let nodes = match nodes {
                None if discriminator.steps.is_none() => {
                    unknown.push(Unknown {
                        constraint: discriminator.constraint(),
                        reason: Reason::DiscriminatorPath,
                    });
                    continue;
                }
                None => {
                    unknown.push(Unknown {
                        constraint: discriminator.constraint(),
                        reason: Reason::NoDiscriminatorValue,
                    });
                    continue;
                }
                Some(Err(reason)) => {
                    unknown.push(Unknown {
                        constraint: discriminator.constraint(),
                        reason: reason.clone(),
                    });
                    continue;
                }
                Some(Ok(nodes)) => nodes,
            };
            match self.discriminate(structure, slice_id, discriminator, nodes, location)? {
                Verdict::Match => {}
                found @ (Verdict::NoMatch | Verdict::Rejected(_)) => return Ok(found),
                Verdict::Unknown(found) => unknown.extend(found),
            }
        }
        Ok(if unknown.is_empty() {
            Verdict::Match
        } else {
            Verdict::Unknown(unknown)
        })
    }
}
