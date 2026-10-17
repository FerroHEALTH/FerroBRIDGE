//! The search parameter registry of a FHIR version: every `SearchParameter`
//! its core package defines, and the standard parameter each resource type
//! answers by code (<https://hl7.org/fhir/R4/searchparameter.html>).
//!
//! A resource type answers the parameters whose `base` names it or a type it
//! specializes, so `_id`, defined on `Resource`, is a parameter of every
//! resource type (<https://hl7.org/fhir/R4/search.html#all>). The
//! specification's example parameters are left out. Every version module
//! emits its own `SEARCH_PARAMETERS` static over these types.

/// One `SearchParameter` (<https://hl7.org/fhir/R4/searchparameter.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchParameter {
    /// `SearchParameter.url`, the canonical a `CapabilityStatement` names
    /// in `searchParam.definition`.
    pub url: &'static str,
    /// `SearchParameter.name`.
    pub name: &'static str,
    /// `SearchParameter.code`, the name a search uses.
    pub code: &'static str,
    /// `SearchParameter.base`, the resource types it applies to.
    pub base: &'static [&'static str],
    /// `SearchParameter.type`.
    pub r#type: SearchParamType,
    /// `SearchParameter.expression`, the FHIRPath that extracts the values.
    pub expression: Option<&'static str>,
    /// `SearchParameter.experimental`.
    pub experimental: bool,
}

/// The search parameter registry of one version.
#[derive(Debug)]
pub struct SearchParameters {
    /// Every parameter, in canonical URL order.
    pub parameters: &'static [SearchParameter],
    /// Every resource type and code with the index of the standard parameter
    /// it answers in [`Self::parameters`], in type and code order.
    pub by_resource: &'static [(&'static str, &'static str, usize)],
}

impl SearchParameters {
    /// Returns the standard search parameter `code` of `resource_type`, its
    /// own or one a type it specializes defines.
    #[must_use]
    pub fn of(&self, resource_type: &str, code: &str) -> Option<&SearchParameter> {
        let found = self
            .by_resource
            .binary_search_by(|(entry_type, entry_code, _)| {
                (*entry_type, *entry_code).cmp(&(resource_type, code))
            })
            .ok()?;
        let (_, _, index) = self.by_resource.get(found)?;
        self.parameters.get(*index)
    }

    /// Returns the parameter whose canonical is `url`.
    #[must_use]
    pub fn by_url(&self, url: &str) -> Option<&SearchParameter> {
        let found = self
            .parameters
            .binary_search_by(|parameter| parameter.url.cmp(url))
            .ok()?;
        self.parameters.get(found)
    }

    /// Returns every standard parameter of `resource_type`, in code order.
    pub fn of_type<'a>(
        &'a self,
        resource_type: &'a str,
    ) -> impl Iterator<Item = &'a SearchParameter> + 'a {
        self.by_resource
            .iter()
            .filter(move |(entry_type, _, _)| *entry_type == resource_type)
            .filter_map(|(_, _, index)| self.parameters.get(*index))
    }
}
