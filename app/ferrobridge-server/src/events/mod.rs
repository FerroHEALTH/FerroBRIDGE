// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The events lane: every commit event the change feed reads, republished to
//! an AMQP 0.9.1 topic exchange.
//!
//! No specification governs eventing: our own design. The lane is a consumer
//! of the change feed ([`crate::change_feed`]), so it runs on either source
//! and its watermark moves only after the broker confirmed every message of
//! an event: delivery is at least once, and a consumer deduplicates on
//! `contribution_id` and `version_index`. Each changed version is one message
//! under its own routing key, `<kind>.<change_type>.<template_id>`, the
//! grammar the reference CDR published under before it handed broker
//! delivery to the bridge, so a queue bound against it keeps matching. The
//! body is the event's envelope with the `version_index` of the version it
//! names: identifiers and provenance, never clinical content.

pub mod amqp;
pub mod report;

use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;

use crate::change_feed::Event;
use crate::change_feed::ReaderName;
use crate::change_feed::follow::Consumer;
use crate::change_feed::follow::ConsumerError;

/// The routing-key word for an absent value.
pub const ABSENT: &str = "-";

/// The name the lane reports to readiness and the logs under.
pub const NAME: &str = "events";

/// The events lane, resolved from `[events]`.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The reader name the lane follows the change feed under.
    pub reader: ReaderName,
    /// The broker URL, with the TLS upgrade applied.
    pub url: SecretString,
    /// The topic exchange.
    pub exchange: String,
    /// The durable queues, each with its binding key.
    pub queues: Vec<Queue>,
    /// How long one publish waits for its confirm.
    pub confirm_timeout: Duration,
}

/// One durable queue bound to the exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    /// The queue name.
    pub name: String,
    /// The topic binding key.
    pub binding: String,
}

/// Why one message did not reach the broker.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PublishError {
    /// The AMQP transport failed: the connection, the channel, a
    /// declaration or the publish.
    #[error("the AMQP broker could not be reached")]
    Amqp(#[source] lapin::Error),
    /// The broker refused the message (a negative confirm).
    #[error("the broker refused the message under {routing_key}")]
    Nack {
        /// The routing key it was published under.
        routing_key: String,
    },
    /// The broker sent no confirm in time.
    #[error("the broker sent no confirm within {0:?}")]
    Timeout(Duration),
    /// The message body could not be written.
    #[error("the event body could not be written")]
    Body(#[source] serde_json::Error),
}

/// The broker seam: one publish that resolves once the broker confirmed it.
#[async_trait::async_trait]
pub trait Publish: std::fmt::Debug + Send + Sync {
    /// Publishes `body` under `routing_key` and waits for the confirm.
    ///
    /// # Errors
    /// Returns [`PublishError`] when the broker did not confirm the message.
    async fn publish(&self, routing_key: &str, body: &[u8]) -> Result<(), PublishError>;
}

/// One message of an event: the routing key and the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The routing key.
    pub routing_key: String,
    /// The JSON body.
    pub body: Vec<u8>,
}

/// Returns the routing key of one version: `<kind>.<change_type>.<template>`.
///
/// AMQP topic keys use `.` between words, so every character of a value
/// outside `A-Z`, `a-z`, `0-9`, `_` and `-` becomes `_`, and an absent or
/// empty value is [`ABSENT`]; the key always has three words.
#[must_use]
pub fn routing_key(kind: &str, change_type: &str, template_id: Option<&str>) -> String {
    format!(
        "{}.{}.{}",
        word(Some(kind)),
        word(Some(change_type)),
        word(template_id)
    )
}

/// Returns `value` as one routing-key word.
fn word(value: Option<&str>) -> String {
    value.filter(|text| !text.is_empty()).map_or_else(
        || ABSENT.to_owned(),
        |text| {
            text.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        },
    )
}

/// Returns the messages of `event`, one per changed version, in order.
///
/// # Errors
/// Returns [`PublishError::Body`] when a body cannot be written.
pub fn messages(event: &Event) -> Result<Vec<Message>, PublishError> {
    event
        .versions
        .iter()
        .enumerate()
        .map(|(index, version)| {
            let mut body = event.envelope.clone();
            body.insert(
                String::from("version_index"),
                serde_json::Value::from(index),
            );
            Ok(Message {
                routing_key: routing_key(
                    &version.kind,
                    &version.change_type,
                    version.template_id.as_deref(),
                ),
                body: serde_json::to_vec(&body).map_err(PublishError::Body)?,
            })
        })
        .collect()
}

/// The change-feed consumer that publishes every event.
#[derive(Debug)]
pub struct Publisher {
    /// The broker.
    broker: Arc<dyn Publish>,
    /// The window the published messages are counted in, when reported.
    /// The status the delivered messages are counted in, when reported.
    following: Option<crate::change_feed::health::Following>,
    usage: Option<Arc<ferrohealth_usage_report::window::Window>>,
}

impl Publisher {
    /// Returns the consumer publishing through `broker`.
    #[must_use]
    pub fn new(broker: Arc<dyn Publish>) -> Self {
        Self {
            broker,
            usage: None,
            following: None,
        }
    }

    /// Returns this consumer counting each published message in the status
    /// `following` holds.
    #[must_use]
    pub fn with_following(mut self, following: crate::change_feed::health::Following) -> Self {
        self.following = Some(following);
        self
    }

    /// Returns this consumer counting each published message in `usage`.
    #[must_use]
    pub fn with_usage(mut self, usage: Arc<ferrohealth_usage_report::window::Window>) -> Self {
        self.usage = Some(usage);
        self
    }
}

#[async_trait::async_trait]
impl Consumer for Publisher {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        // NOTE: no specification governs this: our own design; a body that cannot be
        // written fails the same way on every offer, so it is the event's own fault.
        let messages = messages(event).map_err(ConsumerError::poisoned)?;
        for message in &messages {
            self.broker
                .publish(&message.routing_key, &message.body)
                .await
                .map_err(ConsumerError::transient)?;
            if let Some(usage) = self.usage.as_ref() {
                crate::usage_report::record_event_published(usage);
            }
            if let Some(following) = self.following.as_ref() {
                following.delivered();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::routing_key;

    #[test]
    fn a_routing_key_has_three_words() {
        assert_eq!(
            "COMPOSITION.249.template_x",
            routing_key("COMPOSITION", "249", Some("template_x"))
        );
    }

    #[test]
    fn an_absent_or_empty_template_is_a_dash() {
        assert_eq!("EHR_STATUS.251.-", routing_key("EHR_STATUS", "251", None));
        assert_eq!("FOLDER.523.-", routing_key("FOLDER", "523", Some("")));
        assert_eq!("EHR.erase.-", routing_key("EHR", "erase", None));
    }

    #[test]
    fn a_dotted_template_stays_one_word() {
        assert_eq!(
            "COMPOSITION.249.openEHR-EHR-COMPOSITION_encounter_v1",
            routing_key(
                "COMPOSITION",
                "249",
                Some("openEHR-EHR-COMPOSITION.encounter.v1")
            )
        );
    }
}
