// SPDX-License-Identifier: GPL-3.0-or-later

use crate::server::download::{escape_html, format_content_disposition, guess_mime_type};
use crate::server::progress_stream::ProgressReader;
use crate::server::state::{ServerHandle, ServerState};
use crate::share::session::ShareSession;
use crate::share::transfer::{TransferLifecycleEvent, TransferProgressEvent};
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_util::io::ReaderStream;

const INDEX_HTML_TEMPLATE: &str = include_str!("../../web/index.html");
const STYLE_CSS: &str = include_str!("../../web/style.css");
const ICON_SVG: &str =
    include_str!("../../data/icons/hicolor/scalable/apps/io.github.dragonGR.Dropzone.svg");

/// Handler for the landing page: GET /s/{token}
async fn landing_page(
    State(state): State<Arc<ServerState>>,
    Path(token): Path<String>,
) -> Response {
    let guard = state.session.read().await;
    let session = match guard.as_ref() {
        Some(s) if s.is_authorized(&token) => s,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    let file = session.file();
    let escaped_name = escape_html(file.name());
    let escaped_size = escape_html(&file.formatted_size());
    let download_url = format!("/s/{}/files/{}", token, file.id().as_str());

    let rendered = INDEX_HTML_TEMPLATE
        .replace("{{FILE_NAME}}", &escaped_name)
        .replace("{{FILE_SIZE}}", &escaped_size)
        .replace("{{DOWNLOAD_URL}}", &download_url)
        .replace("{{TOKEN}}", &token);

    Html(rendered).into_response()
}

/// Handler for the CSS stylesheet: GET /s/{token}/style.css
async fn stylesheet(State(state): State<Arc<ServerState>>, Path(token): Path<String>) -> Response {
    let guard = state.session.read().await;
    match guard.as_ref() {
        Some(s) if s.is_authorized(&token) => (
            [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
            STYLE_CSS,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Handler for the favicon: GET /s/{token}/icon.svg or GET /s/{token}/favicon.ico
async fn favicon(State(state): State<Arc<ServerState>>, Path(token): Path<String>) -> Response {
    let guard = state.session.read().await;
    match guard.as_ref() {
        Some(s) if s.is_authorized(&token) => {
            ([(header::CONTENT_TYPE, "image/svg+xml")], ICON_SVG).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Handler for streaming the file: GET /s/{token}/files/{file_id}
async fn download_file(
    State(state): State<Arc<ServerState>>,
    Path((token, file_id)): Path<(String, String)>,
) -> Response {
    let (file_path, file_name, file_size) = {
        let guard = state.session.read().await;
        match guard
            .as_ref()
            .and_then(|s| s.get_authorized_file(&token, &file_id))
        {
            Some(f) => (f.path().to_path_buf(), f.name().to_string(), f.size_bytes()),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let tokio_file = match tokio::fs::File::open(&file_path).await {
        Ok(f) => f,
        Err(_) => {
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let mime_type = guess_mime_type(&file_name);
    let content_disposition = format_content_disposition(&file_name);

    let transfer_id = state
        .transfer_counter
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let _ = state.lifecycle_tx.send(TransferLifecycleEvent::Started {
        transfer_id,
        file_name: file_name.clone(),
        total_bytes: file_size,
    });

    let progress_reader = ProgressReader::new(
        tokio_file,
        transfer_id,
        file_size,
        state.lifecycle_tx.clone(),
        state.progress_tx.clone(),
        state.cancel_token.clone(),
    );
    let stream = ReaderStream::new(progress_reader);
    let body = axum::body::Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Constructs the Axum application router with all routes bound to the state.
pub fn build_router(state: Arc<ServerState>) -> Router {
    Router::new()
        .route("/s/{token}", get(landing_page))
        .route("/s/{token}/", get(landing_page))
        .route("/s/{token}/style.css", get(stylesheet))
        .route("/s/{token}/icon.svg", get(favicon))
        .route("/s/{token}/favicon.ico", get(favicon))
        .route("/s/{token}/files/{file_id}", get(download_file))
        .route("/s/{token}/files/{file_id}/", get(download_file))
        .with_state(state)
}

/// Starts an ephemeral HTTP server on an OS-assigned port.
pub async fn start_server(
    lan_ip: Ipv4Addr,
    session: ShareSession,
    lifecycle_tx: mpsc::UnboundedSender<TransferLifecycleEvent>,
    progress_tx: mpsc::Sender<TransferProgressEvent>,
) -> io::Result<ServerHandle> {
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let listener = TcpListener::bind(bind_addr).await?;
    let bound_port = listener.local_addr()?.port();

    let bound_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), bound_port);
    let published_addr = SocketAddr::new(IpAddr::V4(lan_ip), bound_port);

    let state = Arc::new(ServerState::new(session, lifecycle_tx, progress_tx));
    let router = build_router(Arc::clone(&state));

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
    });

    let join_handle = tokio::spawn(async move {
        let _ = server.await;
    });

    Ok(ServerHandle::new(
        bound_addr,
        published_addr,
        state,
        shutdown_tx,
        join_handle,
    ))
}
