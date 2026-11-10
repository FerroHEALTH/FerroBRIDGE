// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one walk of a reference-model tail below a template node, which the
//! compiler's check and the engine's carried value both read.

use openehr_rm::v1_2::model;
use openehr_rm::v1_2::paths::RmPath;
use openehr_rm::v1_2::typing::PathStep;
use openehr_rm::v1_2::typing::RmType;
use openehr_rm::v1_2::typing::TypingError;
use openehr_rm::v1_2::typing::path_type;

/// Types every step of `tail` below a node of `rm_type` with
/// `openehr_rm::v1_2::typing::path_type`.
///
/// The one rule: a node type, or a type met on the way, that the reference
/// model does not carry stops the walk with `Ok(None)`, since the model
/// cannot say what lies below it; a step the model carries and refuses is the
/// typing error. What a caller then accepts (the compiler's check takes every
/// typed tail, the engine refuses a `Set` or `Hash` step and a structural
/// node) is that caller's policy. No specification governs the walk: our own
/// design.
///
/// # Errors
///
/// Returns the [`TypingError`] of a step the reference model refuses.
pub(crate) fn steps(rm_type: &str, tail: &RmPath) -> Result<Option<Vec<PathStep>>, TypingError> {
    // NOTE: a node type that is no RM type spelling legitimately stops the walk, as an unknown class does.
    let Ok(class) = rm_type.parse::<RmType>() else {
        return Ok(None);
    };
    if model::class(class.name()).is_none() {
        return Ok(None);
    }
    match path_type(&class, tail) {
        Ok(steps) => Ok(Some(steps)),
        Err(TypingError::UnknownClass(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Returns the reference-model class plus every class below it.
///
/// An empty result means the reference model carries no class of that name.
pub(crate) fn concrete_forms(rm_type: &str) -> Vec<&'static str> {
    let Some(class) = model::class(rm_type) else {
        return Vec::new();
    };
    let mut forms: Vec<&'static str> = vec![class.name];
    for &descendant in class.descendants {
        if !forms.contains(&descendant) {
            forms.push(descendant);
        }
    }
    forms
}

/// Returns the one concrete form of an abstract class, or the class itself.
pub(crate) fn concrete_leaf(declared: &str) -> String {
    let Some(class) = model::class(declared) else {
        return String::from(declared);
    };
    if !class.is_abstract {
        return String::from(declared);
    }
    let concrete: Vec<&str> = class
        .descendants
        .iter()
        .copied()
        .filter(|name| model::class(name).is_some_and(|found| !found.is_abstract))
        .collect();
    match *concrete.as_slice() {
        [only] => String::from(only),
        _ => String::from(declared),
    }
}
