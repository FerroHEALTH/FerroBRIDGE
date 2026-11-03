// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MLLP sender: one frame written to a peer and its one reply read,
//! within the [`SendTimeouts`].

use core::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder};

use super::{Codec, FrameError, Malformed, READ_CHUNK};

/// How long [`send`] may wait on its peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendTimeouts {
    /// How long resolving the peer and opening the connection may take.
    pub connect: Duration,
    /// How long the exchange may take once connected, from the first byte
    /// written to the reply's trailer.
    pub reply: Duration,
}

/// Why [`send`] returned no reply.
///
/// No variant carries the message or the reply bytes, which name a patient.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// The peer refused the connection.
    #[error("the peer refused the MLLP connection")]
    Refused(#[source] std::io::Error),
    /// The connection could not be opened for another reason, an address
    /// that does not resolve included.
    #[error("the MLLP connection could not be opened")]
    Connect(#[source] std::io::Error),
    /// The connection was not open within the connect timeout.
    #[error("the MLLP connection was not open within {} ms", limit.as_millis())]
    ConnectTimedOut {
        /// The connect timeout.
        limit: Duration,
    },
    /// The reply's trailer did not arrive within the reply timeout.
    #[error("no MLLP reply arrived within {} ms", limit.as_millis())]
    ReplyTimedOut {
        /// The reply timeout.
        limit: Duration,
    },
    /// The peer closed the connection before a reply began.
    #[error("the peer closed the MLLP connection before replying")]
    Closed,
    /// The peer closed the connection inside the reply frame.
    #[error("the peer closed the MLLP connection inside the reply")]
    Truncated,
    /// The reply runs past the codec's frame ceiling.
    #[error("the MLLP reply runs past the ceiling of {limit} bytes")]
    TooLarge {
        /// The ceiling, in message bytes.
        limit: usize,
    },
    /// The reply is no MLLP frame for another reason.
    #[error("the MLLP reply is malformed: {kind}")]
    Malformed {
        /// Why.
        kind: Malformed,
    },
    /// The message holds a block character and cannot be framed.
    #[error("the message holds a block character and cannot be framed")]
    Unframable,
    /// The connection failed once open.
    #[error("the MLLP connection failed")]
    Io(#[source] std::io::Error),
}

impl From<FrameError> for SendError {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Malformed {
                kind: Malformed::Truncated,
                ..
            } => Self::Truncated,
            FrameError::Malformed {
                kind: Malformed::TooLarge { limit },
                ..
            } => Self::TooLarge { limit },
            FrameError::Malformed { kind, .. } => Self::Malformed { kind },
            FrameError::Unframable => Self::Unframable,
            FrameError::Io(error) => Self::Io(error),
        }
    }
}

/// Sends `message` to `peer` as one MLLP frame and returns the message bytes
/// of the one reply frame, bounded by `codec`'s ceiling and `timeouts`.
///
/// Each call opens its own connection and closes it once the reply is read;
/// the caller reads the acknowledgment from the reply through
/// [`crate::parse`]. Bytes the peer sends after the reply are not read.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
///
/// use ferrobridge_hl7v2::mllp::Codec;
/// use ferrobridge_hl7v2::mllp::sender::{SendTimeouts, send};
///
/// # async fn notify() -> Result<(), ferrobridge_hl7v2::mllp::sender::SendError> {
/// let timeouts = SendTimeouts {
///     connect: Duration::from_secs(5),
///     reply: Duration::from_secs(30),
/// };
/// let message = b"MSH|^~\\&|PIX|HOSP|APP|FAC|20261026||ADT^A31^ADT_A05|M1|P|2.5\r";
/// let reply = send("consumer.example:2575", message, Codec::default(), timeouts).await?;
/// assert!(reply.starts_with(b"MSH|"));
/// # Ok(())
/// # }
/// # let _future = notify();
/// ```
///
/// # Errors
///
/// Returns [`SendError::Unframable`] for a message holding a block
/// character, before any connection is opened; [`SendError::Refused`],
/// [`SendError::Connect`] or [`SendError::ConnectTimedOut`] when no
/// connection opens; [`SendError::ReplyTimedOut`] when the reply is not
/// whole in time; [`SendError::Closed`] or [`SendError::Truncated`] when the
/// peer closes before or inside the reply; [`SendError::TooLarge`] and
/// [`SendError::Malformed`] for a reply that is no frame within the ceiling;
/// and [`SendError::Io`] when a read or write fails.
pub async fn send<A: tokio::net::ToSocketAddrs>(
    peer: A,
    message: &[u8],
    mut codec: Codec,
    timeouts: SendTimeouts,
) -> Result<Vec<u8>, SendError> {
    let mut out = BytesMut::new();
    codec.encode(message, &mut out)?;
    let opened = tokio::time::timeout(timeouts.connect, TcpStream::connect(peer)).await;
    let mut stream = match opened {
        Err(_elapsed) => {
            return Err(SendError::ConnectTimedOut {
                limit: timeouts.connect,
            });
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            return Err(SendError::Refused(error));
        }
        Ok(Err(error)) => return Err(SendError::Connect(error)),
        Ok(Ok(stream)) => stream,
    };
    let exchange = async {
        stream.write_all(&out).await.map_err(SendError::Io)?;
        let mut buffer = BytesMut::new();
        loop {
            if let Some(reply) = codec.decode(&mut buffer)? {
                return Ok(reply);
            }
            buffer.reserve(READ_CHUNK);
            if stream.read_buf(&mut buffer).await.map_err(SendError::Io)? == 0 {
                if buffer.is_empty() {
                    return Err(SendError::Closed);
                }
                return match codec.decode_eof(&mut buffer)? {
                    Some(reply) => Ok(reply),
                    None => Err(SendError::Closed),
                };
            }
        }
    };
    tokio::time::timeout(timeouts.reply, exchange)
        .await
        .unwrap_or(Err(SendError::ReplyTimedOut {
            limit: timeouts.reply,
        }))
}
