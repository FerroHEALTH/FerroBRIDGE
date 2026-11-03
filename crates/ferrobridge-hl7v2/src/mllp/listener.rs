// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MLLP listener: each connection read frame by frame, each frame
//! answered by a [`Handler`], within the [`Timeouts`].

use core::future::Future;
use core::time::Duration;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::codec::{Decoder, Encoder};
use tracing::Instrument;

use super::{Codec, FrameError, Malformed, READ_CHUNK};

/// What one connection counts, recorded on its `mllp_connection` span.
///
/// The listener counts the frames it answered; the handler counts the frames
/// it refused by its own policy before handling them (a sender it does not
/// accept, say), so the span says how many of a peer's messages were turned
/// away.
#[derive(Debug, Default)]
pub struct Connection {
    refused: AtomicUsize,
    peer: Option<SocketAddr>,
}

impl Connection {
    /// The counts of a connection from `peer`.
    #[must_use]
    pub fn with_peer(peer: Option<SocketAddr>) -> Self {
        Self {
            refused: AtomicUsize::new(0),
            peer,
        }
    }

    /// Returns the address of the connection's peer, as the socket reported
    /// it when the connection was accepted; `None` when it could not.
    #[must_use]
    pub const fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }

    /// Counts one frame the handler refused by its own policy.
    pub fn refuse(&self) {
        self.refused.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns how many frames the handler refused.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::Relaxed)
    }
}

/// What the listener hands each frame to.
///
/// The answer is the acknowledgment's bytes, framed by the listener. A
/// handler that cannot answer a frame at all returns `None`, and the listener
/// closes the connection.
pub trait Handler: Send + Sync + 'static {
    /// Answers one frame's message bytes, counting on `connection` what the
    /// connection's span reports.
    fn handle(
        &self,
        message: Vec<u8>,
        connection: &Connection,
    ) -> impl Future<Output = Option<Vec<u8>>> + Send;

    /// Answers a malformed frame from the message bytes read before the
    /// refusal, before the listener closes the connection.
    ///
    /// The default answers nothing; [`crate::ack::reject_frame`] builds the
    /// reject acknowledgment when a header can still be read.
    fn malformed(&self, kind: Malformed, partial: &[u8]) -> Option<Vec<u8>> {
        let (_, _) = (kind, partial);
        None
    }
}

/// How long one connection may wait on its peer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timeouts {
    /// How long a connection may sit with no frame begun before it is
    /// closed; `None` waits for as long as the peer keeps it open.
    pub idle: Option<Duration>,
    /// How long a frame may take from its first byte to its trailer before it
    /// is refused as [`Malformed::Stalled`]; `None` waits for the trailer.
    pub frame: Option<Duration>,
}

/// Accepts connections on `listener` until `shutdown` completes, answering
/// every frame through `handler`, with no timeout on a connection.
///
/// [`serve_with`] states the timeouts.
///
/// # Errors
///
/// Returns the I/O error of a failed `accept`.
pub async fn serve<H: Handler>(
    listener: TcpListener,
    handler: Arc<H>,
    codec: Codec,
    shutdown: impl Future<Output = ()> + Send,
) -> std::io::Result<()> {
    serve_with(listener, handler, codec, Timeouts::default(), shutdown).await
}

/// Accepts connections on `listener` until `shutdown` completes, answering
/// every frame through `handler` under `timeouts`.
///
/// On shutdown the listener stops accepting, each connection finishes the
/// frame it is answering and closes, and the call returns once every
/// connection has closed. Each connection runs in its own `mllp_connection`
/// span naming the peer.
///
/// # Errors
///
/// Returns the I/O error of a failed `accept`.
pub async fn serve_with<H: Handler>(
    listener: TcpListener,
    handler: Arc<H>,
    codec: Codec,
    timeouts: Timeouts,
    shutdown: impl Future<Output = ()> + Send,
) -> std::io::Result<()> {
    let (stop, stopped) = watch::channel(false);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                tracing::debug!(peer = %peer, "MLLP connection accepted");
                let handler = Arc::clone(&handler);
                let stopped = stopped.clone();
                let span = tracing::info_span!(
                    "mllp_connection",
                    peer = %peer,
                    messages = tracing::field::Empty,
                    refused = tracing::field::Empty,
                );
                connections.spawn(
                    async move {
                        let ended =
                            connection(stream, handler.as_ref(), codec, timeouts, stopped).await;
                        if let Err(error) = ended {
                            tracing::warn!(error = %error, "MLLP connection closed on an error");
                        }
                    }
                    .instrument(span),
                );
            }
            Some(finished) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = finished {
                    tracing::warn!(error = %error, "an MLLP connection task failed");
                }
            }
        }
    }
    let _sent = stop.send(true);
    while let Some(finished) = connections.join_next().await {
        if let Err(error) = finished {
            tracing::warn!(error = %error, "an MLLP connection task failed");
        }
    }
    Ok(())
}

/// Answers the frames of one connection until the peer closes it, a frame is
/// malformed or stalls, the connection idles past its timeout, or the
/// listener stops.
async fn connection<H: Handler>(
    mut stream: TcpStream,
    handler: &H,
    mut codec: Codec,
    timeouts: Timeouts,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), FrameError> {
    let mut buffer = BytesMut::new();
    // NOTE: no specification governs this: our own design; a peer address the operating
    // system cannot report is absent, and the handler is told so.
    let counts = Connection::with_peer(stream.peer_addr().ok());
    let mut messages = 0usize;
    let mut idle_since = Instant::now();
    let mut frame_since: Option<Instant> = None;
    loop {
        let decoded = match codec.decode(&mut buffer) {
            Ok(Some(message)) => Some(message),
            Ok(None) => None,
            Err(FrameError::Malformed { kind, partial }) => {
                return refuse(&mut stream, handler, &mut codec, kind, &partial).await;
            }
            Err(other) => return Err(other),
        };
        if let Some(message) = decoded {
            let answered = handler.handle(message, &counts).await;
            messages = messages.saturating_add(1);
            let span = tracing::Span::current();
            span.record("messages", messages);
            span.record("refused", counts.refused());
            let Some(reply) = answered else {
                return Ok(());
            };
            let mut out = BytesMut::new();
            codec.encode(reply.as_slice(), &mut out)?;
            stream.write_all(&out).await?;
            if *stopped.borrow() {
                return Ok(());
            }
            idle_since = Instant::now();
            frame_since = (!buffer.is_empty()).then_some(idle_since);
            continue;
        }
        let deadline = match frame_since {
            None => timeouts
                .idle
                .and_then(|idle| Some((idle_since.checked_add(idle)?, None))),
            Some(since) => timeouts
                .frame
                .and_then(|frame| Some((since.checked_add(frame)?, Some(frame)))),
        };
        buffer.reserve(READ_CHUNK);
        tokio::select! {
            () = expiry(deadline.map(|(at, _)| at)) => {
                if let Some(limit) = deadline.and_then(|(_, frame)| frame) {
                    let partial = buffer.get(1..).unwrap_or_default().to_vec();
                    let kind = Malformed::Stalled { limit };
                    return refuse(&mut stream, handler, &mut codec, kind, &partial).await;
                }
                tracing::debug!("MLLP connection closed after its idle timeout");
                stream.shutdown().await?;
                return Ok(());
            }
            read = stream.read_buf(&mut buffer) => {
                if frame_since.is_none() && !buffer.is_empty() {
                    frame_since = Some(Instant::now());
                }
                if read? == 0 {
                    return match codec.decode_eof(&mut buffer) {
                        Ok(_) => Ok(()),
                        Err(FrameError::Malformed { kind, partial }) => {
                            refuse(&mut stream, handler, &mut codec, kind, &partial).await
                        }
                        Err(other) => Err(other),
                    };
                }
            }
            changed = stopped.changed() => {
                if changed.is_err() || *stopped.borrow() {
                    return Ok(());
                }
            }
        }
    }
}

/// Completes at `deadline`, or never when there is none.
async fn expiry(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Sends the handler's answer to a malformed frame, when it has one, and
/// ends the connection.
async fn refuse<H: Handler>(
    stream: &mut TcpStream,
    handler: &H,
    codec: &mut Codec,
    kind: Malformed,
    partial: &[u8],
) -> Result<(), FrameError> {
    tracing::warn!(refusal = %kind, "MLLP frame refused");
    if let Some(reply) = handler.malformed(kind, partial) {
        let mut out = BytesMut::new();
        codec.encode(reply.as_slice(), &mut out)?;
        stream.write_all(&out).await?;
    }
    stream.shutdown().await?;
    Ok(())
}
