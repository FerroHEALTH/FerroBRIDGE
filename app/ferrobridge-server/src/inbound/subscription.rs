// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R4 Subscription notifications as the Subscriptions R5 Backport IG
//! STU 1.1.0 defines them.
//!
//! A notification is a `history` Bundle whose first entry is the
//! subscription status, a `Parameters` resource in R4
//! (`backport-subscription-status-r4`), and whose other entries carry the
//! resources a `full-resource` payload sends (notifications.html). The
//! reader takes what the intake needs: the subscription, its status, the
//! notification type, the event count since the subscription started, and
//! each event's number, focus and carried resource. A Bundle that breaks a
//! rule of the profile is refused, naming the rule and no value.

use serde_json::Value;

/// The notification types of `SubscriptionNotificationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    /// The channel is being tested at the subscription's start.
    Handshake,
    /// The subscription is alive and has sent nothing since the last one.
    Heartbeat,
    /// One or more events.
    EventNotification,
    /// An answer to a status query.
    QueryStatus,
    /// An answer to an `$events` query.
    QueryEvent,
}

impl NotificationType {
    /// Returns the type `code` names.
    fn of(code: &str) -> Option<Self> {
        Some(match code {
            "handshake" => Self::Handshake,
            "heartbeat" => Self::Heartbeat,
            "event-notification" => Self::EventNotification,
            "query-status" => Self::QueryStatus,
            "query-event" => Self::QueryEvent,
            _ => return None,
        })
    }
}

/// One event of a notification.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationEvent {
    /// The event number, counted from the subscription's start.
    pub number: u64,
    /// The reference of the resource the event is about; absent in an
    /// `empty` payload.
    pub focus: Option<String>,
    /// The resource itself, when a `full-resource` payload carries it.
    pub resource: Option<Value>,
}

/// A notification, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    /// The reference of the subscription, as the status names it.
    pub subscription: String,
    /// The subscription's status code.
    pub status: String,
    /// The notification type.
    pub kind: NotificationType,
    /// The events the subscription has raised since it started, when sent.
    pub events_since_start: Option<u64>,
    /// The events the notification carries.
    pub events: Vec<NotificationEvent>,
}

/// The rule of the profile a Bundle breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Malformed {
    /// The body is no `history` Bundle.
    #[error("the notification is no history Bundle")]
    NotHistory,
    /// The first entry is no `Parameters` status.
    #[error("the first entry is no Parameters subscription status")]
    NoStatus,
    /// A parameter the profile requires once is absent or repeated.
    #[error("the status carries no single {0} parameter")]
    Parameter(&'static str),
    /// A code or a number does not read.
    #[error("the status parameter {0} does not read")]
    Value(&'static str),
}

/// Returns the one parameter named `name` among `parameters`, `None` when
/// absent, refused when repeated.
fn single<'a>(parameters: &'a [Value], name: &'static str) -> Result<Option<&'a Value>, Malformed> {
    let mut found = parameters
        .iter()
        .filter(|parameter| parameter.get("name").and_then(Value::as_str) == Some(name));
    let first = found.next();
    if found.next().is_some() {
        return Err(Malformed::Parameter(name));
    }
    Ok(first)
}

/// Returns the event number `text` writes.
fn number(text: Option<&str>, name: &'static str) -> Result<u64, Malformed> {
    text.and_then(|text| text.parse().ok())
        .ok_or(Malformed::Value(name))
}

/// Reads the notification `body` carries.
///
/// # Errors
/// Returns [`Malformed`] for a Bundle that breaks a rule of the R4
/// subscription status and notification profiles.
pub fn read(body: &Value) -> Result<Notification, Malformed> {
    if body.get("resourceType").and_then(Value::as_str) != Some("Bundle")
        || body.get("type").and_then(Value::as_str) != Some("history")
    {
        return Err(Malformed::NotHistory);
    }
    let entries = body
        .get("entry")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(Malformed::NoStatus)?;
    let (status, carried) = entries.split_first().ok_or(Malformed::NoStatus)?;
    let status = status
        .get("resource")
        .filter(|resource| {
            resource.get("resourceType").and_then(Value::as_str) == Some("Parameters")
        })
        .ok_or(Malformed::NoStatus)?;
    let parameters = status
        .get("parameter")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(Malformed::Parameter("subscription"))?;
    let reference = |parameter: &Value| {
        parameter
            .pointer("/valueReference/reference")
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let subscription = single(parameters, "subscription")?
        .and_then(reference)
        .ok_or(Malformed::Parameter("subscription"))?;
    let status_code = single(parameters, "status")?
        .and_then(|parameter| parameter.get("valueCode"))
        .and_then(Value::as_str)
        .ok_or(Malformed::Parameter("status"))?
        .to_owned();
    let kind = single(parameters, "type")?
        .and_then(|parameter| parameter.get("valueCode"))
        .and_then(Value::as_str)
        .ok_or(Malformed::Parameter("type"))
        .and_then(|code| NotificationType::of(code).ok_or(Malformed::Value("type")))?;
    let events_since_start = single(parameters, "events-since-subscription-start")?
        .map(|parameter| {
            number(
                parameter.get("valueString").and_then(Value::as_str),
                "events-since-subscription-start",
            )
        })
        .transpose()?;
    let events = parameters
        .iter()
        .filter(|parameter| {
            parameter.get("name").and_then(Value::as_str) == Some("notification-event")
        })
        .map(|parameter| {
            let parts = parameter
                .get("part")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .ok_or(Malformed::Parameter("event-number"))?;
            let number = number(
                single(parts, "event-number")?
                    .and_then(|part| part.get("valueString"))
                    .and_then(Value::as_str),
                "event-number",
            )?;
            let focus = single(parts, "focus")?.and_then(reference);
            let resource = focus.as_deref().and_then(|focus| {
                carried
                    .iter()
                    .find(|entry| entry.get("fullUrl").and_then(Value::as_str) == Some(focus))
                    .and_then(|entry| entry.get("resource"))
                    .cloned()
            });
            Ok(NotificationEvent {
                number,
                focus,
                resource,
            })
        })
        .collect::<Result<Vec<_>, Malformed>>()?;
    Ok(Notification {
        subscription,
        status: status_code,
        kind,
        events_since_start,
        events,
    })
}

/// Returns the patient an event is about: the focus when it is a
/// `Patient`, else the `subject` or `patient` reference of the carried
/// resource; `None` when only a read of the focus can tell.
#[must_use]
pub fn patient_of(event: &NotificationEvent) -> Option<String> {
    let focus = event.focus.as_deref()?;
    if is_patient(focus) {
        return Some(focus.to_owned());
    }
    let resource = event.resource.as_ref()?;
    ["/subject/reference", "/patient/reference"]
        .iter()
        .find_map(|pointer| resource.pointer(pointer).and_then(Value::as_str))
        .filter(|reference| is_patient(reference))
        .map(str::to_owned)
}

/// Returns whether `reference` names a `Patient` (FHIR R4 references.html
/// §2.3.0.1, a relative or absolute literal reference).
fn is_patient(reference: &str) -> bool {
    let mut segments = reference.rsplit('/');
    let _id = segments.next();
    segments.next() == Some("Patient")
}
