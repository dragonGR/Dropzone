// SPDX-License-Identifier: GPL-3.0-or-later

use dropzone::server::routes::start_server;
use dropzone::share::files::SharedFile;
use dropzone::share::session::ShareSession;
use dropzone::share::transfer::TransferLifecycleEvent;
use std::net::Ipv4Addr;

#[tokio::test]
async fn test_server_e2e_lifecycle_and_streaming() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_stream.bin");

    let test_data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, _lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    let base_url = format!("http://{}", handle.published_addr);

    let landing_url = format!("{}/s/{}", base_url, token);
    let resp = reqwest_get(&landing_url).await;
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("dropzone_test_stream.bin"));
    assert!(resp.body.contains("64.0 KB"));
    assert!(!resp.body.contains("http-equiv=\"refresh\""));
    assert!(resp.body.contains("class=\"download-button\""));
    assert!(
        resp.body
            .contains(&format!("/s/{}/files/{}", token, file_id))
    );

    let landing_slash_url = format!("{}/s/{}/", base_url, token);
    let resp_slash = reqwest_get(&landing_slash_url).await;
    assert_eq!(resp_slash.status, 200);

    let css_url = format!("{}/s/{}/style.css", base_url, token);
    let resp = reqwest_get(&css_url).await;
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("--accent-color"));

    let icon_url = format!("{}/s/{}/icon.svg", base_url, token);
    let resp = reqwest_get(&icon_url).await;
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("<svg"));

    let download_url = format!("{}/s/{}/files/{}", base_url, token, file_id);
    let (status, downloaded_bytes, cd_header, content_type) = download_bytes(&download_url).await;
    assert_eq!(status, 200);
    assert_eq!(downloaded_bytes, test_data);
    assert!(cd_header.contains("attachment; filename=\"dropzone_test_stream.bin\""));
    assert_eq!(content_type, "content-type: application/octet-stream");

    let download_slash_url = format!("{}/s/{}/files/{}/", base_url, token, file_id);
    let (status_slash, downloaded_bytes_slash, _, _) = download_bytes(&download_slash_url).await;
    assert_eq!(status_slash, 200);
    assert_eq!(downloaded_bytes_slash, test_data);

    let bad_token = "0".repeat(64);
    let bad_url = format!("{}/s/{}", base_url, bad_token);
    let resp = reqwest_get(&bad_url).await;
    assert_eq!(resp.status, 404);

    let bad_file_id = "0".repeat(32);
    let bad_dl_url = format!("{}/s/{}/files/{}", base_url, token, bad_file_id);
    let (bad_dl_status, _, _, _) = download_bytes(&bad_dl_url).await;
    assert_eq!(bad_dl_status, 404);

    handle.stop().await;

    let stopped_attempt = tokio::net::TcpStream::connect(handle.bound_addr).await;
    if let Ok(mut stream) = stopped_attempt {
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 1];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        assert_eq!(n, 0, "Server should close connection immediately");
    }

    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test]
async fn test_server_lan_ip_download() {
    let lan_ip = match dropzone::network::interfaces::find_local_lan_ip() {
        Ok(ip) => ip,
        Err(_) => return,
    };

    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_lan_file.txt");
    let test_data = b"Testing LAN download on real interface".to_vec();
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, _lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(lan_ip, session, lifecycle_tx, progress_tx)
        .await
        .expect("start server on lan IP");

    assert_eq!(handle.published_addr.ip(), std::net::IpAddr::V4(lan_ip));

    let download_url = format!(
        "http://{}/s/{}/files/{}",
        handle.published_addr, token, file_id
    );
    let (status, downloaded_bytes, _, _) = download_bytes(&download_url).await;
    assert_eq!(status, 200);
    assert_eq!(downloaded_bytes, test_data);

    handle.stop().await;
    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_curl_real_download() {
    let temp_dir = std::env::temp_dir();
    let src_file = temp_dir.join("dropzone_curl_src.bin");
    let dest_file = temp_dir.join("dropzone_curl_dest.bin");

    let test_data = b"Upstream quality Dropzone test data with curl 123456789".to_vec();
    tokio::fs::write(&src_file, &test_data)
        .await
        .expect("write src file");

    let shared_file = SharedFile::from_path(src_file.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, _lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        std::net::Ipv4Addr::LOCALHOST,
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    let download_url = format!(
        "http://{}/s/{}/files/{}",
        handle.published_addr, token, file_id
    );

    let status = std::process::Command::new("curl")
        .args(["-s", "-f", "-o", dest_file.to_str().unwrap(), &download_url])
        .status()
        .expect("execute curl");

    assert!(
        status.success(),
        "curl should successfully download the file"
    );

    let downloaded = tokio::fs::read(&dest_file)
        .await
        .expect("read downloaded file");
    assert_eq!(
        downloaded, test_data,
        "Downloaded bytes must match source file exactly"
    );

    handle.stop().await;

    let status_after_stop = std::process::Command::new("curl")
        .args(["-s", "-f", "--connect-timeout", "1", &download_url])
        .status()
        .expect("execute curl after stop");

    assert!(
        !status_after_stop.success(),
        "curl should fail after server is stopped"
    );

    let _ = tokio::fs::remove_file(src_file).await;
    let _ = tokio::fs::remove_file(dest_file).await;
}

struct SimpleResponse {
    status: u16,
    body: String,
}

/// Minimal HTTP client using standard tokio TCP streams to avoid adding heavy reqwest dependency.
async fn reqwest_get(url: &str) -> SimpleResponse {
    let url_parsed = url.strip_prefix("http://").expect("valid http url");
    let (host_port, path) = url_parsed.split_once('/').expect("split host and path");
    let full_path = format!("/{}", path);

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(host_port)
        .await
        .expect("connect to server");

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        full_path, host_port
    );
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .await
        .expect("read response");

    let response_str = String::from_utf8_lossy(&response_bytes);
    let status_line = response_str.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let body = response_str
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();

    SimpleResponse { status, body }
}

async fn download_bytes(url: &str) -> (u16, Vec<u8>, String, String) {
    let url_parsed = url.strip_prefix("http://").expect("valid http url");
    let (host_port, path) = url_parsed.split_once('/').expect("split host and path");
    let full_path = format!("/{}", path);

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(host_port)
        .await
        .expect("connect to server");

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        full_path, host_port
    );
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .await
        .expect("read response");

    let header_end = response_bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(0);

    let headers_str = String::from_utf8_lossy(&response_bytes[..header_end]);
    let status_line = headers_str.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let cd_header = headers_str
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-disposition:"))
        .unwrap_or("")
        .to_string();

    let ct_header = headers_str
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-type:"))
        .unwrap_or("")
        .to_ascii_lowercase();

    let body_bytes = response_bytes[(header_end + 4)..].to_vec();

    (status, body_bytes, cd_header, ct_header)
}

#[tokio::test]
async fn test_transfer_events_lifecycle_and_monotonic_progress() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_monotonic.bin");
    let test_data: Vec<u8> = (0..(128 * 1024)).map(|i| (i % 256) as u8).collect();
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, mut lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    let download_url = format!(
        "http://{}/s/{}/files/{}",
        handle.published_addr, token, file_id
    );

    let (status, downloaded_bytes, _, _) = download_bytes(&download_url).await;
    assert_eq!(status, 200);
    assert_eq!(downloaded_bytes, test_data);

    // Lifecycle: Started must be received first
    let started = lifecycle_rx.recv().await.expect("receive started event");
    let transfer_id = match started {
        TransferLifecycleEvent::Started {
            transfer_id,
            total_bytes,
            ..
        } => {
            assert_eq!(total_bytes, 128 * 1024);
            transfer_id
        }
        other => panic!("Expected Started event, got {:?}", other),
    };

    // Progress events must be monotonic
    let mut last_streamed = 0;
    while let Ok(progress) = progress_rx.try_recv() {
        assert_eq!(progress.transfer_id, transfer_id);
        assert!(
            progress.bytes_streamed >= last_streamed,
            "Progress must be monotonically increasing: {} >= {}",
            progress.bytes_streamed,
            last_streamed
        );
        assert!(progress.bytes_streamed <= 128 * 1024);
        last_streamed = progress.bytes_streamed;
    }

    // Lifecycle: Completed must follow
    let completed = lifecycle_rx.recv().await.expect("receive completed event");
    assert_eq!(completed, TransferLifecycleEvent::Completed { transfer_id });

    // No Cancelled or Failed event after Completed
    assert!(lifecycle_rx.try_recv().is_err());

    handle.stop().await;
    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test]
async fn test_transfer_early_client_disconnect_emits_cancelled() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_disconnect.bin");
    let test_data: Vec<u8> = vec![0xAB; 4 * 1024 * 1024];
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, mut lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let host_port = handle.published_addr.to_string();
    let mut stream = tokio::net::TcpStream::connect(&host_port)
        .await
        .expect("connect");

    let req = format!(
        "GET /s/{}/files/{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        token, file_id, host_port
    );
    stream.write_all(req.as_bytes()).await.expect("write req");

    // Read only headers + small slice, then abort connection
    let mut buf = [0u8; 512];
    let _ = stream.read(&mut buf).await.expect("read chunk");
    drop(stream); // Client terminates connection early!

    let started = lifecycle_rx.recv().await.expect("started event");
    let transfer_id = match started {
        TransferLifecycleEvent::Started { transfer_id, .. } => transfer_id,
        other => panic!("Expected Started event, got {:?}", other),
    };

    // Server must emit Cancelled because the download did not reach clean EOF
    let cancelled = lifecycle_rx.recv().await.expect("cancelled event");
    match cancelled {
        TransferLifecycleEvent::Cancelled {
            transfer_id: cancelled_id,
            bytes_streamed,
        } => {
            assert_eq!(cancelled_id, transfer_id);
            assert!(bytes_streamed < 4 * 1024 * 1024);
        }
        other => panic!(
            "Expected Cancelled event on early disconnect, got {:?}",
            other
        ),
    }

    // Must NOT emit Completed
    assert!(lifecycle_rx.try_recv().is_err());

    handle.stop().await;
    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test]
async fn test_transfer_concurrent_downloads_independent_ids() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_concurrent.bin");
    let test_data = b"Dropzone concurrent download verification test".to_vec();
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, mut lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    let url1 = format!(
        "http://{}/s/{}/files/{}",
        handle.published_addr, token, file_id
    );
    let url2 = url1.clone();

    // Launch 2 concurrent downloads
    let (res1, res2) = tokio::join!(download_bytes(&url1), download_bytes(&url2));

    assert_eq!(res1.0, 200);
    assert_eq!(res1.1, test_data);
    assert_eq!(res2.0, 200);
    assert_eq!(res2.1, test_data);

    let mut ids = Vec::new();
    while let Ok(event) = lifecycle_rx.try_recv() {
        if let TransferLifecycleEvent::Started { transfer_id, .. } = event {
            ids.push(transfer_id);
        }
    }

    assert_eq!(ids.len(), 2, "Must receive exactly 2 Started events");
    assert_ne!(
        ids[0], ids[1],
        "Each concurrent transfer must have a distinct transfer_id"
    );

    handle.stop().await;
    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test]
async fn test_stop_sharing_promptly_terminates_active_download_and_invalidates_session() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_stop_abort.bin");
    // 4 MB test payload
    let total_size = 4 * 1024 * 1024;
    let test_data = vec![0x3C; total_size];
    tokio::fs::write(&file_path, &test_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, mut lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    let host_port = handle.published_addr.to_string();
    let mut stream = tokio::net::TcpStream::connect(&host_port)
        .await
        .expect("connect");

    let req = format!(
        "GET /s/{}/files/{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        token, file_id, host_port
    );
    stream.write_all(req.as_bytes()).await.expect("write req");

    // Read initial response headers and a small portion of the body
    let mut initial_buf = [0u8; 4096];
    let n = stream
        .read(&mut initial_buf)
        .await
        .expect("read initial chunk");
    assert!(n > 0, "Must have received initial HTTP response chunk");

    // In-flight download is now active. Trigger Stop Sharing!
    handle.stop().await;

    // The client attempts to read the remainder of the file.
    // The stream must terminate promptly and must NOT receive the complete original file.
    let mut remainder = Vec::new();
    let read_result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut chunk = [0u8; 64 * 1024];
        loop {
            match stream.read(&mut chunk).await {
                Ok(0) => break, // EOF reached prematurely
                Ok(bytes) => remainder.extend_from_slice(&chunk[..bytes]),
                Err(_) => break, // Connection closed / aborted / reset
            }
        }
    })
    .await;

    assert!(
        read_result.is_ok(),
        "Active download must terminate promptly after stop sharing (within timeout)"
    );

    let total_received = n + remainder.len();
    assert!(
        total_received < total_size,
        "Download must be cut off: received {} bytes, expected less than {} bytes",
        total_received,
        total_size
    );

    // Verify lifecycle events: Started -> Cancelled (not Completed, not Failed)
    let started = lifecycle_rx.recv().await.expect("Started event");
    let transfer_id = match started {
        TransferLifecycleEvent::Started { transfer_id, .. } => transfer_id,
        other => panic!("Expected Started event, got {:?}", other),
    };

    let cancelled = lifecycle_rx.recv().await.expect("Cancelled event");
    match cancelled {
        TransferLifecycleEvent::Cancelled {
            transfer_id: cid,
            bytes_streamed,
        } => {
            assert_eq!(cid, transfer_id);
            assert!(
                bytes_streamed < total_size as u64,
                "Streamed bytes {} must be less than total {}",
                bytes_streamed,
                total_size
            );
        }
        other => panic!("Expected Cancelled event, got {:?}", other),
    }

    assert!(
        lifecycle_rx.try_recv().is_err(),
        "No trailing lifecycle events after Cancelled"
    );

    // Verify authorization invalidation: new requests to old URLs must fail.
    // Either connection is refused (server listener closed) or request is rejected (403/404).
    if let Ok(Ok(mut s)) = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(&host_port),
    )
    .await
    {
        let req = format!(
            "GET /s/{}/files/{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            token, file_id, host_port
        );
        let _ = s.write_all(req.as_bytes()).await;
        let mut buf = [0u8; 512];
        let n = s.read(&mut buf).await.unwrap_or(0);
        let resp = String::from_utf8_lossy(&buf[..n]);
        assert!(
            !resp.starts_with("HTTP/1.1 200"),
            "Old download URL must not return 200 OK after stop: {}",
            resp
        );
    }

    let _ = tokio::fs::remove_file(file_path).await;
}

#[tokio::test]
async fn test_transfer_early_eof_truncated_file_emits_failed_and_errors() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("dropzone_test_truncation.bin");

    // Initially create a 64 KB file
    let initial_data = vec![0x55; 64 * 1024];
    tokio::fs::write(&file_path, &initial_data)
        .await
        .expect("write test file");

    let shared_file = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
    let file_id = shared_file.id().as_str().to_string();
    let session = ShareSession::new(shared_file);
    let token = session.token().as_str().to_string();

    let (lifecycle_tx, mut lifecycle_rx) = tokio::sync::mpsc::unbounded_channel();
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel(64);

    let mut handle = start_server(
        Ipv4Addr::new(127, 0, 0, 1),
        session,
        lifecycle_tx,
        progress_tx,
    )
    .await
    .expect("start server");

    // Truncate the file on disk to 16 KB while the server expected 64 KB
    let truncated_data = vec![0x55; 16 * 1024];
    tokio::fs::write(&file_path, &truncated_data)
        .await
        .expect("truncate file");

    let download_url = format!(
        "http://{}/s/{}/files/{}",
        handle.published_addr, token, file_id
    );

    // Download will hit EOF prematurely at 16 KB instead of 64 KB
    let (status, body, _, _) = download_bytes(&download_url).await;
    // Status is sent before body streaming, but body must not be complete 64 KB
    assert_eq!(status, 200);
    assert!(
        body.len() < 64 * 1024,
        "Truncated download body must have fewer bytes than expected"
    );

    // Lifecycle must observe Failed event due to early EOF size mismatch
    let started = lifecycle_rx.recv().await.expect("Started event");
    let transfer_id = match started {
        TransferLifecycleEvent::Started { transfer_id, .. } => transfer_id,
        other => panic!("Expected Started event, got {:?}", other),
    };

    let failed = lifecycle_rx.recv().await.expect("Failed event");
    match failed {
        TransferLifecycleEvent::Failed {
            transfer_id: fid,
            bytes_streamed,
        } => {
            assert_eq!(fid, transfer_id);
            assert!(bytes_streamed < 64 * 1024);
        }
        other => panic!("Expected Failed event for truncated file, got {:?}", other),
    }

    handle.stop().await;
    let _ = tokio::fs::remove_file(file_path).await;
}
