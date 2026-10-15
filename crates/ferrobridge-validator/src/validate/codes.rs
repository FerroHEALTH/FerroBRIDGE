// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The codes of a value set that enumerates them, read from the package set
//! with no expansion service, for a `value` or `pattern` discriminator over a
//! required binding (<https://hl7.org/fhir/R4/profiling.html#discriminator>).

use std::collections::BTreeSet;

use fhir_types::codec::Value;
use fhir_types::r4::code_system::CodeSystemConcept;
use fhir_types::r4::value_set::ValueSetComposeInclude;
use fhir_types::r4::value_set::ValueSetExpansionContains;

use crate::set::PackageId;
use crate::set::PackageSet;
use crate::set::ResolveError;
use crate::validate::ValidateError;

/// One code of a value set: its system, when the value set names one, and
/// the code.
pub(crate) type Code = (Option<String>, String);

/// The codes of a value set, when the package set enumerates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Enumeration {
    /// Every code the value set holds.
    Codes(BTreeSet<Code>),
    /// The value set needs a filter, a code system the set does not hold
    /// complete, or a value set the set does not carry.
    NotEnumerated,
}

impl Enumeration {
    /// Answers whether the coded value `value` holds a code of the set: a
    /// `code` (or other string) by its code, a `Coding` or `Quantity` by
    /// system and code, a `CodeableConcept` by any of its codings.
    pub(crate) fn holds(&self, value: &Value) -> bool {
        let Self::Codes(codes) = self else {
            return false;
        };
        match value {
            Value::String(code) => codes.iter().any(|(_, held)| held == code),
            Value::Object(object) => {
                if let Some(codings) = object.get("coding").and_then(Value::as_array) {
                    return codings.iter().any(|coding| self.holds(coding));
                }
                let system = object.get("system").and_then(Value::as_str);
                let code = object.get("code").and_then(Value::as_str);
                match (system, code) {
                    (Some(system), Some(code)) => codes.iter().any(|(held_system, held)| {
                        held == code && held_system.as_deref().is_none_or(|held| held == system)
                    }),
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

/// Enumerates the value set `canonical` names, as a definition of the
/// package `scope` references it.
///
/// `compose` is read when every include lists its concepts or names a code
/// system the set carries with `content` `complete`; otherwise an
/// `expansion` the value set carries whole, unpaged, is read. An include
/// that filters or imports a value set is not enumerated.
///
/// # Errors
///
/// Returns [`ValidateError::Resolve`] when a resource the set carries cannot
/// be read or decoded.
pub(crate) fn enumerate(
    set: &PackageSet,
    canonical: &str,
    scope: PackageId,
) -> Result<Enumeration, ValidateError> {
    let (value_set, origin) = match set.value_set_in(canonical, Some(scope)) {
        Ok(found) => found,
        Err(ResolveError::NotFound { .. } | ResolveError::Fragment { .. }) => {
            return Ok(Enumeration::NotEnumerated);
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(compose) = &value_set.compose {
        let mut codes = BTreeSet::new();
        let mut whole = !compose.include.is_empty();
        for include in &compose.include {
            match included(set, include, origin)? {
                Some(found) => codes.extend(found),
                None => whole = false,
            }
        }
        for exclude in &compose.exclude {
            match included(set, exclude, origin)? {
                Some(found) => codes.retain(|code| !found.contains(code)),
                None => whole = false,
            }
        }
        if whole {
            return Ok(Enumeration::Codes(codes));
        }
    }
    if let Some(expansion) = &value_set.expansion {
        let mut codes = BTreeSet::new();
        let entries = flatten_expansion(&expansion.contains, &mut codes);
        let total = expansion.total.as_ref().and_then(|total| total.value);
        let paged = expansion
            .offset
            .as_ref()
            .and_then(|offset| offset.value)
            .is_some_and(|offset| offset != 0);
        // NOTE: ValueSet.expansion.offset and .total definitions: a paged expansion is a partial
        // view, so it counts only unpaged and holding every concept `total` counts.
        let complete =
            total.is_none_or(|total| usize::try_from(total).is_ok_and(|total| total == entries));
        if !paged && complete && !codes.is_empty() {
            return Ok(Enumeration::Codes(codes));
        }
    }
    Ok(Enumeration::NotEnumerated)
}

/// The codes one include or exclude names, `None` when they cannot be
/// enumerated.
fn included(
    set: &PackageSet,
    include: &ValueSetComposeInclude,
    origin: PackageId,
) -> Result<Option<BTreeSet<Code>>, ValidateError> {
    if !include.filter.is_empty() || !include.value_set.is_empty() {
        return Ok(None);
    }
    let Some(system) = include
        .system
        .as_ref()
        .and_then(|system| system.value.clone())
    else {
        return Ok(None);
    };
    if include.concept.is_empty() {
        return whole_code_system(set, &system, origin);
    }
    Ok(Some(
        include
            .concept
            .iter()
            .filter_map(|concept| concept.code.value.clone())
            .map(|code| (Some(system.clone()), code))
            .collect(),
    ))
}

/// Every code of the code system `system`, when the set carries it with
/// `content` `complete`.
fn whole_code_system(
    set: &PackageSet,
    system: &str,
    origin: PackageId,
) -> Result<Option<BTreeSet<Code>>, ValidateError> {
    let code_system = match set.code_system_in(system, Some(origin)) {
        Ok((code_system, _)) => code_system,
        Err(ResolveError::NotFound { .. } | ResolveError::Fragment { .. }) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if code_system.content.value.as_deref() != Some("complete") {
        return Ok(None);
    }
    let mut codes = BTreeSet::new();
    flatten_concepts(system, &code_system.concept, &mut codes);
    Ok(Some(codes))
}

/// Adds every code of `concepts` and their nested concepts.
fn flatten_concepts(system: &str, concepts: &[CodeSystemConcept], codes: &mut BTreeSet<Code>) {
    for concept in concepts {
        if let Some(code) = &concept.code.value {
            codes.insert((Some(system.to_owned()), code.clone()));
        }
        flatten_concepts(system, &concept.concept, codes);
    }
}

/// Adds every code of an expansion's `contains` and their nested entries,
/// returning how many entries it read.
fn flatten_expansion(contains: &[ValueSetExpansionContains], codes: &mut BTreeSet<Code>) -> usize {
    let mut entries = 0_usize;
    for entry in contains {
        entries = entries.saturating_add(1);
        let abstract_entry = entry
            .r#abstract
            .as_ref()
            .and_then(|flag| flag.value)
            .unwrap_or(false);
        if let (Some(code), false) = (
            entry.code.as_ref().and_then(|code| code.value.clone()),
            abstract_entry,
        ) {
            let system = entry
                .system
                .as_ref()
                .and_then(|system| system.value.clone());
            codes.insert((system, code));
        }
        entries = entries.saturating_add(flatten_expansion(&entry.contains, codes));
    }
    entries
}
