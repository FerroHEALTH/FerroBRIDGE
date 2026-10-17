// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-version search parameter registry.
//!
//! It holds every `SearchParameter` the package defines
//! (<https://hl7.org/fhir/R4/searchparameter.html>), and the standard
//! parameter each concrete resource type answers by code, its own and those
//! its base types define (<https://hl7.org/fhir/R4/search.html#all>).
//!
//! The registry is a declared root-set addition: every `SearchParameter`
//! resource the package defines is emitted, in canonical URL order, apart from
//! the specification's examples the loader leaves out
//! ([`crate::package::Package::search_parameters`]). Two parameters that
//! answer one code of one resource type fail the emit.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

use crate::lower::{TypeDef, VersionModule};
use crate::package::Package;

/// One search parameter as the registry carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// `SearchParameter.url`, the canonical.
    pub url: String,
    /// `SearchParameter.name`.
    pub name: String,
    /// `SearchParameter.code`, the name a search uses.
    pub code: String,
    /// `SearchParameter.base`, the resource types it applies to.
    pub base: Vec<String>,
    /// `SearchParameter.type`, the code of `search-param-type`.
    pub type_code: String,
    /// `SearchParameter.expression`, the FHIRPath that extracts the values.
    pub expression: Option<String>,
    /// `SearchParameter.experimental`.
    pub experimental: bool,
}

/// A registry that cannot be lowered.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// Two parameters answer one code of one resource type.
    #[error(
        "{package}: {first} and {second} both define the search parameter `{code}` of {resource}"
    )]
    Ambiguous {
        /// The package.
        package: String,
        /// The resource type.
        resource: String,
        /// The code.
        code: String,
        /// The first parameter's canonical.
        first: String,
        /// The second parameter's canonical.
        second: String,
    },
}

/// The search parameter registry of one version.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    /// Every parameter, in canonical URL order.
    pub parameters: Vec<Parameter>,
    /// The standard parameter each concrete resource type answers by code, as
    /// an index into [`Self::parameters`].
    pub by_resource: BTreeMap<(String, String), usize>,
}

impl Registry {
    /// Lowers the registry of `package`, whose resource types `model` holds.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::Ambiguous`] when two parameters answer one code
    /// of one resource type.
    pub fn lower(package: &Package, model: &VersionModule) -> Result<Self, SearchError> {
        let parameters: Vec<Parameter> = package
            .search_parameters()
            .values()
            .map(|parameter| Parameter {
                url: parameter.url.clone(),
                name: parameter.name.clone(),
                code: parameter.code.clone(),
                base: parameter.base.clone(),
                type_code: parameter.type_code.clone(),
                expression: parameter.expression.clone(),
                experimental: parameter.experimental.unwrap_or(false),
            })
            .collect();
        let mut by_base: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (index, parameter) in parameters.iter().enumerate() {
            for base in &parameter.base {
                by_base.entry(base.as_str()).or_default().push(index);
            }
        }
        let mut by_resource: BTreeMap<(String, String), usize> = BTreeMap::new();
        for resource in model
            .types
            .values()
            .filter(|ty| ty.is_resource && !ty.table_only)
        {
            let mut depth_of: BTreeMap<&str, usize> = BTreeMap::new();
            for (depth, ancestor) in lineage(model, resource).iter().enumerate() {
                for &index in by_base.get(ancestor.as_str()).into_iter().flatten() {
                    let Some(parameter) = parameters.get(index) else {
                        continue;
                    };
                    let key = (resource.path.clone(), parameter.code.clone());
                    match depth_of.get(parameter.code.as_str()) {
                        // NOTE: no specification orders two definitions of one code on a type
                        // and its base (R4B and R5 `Resource-text`, `DomainResource-text`): our
                        // own design, the nearer type's definition stands, as an element's does.
                        Some(&nearer) if nearer < depth => continue,
                        Some(_) if is_tolerated(&model.package_name, ancestor, &parameter.code) => {
                            by_resource.remove(&key);
                            continue;
                        }
                        Some(_) => {
                            let first = by_resource
                                .get(&key)
                                .and_then(|&first| parameters.get(first))
                                .map_or_else(String::new, |first| first.url.clone());
                            return Err(SearchError::Ambiguous {
                                package: model.package_name.clone(),
                                resource: key.0,
                                code: key.1,
                                first,
                                second: parameter.url.clone(),
                            });
                        }
                        None => {}
                    }
                    depth_of.insert(parameter.code.as_str(), depth);
                    by_resource.insert(key, index);
                }
            }
        }
        Ok(Self {
            parameters,
            by_resource,
        })
    }

    /// Every `search-param-type` code the registry uses.
    #[must_use]
    pub fn type_codes(&self) -> BTreeSet<&str> {
        self.parameters
            .iter()
            .map(|parameter| parameter.type_code.as_str())
            .collect()
    }
}

/// The codes a package defines twice on one base type, as package, base and
/// code; the lookup by resource type and code names neither definition.
///
/// The R4B, R5 and R6 core packages define `_filter` on `Resource` both as
/// `SearchParameter/filter` (type `special`) and as
/// `SearchParameter/Resource-filter` (type `token`), and nothing in either
/// resource says which one stands. Any other such pair fails the emit.
pub const TOLERATED_AMBIGUITIES: [(&str, &str, &str); 3] = [
    ("hl7.fhir.r4b.core", "Resource", "_filter"),
    ("hl7.fhir.r5.core", "Resource", "_filter"),
    ("hl7.fhir.r6.core", "Resource", "_filter"),
];

/// Whether `package` is known to define `code` twice on `base`.
fn is_tolerated(package: &str, base: &str, code: &str) -> bool {
    TOLERATED_AMBIGUITIES.contains(&(package, base, code))
}

/// The FHIR names of `resource` and of every type it specializes, nearest
/// first (`Condition`, `DomainResource`, `Resource`).
fn lineage(model: &VersionModule, resource: &TypeDef) -> Vec<String> {
    let mut names = vec![resource.path.clone()];
    let mut next = resource.base.as_deref();
    while let Some(base) = next {
        let Some(ty) = model
            .types
            .get(base)
            .or_else(|| model.abstract_types.get(base))
        else {
            break;
        };
        if names.contains(&ty.path) {
            break;
        }
        names.push(ty.path.clone());
        next = ty.base.as_deref();
    }
    names
}

/// The Rust variant name of a `search-param-type` code.
fn variant(code: &str) -> String {
    crate::naming::type_name(code)
}

/// Renders the crate-level `search.rs`: the registry's shape and the
/// `search-param-type` enum over the codes every version uses.
///
/// # Errors
///
/// Returns [`fmt::Error`] only if writing to the string fails, which `String` never does.
pub fn render_shared(banner: &str, codes: &BTreeSet<&str>) -> Result<String, fmt::Error> {
    let mut out = banner.to_owned();
    out.push_str(include_str!("templates/search.rs"));
    out.push_str(
        "\n/// `SearchParameter.type`, a code of `search-param-type`\n/// (<https://hl7.org/fhir/R4/valueset-search-param-type.html>), over the codes\n/// every emitted version uses.\n#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\npub enum SearchParamType {\n",
    );
    for code in codes {
        writeln!(out, "    /// `{code}`.\n    {},", variant(code))?;
    }
    out.push_str("}\n\nimpl SearchParamType {\n    /// Returns the code.\n    #[must_use]\n    pub const fn code(self) -> &'static str {\n        match self {\n");
    for code in codes {
        writeln!(out, "            Self::{} => {code:?},", variant(code))?;
    }
    out.push_str("        }\n    }\n}\n");
    Ok(out)
}

/// Renders the `search_parameters.rs` module of `model`.
///
/// # Errors
///
/// Returns [`fmt::Error`] only if writing to the string fails, which `String` never does.
pub fn render(model: &VersionModule, registry: &Registry) -> Result<String, fmt::Error> {
    let mut out = crate::render::banner(model);
    writeln!(
        out,
        "//! The search parameters `{}` {} defines, and the standard parameter each\n//! resource type answers by code.\n",
        model.package_name, model.package_version,
    )?;
    out.push_str(
        "use super::super::search::{SearchParamType, SearchParameter, SearchParameters};\n\n",
    );
    out.push_str("/// The version's search parameter registry.\npub static SEARCH_PARAMETERS: SearchParameters = SearchParameters {\n    parameters: &[\n");
    for parameter in &registry.parameters {
        let base: Vec<String> = parameter.base.iter().map(|b| format!("{b:?}")).collect();
        let expression = parameter
            .expression
            .as_ref()
            .map_or_else(|| String::from("None"), |e| format!("Some({e:?})"));
        writeln!(
            out,
            "        SearchParameter {{ url: {:?}, name: {:?}, code: {:?}, base: &[{}], r#type: SearchParamType::{}, expression: {expression}, experimental: {} }},",
            parameter.url,
            parameter.name,
            parameter.code,
            base.join(", "),
            variant(&parameter.type_code),
            parameter.experimental
        )?;
    }
    out.push_str("    ],\n    by_resource: &[\n");
    for ((resource, code), index) in &registry.by_resource {
        writeln!(out, "        ({resource:?}, {code:?}, {index}),")?;
    }
    out.push_str("    ],\n};\n");
    Ok(out)
}
