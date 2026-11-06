// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The package set: the packages given to the validator, checked for a
//! complete dependency closure, and the `StructureDefinitions`, `ValueSets`
//! and `CodeSystems` they carry, resolved by canonical.
//!
//! Each resource decodes into the generated model of the set's [`Release`].
//!
//! The set is read in one FHIR release, as a dependency tree is resolved in
//! the one FHIR version its core package dependency defines (FHIR
//! 6.0.0-ballot4, packages.html §2.1.10.3, `dependencies`). A package whose
//! `fhirVersions` name another release is held: it satisfies the dependency
//! that names it, its resources are never read, its own dependencies are not
//! required, and a canonical only it defines is refused with
//! [`ResolveError::OtherRelease`]. The specification is silent on a
//! dependency of another FHIR version, so that much is our own design.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use fhir_types::codec::DecodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Value;

use crate::canonical::Canonical;
use crate::canonical::compare_versions;
use crate::package::Content;
use crate::package::Package;
use crate::package::PackageError;
use crate::release::R4;
use crate::release::Release;

/// A set of packages with a complete dependency closure, read as the release
/// `R`.
#[derive(Debug)]
pub struct PackageSet<R: Release = R4> {
    packages: Vec<Package>,
    structures: BTreeMap<String, Vec<Entry<R::StructureDefinition>>>,
    value_sets: BTreeMap<String, Vec<Entry<R::ValueSet>>>,
    code_systems: BTreeMap<String, Vec<Entry<R::CodeSystem>>>,
    /// The dependency closure of each package, by position.
    closures: Vec<BTreeSet<PackageId>>,
    /// The packages of another FHIR release, held and never read.
    other_releases: Vec<Package>,
    /// The resources the packages of another release define, by canonical
    /// URL.
    elsewhere: BTreeMap<String, Vec<Elsewhere>>,
}

/// One resource a package of another FHIR release defines.
#[derive(Debug)]
struct Elsewhere {
    /// The `resourceType`.
    type_name: String,
    /// The business version, when the resource has one.
    version: Option<String>,
    /// The package, as `name#version`.
    package: String,
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
    /// The canonical names a resource only a package of another FHIR release
    /// defines, which the set holds and does not read.
    #[error(
        "{canonical} is defined only in {package}, a package of another FHIR release than the {release} the set is read in"
    )]
    OtherRelease {
        /// The canonical asked for.
        canonical: String,
        /// The package that defines it, as `name#version`.
        package: String,
        /// The release the set is read in, `R4` or `STU3`.
        release: &'static str,
    },
    /// The resource does not decode into the set's release model of its type.
    #[error("{canonical} in {package} does not decode into the release model of its type")]
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

impl<R: Release> PackageSet<R> {
    /// Reads every package at `paths` (`.tgz` files or unpacked directories)
    /// and builds the set, read as the release `R`.
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

    /// Builds the set from packages already read, checking the closure, read
    /// as the release `R`.
    ///
    /// Every dependency a manifest names SHALL be listed, and a version
    /// whose patch is `x` accepts the highest patch present
    /// (<https://hl7.org/fhir/packages.html>, §2.1.10.2). A package of
    /// another FHIR release (see the module documentation) satisfies a
    /// dependency on it and is otherwise held unread.
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
        // NOTE: packages.html (6.0.0-ballot4) §2.1.10.3 is silent on a dependency of another
        // FHIR version; no specification governs this: our own design, held and never read.
        let (packages, other_releases): (Vec<Package>, Vec<Package>) =
            packages.into_iter().partition(of_release::<R>);
        for package in &packages {
            for (dependency, wanted) in package.dependencies() {
                let held = packages.iter().chain(&other_releases).any(|candidate| {
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
        let mut structures: BTreeMap<String, Vec<Entry<R::StructureDefinition>>> = BTreeMap::new();
        let mut value_sets: BTreeMap<String, Vec<Entry<R::ValueSet>>> = BTreeMap::new();
        let mut code_systems: BTreeMap<String, Vec<Entry<R::CodeSystem>>> = BTreeMap::new();
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
        let mut elsewhere: BTreeMap<String, Vec<Elsewhere>> = BTreeMap::new();
        for package in &other_releases {
            for resource in package.resources() {
                let Some(url) = &resource.url else { continue };
                elsewhere.entry(url.clone()).or_default().push(Elsewhere {
                    type_name: resource.type_name.clone(),
                    version: resource.version.clone(),
                    package: format!("{}#{}", package.name(), package.version()),
                });
            }
        }
        Ok(Self {
            packages,
            structures,
            value_sets,
            code_systems,
            closures,
            other_releases,
            elsewhere,
        })
    }

    /// Returns the packages of another FHIR release the set holds and does
    /// not read, sorted by name and version.
    #[must_use]
    pub fn other_releases(&self) -> &[Package] {
        &self.other_releases
    }

    /// Turns `error`, a canonical of type `kind` the set does not carry,
    /// into [`ResolveError::OtherRelease`] when a package of another release
    /// defines it.
    fn elsewhere(&self, error: ResolveError, kind: &str) -> ResolveError {
        let ResolveError::NotFound { canonical } = error else {
            return error;
        };
        let parsed = Canonical::parse(&canonical);
        let found = self.elsewhere.get(parsed.url()).and_then(|held| {
            held.iter().find(|resource| {
                resource.type_name == kind
                    && parsed
                        .version()
                        .is_none_or(|version| resource.version.as_deref() == Some(version))
            })
        });
        match found {
            Some(resource) => ResolveError::OtherRelease {
                package: resource.package.clone(),
                canonical,
                release: R::NAME,
            },
            None => ResolveError::NotFound { canonical },
        }
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
    ) -> Result<Arc<R::StructureDefinition>, ResolveError> {
        self.structure_definition_in(canonical, None)
            .map(|(definition, _)| definition)
    }

    /// Resolves `canonical` to the `ValueSet` it names, by the rules of
    /// [`PackageSet::structure_definition`].
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] as [`PackageSet::structure_definition`] does.
    pub fn value_set(&self, canonical: &str) -> Result<Arc<R::ValueSet>, ResolveError> {
        self.value_set_in(canonical, None)
            .map(|(value_set, _)| value_set)
    }

    /// Resolves `canonical` to the `CodeSystem` it names, by the rules of
    /// [`PackageSet::structure_definition`].
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] as [`PackageSet::structure_definition`] does.
    pub fn code_system(&self, canonical: &str) -> Result<Arc<R::CodeSystem>, ResolveError> {
        self.code_system_in(canonical, None)
            .map(|(code_system, _)| code_system)
    }

    /// Resolves `canonical` as referenced from the package `scope`, and
    /// returns the definition with the package that carries it.
    pub(crate) fn structure_definition_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<R::StructureDefinition>, PackageId), ResolveError> {
        resolve(
            &self.packages,
            &self.structures,
            canonical,
            "StructureDefinition",
            self.closure(scope),
        )
        .map_err(|error| self.elsewhere(error, "StructureDefinition"))
    }

    /// Resolves the `ValueSet` `canonical` names as referenced from `scope`.
    pub(crate) fn value_set_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<R::ValueSet>, PackageId), ResolveError> {
        resolve(
            &self.packages,
            &self.value_sets,
            canonical,
            "ValueSet",
            self.closure(scope),
        )
        .map_err(|error| self.elsewhere(error, "ValueSet"))
    }

    /// Resolves the `CodeSystem` `canonical` names as referenced from `scope`.
    pub(crate) fn code_system_in(
        &self,
        canonical: &str,
        scope: Option<PackageId>,
    ) -> Result<(Arc<R::CodeSystem>, PackageId), ResolveError> {
        resolve(
            &self.packages,
            &self.code_systems,
            canonical,
            "CodeSystem",
            self.closure(scope),
        )
        .map_err(|error| self.elsewhere(error, "CodeSystem"))
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

/// Resolves `canonical` among `index`, the resources of type `kind` that
/// `packages` carry, within `closure` when given.
fn resolve<T: Json>(
    packages: &[Package],
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
    let package = |entry: &Entry<T>| packages.get(entry.origin.0);
    // NOTE: packages.html is silent on one canonical carried by two versions of one package;
    // no specification governs this: our own design, the higher package version wins.
    let matching: Vec<&Entry<T>> = matching
        .iter()
        .copied()
        .filter(|entry| {
            !matching
                .iter()
                .any(|other| match (package(entry), package(other)) {
                    (Some(held), Some(rival)) => {
                        held.name() == rival.name()
                            && compare_versions(rival.version(), held.version())
                                == Ordering::Greater
                    }
                    _ => false,
                })
        })
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

/// Answers whether `package` is of the release `R`: its `fhirVersions`
/// name a version of `R`, or, when it names none, it is `R`'s core package,
/// or depends on it or on no core package at all (packages.html
/// 6.0.0-ballot4 §2.1.10.3, `dependencies` and `fhirVersions`).
fn of_release<R: Release>(package: &Package) -> bool {
    let versions = package.fhir_versions();
    if !versions.is_empty() {
        return versions
            .iter()
            .any(|version| same_release(version, R::FHIR_VERSION));
    }
    if is_core_package(package.name()) {
        return package.name() == R::CORE_PACKAGE;
    }
    let mut cores = package
        .dependencies()
        .keys()
        .filter(|name| is_core_package(name))
        .peekable();
    cores.peek().is_none() || cores.any(|name| name == R::CORE_PACKAGE)
}

/// Answers whether the FHIR version `version` has the major and minor
/// `release`, as `4.0.1` has `4.0`.
fn same_release(version: &str, release: &str) -> bool {
    version
        .strip_prefix(release)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// Answers whether `name` names a core package, `hl7.fhir.r<release>.core`
/// (packages.html §2.1.10).
fn is_core_package(name: &str) -> bool {
    name.strip_prefix("hl7.fhir.r")
        .and_then(|rest| rest.strip_suffix(".core"))
        .is_some_and(|release| !release.is_empty() && !release.contains('.'))
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
    use super::R4;
    use super::is_core_package;
    use super::same_release;
    use super::version_satisfies;

    #[test]
    fn a_fhir_version_belongs_to_the_release_of_its_major_and_minor() {
        assert!(same_release("4.0.1", "4.0"));
        assert!(same_release("4.0", "4.0"));
        assert!(!same_release("4.3.0", "4.0"));
        assert!(!same_release("5.0.0", "4.0"));
        assert!(!same_release("4.01", "4.0"));
    }

    #[test]
    fn a_core_package_is_named_by_its_release() {
        assert!(is_core_package("hl7.fhir.r4.core"));
        assert!(is_core_package("hl7.fhir.r4b.core"));
        assert!(!is_core_package("hl7.fhir.r4.examples"));
        assert!(!is_core_package("hl7.fhir.uv.extensions.r4"));
        assert!(!is_core_package("hl7.fhir.r.core"));
    }

    #[test]
    fn a_patch_wildcard_accepts_any_patch_of_its_minor() {
        assert!(version_satisfies("4.0.1", "4.0.1"));
        assert!(version_satisfies("4.0.1", "4.0.x"));
        assert!(!version_satisfies("4.1.0", "4.0.x"));
        assert!(!version_satisfies("4.0.1", "4.0.2"));
    }

    // scripts/vendor/profile-packages.sh fetches the EU trees at build time; the test
    // returns early without them, and fails under CI as tests/it/support.rs
    // `profile_packages` does, since a unit test cannot reach that module.
    #[test]
    fn a_canonical_eps_names_resolves_to_the_eu_base_version_eps_depends_on() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages");
        if !root
            .join("hl7.fhir.eu.eps#1.0.0-ballot/package/package.json")
            .is_file()
        {
            assert_ne!(
                std::env::var("CI").as_deref(),
                Ok("true"),
                "the profile packages are absent under CI; run scripts/vendor/profile-packages.sh"
            );
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
        let set = PackageSet::<R4>::load(paths).expect("the EU closure");
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
