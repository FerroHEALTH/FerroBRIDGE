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

/// A webhook source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Webhook {
    /// How a notification is verified.
    pub verify: crate::inbound::verify::Verifier,
    /// The keys the verifier checks with, any of which verifies, newest
    /// first; none for `mtls`.
    #[serde(default)]
    pub keys: Vec<SecretRef>,
    /// How a verified notification is read.
    pub events: crate::inbound::event::EventMap,
    /// The tenants.
    #[serde(default)]
    pub tenants: Vec<Tenant>,
    /// The bounds of what the source may send and the engine may do.
    #[serde(default)]
    pub limits: Limits,
    /// The recovery steps: redelivery and reconciliation.
    #[serde(default)]
    pub recovery: crate::inbound::recover::Recovery,
}

/// The bounds of one webhook source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// The largest body read, in bytes, within `[inbound] body_limit`.
    pub body_limit: usize,
    /// The most remote calls in flight for one tenant across every replica.
    pub concurrency: u32,
    /// The most work items waiting before the intake answers `503`.
    pub max_pending: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            body_limit: 64 * 1024,
            concurrency: 4,
            max_pending: 100_000,
        }
    }
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
    /// The header the sender authenticates with (`Subscription.channel.header`).
    #[serde(default)]
    pub header: Option<HeaderAuth>,
    /// The names the sender's verified TLS client certificate must carry,
    /// as the NL Generic Functions notification endpoint requires.
    #[serde(default)]
    pub mtls: Option<crate::inbound::verify::Mtls>,
    /// The tenant each subscription serves, by the subscription reference its
    /// notifications name.
    #[serde(default)]
    pub subscriptions: std::collections::BTreeMap<String, String>,
    /// The FHIR base a relative subscription reference resolves against for
    /// `$events`.
    #[serde(default)]
    pub base: Option<String>,
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

/// What stands behind an MLLP sender.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// A system that names its clinicians.
    #[default]
    Clinician,
    /// An analyser feed with no human, whose results commit as acts of the
    /// device.
    Device,
}

/// What an entry no program maps does to an MLLP sender's message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unmapped {
    /// The entry is skipped with a typed reason, and the others commit.
    #[default]
    SkipAndCount,
    /// The whole message is refused, naming every such entry.
    Refuse,
}

impl From<Unmapped> for crate::facade::ingest::UnmappedEntries {
    fn from(unmapped: Unmapped) -> Self {
        match unmapped {
            Unmapped::SkipAndCount => Self::SkipAndCount,
            Unmapped::Refuse => Self::Refuse,
        }
    }
}

/// An MLLP source: the HL7 v2 senders it routes and how their messages are
/// read and committed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mllp {
    /// The routes.
    pub routes: Vec<Route>,
    /// What stands behind the senders.
    pub origin: Origin,
    /// The `PARTY_REF.namespace` a device's identifiers are issued in,
    /// required with `origin = "device"`.
    pub device_identifier_issuer: Option<String>,
    /// The profile each R4 resource type claims when the message map wrote
    /// none, by resource type.
    pub profiles: std::collections::BTreeMap<String, String>,
    /// What an entry no program maps does to the message.
    pub unmapped_entries: Unmapped,
    /// The EHR policy, `existing` or `create_on_first_write`; absent, the
    /// facade's.
    pub ehr_policy: Option<String>,
    /// The HL7 table 0211 code a message with an empty MSH-18 is read in.
    pub default_charset: String,
    /// The `DV_IDENTIFIER.issuer` of a facility's namespace id; absent, the
    /// identifier names no issuer.
    pub facility_identifier_issuer: Option<String>,
    /// Whether an `AE` or an `AR` also logs the counted outcomes.
    pub log_outcomes: bool,
}

impl Default for Mllp {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            origin: Origin::Clinician,
            device_identifier_issuer: None,
            profiles: std::collections::BTreeMap::new(),
            unmapped_entries: Unmapped::SkipAndCount,
            ehr_policy: None,
            default_charset: String::from("ASCII"),
            facility_identifier_issuer: None,
            log_outcomes: false,
        }
    }
}

impl Mllp {
    /// Returns the character set a message with an empty MSH-18 is read in.
    #[must_use]
    pub fn charset(&self) -> ferrobridge_hl7v2::decode::Charset {
        ferrobridge_hl7v2::decode::Charset::from_code(&self.default_charset)
            .unwrap_or(ferrobridge_hl7v2::decode::Charset::Ascii)
    }

    /// Returns the EHR policy the senders' messages commit under, `None` for
    /// the facade's.
    #[must_use]
    pub fn policy(&self) -> Option<crate::identity::ehr::Policy> {
        match self.ehr_policy.as_deref() {
            Some("existing") => Some(crate::identity::ehr::Policy::Existing),
            Some("create_on_first_write") => Some(crate::identity::ehr::Policy::CreateOnFirstWrite),
            _ => None,
        }
    }

    /// Refuses a setting that does not read.
    fn check(&self) -> Result<(), Invalid> {
        routes(&self.routes)?;
        if ferrobridge_hl7v2::decode::Charset::from_code(&self.default_charset).is_none() {
            return Err(Invalid(format!(
                "default_charset {} is no HL7 table 0211 code",
                self.default_charset
            )));
        }
        if self.ehr_policy.is_some() && self.policy().is_none() {
            return Err(Invalid(String::from(
                "ehr_policy is existing or create_on_first_write",
            )));
        }
        let issued = self
            .device_identifier_issuer
            .as_deref()
            .is_some_and(|issuer| !issuer.is_empty());
        if (self.origin == Origin::Device) != issued {
            return Err(Invalid(String::from(
                "device_identifier_issuer is named with origin = device, and only then",
            )));
        }
        for (resource_type, profile) in &self.profiles {
            if !fhir_types::r4::schema::SCHEMAS.is_resource(resource_type) {
                return Err(Invalid(format!("{resource_type} is no R4 resource type")));
            }
            if url::Url::parse(profile).is_err() {
                return Err(Invalid(format!("the profile of {resource_type} is no URL")));
            }
        }
        Ok(())
    }
}

/// A document of one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    /// A webhook.
    Webhook(Box<Webhook>),
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
            Kind::Webhook => Self::Webhook(Box::new(Webhook::deserialize(value).map_err(read)?)),
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
            Self::Webhook(webhook) => webhook.check(),
            Self::FhirSubscription(subscription) => {
                if let Some(header) = &subscription.header
                    && (header.name.trim().is_empty() || header.keys.is_empty())
                {
                    return Err(Invalid(String::from("header names no header or no key")));
                }
                if subscription.header.is_none()
                    && subscription
                        .mtls
                        .as_ref()
                        .is_none_or(|mtls| mtls.subjects.iter().all(|name| name.trim().is_empty()))
                {
                    return Err(Invalid(String::from(
                        "a FHIR Subscription source names a header or mtls subjects",
                    )));
                }
                if let Some(tenant) = subscription.subscriptions.values().find(|tenant| {
                    !subscription
                        .tenants
                        .iter()
                        .any(|known| &known.name == *tenant)
                }) {
                    return Err(Invalid(format!(
                        "subscriptions names the unknown tenant {tenant}"
                    )));
                }
                tenants(&subscription.tenants)
            }
            Self::Mllp(mllp) => mllp.check(),
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
                .keys
                .iter()
                .map(|key| key.secret.as_str())
                .chain(tenants(&webhook.tenants))
                .chain(webhook.recovery.secrets())
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

impl Webhook {
    /// Refuses a webhook whose verifier, keys, event map, tenants or limits
    /// break a rule.
    fn check(&self) -> Result<(), Invalid> {
        use crate::inbound::verify::{Scheme, Verifier};

        let needs_keys = match &self.verify {
            Verifier::Hmac(hmac) => {
                named(&hmac.header, "verify.header")?;
                true
            }
            Verifier::StaticHeader(header) => {
                named(&header.header, "verify.header")?;
                true
            }
            Verifier::StandardWebhooks(standard) => {
                if standard.schemes.is_empty() {
                    return Err(Invalid(String::from("verify.schemes names no scheme")));
                }
                if standard.schemes.contains(&Scheme::V2a)
                    && standard
                        .endpoint_id
                        .as_deref()
                        .is_none_or(|id| id.is_empty() || id.contains('.'))
                {
                    return Err(Invalid(String::from(
                        "the v2a scheme needs an endpoint_id without a '.'",
                    )));
                }
                true
            }
            Verifier::Mtls(mtls) => {
                if mtls
                    .subjects
                    .iter()
                    .all(|subject| subject.trim().is_empty())
                {
                    return Err(Invalid(String::from("verify.subjects names no subject")));
                }
                false
            }
        };
        if needs_keys && self.keys.is_empty() {
            return Err(Invalid(String::from("keys names no key")));
        }
        if !needs_keys && !self.keys.is_empty() {
            return Err(Invalid(String::from("an mtls verifier takes no keys")));
        }
        named(&self.events.entity, "events.entity")?;
        if self.events.tenant.is_none() && self.tenants.len() > 1 {
            return Err(Invalid(String::from(
                "a source with more than one tenant names events.tenant",
            )));
        }
        if self.events.dedupe.headers.is_empty() && self.events.dedupe.pointers.is_empty() {
            return Err(Invalid(String::from(
                "events.dedupe names no header and no pointer",
            )));
        }
        let pointers = self
            .events
            .tenant
            .iter()
            .chain(std::iter::once(&self.events.entity))
            .chain(self.events.stamp.iter())
            .chain(self.events.dedupe.pointers.iter())
            .chain(self.events.rules.iter().flat_map(|rule| rule.when.keys()));
        for pointer in pointers {
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err(Invalid(format!("{pointer} is no JSON Pointer")));
            }
        }
        self.recovery.check().map_err(Invalid)?;
        if self.limits.body_limit == 0
            || self.limits.concurrency == 0
            || self.limits.max_pending == 0
        {
            return Err(Invalid(String::from("a limit is zero")));
        }
        tenants(&self.tenants)
    }
}

/// Refuses `value`, the value of `key`, when it is blank.
fn named(value: &str, key: &str) -> Result<(), Invalid> {
    if value.trim().is_empty() {
        return Err(Invalid(format!("{key} is empty")));
    }
    Ok(())
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
                "verify": {"kind": "hmac", "header": "X-Signature"},
                "keys": [{"secret": "b"}, {"secret": "a"}],
                "events": {"entity": "/id", "dedupe": {"pointers": ["/id"]}},
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
        let events = json!({"entity": "/id", "dedupe": {"pointers": ["/id"]}});
        let hmac = json!({"kind": "hmac", "header": "X"});
        let key = json!([{"secret": "a"}]);
        for (kind, value) in [
            (
                Kind::Webhook,
                json!({"verify": {"kind": "hmac", "algorithm": "md5", "header": "X"}, "keys": key, "events": events}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": [], "events": events}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": events, "extra": 1}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": {"entity": "/id", "tenant": "/t", "dedupe": {"pointers": ["/id"]}}, "tenants": [
                {"tenant": "t", "identifier_system": "s"}, {"tenant": "t", "identifier_system": "s"}]}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": events, "tenants": [
                {"tenant": "t", "identifier_system": "s"}, {"tenant": "u", "identifier_system": "s"}]}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"kind": "hmac", "header": " "}, "keys": key, "events": events}),
            ),
            (Kind::Webhook, json!({"verify": hmac, "keys": key})),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": {"entity": "/id"}}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": {"entity": "id", "dedupe": {"pointers": ["/id"]}}}),
            ),
            (
                Kind::Webhook,
                json!({"verify": hmac, "keys": key, "events": events, "limits": {"concurrency": 0}}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"kind": "standard-webhooks", "schemes": ["v2a"]}, "keys": key, "events": events}),
            ),
            (
                Kind::Webhook,
                json!({"verify": {"kind": "mtls", "subjects": ["CN=a"]}, "keys": key, "events": events}),
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
