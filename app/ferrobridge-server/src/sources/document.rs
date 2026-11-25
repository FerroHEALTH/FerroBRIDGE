// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document of each kind of source, as a write is validated against it.
//!
//! A document names its secrets (`{"secret": "<name>"}`), which resolve
//! through the `[secrets]` directory when used, so a value never sits in a
//! row, a log or a `Debug` output. A verifier takes a list of keys, so a key
//! is rotated by adding the new one before removing the old. No
//! specification governs a bridge's source documents: our own design.

use std::collections::BTreeSet;
use std::net::IpAddr;

use serde::Deserialize;

/// A secret, by its name in the `[secrets]` directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRef {
    /// The name.
    pub secret: String,
}

/// One tenant of a webhook or a FHIR Subscription source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tenant {
    /// The tenant as the source names it.
    #[serde(rename = "tenant")]
    pub name: String,
    /// The identifier system the tenant's local patient identifiers are
    /// written in.
    pub identifier_system: String,
    /// The secret the tenant's credentials are read from, when it has its
    /// own.
    pub credentials: Option<SecretRef>,
    /// Whether the tenant is served.
    pub enabled: bool,
}

impl Default for Tenant {
    fn default() -> Self {
        Self {
            name: String::new(),
            identifier_system: String::new(),
            credentials: None,
            enabled: true,
        }
    }
}

/// How a webhook proves a notification is the source's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verify {
    /// The scheme: `hmac-sha256` over the raw body.
    pub scheme: Scheme,
    /// The header the signature travels in.
    pub header: String,
    /// The keys, any of which verifies, newest first.
    pub keys: Vec<SecretRef>,
}

/// The signature schemes a webhook verifier knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    /// HMAC-SHA-256 over the raw body (RFC 2104).
    HmacSha256,
}

/// A webhook source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Webhook {
    /// How a notification is verified.
    pub verify: Verify,
    /// The tenants.
    #[serde(default)]
    pub tenants: Vec<Tenant>,
}

/// A header a FHIR Subscription sender authenticates with
/// (`Subscription.channel.header`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderAuth {
    /// The header's name.
    pub name: String,
    /// The values it may carry, any of which admits, newest first.
    pub keys: Vec<SecretRef>,
}

/// A FHIR Subscription source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FhirSubscription {
    /// The header the sender authenticates with, absent when the TLS client
    /// certificate alone admits it.
    #[serde(default)]
    pub header: Option<HeaderAuth>,
    /// The tenants.
    #[serde(default)]
    pub tenants: Vec<Tenant>,
}

/// One route of an MLLP source: the sender by MSH-3 and MSH-4, bound to the
/// connection it may arrive on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// MSH-3, the sending application, as its namespace id.
    pub sending_application: String,
    /// MSH-4, the sending facility, as its namespace id.
    pub sending_facility: String,
    /// The subject a TLS connection's client certificate names.
    #[serde(default)]
    pub certificate_subject: Option<String>,
    /// The networks a plain connection's peer lies in.
    #[serde(default)]
    pub source_cidrs: Vec<String>,
}

/// An MLLP source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mllp {
    /// The routes.
    pub routes: Vec<Route>,
}

/// A document of one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    /// A webhook.
    Webhook(Webhook),
    /// A FHIR Subscription.
    FhirSubscription(FhirSubscription),
    /// An MLLP sender.
    Mllp(Mllp),
}

/// Why a document was refused, in words that quote no secret value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Invalid(pub String);

impl Document {
    /// Returns the document of `kind` that `value` holds.
    ///
    /// # Errors
    /// Returns [`Invalid`] when `value` is no document of `kind` or breaks
    /// one of its rules.
    pub fn read(
        kind: ferrobridge_operator_api::sources::Kind,
        value: &serde_json::Value,
    ) -> Result<Self, Invalid> {
        use ferrobridge_operator_api::sources::Kind;

        let read = |error: serde_json::Error| {
            Invalid(format!(
                "the {} document does not read: {error}",
                kind.as_str()
            ))
        };
        let document = match kind {
            Kind::Webhook => Self::Webhook(Webhook::deserialize(value).map_err(read)?),
            Kind::FhirSubscription => {
                Self::FhirSubscription(FhirSubscription::deserialize(value).map_err(read)?)
            }
            Kind::Mllp => Self::Mllp(Mllp::deserialize(value).map_err(read)?),
        };
        document.check()?;
        Ok(document)
    }

    /// Refuses a document that breaks a rule its shape cannot carry.
    fn check(&self) -> Result<(), Invalid> {
        match self {
            Self::Webhook(webhook) => {
                if webhook.verify.header.trim().is_empty() {
                    return Err(Invalid(String::from("verify.header names no header")));
                }
                if webhook.verify.keys.is_empty() {
                    return Err(Invalid(String::from("verify.keys names no key")));
                }
                tenants(&webhook.tenants)
            }
            Self::FhirSubscription(subscription) => {
                if let Some(header) = &subscription.header
                    && (header.name.trim().is_empty() || header.keys.is_empty())
                {
                    return Err(Invalid(String::from("header names no header or no key")));
                }
                tenants(&subscription.tenants)
            }
            Self::Mllp(mllp) => routes(&mllp.routes),
        }
    }

    /// Returns every secret the document names, sorted.
    #[must_use]
    pub fn secrets(&self) -> BTreeSet<&str> {
        fn tenants(tenants: &[Tenant]) -> impl Iterator<Item = &str> {
            tenants
                .iter()
                .filter_map(|tenant| tenant.credentials.as_ref())
                .map(|secret| secret.secret.as_str())
        }
        match self {
            Self::Webhook(webhook) => webhook
                .verify
                .keys
                .iter()
                .map(|key| key.secret.as_str())
                .chain(tenants(&webhook.tenants))
                .collect(),
            Self::FhirSubscription(subscription) => subscription
                .header
                .iter()
                .flat_map(|header| header.keys.iter().map(|key| key.secret.as_str()))
                .chain(tenants(&subscription.tenants))
                .collect(),
            Self::Mllp(_) => BTreeSet::new(),
        }
    }

    /// Returns the tenants the document declares.
    #[must_use]
    pub fn tenants(&self) -> &[Tenant] {
        match self {
            Self::Webhook(webhook) => &webhook.tenants,
            Self::FhirSubscription(subscription) => &subscription.tenants,
            Self::Mllp(_) => &[],
        }
    }

    /// Returns the MLLP routes the document declares.
    #[must_use]
    pub fn routes(&self) -> &[Route] {
        match self {
            Self::Mllp(mllp) => &mllp.routes,
            Self::Webhook(_) | Self::FhirSubscription(_) => &[],
        }
    }
}

/// Refuses an empty or repeated tenant, or one without an identifier system.
fn tenants(tenants: &[Tenant]) -> Result<(), Invalid> {
    let mut seen = BTreeSet::new();
    for tenant in tenants {
        if tenant.name.is_empty() || tenant.identifier_system.trim().is_empty() {
            return Err(Invalid(String::from(
                "a tenant names no tenant or no identifier system",
            )));
        }
        if !seen.insert(tenant.name.as_str()) {
            return Err(Invalid(format!(
                "the tenant {} is named twice",
                tenant.name
            )));
        }
    }
    Ok(())
}

/// Refuses an MLLP source without routes, a route without a sender, one bound
/// to neither a certificate subject nor a network, a network that does not
/// read, and a sender routed twice.
fn routes(routes: &[Route]) -> Result<(), Invalid> {
    if routes.is_empty() {
        return Err(Invalid(String::from(
            "an MLLP source names at least one route",
        )));
    }
    let mut seen = BTreeSet::new();
    for route in routes {
        if route.sending_application.is_empty() || route.sending_facility.is_empty() {
            return Err(Invalid(String::from(
                "a route names its sending application and facility",
            )));
        }
        if route
            .certificate_subject
            .as_deref()
            .is_none_or(str::is_empty)
            && route.source_cidrs.is_empty()
        {
            return Err(Invalid(format!(
                "the route {}^{} is bound to no certificate subject and no network",
                route.sending_application, route.sending_facility
            )));
        }
        for network in &route.source_cidrs {
            if cidr(network).is_none() {
                return Err(Invalid(format!("{network} is no network in CIDR notation")));
            }
        }
        if !seen.insert((
            route.sending_application.as_str(),
            route.sending_facility.as_str(),
        )) {
            return Err(Invalid(format!(
                "the route {}^{} is named twice",
                route.sending_application, route.sending_facility
            )));
        }
    }
    Ok(())
}

/// Returns the network `text` names in CIDR notation, `None` when it names
/// none.
#[must_use]
pub fn cidr(text: &str) -> Option<(IpAddr, u8)> {
    let (address, length) = text.split_once('/')?;
    let address: IpAddr = address.parse().ok()?;
    let length: u8 = length.parse().ok()?;
    let bound = if address.is_ipv4() { 32 } else { 128 };
    (length <= bound).then_some((address, length))
}

#[cfg(test)]
mod tests {
    use ferrobridge_operator_api::sources::Kind;
    use serde_json::json;

    use super::{Document, cidr};

    #[test]
    fn a_webhook_names_its_keys_and_tenants() {
        let document = Document::read(
            Kind::Webhook,
            &json!({
                "verify": {"scheme": "hmac-sha256", "header": "X-Signature", "keys": [{"secret": "b"}, {"secret": "a"}]},
                "tenants": [{"tenant": "t1", "identifier_system": "urn:oid:2.16.840.1.113883.2.4.6.3", "credentials": {"secret": "c"}}]
            }),
        )
        .expect("a webhook");
        assert_eq!(
            vec!["a", "b", "c"],
            document.secrets().into_iter().collect::<Vec<_>>()
        );
        assert!(document.tenants()[0].enabled);
    }

    #[test]
    fn each_broken_rule_is_refused() {
        for (kind, value) in [
            (
                Kind::Webhook,
                json!({"verify": {"scheme": "md5", "header": "X", "keys": [{"secret": "a"}]}}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"scheme": "hmac-sha256", "header": "X", "keys": []}}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"scheme": "hmac-sha256", "header": "X", "keys": [{"secret": "a"}]}, "extra": 1}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"scheme": "hmac-sha256", "header": "X", "keys": [{"secret": "a"}]}, "tenants": [
                {"tenant": "t", "identifier_system": "s"}, {"tenant": "t", "identifier_system": "s"}]}),
            ),
            (Kind::Mllp, json!({"routes": []})),
            (
                Kind::Mllp,
                json!({"routes": [{"sending_application": "A", "sending_facility": "F"}]}),
            ),
            (
                Kind::Mllp,
                json!({"routes": [{"sending_application": "A", "sending_facility": "F", "source_cidrs": ["10.0.0.0/33"]}]}),
            ),
        ] {
            assert!(Document::read(kind, &value).is_err(), "{value}");
        }
    }

    #[test]
    fn a_network_reads_in_cidr_notation() {
        assert!(cidr("10.1.0.0/16").is_some());
        assert!(cidr("2001:db8::/32").is_some());
        assert!(cidr("10.1.0.0").is_none());
        assert!(cidr("2001:db8::/129").is_none());
    }
}
