// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::session::ShareSession;
use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{RwLock, mpsc};
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
///
/// Dropping the handle shuts the server down; `stop` additionally waits until
/// every connection has been torn down.
pub struct ServerHandle {
    pub bound_addr: SocketAddr,
    pub published_addr: SocketAddr,
    pub state: Arc<ServerState>,
    serve_task: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn new(
        bound_addr: SocketAddr,
        published_addr: SocketAddr,
        state: Arc<ServerState>,
        serve_task: JoinHandle<()>,
    ) -> Self {
        Self {
            bound_addr,
            published_addr,
            state,
            serve_task: Some(serve_task),
        }
    }

    /// Invalidates the session, closes the listener and terminates every connection,
    /// returning once all of them are gone.
    pub async fn stop(&mut self) {
        {
            let mut guard = self.state.session.write().await;
            if let Some(session) = guard.as_mut() {
                session.stop();
            }
            *guard = None;
        }

        self.state.cancel_token.cancel();

        if let Some(task) = self.serve_task.take() {
            // The task returns only after aborting and awaiting every connection.
            // It cannot fail other than by panicking, which aborts in release builds.
            let _ = task.await;
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.state.cancel_token.cancel();
    }
}
