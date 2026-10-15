// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The package set: the packages given to the validator, checked for a
//! complete dependency closure, and the `StructureDefinitions`, `ValueSets`
//! and `CodeSystems` they carry, resolved by canonical.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use fhir_types::codec::DecodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Value;
use fhir_types::r4::code_system::CodeSystem;
use fhir_types::r4::structure_definition::StructureDefinition;
use fhir_types::r4::value_set::ValueSet;

use crate::canonical::Canonical;
use crate::canonical::compare_versions;
use crate::package::Content;
use crate::package::Package;
use crate::package::PackageError;

/// A set of packages with a complete dependency closure.
#[derive(Debug)]
pub struct PackageSet {
    packages: Vec<Package>,
    structures: BTreeMap<String, Vec<Entry<StructureDefinition>>>,
    value_sets: BTreeMap<String, Vec<Entry<ValueSet>>>,
    code_systems: BTreeMap<String, Vec<Entry<CodeSystem>>>,
    /// The dependency closure of each package, by position.
    closures: Vec<BTreeSet<PackageId>>,
}

/// One resource the set carries, decoded on first use.
#[derive(Debug)]
struct Entry<T> {
    version: Option<String>,
    /// The package and file, as `name#version/file`.
    package: String,
    content: Content,
    /// The package that carries it.
    origin: PackageId,
    decoded: OnceLock<Arc<T>>,
}

impl<T> Entry<T> {
    /// Creates the entry of a resource not yet decoded.
    fn new(version: Option<String>, package: String, content: Content, origin: PackageId) -> Self {
        Self {
            version,
            package,
            content,
            origin,
            decoded: OnceLock::new(),
        }
    }
}

/// Why a package set could not be built.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A package could not be read.
    #[error(transparent)]
    Package(#[from] PackageError),
    /// Two packages carry the same name and version.
    #[error("package {name}#{version} is given twice")]
    DuplicatePackage {
        /// The package name.
        name: String,
        /// The package version.
        version: String,
    },
    /// A dependency a manifest names is not in the set.
    #[error(
        "{package}#{version} depends on {dependency}#{dependency_version}, which the set does not hold"
    )]
    MissingDependency {
        /// The dependent package.
        package: String,
        /// The dependent's version.
        version: String,
        /// The package it names.
        dependency: String,
        /// The version it names.
        dependency_version: String,
    },
}

/// Why a canonical did not resolve to one resource of the type asked for.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// No package of the set carries the canonical.
    #[error("no resource {canonical} of the type asked for in the package set")]
    NotFound {
        /// The canonical asked for.
        canonical: String,
    },
    /// The canonical names a contained resource, which a package does not
    /// index.
    #[error("{canonical} names a contained resource, which the package set does not resolve")]
    Fragment {
        /// The canonical asked for.
        canonical: String,
    },
    /// Two packages carry different definitions under one URL and version.
    #[error("{canonical} resolves to different definitions in {first} and {second}")]
    Ambiguous {
        /// The canonical asked for.
        canonical: String,
        /// The first package and file, as `name#version/file`.
        first: String,
        /// The second package and file, as `name#version/file`.
        second: String,
    },
    /// The file could not be read.
    #[error("{canonical} in {package} cannot be read")]
    Io {
        /// The canonical asked for.
        canonical: String,
        /// The package and file, as `name#version/file`.
        package: String,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file does not parse as JSON.
    #[error("{canonical} in {package} is no JSON document")]
    Json {
        /// The canonical asked for.
        canonical: String,
        /// The package and file, as `name#version/file`.
        package: String,
        /// The parse error.
        #[source]
        source: serde_json::Error,
    },
    /// The resource does not decode into the R4 model of its type.
    #[error("{canonical} in {package} does not decode into the R4 model of its type")]
    Decode {
        /// The canonical asked for.
        canonical: String,
        /// The package and file, as `name#version/file`.
        package: String,
        /// The decode error, located by its element path.
        #[source]
        source: DecodeError,
    },
}

impl PackageSet {
    /// Reads every package at `paths` (`.tgz` files or unpacked directories)
    /// and builds the set.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when a package cannot be read, or as
    /// [`PackageSet::new`] does.
    pub fn load<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Result<Self, LoadError> {
        let packages = paths
            .into_iter()
            .map(|path| Package::read(path.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(packages)
    }

    /// Builds the set from packages already read, checking the closure.
    ///
    /// Every dependency a manifest names SHALL be listed, and a version
    /// whose patch is `x` accepts the highest patch present
    /// (<https://hl7.org/fhir/packages.html>, §2.1.10.2).
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::DuplicatePackage`] for a name and version given
    /// twice, and [`LoadError::MissingDependency`] naming the first dependency
    /// the set does not hold.
    pub fn new(mut packages: Vec<Package>) -> Result<Self, LoadError> {
        packages.sort_by(|a, b| {
            a.name()
                .cmp(b.name())
                .then_with(|| compare_versions(a.version(), b.version()))
        });
        for pair in packages.windows(2) {
            if let [first, second] = pair
                && first.name() == second.name()
                && first.version() == second.version()
            {
                return Err(LoadError::DuplicatePackage {
                    name: first.name().to_owned(),
                    version: first.version().to_owned(),
                });
            }
        }
        for package in &packages {
            for (dependency, wanted) in package.dependencies() {
                let held = packages.iter().any(|candidate| {
                    candidate.name() == dependency && version_satisfies(candidate.version(), wanted)
                });
                if !held {
                    return Err(LoadError::MissingDependency {
                        package: package.name().to_owned(),
                        version: package.version().to_owned(),
                        dependency: dependency.clone(),
                        dependency_version: wanted.clone(),
                    });
                }
            }
        }
        let mut structures: BTreeMap<String, Vec<Entry<StructureDefinition>>> = BTreeMap::new();
        let mut value_sets: BTreeMap<String, Vec<Entry<ValueSet>>> = BTreeMap::new();
        let mut code_systems: BTreeMap<String, Vec<Entry<CodeSystem>>> = BTreeMap::new();
        for (index, package) in packages.iter().enumerate() {
            let label = format!("{}#{}", package.name(), package.version());
            let origin = PackageId(index);
            for resource in package.resources() {
                let Some(url) = &resource.url else { continue };
                let version = resource.version.clone();
                let source = format!("{label}/{}", resource.file);
                let content = resource.content.clone();
                match resource.type_name.as_str() {
                    "StructureDefinition" => structures
                        .entry(url.clone())
                        .or_default()
                        .push(Entry::new(version, source, content, origin)),
                    "ValueSet" => value_sets
                        .entry(url.clone())
                        .or_default()
                        .push(Entry::new(version, source, content, origin)),
                    "CodeSystem" => code_systems
                        .entry(url.clone())
                        .or_default()
                        .push(Entry::new(version, source, content, origin)),
                    _ => {}
                }
            }
        }
        let closures = (0..packages.len())
            .map(|index| dependency_closure(&packages, index))
            .collect();
        Ok(Self {
            packages,
            structures,
            value_sets,
            code_systems,
            closures,
        })
    }

    /// Returns the packages of the set, sorted by name and version.
    #[must_use]
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Resolves `canonical` to the `StructureDefinition` it names.
    ///
    /// `url|version` resolves to that version. A bare `url` resolves to its
    /// one version, or to the latest when the set carries several, as R4
    /// asks (<https://hl7.org/fhir/R4/references.html#canonical>, §2.3.0.5);
    /// the order is [`compare_versions`], and a definition without a version
    /// sorts below every versioned one.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when nothing matches, the canonical names a
    /// contained resource, two different definitions share the URL and
    /// version, or the definition does not decode.
    pub fn structure_definition(
        &self,
        canonical: &str,
    ) -> Result<Arc<StructureDefinition>, ResolveError> {
        self.structure_definition_in(canonical, None)
            .map(|(definition, _)| definition)
    }

    /// Resolves `canonical` to the `ValueSet` it names, by the rules of
    /// [`PackageSet::structure_definition`].
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] as [`PackageSet::structure_definition`] does.
    pub fn value_set(&self, canonical: &str) -> Result<Arc<ValueSet>, ResolveError> {
        self.value_set_in(canonical, None)
            .map(|(value_set, _)| value_set)
    }

    /// Resolves `canonical` to the `CodeSystem` it names, by the rules of
    /// [`PackageSet::structure_definition`].
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] as [`PackageSet::structure_definition`] does.
    pub fn code_system(&self, canonical: &str) -> Result<Arc<CodeSystem>, ResolveError> {
        self.code_system_in(canonical, None)
            .map(|(code_system, _)| code_system)
    }

    /// Resolves `canonical` as referenced from the package `scope`, and
    /// returns the definition with the package that carries it.
    pub(crate) fn structure_definition_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<StructureDefinition>, PackageId), ResolveError> {
        resolve(
            &self.structures,
            canonical,
            "StructureDefinition",
            self.closure(scope),
        )
    }

    /// Resolves the `ValueSet` `canonical` names as referenced from `scope`.
    pub(crate) fn value_set_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<ValueSet>, PackageId), ResolveError> {
        resolve(&self.value_sets, canonical, "ValueSet", self.closure(scope))
    }

    /// Resolves the `CodeSystem` `canonical` names as referenced from `scope`.
    pub(crate) fn code_system_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<CodeSystem>, PackageId), ResolveError> {
        resolve(
            &self.code_systems,
            canonical,
            "CodeSystem",
            self.closure(scope),
        )
    }

    /// Returns the dependency closure of the package `scope`.
    fn closure(&self, scope: Option<PackageId>) -> Option<&BTreeSet<PackageId>> {
        scope.and_then(|PackageId(index)| self.closures.get(index))
    }
}

/// One package of a set, by its position in [`PackageSet::packages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackageId(usize);

/// The dependency closure of the package at `start`: itself, and for every
/// dependency a manifest names the highest version of the set that
/// satisfies it, followed transitively.
fn dependency_closure(packages: &[Package], start: usize) -> BTreeSet<PackageId> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(index) = pending.pop() {
        if !seen.insert(PackageId(index)) {
            continue;
        }
        let Some(package) = packages.get(index) else {
            continue;
        };
        for (dependency, wanted) in package.dependencies() {
            let best = packages
                .iter()
                .enumerate()
                .filter(|(_, candidate)| {
                    candidate.name() == dependency && version_satisfies(candidate.version(), wanted)
                })
                .max_by(|(_, a), (_, b)| compare_versions(a.version(), b.version()));
            if let Some((found, _)) = best {
                pending.push(found);
            }
        }
    }
    seen
}

/// Resolves `canonical` among `index`, the resources of type `kind`, within
/// `closure` when given.
fn resolve<T: Json>(
    index: &BTreeMap<String, Vec<Entry<T>>>,
    canonical: &str,
    kind: &str,
    closure: Option<&BTreeSet<PackageId>>,
) -> Result<(Arc<T>, PackageId), ResolveError> {
    let parsed = Canonical::parse(canonical);
    if parsed.fragment().is_some() {
        return Err(ResolveError::Fragment {
            canonical: canonical.to_owned(),
        });
    }
    let not_found = || ResolveError::NotFound {
        canonical: canonical.to_owned(),
    };
    let all = index.get(parsed.url()).ok_or_else(not_found)?;
    let scoped: Vec<&Entry<T>> = all
        .iter()
        .filter(|entry| closure.is_none_or(|closure| closure.contains(&entry.origin)))
        .collect();
    // NOTE: packages.html names a package's dependencies and is silent on a canonical they do
    // not carry; no specification governs this: our own design, it falls back to the whole set.
    let entries: Vec<&Entry<T>> = if scoped.is_empty() || parsed.version().is_some() {
        all.iter().collect()
    } else {
        scoped
    };
    let wanted = if let Some(version) = parsed.version() {
        Some(version)
    } else {
        entries
            .iter()
            .max_by(|a, b| order_versions(a.version.as_deref(), b.version.as_deref()))
            .ok_or_else(not_found)?
            .version
            .as_deref()
    };
    let matching: Vec<&Entry<T>> = entries
        .iter()
        .copied()
        .filter(|entry| entry.version.as_deref() == wanted)
        .collect();
    let (entry, others) = matching.split_first().ok_or_else(not_found)?;
    let origin = entry.origin;
    if let Some(decoded) = entry.decoded.get() {
        return Ok((Arc::clone(decoded), origin));
    }
    let bytes = read(canonical, entry)?;
    for other in others {
        // NOTE: no specification governs this: our own design; one definition
        // carried byte for byte by two packages is one definition, not a conflict.
        if read(canonical, other)? != bytes {
            return Err(ResolveError::Ambiguous {
                canonical: canonical.to_owned(),
                first: entry.package.clone(),
                second: other.package.clone(),
            });
        }
    }
    let decoded = Arc::new(decode(canonical, entry, &bytes, kind)?);
    Ok((Arc::clone(entry.decoded.get_or_init(|| decoded)), origin))
}

/// Reads the bytes of the resource `entry` points at.
fn read<T>(canonical: &str, entry: &Entry<T>) -> Result<Arc<[u8]>, ResolveError> {
    entry.content.read().map_err(|source| ResolveError::Io {
        canonical: canonical.to_owned(),
        package: entry.package.clone(),
        source,
    })
}

/// Decodes `bytes`, the resource of type `kind` that `entry` points at.
fn decode<T: Json>(
    canonical: &str,
    entry: &Entry<T>,
    bytes: &[u8],
    kind: &str,
) -> Result<T, ResolveError> {
    let document: Value = serde_json::from_slice(bytes).map_err(|source| ResolveError::Json {
        canonical: canonical.to_owned(),
        package: entry.package.clone(),
        source,
    })?;
    let mut path = fhir_types::codec::Path::root(kind);
    let decode = |source| ResolveError::Decode {
        canonical: canonical.to_owned(),
        package: entry.package.clone(),
        source,
    };
    let object = fhir_types::codec::expect_object(&document, &path).map_err(decode)?;
    T::from_json(object, &mut path).map_err(decode)
}

/// Orders two optional versions, an absent one below every present one.
fn order_versions(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => compare_versions(left, right),
    }
}

/// Answers whether `version` satisfies the dependency version `wanted`: the
/// same version, or the same major and minor when the patch of `wanted` is
/// `x` (packages.html §2.1.10.2).
fn version_satisfies(version: &str, wanted: &str) -> bool {
    match wanted.strip_suffix(".x") {
        Some(prefix) => version
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|patch| !patch.is_empty() && !patch.contains('.')),
        None => version == wanted,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::PackageId;
    use super::PackageSet;
    use super::version_satisfies;

    #[test]
    fn a_patch_wildcard_accepts_any_patch_of_its_minor() {
        assert!(version_satisfies("4.0.1", "4.0.1"));
        assert!(version_satisfies("4.0.1", "4.0.x"));
        assert!(!version_satisfies("4.1.0", "4.0.x"));
        assert!(!version_satisfies("4.0.1", "4.0.2"));
    }

    // scripts/vendor/profile-packages.sh fetches the EU trees at build time; the test
    // returns early without them, as the other build-time corpora do.
    #[test]
    fn a_canonical_eps_names_resolves_to_the_eu_base_version_eps_depends_on() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages");
        if !root
            .join("hl7.fhir.eu.eps#1.0.0-ballot/package/package.json")
            .is_file()
        {
            return;
        }
        let mut paths = vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/fhir-codegen/vendor/hl7.fhir.r4.core"),
        ];
        for entry in std::fs::read_dir(&root).expect("the fetched packages") {
            let path = entry.expect("a fetched package").path();
            if path.join("package/package.json").is_file() {
                paths.push(path);
            }
        }
        let set = PackageSet::load(paths).expect("the EU closure");
        let eps = set
            .packages()
            .iter()
            .position(|package| package.name() == "hl7.fhir.eu.eps")
            .expect("the EPS package");
        let canonical = "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core";
        let (scoped, _) = set
            .structure_definition_in(canonical, Some(PackageId(eps)))
            .expect("patient-eu-core in the EPS closure");
        assert_eq!(
            scoped
                .version
                .as_ref()
                .and_then(|version| version.value.as_deref()),
            Some("2.0.0")
        );
        let latest = set
            .structure_definition(canonical)
            .expect("patient-eu-core");
        assert_eq!(
            latest
                .version
                .as_ref()
                .and_then(|version| version.value.as_deref()),
            Some("2.0.1")
        );
    }
}
