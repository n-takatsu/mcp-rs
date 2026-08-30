//! Integration tests for anti-replay protection wired into the HTTP transport
//! (Issue #259). Exercises the real `HttpTransport`/`handle_jsonrpc_request`
//! path end-to-end via `reqwest`, rather than the anti_replay module in
//! isolation (see `tests/anti_replay_test.rs` for that).

use mcp_rs::security::anti_replay::NonceManager;
use mcp_rs::transport::http::{HttpConfig, HttpTransport};
use std::net::SocketAddr;
use std::time::Duration;

/// Starts an `HttpTransport` with anti-replay enabled on an ephemeral port
/// and waits until it accepts connections.
async fn start_test_server() -> (HttpTransport, SocketAddr) {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = HttpConfig {
        bind_addr,
        anti_replay_enabled: true,
        ..HttpConfig::default()
    };

    let transport = HttpTransport::new(config).unwrap();
    transport.start_server().await.unwrap();
    let server_addr = transport.bound_addr().await.unwrap();

    // Small readiness wait: the listener is bound synchronously but axum::serve
    // runs in a spawned task, so give it a moment to start accepting.
    let client = reqwest::Client::new();
    for _ in 0..20 {
        if client
            .post(format!("http://{}/mcp", server_addr))
            .json(&serde_json::json!({"jsonrpc": "2.0", "method": "ping", "params": {}}))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    (transport, server_addr)
}

fn notification_body() -> serde_json::Value {
    // No "id" field: the handler treats this as a JSON-RPC notification and
    // replies immediately (200) instead of waiting up to 30s for a response
    // from a downstream handler that no test here provides.
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {}
    })
}

#[tokio::test]
async fn valid_nonce_and_timestamp_are_accepted() {
    let (_transport, server_addr) = start_test_server().await;
    let client = reqwest::Client::new();

    let nonce = NonceManager::generate_nonce();
    let timestamp = chrono::Utc::now().to_rfc3339();

    let response = client
        .post(format!("http://{}/mcp", server_addr))
        .header("X-Nonce", nonce)
        .header("X-Timestamp", timestamp)
        .json(&notification_body())
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "expected success, got {}",
        response.status()
    );
}

#[tokio::test]
async fn reused_nonce_is_rejected_with_conflict() {
    let (_transport, server_addr) = start_test_server().await;
    let client = reqwest::Client::new();

    let nonce = NonceManager::generate_nonce();

    let first = client
        .post(format!("http://{}/mcp", server_addr))
        .header("X-Nonce", &nonce)
        .header("X-Timestamp", chrono::Utc::now().to_rfc3339())
        .json(&notification_body())
        .send()
        .await
        .unwrap();
    assert!(first.status().is_success());

    let replay = client
        .post(format!("http://{}/mcp", server_addr))
        .header("X-Nonce", &nonce)
        .header("X-Timestamp", chrono::Utc::now().to_rfc3339())
        .json(&notification_body())
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), reqwest::StatusCode::CONFLICT);
}

#[tokio::test]
async fn expired_timestamp_is_rejected_with_request_timeout() {
    let (_transport, server_addr) = start_test_server().await;
    let client = reqwest::Client::new();

    let nonce = NonceManager::generate_nonce();
    let old_timestamp = (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339();

    let response = client
        .post(format!("http://{}/mcp", server_addr))
        .header("X-Nonce", nonce)
        .header("X-Timestamp", old_timestamp)
        .json(&notification_body())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::REQUEST_TIMEOUT);
}

#[tokio::test]
async fn missing_nonce_is_rejected_with_bad_request() {
    let (_transport, server_addr) = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("http://{}/mcp", server_addr))
        .header("X-Timestamp", chrono::Utc::now().to_rfc3339())
        .json(&notification_body())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn anti_replay_disabled_by_default_allows_requests_without_headers() {
    // Confirms the safe-default behavior: without anti_replay_enabled,
    // requests without X-Nonce/X-Timestamp headers are unaffected.
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = HttpConfig {
        bind_addr,
        ..HttpConfig::default()
    };
    let transport = HttpTransport::new(config).unwrap();
    transport.start_server().await.unwrap();
    let server_addr = transport.bound_addr().await.unwrap();

    let client = reqwest::Client::new();
    let mut response = None;
    for _ in 0..20 {
        match client
            .post(format!("http://{}/mcp", server_addr))
            .json(&notification_body())
            .send()
            .await
        {
            Ok(r) => {
                response = Some(r);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    let response = response.expect("server should have become ready");
    assert!(response.status().is_success());
}
