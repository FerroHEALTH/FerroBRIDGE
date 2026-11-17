// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client the ITI-20 ATX: FHIR Feed sends through: the bridge's own
//! `ferrohealth-smart` outbound client.
//!
//! `ihe-atna` owns the Send Audit Resource Request and its outcomes (the
//! `RESTful` ATNA supplement, ITI TF-2 §3.20.4.2 and §3.20.4.3); this client
//! owns the TLS, the timeouts, the client credentials token renewed once on a
//! `401` (RFC 6750 §3.1) and the tries of one round. No specification governs
//! the seam: our own design.

use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::client::outbound::SendError;
use http::Method;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use ihe_atna::atna::feed::FeedClient;
use ihe_atna::atna::feed::FeedConfigError;
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::feed::FeedSendError;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Returns the Audit Record Repository at `client`'s URL, sent to through
/// `client`, over `https` alone unless `cleartext` accepts `http`.
///
/// A failed round waits the longest backoff of the endpoint's own `retry`
/// table before the forwarder sends again (no specification governs the wait:
/// our own design).
///
/// # Errors
///
/// The [`FeedConfigError`] of a URL the FHIR Feed refuses, such as an `http`
/// URL without `cleartext`, or of a zero `max_backoff`.
pub fn repository(
    client: OutboundClient,
    cleartext: bool,
) -> Result<FeedRepository, FeedConfigError> {
    let base = client.url().clone();
    let between_rounds = Duration::from(client.retry().max_backoff);
    let sender = Arc::new(Feed(client));
    if cleartext {
        FeedRepository::cleartext(&base, sender, between_rounds)
    } else {
        FeedRepository::new(&base, sender, between_rounds)
    }
}

/// The `[audit.endpoint]` client, as the FHIR Feed's client.
#[derive(Debug)]
struct Feed(OutboundClient);

#[async_trait::async_trait]
impl FeedClient for Feed {
    fn authorizes(&self) -> bool {
        self.0.authorization().is_some()
    }

    async fn post(
        &self,
        url: &Url,
        content_type: &'static str,
        body: &[u8],
    ) -> Result<StatusCode, FeedSendError> {
        let answer = self
            .0
            .send(&Method::POST, url, |request| {
                request
                    .header(CONTENT_TYPE, content_type)
                    .body(body.to_vec())
            })
            .await
            .map_err(|error| match error {
                // NOTE: no specification governs this: our own design; the URL leaves the error so
                // nothing the library logs names the repository's path beyond its configured key.
                SendError::Transport(error) => {
                    FeedSendError::Transport(Box::new(error.without_url()))
                }
                other => FeedSendError::Credentials(Box::new(other)),
            })?;
        Ok(answer.status())
    }
}
