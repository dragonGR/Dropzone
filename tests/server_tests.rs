// SPDX-License-Identifier: GPL-3.0-or-later

use dropzone::server::routes::start_server;
use dropzone::share::files::SharedFile;
use dropzone::share::session::ShareSession;
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

    let mut handle = start_server(Ipv4Addr::new(127, 0, 0, 1), session)
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

    let mut handle = start_server(lan_ip, session)
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

    let mut handle = start_server(std::net::Ipv4Addr::LOCALHOST, session)
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
