// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AMQP 0.9.1 broker, reached through `lapin` with publisher confirms.
//!
//! No specification governs the lane: our own design. The connection opens
//! on the first publish and again after any failure, so a broker that is
//! down at boot is tolerated and the events wait at the change-feed source.
//! Each fresh connection declares the topology `[events]` names: the durable
//! topic exchange, then each durable queue and its binding. Declaring an
//! existing exchange or queue with the same arguments changes nothing
//! (<https://www.rabbitmq.com/docs/queues#property-equivalence>). A message
//! is persistent, and a publish resolves only on the broker's confirm
//! (<https://www.rabbitmq.com/docs/confirms#publisher-confirms>).

use std::time::Duration;

use lapin::BasicProperties;
use lapin::Channel;
use lapin::Connection;
use lapin::ConnectionProperties;
use lapin::ExchangeKind;
use lapin::options::BasicPublishOptions;
use lapin::options::ConfirmSelectOptions;
use lapin::options::ExchangeDeclareOptions;
use lapin::options::QueueBindOptions;
use lapin::options::QueueDeclareOptions;
use lapin::types::FieldTable;
use secrecy::ExposeSecret;
use tokio::sync::Mutex;

use crate::events::Publish;
use crate::events::PublishError;
use crate::events::Settings;

/// The AMQP delivery mode of a persistent message.
const PERSISTENT: u8 = 2;

/// A broker the lane publishes to, connected on demand.
#[derive(Debug)]
pub struct AmqpBroker {
    /// The lane's settings: the URL, the exchange, the queues and the confirm
    /// timeout.
    settings: Settings,
    /// The open connection and its confirming channel; the connection is
    /// kept so its I/O lives as long as the channel.
    open: Mutex<Option<(Connection, Channel)>>,
}

impl AmqpBroker {
    /// Returns the broker `settings` describes; nothing is opened yet.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            open: Mutex::new(None),
        }
    }

    /// Returns a confirming channel, opening the connection and declaring
    /// the topology when none is open.
    async fn channel(&self) -> Result<Channel, PublishError> {
        let mut open = self.open.lock().await;
        if let Some((_connection, channel)) = open.as_ref()
            && channel.status().connected()
        {
            return Ok(channel.clone());
        }
        *open = None;
        let connection = Connection::connect(
            self.settings.url.expose_secret(),
            ConnectionProperties::default(),
        )
        .await
        .map_err(PublishError::Amqp)?;
        let channel = connection
            .create_channel()
            .await
            .map_err(PublishError::Amqp)?;
        self.declare(&channel).await?;
        let handle = channel.clone();
        *open = Some((connection, channel));
        tracing::info!(
            lane = crate::events::NAME,
            exchange = %self.settings.exchange,
            queues = self.settings.queues.len(),
            "connected to the AMQP broker and declared the topology"
        );
        Ok(handle)
    }

    /// Turns on publisher confirms on `channel` and declares the exchange,
    /// the queues and their bindings.
    async fn declare(&self, channel: &Channel) -> Result<(), PublishError> {
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(PublishError::Amqp)?;
        channel
            .exchange_declare(
                self.settings.exchange.as_str().into(),
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(PublishError::Amqp)?;
        for queue in &self.settings.queues {
            channel
                .queue_declare(
                    queue.name.as_str().into(),
                    QueueDeclareOptions {
                        durable: true,
                        ..QueueDeclareOptions::default()
                    },
                    FieldTable::default(),
                )
                .await
                .map_err(PublishError::Amqp)?;
            channel
                .queue_bind(
                    queue.name.as_str().into(),
                    self.settings.exchange.as_str().into(),
                    queue.binding.as_str().into(),
                    QueueBindOptions::default(),
                    FieldTable::default(),
                )
                .await
                .map_err(PublishError::Amqp)?;
        }
        Ok(())
    }

    /// Publishes `body` under `routing_key` on `channel` and waits for the
    /// confirm.
    async fn confirmed(
        &self,
        channel: &Channel,
        routing_key: &str,
        body: &[u8],
    ) -> Result<(), PublishError> {
        let confirm = channel
            .basic_publish(
                self.settings.exchange.as_str().into(),
                routing_key.into(),
                BasicPublishOptions::default(),
                body,
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(PERSISTENT),
            )
            .await
            .map_err(PublishError::Amqp)?
            .await
            .map_err(PublishError::Amqp)?;
        if confirm.is_nack() {
            return Err(PublishError::Nack {
                routing_key: routing_key.to_owned(),
            });
        }
        Ok(())
    }

    /// Drops the open connection, so the next publish opens a fresh one.
    async fn reset(&self) {
        *self.open.lock().await = None;
    }

    /// Returns how long a publish waits for its confirm.
    const fn timeout(&self) -> Duration {
        self.settings.confirm_timeout
    }
}

#[async_trait::async_trait]
impl Publish for AmqpBroker {
    async fn publish(&self, routing_key: &str, body: &[u8]) -> Result<(), PublishError> {
        let outcome = tokio::time::timeout(self.timeout(), async {
            let channel = self.channel().await?;
            self.confirmed(&channel, routing_key, body).await
        })
        .await
        .unwrap_or(Err(PublishError::Timeout(self.timeout())));
        if matches!(
            outcome,
            Err(PublishError::Amqp(_) | PublishError::Timeout(_))
        ) {
            self.reset().await;
        }
        outcome
    }
}
