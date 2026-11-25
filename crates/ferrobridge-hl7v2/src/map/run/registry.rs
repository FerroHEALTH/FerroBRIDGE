// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The guide's `IF CX.4 IN` identifier registry condition: an `HD` that
//! carries a universal id is written by the `HD` to `uri` map, and any other
//! assigning authority is searched as an identifier `NamingSystem` on the
//! terminology server, whose `uri` unique id becomes the system.

use std::collections::BTreeMap;

use ferrobridge_term::client::Client;

use crate::map::maps::Row;
use crate::map::notation::Target;
use crate::map::run::datum::Datum;
use crate::map::run::{Base, Lookup, Pending, Run, Scope};
use crate::map::{MapError, Outcome, RowRef};

/// The universal id types the `HD` to `uri` map writes as a URN
/// (`datatype-hd-to-uri`, HD.2 under `HD-3 = "ISO"` and `"UUID"`).
const UNIVERSAL_TYPES: [&str; 2] = ["ISO", "UUID"];

impl<'a> Run<'a> {
    /// Runs a row gated by the identifier registry over the valued `HD`
    /// `datum`.
    ///
    /// An `HD` with no namespace id and a universal id of a type in
    /// [`UNIVERSAL_TYPES`] names its system itself, so the row applies as
    /// written. Any other `HD` names its assigning authority by the
    /// namespace id, else the universal id, and the write waits for the
    /// registry search ([`Run::register`]).
    ///
    /// # Errors
    ///
    /// Returns what [`Run::apply`] returns for a universal id.
    pub(super) fn registry(
        &mut self,
        scope: &Scope<'a>,
        row: &Row,
        reference: &RowRef,
        datum: Datum<'a>,
        base: &Base,
    ) -> Result<(), MapError> {
        let text = |position: usize| datum.at(&[position]).text().filter(|text| !text.is_empty());
        let universal = text(1).is_none()
            && text(2).is_some()
            && text(3).is_some_and(|kind| UNIVERSAL_TYPES.contains(&kind));
        if universal {
            return self.apply(scope, row, reference, datum, 0, base);
        }
        let Some(authority) = text(1).or_else(|| text(2)).map(String::from) else {
            return Ok(());
        };
        let (instance, steps) = match &row.target {
            Ok(Target::Path { instance, steps }) => (instance.as_ref(), steps.as_slice()),
            Ok(Target::Value { instance }) => (instance.as_ref(), &[][..]),
            Err(error) => {
                self.outcomes.push(Outcome::UnsupportedTarget {
                    at: scope.at.clone(),
                    row: reference.clone(),
                    error: error.clone(),
                });
                return Ok(());
            }
        };
        if steps.iter().any(|step| step.reference.is_some()) {
            self.outcomes.push(Outcome::UnsupportedShape {
                at: scope.at.clone(),
                row: reference.clone(),
            });
            return Ok(());
        }
        let Some(base) = self.instance(base, instance, &scope.at, reference) else {
            return Ok(());
        };
        let Some(slots) = self.slots(&base, steps, 0, &scope.at, reference) else {
            return Ok(());
        };
        let leaf = Base {
            resource: base.resource,
            slots,
        };
        let lookup = self.lookups.len();
        self.lookups.push(Lookup {
            authority,
            at: scope.at.clone(),
            row: reference.clone(),
            answer: None,
        });
        self.write(
            &leaf,
            Pending::Registry { lookup },
            &scope.at,
            reference,
            false,
        );
        Ok(())
    }

    /// Searches the terminology server's identifier registry for every
    /// assigning authority the walk recorded, each once, and returns how
    /// many searches it sent.
    ///
    /// One `NamingSystem` answers the authority with its `uri`; none is
    /// [`Outcome::Unregistered`] and more than one
    /// [`Outcome::RegistryAmbiguous`], and either leaves the system
    /// unwritten. With no terminology server each is
    /// [`Outcome::NoTerminology`].
    ///
    /// # Errors
    ///
    /// Returns [`MapError::Registry`] when the server refuses a search or
    /// cannot be reached.
    pub(in crate::map) async fn register(
        &mut self,
        terminology: Option<&Client>,
    ) -> Result<usize, MapError> {
        let Some(terminology) = terminology else {
            let unanswered: Vec<Outcome> = self
                .lookups
                .iter()
                .map(|lookup| Outcome::NoTerminology {
                    at: lookup.at.clone(),
                    row: lookup.row.clone(),
                })
                .collect();
            self.outcomes.extend(unanswered);
            return Ok(0);
        };
        let mut answered: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for lookup in &self.lookups {
            if answered.contains_key(&lookup.authority) {
                continue;
            }
            let systems = terminology
                .identifier_systems(&lookup.authority)
                .await
                .map_err(|source| MapError::Registry {
                    authority: lookup.authority.clone(),
                    source: Box::new(source),
                })?;
            answered.insert(lookup.authority.clone(), systems);
        }
        let mut outcomes = Vec::new();
        for lookup in &mut self.lookups {
            let systems = answered
                .get(&lookup.authority)
                .map(Vec::as_slice)
                .unwrap_or_default();
            match systems {
                [system] => lookup.answer = Some(system.clone()),
                [] => outcomes.push(Outcome::Unregistered {
                    at: lookup.at.clone(),
                    row: lookup.row.clone(),
                    authority: lookup.authority.clone(),
                }),
                found => outcomes.push(Outcome::RegistryAmbiguous {
                    at: lookup.at.clone(),
                    row: lookup.row.clone(),
                    authority: lookup.authority.clone(),
                    systems: found.to_vec(),
                }),
            }
        }
        self.outcomes.extend(outcomes);
        Ok(answered.len())
    }
}
