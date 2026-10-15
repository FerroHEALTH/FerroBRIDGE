// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reference resolution for a `resolve()` in a slice discriminator: the seam
//! a resolver fills, and the resolver of contained resources
//! (<https://hl7.org/fhir/R4/references.html#contained>).

use std::fmt;

use fhir_types::codec::Value;

/// What resolving one reference found.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Resolution<'i> {
    /// The resource the reference points at.
    Found(&'i Value),
    /// The reference points into the scope the resolver searches and names
    /// nothing there: a broken reference.
    Missing,
    /// The reference points outside the scope the resolver searches.
    OutOfScope,
}

/// Resolves a reference from inside the resources being validated.
///
/// `scope` holds the resources the walk is inside, the outermost first, so a
/// resolver can search the container of the item, or a Bundle around it.
pub(crate) trait Resolver: fmt::Debug + Send + Sync {
    /// Resolves `reference`, a `Reference.reference`, from inside `scope`.
    fn resolve<'i>(&self, reference: &str, scope: &[&'i Value]) -> Resolution<'i>;
}

/// Resolves the local references `#id` against the contained resources of
/// the resource that holds them, and `#` against that resource itself.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Contained;

impl Resolver for Contained {
    fn resolve<'i>(&self, reference: &str, scope: &[&'i Value]) -> Resolution<'i> {
        // TODO(#441): resolve a reference through the Bundle around the resource.
        let Some(id) = reference.strip_prefix('#') else {
            return Resolution::OutOfScope;
        };
        let Some((&innermost, outer)) = scope.split_last() else {
            return Resolution::Missing;
        };
        // NOTE: references.html §2.3.0.8: `#id` names a resource the container holds and `#`
        // the container, which is the resource that lists the innermost one, or that one.
        let container = outer
            .iter()
            .rev()
            .copied()
            .find(|candidate| {
                contained(candidate)
                    .iter()
                    .any(|item| std::ptr::eq(item, innermost))
            })
            .unwrap_or(innermost);
        if id.is_empty() {
            return Resolution::Found(container);
        }
        contained(container)
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
            .map_or(Resolution::Missing, Resolution::Found)
    }
}

/// The contained resources of `resource`.
fn contained(resource: &Value) -> &[Value] {
    resource
        .get("contained")
        .and_then(Value::as_array)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Value;

    use super::Contained;
    use super::Resolution;
    use super::Resolver;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).expect("test JSON")
    }

    #[test]
    fn a_local_reference_resolves_to_the_contained_resource() {
        let resource =
            json(r#"{"resourceType":"List","contained":[{"resourceType":"Patient","id":"p1"}]}"#);
        let found = Contained.resolve("#p1", &[&resource]);
        assert!(
            matches!(found, Resolution::Found(value) if value.get("id").and_then(Value::as_str) == Some("p1"))
        );
        assert!(matches!(
            Contained.resolve("#p2", &[&resource]),
            Resolution::Missing
        ));
        assert!(matches!(
            Contained.resolve("Patient/p1", &[&resource]),
            Resolution::OutOfScope
        ));
    }

    #[test]
    fn a_bare_hash_inside_a_contained_resource_resolves_to_its_container() {
        let resource = json(
            r#"{"resourceType":"List","id":"l","contained":[{"resourceType":"Patient","id":"p1"}]}"#,
        );
        let inner = resource
            .get("contained")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .expect("the contained patient");
        let found = Contained.resolve("#", &[&resource, inner]);
        assert!(
            matches!(found, Resolution::Found(value) if value.get("id").and_then(Value::as_str) == Some("l"))
        );
    }
}
