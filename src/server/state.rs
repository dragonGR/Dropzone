// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::session::ShareSession;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;

/// Shared application state accessible by HTTP route handlers.
pub struct ServerState {
    pub session: RwLock<Option<ShareSession>>,
}

impl ServerState {
    pub fn new(session: ShareSession) -> Self {
        Self {
            session: RwLock::new(Some(session)),
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
        {
            let mut guard = self.state.session.write().await;
            if let Some(session) = guard.as_mut() {
                session.stop();
            }
            *guard = None;
        }

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.await;
        }
    }
}
