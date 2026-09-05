// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::session::ShareSession;
use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Shared application state accessible by HTTP route handlers.
pub struct ServerState {
    pub session: RwLock<Option<ShareSession>>,
    pub lifecycle_tx: mpsc::UnboundedSender<TransferLifecycleEvent>,
    pub progress_tx: mpsc::Sender<TransferProgressEvent>,
    pub transfer_counter: AtomicU64,
    pub cancel_token: CancellationToken,
}

impl ServerState {
    pub fn new(
        session: ShareSession,
        lifecycle_tx: mpsc::UnboundedSender<TransferLifecycleEvent>,
        progress_tx: mpsc::Sender<TransferProgressEvent>,
    ) -> Self {
        Self {
            session: RwLock::new(Some(session)),
            lifecycle_tx,
            progress_tx,
            transfer_counter: AtomicU64::new(1),
            cancel_token: CancellationToken::new(),
        }
    }
}

/// Control handle for the running ephemeral HTTP server.
pub struct ServerHandle {
    pub bound_addr: SocketAddr,
    pub published_addr: SocketAddr,
    pub state: Arc<ServerState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn new(
        bound_addr: SocketAddr,
        published_addr: SocketAddr,
        state: Arc<ServerState>,
        shutdown_tx: oneshot::Sender<()>,
        join_handle: JoinHandle<()>,
    ) -> Self {
        Self {
            bound_addr,
            published_addr,
            state,
            shutdown_tx: Some(shutdown_tx),
            join_handle: Some(join_handle),
        }
    }

    /// Stops the server, terminates all active connections, and invalidates the session.
    pub async fn stop(&mut self) {
        // 1. Invalidate session so future requests cannot be authorized
        {
            let mut guard = self.state.session.write().await;
            if let Some(session) = guard.as_mut() {
                session.stop();
            }
            *guard = None;
        }

        // 2. Cancel all in-flight file download streams promptly
        self.state.cancel_token.cancel();

        // 3. Initiate graceful shutdown to stop accepting new connections
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // 4. Clean up top-level server task as secondary safety mechanism
        if let Some(mut handle) = self.join_handle.take() {
            tokio::select! {
                _ = &mut handle => {}
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    handle.abort();
                    let _ = handle.await;
                }
            }
        }
    }
}
