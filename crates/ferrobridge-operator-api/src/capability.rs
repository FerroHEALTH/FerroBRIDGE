// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the FHIR facade's `CapabilityStatement` declares, reduced to what an
//! operator reads of it: the FHIR version and, per resource type, its
//! profiles, interactions, search parameters and operations.
//!
//! The bridge answers `GET /fhir/metadata` with an R4 `CapabilityStatement`
//! (FHIR R4 `capabilitystatement.html`). The console's server reads it whole
//! and reduces it to [`Capability`], so a page drawn in the browser reads the
//! facts the operator console shows without the FHIR model behind them. No
//! specification governs the reduction: our own design.

use serde::{Deserialize, Serialize};

/// What a `CapabilityStatement` declares, as the operator console draws it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    /// `CapabilityStatement.fhirVersion`, when it carries one.
    pub fhir_version: Option<String>,
    /// Every `CapabilityStatement.rest.resource`, in the statement's order.
    pub resources: Vec<Resource>,
}

/// One resource type a `CapabilityStatement` declares
/// (`CapabilityStatement.rest.resource`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    /// `resource.type`; empty when the statement carries none.
    pub r#type: String,
    /// How many profiles `resource.supportedProfile` names.
    pub supported_profiles: u32,
    /// Each `resource.interaction.code`, in order.
    pub interactions: Vec<String>,
    /// Each `resource.searchParam.name`, in order.
    pub search_params: Vec<String>,
    /// Each `resource.operation.name`, in order, without its `$`.
    pub operations: Vec<String>,
}

impl Capability {
    /// Returns the resource types that declare the operation `name`, in the
    /// statement's order.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrobridge_operator_api::capability::{Capability, Resource};
    ///
    /// let capability = Capability {
    ///     fhir_version: Some(String::from("4.0.1")),
    ///     resources: vec![
    ///         Resource {
    ///             r#type: String::from("Condition"),
    ///             operations: vec![String::from("validate")],
    ///             ..Resource::default()
    ///         },
    ///         Resource {
    ///             r#type: String::from("Patient"),
    ///             ..Resource::default()
    ///         },
    ///     ],
    /// };
    /// assert_eq!(vec!["Condition"], capability.declaring("validate"));
    /// ```
    #[must_use]
    pub fn declaring(&self, name: &str) -> Vec<&str> {
        self.resources
            .iter()
            .filter(|resource| {
                resource
                    .operations
                    .iter()
                    .any(|operation| operation == name)
            })
            .map(|resource| resource.r#type.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Capability, Resource};

    #[test]
    fn a_capability_crosses_as_json_and_refuses_an_unknown_field() {
        let capability = Capability {
            fhir_version: Some(String::from("4.0.1")),
            resources: vec![Resource {
                r#type: String::from("Condition"),
                supported_profiles: 1,
                interactions: vec![String::from("read")],
                search_params: vec![String::from("patient")],
                operations: vec![String::from("validate")],
            }],
        };
        let text = serde_json::to_string(&capability).expect("it serializes");
        assert_eq!(
            capability,
            serde_json::from_str::<Capability>(&text).expect("it reads back")
        );
        assert!(
            serde_json::from_str::<Capability>(r#"{"fhir_version":null,"resources":[],"rest":[]}"#)
                .is_err()
        );
    }
}
