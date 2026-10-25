// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One request on the wire: its send, and the challenge a refusal carries.

use http::HeaderMap;
use http::header::WWW_AUTHENTICATE;
use url::Url;

use crate::error::Error;

/// Returns the response to `request`, sent once, for `url`.
pub(super) async fn dispatch(
    url: &Url,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response, Error> {
    request.send().await.map_err(|source| {
        if source.is_timeout() {
            Error::Timeout {
                url: url.clone(),
                source,
            }
        } else {
            Error::Transport {
                url: url.clone(),
                source,
            }
        }
    })
}

/// Returns the `WWW-Authenticate` challenge, when the answer carries usable
/// text.
pub(super) fn challenge(headers: &HeaderMap) -> Option<String> {
    headers
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}
