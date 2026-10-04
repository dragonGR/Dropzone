// SPDX-License-Identifier: GPL-3.0-or-later

use axum::Router;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const MAX_CONNECTIONS: usize = 64;
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(250);

/// Bounds on the resources a single share's HTTP server may use.
#[derive(Debug, Clone, Copy)]
pub struct ServerLimits {
    /// Maximum number of simultaneously open client connections.
    pub max_connections: usize,
    /// Time a client has to send a complete request head, including while an
    /// idle keep-alive connection waits for its next request.
    pub header_read_timeout: Duration,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            max_connections: MAX_CONNECTIONS,
            header_read_timeout: HEADER_READ_TIMEOUT,
        }
    }
}

/// Accepts and serves connections until `shutdown` is cancelled.
///
/// Every connection task is owned by this function. On shutdown the listener is
/// dropped and all connections are aborted and awaited before it returns, so no
/// connection, open file or reference to the server state outlives the call.
pub(crate) async fn serve(
    listener: TcpListener,
    router: Router,
    limits: ServerLimits,
    shutdown: CancellationToken,
) {
    let slots = Arc::new(Semaphore::new(limits.max_connections));
    let mut connections = JoinSet::new();

    loop {
        let slot = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            Some(_) = connections.join_next() => continue,
            slot = Arc::clone(&slots).acquire_owned() => match slot {
                Ok(slot) => slot,
                // The semaphore is never closed.
                Err(_) => break,
            },
        };

        let stream = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(err) if is_connection_error(&err) => continue,
                Err(_) => {
                    // Resource exhaustion such as EMFILE; retrying at once would spin.
                    tokio::select! {
                        () = shutdown.cancelled() => break,
                        () = tokio::time::sleep(ACCEPT_ERROR_BACKOFF) => continue,
                    }
                }
            },
        };

        connections.spawn(serve_connection(
            stream,
            router.clone(),
            limits.header_read_timeout,
            slot,
        ));
    }

    drop(listener);
    connections.shutdown().await;
}

async fn serve_connection(
    stream: TcpStream,
    router: Router,
    header_read_timeout: Duration,
    _slot: OwnedSemaphorePermit,
) {
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(header_read_timeout);

    // An error here means the client sent an invalid request, timed out or went
    // away; the connection is finished either way and there is nobody to tell.
    let _ = builder
        .serve_connection(TokioIo::new(stream), TowerToHyperService::new(router))
        .await;
}

fn is_connection_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}
