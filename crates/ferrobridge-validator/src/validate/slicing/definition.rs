// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A discriminator path followed through a slice's definition: the element
//! definition that constrains the element the path selects, in the slice's
//! own sub-tree, or in the profile or core definition of a type it hops into
//! where the snapshot stops.

use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::definition::ElementDefinitionRead;

use crate::canonical::Canonical;
use crate::release::Release;
use crate::set::ResolveError;
use crate::validate::Element;
use crate::validate::ValidateError;
use crate::validate::Validator;
use crate::validate::matching::Stem;
use crate::validate::matching::value_of;
use crate::validate::slicing::path::Step;
use crate::validate::structure::Structure;

/// Where a discriminator path ends in a slice's definition.
#[derive(Debug)]
pub(crate) enum Located<R: Release> {
    /// An element definition, narrowed to one type by `ofType()`.
    Element {
        /// The snapshot holding it.
        structure: Arc<Structure<R>>,
        /// Its id.
        id: String,
        /// The type `ofType()` narrowed it to.
        only: Option<String>,
    },
    /// The target profiles of the reference a final `resolve()` resolves.
    Targets(Vec<String>),
    /// The path selects nothing the definitions constrain.
    Missing,
}

impl<R: Release> Located<R> {
    /// Returns the snapshot the path ends in, when it ends at an element.
    pub(crate) fn structure(&self) -> Option<&Structure<R>> {
        match self {
            Self::Element { structure, .. } => Some(structure),
            Self::Targets(_) | Self::Missing => None,
        }
    }

    /// Returns the element definition, when the path ends at one.
    pub(crate) fn element(&self) -> Option<&Element<R>> {
        match self {
            Self::Element { structure, id, .. } => structure.element(id),
            Self::Targets(_) | Self::Missing => None,
        }
    }

    /// Returns the types of the element the path ends at, narrowed by
    /// `ofType()`, with the profiles each names.
    pub(crate) fn types(&self) -> Vec<(String, Vec<String>)> {
        let only = match self {
            Self::Element { only, .. } => only.as_deref(),
            Self::Targets(_) | Self::Missing => None,
        };
        self.element()
            .into_iter()
            .flat_map(ElementDefinitionRead::types)
            .filter_map(|kind| {
                let code = kind.code?.to_owned();
                let profiles = kind
                    .profiles
                    .iter()
                    .map(|profile| (*profile).to_owned())
                    .collect();
                only.is_none_or(|only| only == code)
                    .then_some((code, profiles))
            })
            .collect()
    }
}

/// Follows `steps` from the slice `slice` of `structure`.
///
/// # Errors
///
/// Returns [`ValidateError`] when a definition the path hops into cannot be
/// read; one the set does not carry ends the path as [`Located::Missing`].
pub(crate) fn locate<R: Release>(
    validator: &Validator<'_, R>,
    structure: &Arc<Structure<R>>,
    slice: &str,
    steps: &[Step],
) -> Result<Located<R>, ValidateError> {
    let mut structure = Arc::clone(structure);
    let mut id = slice.to_owned();
    let mut only: Option<String> = None;
    for (index, step) in steps.iter().enumerate() {
        let last = index.saturating_add(1) == steps.len();
        let here = Located::Element {
            structure: Arc::clone(&structure),
            id: id.clone(),
            only: only.clone(),
        };
        if let Step::Resolve = step {
            let targets: Vec<String> = here
                .element()
                .into_iter()
                .flat_map(ElementDefinitionRead::types)
                .filter(|kind| only.as_deref().is_none_or(|only| kind.code == Some(only)))
                .flat_map(|kind| kind.target_profiles)
                .map(str::to_owned)
                .collect();
            if last {
                return Ok(Located::Targets(targets));
            }
            let [target] = targets.as_slice() else {
                return Ok(Located::Missing);
            };
            let Some(next) = resolved(validator, target, &structure)? else {
                return Ok(Located::Missing);
            };
            next.type_name().clone_into(&mut id);
            structure = next;
            only = None;
            continue;
        }
        let mut found = within(&structure, &id, step)?;
        if found.is_none()
            && let Some((next, root)) = hop(validator, &here, &structure)?
        {
            found = within(&next, &root, step)?;
            structure = next;
        }
        if found.is_none()
            && let Step::Extension(url) = step
            && let Some(definition) = resolved(validator, url, &structure)?
        {
            found = Some(("Extension".to_owned(), None));
            structure = definition;
        }
        let Some((next_id, next_only)) = found else {
            return Ok(Located::Missing);
        };
        id = next_id;
        only = next_only;
    }
    Ok(Located::Element {
        structure,
        id,
        only,
    })
}

/// Takes `step` inside `structure` from the element `id`: the id it leads
/// to and the type it narrows to, `None` when the snapshot defines nothing
/// there.
fn within<R: Release>(
    structure: &Structure<R>,
    id: &str,
    step: &Step,
) -> Result<Option<(String, Option<String>)>, ValidateError> {
    Ok(match step {
        Step::Member(name) => [format!("{id}.{name}"), format!("{id}.{name}[x]")]
            .into_iter()
            .find(|candidate| structure.element(candidate).is_some())
            .map(|found| (found, None)),
        Step::OfType(code) => {
            if structure.element(id).is_none() {
                return Ok(None);
            }
            // NOTE: elementdefinition.html §2.30.0.3: the id of a type slice is the choice
            // id with the slice named for the type, `Patient.deceased[x]:deceasedBoolean`.
            let choice = id.rsplit('.').next().unwrap_or(id).trim_end_matches("[x]");
            let suffix = structure
                .element(id)
                .and_then(|element| element.path())
                .and_then(|path| R::schemas().element(path))
                .and_then(|(_, field)| field.choices().find(|form| form.code == code.as_str()))
                .map(|form| form.suffix);
            let slice = suffix.map(|suffix| format!("{id}:{choice}{suffix}"));
            let found = match slice {
                Some(slice) if structure.element(&slice).is_some() => slice,
                _ => id.to_owned(),
            };
            Some((found, Some(code.clone())))
        }
        Step::Extension(url) => {
            let mut found = None;
            for (slice, element) in structure.slices(&format!("{id}.extension")) {
                if names_extension(structure, slice, element, url)? {
                    found = Some((slice.to_owned(), None));
                    break;
                }
            }
            found
        }
        Step::Resolve => None,
    })
}

/// Answers whether the extension slice `slice` is the one for `url`: its type
/// names the url's profile, or its `url` is fixed to it.
fn names_extension<R: Release>(
    structure: &Structure<R>,
    slice: &str,
    element: &Element<R>,
    url: &str,
) -> Result<bool, ValidateError> {
    let profiled = element
        .types()
        .iter()
        .flat_map(|kind| kind.profiles.iter())
        .any(|profile| Canonical::parse(profile).url() == url);
    if profiled {
        return Ok(true);
    }
    let id = format!("{slice}.url");
    let Some(element) = structure.element(&id) else {
        return Ok(false);
    };
    let fixed = value_of(element, Stem::Fixed).map_err(|source| ValidateError::Encode {
        structure: structure.url().to_owned(),
        element: id,
        source,
    })?;
    Ok(fixed.is_some_and(|fixed| fixed == Value::String(url.to_owned())))
}

/// A snapshot a path hops into, and the id of its root.
type Hop<R> = (Arc<Structure<R>>, String);

/// Hops from the element `here` into the definition of its one type: the
/// profile the type names when it names one, the core definition otherwise.
/// Returns the snapshot and the id of its root.
fn hop<R: Release>(
    validator: &Validator<'_, R>,
    here: &Located<R>,
    from: &Structure<R>,
) -> Result<Option<Hop<R>>, ValidateError> {
    let types = here.types();
    let [(code, profiles)] = types.as_slice() else {
        return Ok(None);
    };
    let canonical = match profiles.as_slice() {
        [] => format!("http://hl7.org/fhir/StructureDefinition/{code}"),
        [profile] => profile.clone(),
        _ => return Ok(None),
    };
    Ok(resolved(validator, &canonical, from)?.map(|structure| {
        let root = structure.type_name().to_owned();
        (structure, root)
    }))
}

/// Resolves `canonical` as the definition `from` names it, `None` when the
/// set does not carry it.
fn resolved<R: Release>(
    validator: &Validator<'_, R>,
    canonical: &str,
    from: &Structure<R>,
) -> Result<Option<Arc<Structure<R>>>, ValidateError> {
    match validator.structure(canonical, Some(from)) {
        Ok(structure) => Ok(Some(structure)),
        Err(ValidateError::Resolve(
            ResolveError::NotFound { .. } | ResolveError::Fragment { .. },
        )) => Ok(None),
        Err(error) => Err(error),
    }
}
