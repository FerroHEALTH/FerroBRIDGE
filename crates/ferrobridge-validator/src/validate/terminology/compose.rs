// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The membership of one code in a value set, read from its
//! `compose` by the composition rules of valueset.html §4.9.5 and, where
//! `compose` cannot answer, from an `expansion` the value set carries whole.

use std::sync::Arc;
use std::sync::PoisonError;

use fhir_types::definition::ComposeRead;
use fhir_types::definition::ContainsRead;
use fhir_types::definition::ExpansionRead;
use fhir_types::definition::FilterRead;
use fhir_types::definition::IncludeRead;
use fhir_types::definition::ValueSetRead;

use crate::release::Release;
use crate::report::Reason;
use crate::set::PackageId;
use crate::set::ResolveError;
use crate::validate::ValidateError;
use crate::validate::Validator;
use crate::validate::terminology::Coded;
use crate::validate::terminology::Coding;
use crate::validate::terminology::Form;
use crate::validate::terminology::Membership;
use crate::validate::terminology::index::CodeIndex;
use crate::validate::terminology::index::Concept;

impl<R: Release> Validator<'_, R> {
    /// Answers whether `coded` holds a code of the value set `canonical`
    /// names, as a definition of the package `scope` references it.
    ///
    /// A `CodeableConcept` is in the set when one of its codings is
    /// (terminologies.html §4.1.5.1); one without codings is not.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError::Resolve`] when a resource the set carries
    /// cannot be read or decoded.
    pub(crate) fn holds(
        &self,
        canonical: &str,
        scope: PackageId,
        coded: &Coded<'_>,
    ) -> Result<Membership, ValidateError> {
        let mut answer = Membership::NotMember;
        for code in &coded.codes {
            // NOTE: no specification governs this: our own design; a Coding without a system
            // cannot be one of the system and code pairs a value set holds (valueset.html §4.9.8.2).
            if coded.form != Form::Code && code.system.is_none() {
                continue;
            }
            answer = answer.or(self.contains(canonical, scope, code, &mut Vec::new())?);
            if answer == Membership::Member {
                break;
            }
        }
        Ok(answer)
    }

    /// Returns the one code system every include of the value set
    /// `canonical` draws from, when it names exactly one and imports none.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError::Resolve`] when the value set cannot be read.
    pub(crate) fn single_system(
        &self,
        canonical: &str,
        scope: PackageId,
    ) -> Result<Option<String>, ValidateError> {
        let value_set = match self.set.value_set_in(canonical, Some(scope)) {
            Ok((value_set, _)) => value_set,
            Err(
                ResolveError::NotFound { .. }
                | ResolveError::Fragment { .. }
                | ResolveError::Ambiguous { .. },
            ) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let Some(compose) = value_set.compose() else {
            return Ok(None);
        };
        let mut systems = compose.includes.iter().map(|include| {
            include
                .value_sets
                .is_empty()
                .then(|| include.system.map(str::to_owned))
                .flatten()
        });
        let Some(Some(first)) = systems.next() else {
            return Ok(None);
        };
        Ok(systems
            .all(|other| other.as_deref() == Some(first.as_str()))
            .then_some(first))
    }

    /// Returns the code system `canonical` names, indexed, as a definition of
    /// the package `scope` references it, or why the set cannot give it: it
    /// does not carry it, or carries two different definitions under one
    /// version.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError::Resolve`] when the code system cannot be read.
    pub(crate) fn code_index(
        &self,
        canonical: &str,
        scope: PackageId,
    ) -> Result<Result<Arc<CodeIndex>, Reason>, ValidateError> {
        let key = (canonical.to_owned(), Some(scope));
        if let Some(found) = self
            .code_systems
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(found.clone());
        }
        let index = match self.set.code_system_in(canonical, Some(scope)) {
            Ok((system, _)) => Ok(Arc::new(CodeIndex::new(&*system).map_err(|source| {
                ValidateError::CodeSystemValue {
                    canonical: canonical.to_owned(),
                    source,
                }
            })?)),
            Err(ResolveError::NotFound { .. } | ResolveError::Fragment { .. }) => {
                Err(Reason::CodeSystemNotFound(canonical.to_owned()))
            }
            Err(ResolveError::Ambiguous { .. }) => {
                Err(Reason::AmbiguousDefinition(canonical.to_owned()))
            }
            Err(error) => return Err(error.into()),
        };
        self.code_systems
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, index.clone());
        Ok(index)
    }

    /// Answers whether `code` is in the value set `canonical`, with `stack`
    /// the imports being followed.
    fn contains(
        &self,
        canonical: &str,
        scope: PackageId,
        code: &Coding<'_>,
        stack: &mut Vec<String>,
    ) -> Result<Membership, ValidateError> {
        let (value_set, origin) = match self.set.value_set_in(canonical, Some(scope)) {
            Ok(found) => found,
            Err(ResolveError::NotFound { .. } | ResolveError::Fragment { .. }) => {
                return Ok(Membership::Unknown(Reason::ValueSetNotFound(
                    canonical.to_owned(),
                )));
            }
            Err(ResolveError::Ambiguous { .. }) => {
                return Ok(Membership::Unknown(Reason::AmbiguousDefinition(
                    canonical.to_owned(),
                )));
            }
            Err(error) => return Err(error.into()),
        };
        if stack.iter().any(|held| held == canonical) {
            return Ok(Membership::Unknown(Reason::ImportCycle(
                canonical.to_owned(),
            )));
        }
        stack.push(canonical.to_owned());
        let composed = value_set
            .compose()
            .map(|compose| self.composed(&compose, origin, code, stack))
            .transpose();
        stack.pop();
        let composed = composed?;
        if let Some(answer @ (Membership::Member | Membership::NotMember)) = composed {
            return Ok(answer);
        }
        if let Some(answer) = value_set
            .expansion()
            .and_then(|expansion| expanded(&expansion, code))
        {
            return Ok(answer);
        }
        Ok(composed.unwrap_or(Membership::Unknown(Reason::ValueSetNotEnumerated)))
    }

    /// Answers whether `code` is in the union of the includes of `compose`
    /// and in none of its excludes (valueset.html §4.9.5).
    fn composed(
        &self,
        compose: &ComposeRead<'_>,
        origin: PackageId,
        code: &Coding<'_>,
        stack: &mut Vec<String>,
    ) -> Result<Membership, ValidateError> {
        let active_only = compose.inactive == Some(false);
        let mut included = if compose.includes.is_empty() {
            Membership::Unknown(Reason::ValueSetNotEnumerated)
        } else {
            Membership::NotMember
        };
        for include in &compose.includes {
            included = included.or(self.included(include, origin, code, active_only, stack)?);
            if included == Membership::Member {
                break;
            }
        }
        if included == Membership::NotMember {
            return Ok(included);
        }
        let mut excluded = Membership::NotMember;
        for exclude in &compose.excludes {
            excluded = excluded.or(self.included(exclude, origin, code, false, stack)?);
        }
        Ok(included.and(excluded.not()))
    }

    /// Answers whether `code` meets every criterion of one include: its
    /// system selection and each value set it imports.
    fn included(
        &self,
        include: &IncludeRead<'_>,
        origin: PackageId,
        code: &Coding<'_>,
        active_only: bool,
        stack: &mut Vec<String>,
    ) -> Result<Membership, ValidateError> {
        let mut answer = include
            .system
            .map(|system| self.selected(include, system, origin, code, active_only))
            .transpose()?;
        // NOTE: valueset.html §4.9.5 and §4.9.8: a code must be in all the imported value sets;
        // the ValueSet.compose.include.valueSet definition's "union" contradicts it and loses.
        for &imported in &include.value_sets {
            if answer == Some(Membership::NotMember) {
                break;
            }
            let found = self.contains(imported, origin, code, stack)?;
            answer = Some(match answer {
                Some(held) => held.and(found),
                None => found,
            });
        }
        Ok(answer.unwrap_or(Membership::Unknown(Reason::ValueSetNotEnumerated)))
    }

    /// Answers whether `code` is selected from `system` by `include`: every
    /// code of the system, the concepts it lists, or the codes its filters
    /// select.
    fn selected(
        &self,
        include: &IncludeRead<'_>,
        system: &str,
        origin: PackageId,
        code: &Coding<'_>,
        active_only: bool,
    ) -> Result<Membership, ValidateError> {
        if code.system.is_some_and(|held| held != system) {
            return Ok(Membership::NotMember);
        }
        let Some(text) = code.code else {
            return Ok(Membership::NotMember);
        };
        let pinned = include.version;
        let every_version = pinned == Some("*");
        let pinned = pinned.filter(|version| *version != "*");
        // NOTE: valueset.html §4.9.8.2: a concept is identified by system, version and code, so
        // a code of another version than the include pins is another concept.
        if let (Some(pin), Some(held)) = (pinned, code.version)
            && pin != held
        {
            return Ok(Membership::NotMember);
        }
        let listed = !include.concepts.is_empty();
        if listed && !include.concepts.iter().any(|concept| *concept == text) {
            return Ok(Membership::NotMember);
        }
        if listed && !active_only {
            return Ok(Membership::Member);
        }
        let label = match pinned.or(code.version) {
            Some(version) => format!("{system}|{version}"),
            None => system.to_owned(),
        };
        let index = match self.code_index(&label, origin)? {
            Ok(index) => index,
            Err(reason) => return Ok(Membership::Unknown(reason)),
        };
        let Some(concept) = index.concept(text) else {
            return Ok(if index.complete() && !every_version {
                Membership::NotMember
            } else {
                Membership::Unknown(Reason::CodeSystemIncomplete(label))
            });
        };
        if active_only {
            match inactive(concept) {
                Some(true) => return Ok(Membership::NotMember),
                Some(false) => {}
                None => return Ok(Membership::Unknown(Reason::ConceptStatus(label))),
            }
        }
        let mut answer = Membership::Member;
        for filter in &include.filters {
            answer = answer.and(filtered(&index, concept, filter, &label));
            if answer == Membership::NotMember {
                break;
            }
        }
        Ok(answer)
    }
}

/// Answers whether `concept` is inactive: by the `inactive` property, or by
/// a `status` of `retired` or `inactive`; `None` when the status says
/// neither.
fn inactive(concept: &Concept) -> Option<bool> {
    if let Some(values) = concept.properties.get("inactive") {
        return match values.first().map(String::as_str) {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        };
    }
    // NOTE: ValueSet.compose.inactive comment: a code system that does not define inactive codes
    // has every code active; a `deprecated` status is left undecided, as that comment says.
    match concept
        .properties
        .get("status")
        .and_then(|values| values.first())
    {
        None => Some(false),
        Some(status) => match status.as_str() {
            "retired" | "inactive" => Some(true),
            "active" | "experimental" => Some(false),
            _ => None,
        },
    }
}

/// Answers whether `concept` meets one filter (valueset.html §4.9.5, the
/// `FilterOperator` codes, codesystem.html §4.8.13 and §4.8.14).
fn filtered(
    index: &CodeIndex,
    concept: &Concept,
    filter: &FilterRead<'_>,
    label: &str,
) -> Membership {
    let property = filter.property.unwrap_or_default();
    let op = filter.op.unwrap_or_default();
    let value = filter.value.unwrap_or_default();
    let not_evaluated = || {
        Membership::Unknown(Reason::FilterNotEvaluated(format!(
            "{property} {op} {value}"
        )))
    };
    if !index.complete() {
        return Membership::Unknown(Reason::CodeSystemIncomplete(label.to_owned()));
    }
    let answer = |held: bool| {
        if held {
            Membership::Member
        } else {
            Membership::NotMember
        }
    };
    match op {
        "is-a" | "descendent-of" | "is-not-a" | "generalizes" => {
            // NOTE: codesystem.html §4.8.13: subsumption follows the concept hierarchy and only
            // where it means is-a; R4 core value sets name it with the property `concept`.
            if property != "concept" {
                return not_evaluated();
            }
            if !index.is_a {
                return Membership::Unknown(Reason::HierarchyNotDeclared(label.to_owned()));
            }
            let Some(target) = index.concept(value) else {
                return answer(op == "is-not-a");
            };
            let target = target.code.as_str();
            let code = concept.code.as_str();
            let below = index.descends(code, target);
            answer(match op {
                "is-a" => code == target || below,
                "descendent-of" => below,
                "is-not-a" => code != target && !below,
                _ => code == target || index.descends(target, code),
            })
        }
        "=" | "in" | "not-in" | "exists" if index.declared.contains(property) => {
            let held = concept
                .properties
                .get(property)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let listed: Vec<&str> = value.split(',').map(str::trim).collect();
            match op {
                "=" => answer(held.iter().any(|found| found == value)),
                "in" => answer(held.iter().any(|found| listed.contains(&found.as_str()))),
                "not-in" => answer(!held.iter().any(|found| listed.contains(&found.as_str()))),
                _ => match value {
                    "true" => answer(!held.is_empty()),
                    "false" => answer(held.is_empty()),
                    _ => not_evaluated(),
                },
            }
        }
        _ => not_evaluated(),
    }
}

/// Answers whether `code` is in a complete, unpaged expansion; `None` when
/// the expansion is partial.
fn expanded<C: ContainsRead>(
    expansion: &ExpansionRead<'_, C>,
    code: &Coding<'_>,
) -> Option<Membership> {
    let total = expansion.total;
    let paged = expansion.offset.is_some_and(|offset| offset != 0);
    let mut found = false;
    let entries = walk_expansion(expansion.contains, code, &mut found);
    // NOTE: ValueSet.expansion.offset and .total definitions: a paged expansion is a partial
    // view, so it counts only unpaged and holding every concept `total` counts.
    let complete =
        total.is_none_or(|total| usize::try_from(total).is_ok_and(|total| total == entries));
    if paged || !complete || entries == 0 {
        return None;
    }
    Some(if found {
        Membership::Member
    } else {
        Membership::NotMember
    })
}

/// Counts the entries of an expansion's `contains` and their nested entries,
/// setting `found` when a selectable one is `code`.
fn walk_expansion<C: ContainsRead>(contains: &[C], code: &Coding<'_>, found: &mut bool) -> usize {
    let mut entries = 0_usize;
    for entry in contains {
        entries = entries.saturating_add(1);
        let selectable = !entry.is_abstract().unwrap_or(false);
        let same_code = entry.code() == code.code;
        let same_system = code
            .system
            .is_none_or(|system| entry.system() == Some(system));
        let entry_version = entry.version();
        let same_version = match (entry_version, code.version) {
            (Some(held), Some(asked)) => held == asked,
            _ => true,
        };
        if selectable && code.code.is_some() && same_code && same_system && same_version {
            *found = true;
        }
        entries = entries.saturating_add(walk_expansion(entry.contains(), code, found));
    }
    entries
}
