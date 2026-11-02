// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The events lane: the change feed republished to an AMQP broker, against
//! a stub CDR and a recording broker, and against a real `RabbitMQ` in the
//! container lane.
//!
//! Every body is synthetic: identifiers and instants invented for the test.

mod boot;
mod broker;
mod feed;

use std::sync::Mutex;
use std::sync::PoisonError;

use ferrobridge_server::events::Publish;
use ferrobridge_server::events::PublishError;

/// A broker that records each confirmed message, and refuses every publish
/// from a chosen count on.
#[derive(Debug, Default)]
pub(crate) struct Recording {
    /// The routing keys and bodies confirmed, in order.
    confirmed: Mutex<Vec<(String, serde_json::Value)>>,
    /// How many publishes are confirmed before the broker refuses.
    accepts: Option<usize>,
}

impl Recording {
    /// Returns a broker that confirms `count` publishes and refuses the
    /// rest.
    pub(crate) fn accepting(count: usize) -> Self {
        Self {
            accepts: Some(count),
            ..Self::default()
        }
    }

    /// Returns the messages confirmed.
    pub(crate) fn confirmed(&self) -> Vec<(String, serde_json::Value)> {
        self.confirmed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl Publish for Recording {
    async fn publish(&self, routing_key: &str, body: &[u8]) -> Result<(), PublishError> {
        let mut confirmed = self
            .confirmed
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self
            .accepts
            .is_some_and(|accepts| confirmed.len() >= accepts)
        {
            return Err(PublishError::Nack {
                routing_key: routing_key.to_owned(),
            });
        }
        let body = serde_json::from_slice(body).map_err(PublishError::Body)?;
        confirmed.push((routing_key.to_owned(), body));
        Ok(())
    }
}
