// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reference resolution for a `resolve()` in a slice discriminator, a
//! target profile, an aggregation and the document rules: the seam a
//! resolver fills, the resolver of contained resources
//! (<https://hl7.org/fhir/R4/references.html#contained>), and the resolver
//! of references inside a Bundle
//! (<https://hl7.org/fhir/R4/bundle.html#references>, §2.36.4.1).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use fhir_types::codec::Value;
use fhir_types::schema::Schemas;

/// What resolving one reference found.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Resolution<'i> {
    /// The resource the reference points at, and how it was reached.
    Found {
        /// The resource.
        target: &'i Value,
        /// How it was reached.
        via: Via<'i>,
    },
    /// The reference points into the scope the resolver searches and names
    /// nothing there: a broken reference.
    Missing,
    /// The reference points outside the scope the resolver searches.
    OutOfScope,
    /// The reference matches more than one entry of the Bundle.
    Ambiguous,
}

/// How a resolved reference reached its resource.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Via<'i> {
    /// A local reference to a contained resource, or `#` to its container.
    Contained,
    /// An entry of the Bundle around the resource holding the reference.
    Entry {
        /// The entry's resource the reference is held in, directly or in a
        /// resource it contains.
        from: &'i Value,
    },
}

/// The entries of one Bundle, by `fullUrl` in entry order and by the
/// address of their resource.
#[derive(Debug, Default)]
pub(crate) struct Index<'i> {
    by_url: BTreeMap<&'i str, Vec<&'i Value>>,
    by_resource: BTreeMap<usize, &'i Value>,
}

impl<'i> Index<'i> {
    /// Indexes the entries of `bundle`.
    fn new(bundle: &'i Value) -> Self {
        let mut index = Self::default();
        for entry in entries(bundle) {
            if let Some(url) = entry.get("fullUrl").and_then(Value::as_str) {
                index.by_url.entry(url).or_default().push(entry);
            }
            if let Some(resource) = entry.get("resource") {
                index
                    .by_resource
                    .entry(std::ptr::from_ref(resource).addr())
                    .or_insert(entry);
            }
        }
        index
    }
}

/// The indexes of the Bundles one validation meets, built on first use and
/// keyed by the address of the Bundle, which the instance keeps for the
/// whole validation.
#[derive(Debug, Default)]
pub(crate) struct Indexes<'i> {
    built: RefCell<BTreeMap<usize, Rc<Index<'i>>>>,
}

impl<'i> Indexes<'i> {
    /// Returns the index of `bundle`.
    fn of(&self, bundle: &'i Value) -> Rc<Index<'i>> {
        let key = std::ptr::from_ref(bundle).addr();
        Rc::clone(
            self.built
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(Index::new(bundle))),
        )
    }
}

/// Resolves a reference from inside the resources being validated.
///
/// `scope` holds the resources the walk is inside, the outermost first, so a
/// resolver can search the container of the item, or a Bundle around it.
pub(crate) trait Resolver: fmt::Debug + Send + Sync {
    /// Resolves `reference`, a `Reference.reference`, from inside `scope`,
    /// reading a Bundle through `indexes`.
    fn resolve<'i>(
        &self,
        reference: &str,
        scope: &[&'i Value],
        indexes: &Indexes<'i>,
    ) -> Resolution<'i>;

    /// Returns the Bundle whose entries the resolver searches from inside
    /// `scope`, when there is one.
    fn bundle<'i>(&self, scope: &[&'i Value], indexes: &Indexes<'i>) -> Option<&'i Value>;
}

/// Resolves the local references `#id` against the contained resources of
/// the resource that holds them, and `#` against that resource itself.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Contained;

impl Resolver for Contained {
    fn resolve<'i>(
        &self,
        reference: &str,
        scope: &[&'i Value],
        _indexes: &Indexes<'i>,
    ) -> Resolution<'i> {
        let Some(id) = reference.strip_prefix('#') else {
            return Resolution::OutOfScope;
        };
        // NOTE: references.html §2.3.0.8: `#id` names a resource the container holds and `#`
        // the container, which is the resource that lists the innermost one, or that one.
        let Some(container) = container(scope) else {
            return Resolution::Missing;
        };
        let found = if id.is_empty() {
            Some(container)
        } else {
            contained(container)
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
        };
        found.map_or(Resolution::Missing, |target| Resolution::Found {
            target,
            via: Via::Contained,
        })
    }

    fn bundle<'i>(&self, _scope: &[&'i Value], _indexes: &Indexes<'i>) -> Option<&'i Value> {
        None
    }
}

/// Resolves a local reference as [`Contained`] does, and every other
/// reference against the entries of the Bundle around the resource that
/// holds it, by the method of bundle.html §2.36.4.1, reading the resource
/// types of a FHIR REST URL from its element table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bundled {
    schemas: &'static Schemas,
}

impl Bundled {
    /// Creates the resolver over the element table `schemas`.
    pub(crate) const fn new(schemas: &'static Schemas) -> Self {
        Self { schemas }
    }
}

impl Resolver for Bundled {
    fn resolve<'i>(
        &self,
        reference: &str,
        scope: &[&'i Value],
        indexes: &Indexes<'i>,
    ) -> Resolution<'i> {
        if reference.starts_with('#') {
            return Contained.resolve(reference, scope, indexes);
        }
        let Some((bundle, entry)) = context(scope, indexes) else {
            return Resolution::OutOfScope;
        };
        let Some(from) = entry.get("resource") else {
            return Resolution::OutOfScope;
        };
        let full_url = entry.get("fullUrl").and_then(Value::as_str);
        let Some((url, version)) = absolute(self.schemas, reference, full_url) else {
            // NOTE: bundle.html §2.36.4.1: a relative reference from an entry whose fullUrl is
            // not RESTful "has no defined meaning within this specification".
            return Resolution::OutOfScope;
        };
        lookup((bundle, indexes), &url, version, from)
    }

    fn bundle<'i>(&self, scope: &[&'i Value], indexes: &Indexes<'i>) -> Option<&'i Value> {
        context(scope, indexes).map(|(bundle, _)| bundle)
    }
}

/// Resolves a reference that carries a URI scheme against the entries of
/// `bundle` itself, from outside every entry: an invariant on an element of
/// the Bundle reaches references that no entry around the walk holds.
///
/// A relative reference has no entry `fullUrl` to resolve against here, so
/// it is out of scope.
pub(crate) fn from_bundle<'i>(
    schemas: &Schemas,
    bundle: &'i Value,
    reference: &str,
    indexes: &Indexes<'i>,
) -> Resolution<'i> {
    if !has_scheme(reference) {
        return Resolution::OutOfScope;
    }
    match absolute(schemas, reference, None) {
        Some((url, version)) => lookup((bundle, indexes), &url, version, bundle),
        None => Resolution::OutOfScope,
    }
}

/// The entry of `bundle` whose `fullUrl` is `url`, at `version` when one is
/// given, reached from the entry resource `from`.
fn lookup<'i>(
    (bundle, indexes): (&'i Value, &Indexes<'i>),
    url: &str,
    version: Option<&str>,
    from: &'i Value,
) -> Resolution<'i> {
    // NOTE: bundle.html §2.36.4.1: a version-specific reference drops its version to match
    // the fullUrl, "and then match the version based on Resource.meta.versionId".
    let index = indexes.of(bundle);
    let found: Vec<&'i Value> = index
        .by_url
        .get(url)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .filter(|resource| version.is_none_or(|version| version_id(resource) == Some(version)))
        .collect();
    match found.as_slice() {
        [target] => Resolution::Found {
            target,
            via: Via::Entry { from },
        },
        // NOTE: bundle.html §2.36.4.1: with no match, "if the URI is a URL that can be
        // resolved (e.g. if an http: URL), try accessing it directly"; any other is broken.
        [] if url.starts_with("http://") || url.starts_with("https://") => Resolution::OutOfScope,
        [] => Resolution::Missing,
        _ => Resolution::Ambiguous,
    }
}

/// The Bundle around the innermost resource of `scope` that is one of its
/// entries, and that entry.
///
/// "the rules for resolving references in contained resources are the same
/// as those for resolving resources in the resource that contains the
/// contained resource" (bundle.html §2.36.4.1), so a contained resource
/// resolves through the entry of its container.
fn context<'i>(scope: &[&'i Value], indexes: &Indexes<'i>) -> Option<(&'i Value, &'i Value)> {
    for (index, &resource) in scope.iter().enumerate().rev() {
        let outer = scope.get(..index).unwrap_or_default();
        for &candidate in outer.iter().rev() {
            if candidate.get("resourceType").and_then(Value::as_str) != Some("Bundle") {
                continue;
            }
            let index = indexes.of(candidate);
            if let Some(&entry) = index.by_resource.get(&std::ptr::from_ref(resource).addr()) {
                return Some((candidate, entry));
            }
        }
    }
    None
}

/// The container of the innermost resource of `scope`: the resource whose
/// `contained` lists it, or the innermost resource itself when none does.
pub(crate) fn container<'i>(scope: &[&'i Value]) -> Option<&'i Value> {
    let (&innermost, outer) = scope.split_last()?;
    Some(
        outer
            .iter()
            .rev()
            .copied()
            .find(|candidate| {
                contained(candidate)
                    .iter()
                    .any(|item| std::ptr::eq(item, innermost))
            })
            .unwrap_or(innermost),
    )
}

/// The entries of `bundle`.
fn entries(bundle: &Value) -> &[Value] {
    bundle
        .get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default()
}

/// The `meta.versionId` of `resource`.
fn version_id(resource: &Value) -> Option<&str> {
    resource
        .get("meta")
        .and_then(|meta| meta.get("versionId"))
        .and_then(Value::as_str)
}

/// The absolute URL `reference` names, without its version, and that
/// version, from an entry whose fullUrl is `full_url`; `None` when the
/// reference is relative and the `fullUrl` is no FHIR REST URL.
fn absolute<'r>(
    schemas: &Schemas,
    reference: &'r str,
    full_url: Option<&str>,
) -> Option<(String, Option<&'r str>)> {
    if has_scheme(reference) {
        return Some(match restful(schemas, reference) {
            Some(parts) if parts.version.is_some() => (parts.unversioned(), parts.version),
            _ => (reference.to_owned(), None),
        });
    }
    let relative = restful(schemas, reference).filter(|parts| parts.root.is_empty())?;
    let root = restful(schemas, full_url?)?.root;
    Some((
        format!("{root}{}/{}", relative.kind, relative.id),
        relative.version,
    ))
}

/// Answers whether `text` starts with a URI scheme (RFC 3986 §3.1).
fn has_scheme(text: &str) -> bool {
    text.split_once(':').is_some_and(|(scheme, _)| {
        let mut chars = scheme.chars();
        chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

/// A URL in the form the FHIR REST URL regex of references.html §2.3.0.1 admits:
/// `[root][type]/[id]` with an optional `/_history/[version]`.
#[derive(Debug, Clone, Copy)]
struct Restful<'a> {
    /// The service base with its trailing `/`, empty for a relative URL.
    root: &'a str,
    /// The resource type.
    kind: &'a str,
    /// The logical id.
    id: &'a str,
    /// The version id.
    version: Option<&'a str>,
}

impl Restful<'_> {
    /// The URL without its version.
    fn unversioned(&self) -> String {
        format!("{}{}/{}", self.root, self.kind, self.id)
    }
}

/// Reads `text` as a FHIR REST URL: `((http|https)://([A-Za-z0-9\-\\\.\:\%\$]*/)+)?`,
/// a resource type of the element table `schemas`, `/[A-Za-z0-9\-\.]{1,64}`
/// and an optional `/_history/[A-Za-z0-9\-\.]{1,64}`.
fn restful<'t>(schemas: &Schemas, text: &'t str) -> Option<Restful<'t>> {
    let (path, version) = match text.rsplit_once("/_history/") {
        Some((path, version)) => (path, Some(version)),
        None => (text, None),
    };
    if version.is_some_and(|version| !is_id(version)) {
        return None;
    }
    let (head, id) = path.rsplit_once('/')?;
    let kind = head.rsplit('/').next()?;
    let root = head.strip_suffix(kind)?;
    let base_ok = root.is_empty()
        || root
            .strip_prefix("http://")
            .or_else(|| root.strip_prefix("https://"))
            .is_some_and(|rest| {
                rest.ends_with('/')
                    && rest.chars().all(|c| {
                        c.is_ascii_alphanumeric()
                            || matches!(c, '-' | '\\' | '.' | ':' | '%' | '$' | '/')
                    })
            });
    (base_ok && is_id(id) && schemas.is_resource(kind)).then_some(Restful {
        root,
        kind,
        id,
        version,
    })
}

/// Answers whether `text` is a logical id: `[A-Za-z0-9\-\.]{1,64}`.
fn is_id(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
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
    use fhir_types::r4::schema::SCHEMAS;

    use super::Bundled;
    use super::Contained;
    use super::Indexes;
    use super::Resolution;
    use super::Resolver;
    use super::Via;
    use super::restful;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).expect("test JSON")
    }

    fn id_of(found: Resolution<'_>) -> Option<&str> {
        match found {
            Resolution::Found { target, .. } => target.get("id").and_then(Value::as_str),
            Resolution::Missing | Resolution::OutOfScope | Resolution::Ambiguous => None,
        }
    }

    #[test]
    fn a_local_reference_resolves_to_the_contained_resource() {
        let resource =
            json(r#"{"resourceType":"List","contained":[{"resourceType":"Patient","id":"p1"}]}"#);
        let found = Contained.resolve("#p1", &[&resource], &Indexes::default());
        assert!(matches!(
            found,
            Resolution::Found {
                via: Via::Contained,
                ..
            }
        ));
        assert_eq!(id_of(found), Some("p1"));
        assert!(matches!(
            Contained.resolve("#p2", &[&resource], &Indexes::default()),
            Resolution::Missing
        ));
        assert!(matches!(
            Contained.resolve("Patient/p1", &[&resource], &Indexes::default()),
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
        assert_eq!(
            id_of(Contained.resolve("#", &[&resource, inner], &Indexes::default())),
            Some("l")
        );
    }

    #[test]
    fn a_restful_url_splits_into_its_root_type_id_and_version() {
        let parts =
            restful(&SCHEMAS, "http://example.org/fhir/Patient/p1/_history/2").expect("RESTful");
        assert_eq!(
            (parts.root, parts.kind, parts.id, parts.version),
            ("http://example.org/fhir/", "Patient", "p1", Some("2"))
        );
        let relative = restful(&SCHEMAS, "Patient/p1").expect("RESTful");
        assert_eq!((relative.root, relative.version), ("", None));
        assert!(restful(&SCHEMAS, "urn:uuid:4a1e8f9e-0000-4000-8000-000000000001").is_none());
        assert!(restful(&SCHEMAS, "http://example.org/fhir/NotAType/p1").is_none());
        assert!(restful(&SCHEMAS, "ftp://example.org/Patient/p1").is_none());
    }

    #[test]
    fn a_contained_resource_in_an_entry_resolves_through_its_containers_entry() {
        let bundle = json(
            r#"{"resourceType":"Bundle","type":"collection","entry":[
                {"fullUrl":"http://example.org/fhir/Observation/o1","resource":{"resourceType":"Observation","id":"o1",
                  "contained":[{"resourceType":"Device","id":"d1"}]}},
                {"fullUrl":"http://example.org/fhir/Patient/p1","resource":{"resourceType":"Patient","id":"p1"}}]}"#,
        );
        let entries = bundle
            .get("entry")
            .and_then(Value::as_array)
            .expect("entries");
        let observation = entries
            .first()
            .and_then(|entry| entry.get("resource"))
            .expect("the observation");
        let device = observation
            .get("contained")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .expect("the device");
        let found = Bundled::new(&SCHEMAS).resolve(
            "Patient/p1",
            &[&bundle, observation, device],
            &Indexes::default(),
        );
        assert_eq!(id_of(found), Some("p1"));
        assert!(matches!(
            found,
            Resolution::Found { via: Via::Entry { from }, .. } if std::ptr::eq(from, observation)
        ));
    }
}
