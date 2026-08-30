//! Integration tests for anti-replay protection wired into the WebSocket
//! transport (Issue #259). Exercises the real `WebSocketServer` handshake
//! and per-message paths end-to-end via `tokio-tungstenite`, rather than the
//! anti_replay module in isolation (see `tests/anti_replay_test.rs`).

use futures::{SinkExt, StreamExt};
use mcp_rs::security::anti_replay::NonceManager;
use mcp_rs::transport::websocket::{ServerConfig, WebSocketServer};
use std::net::SocketAddr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn start_test_server() -> (WebSocketServer, SocketAddr) {
    let config = ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        anti_replay_enabled: true,
        ..ServerConfig::default()
    };
    let mut server = WebSocketServer::new(config);
    server.start().await.unwrap();
    let addr = server.bound_addr().await.unwrap();

    // give axum::serve's spawned task a moment to start accepting
    tokio::time::sleep(Duration::from_millis(100)).await;

    (server, addr)
}

fn client_request(addr: SocketAddr, nonce: &str, timestamp: &str) -> tokio_tungstenite::tungstenite::handshake::client::Request {
    let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
    request
        .headers_mut()
        .insert("x-nonce", nonce.parse().unwrap());
    request
        .headers_mut()
        .insert("x-timestamp", timestamp.parse().unwrap());
    request
}

#[tokio::test]
async fn valid_handshake_nonce_and_timestamp_connects() {
    let (_server, addr) = start_test_server().await;

    let nonce = NonceManager::generate_nonce();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let request = client_request(addr, &nonce, &timestamp);

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "expected handshake to succeed: {:?}", result.err());
}

#[tokio::test]
async fn reused_handshake_nonce_is_rejected() {
    let (_server, addr) = start_test_server().await;

    let nonce = NonceManager::generate_nonce();

    let first = tokio_tungstenite::connect_async(client_request(
        addr,
        &nonce,
        &chrono::Utc::now().to_rfc3339(),
    ))
    .await;
    assert!(first.is_ok(), "first handshake should succeed");

    let second = tokio_tungstenite::connect_async(client_request(
        addr,
        &nonce,
        &chrono::Utc::now().to_rfc3339(),
    ))
    .await;
    assert!(
        second.is_err(),
        "second handshake reusing the same nonce should be rejected"
    );
}

#[tokio::test]
async fn reused_message_nonce_is_dropped_silently() {
    let (_server, addr) = start_test_server().await;

    let handshake_nonce = NonceManager::generate_nonce();
    let (ws_stream, _) = tokio_tungstenite::connect_async(client_request(
        addr,
        &handshake_nonce,
        &chrono::Utc::now().to_rfc3339(),
    ))
    .await
    .unwrap();
    let (mut write, mut read) = ws_stream.split();

    let message_nonce = NonceManager::generate_nonce();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "ping",
        "nonce": message_nonce,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
    .to_string();

    // First send should be echoed back by the default EchoHandler.
    write.send(WsMessage::Text(payload.clone().into())).await.unwrap();
    let first_reply = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("expected a reply to the first message")
        .unwrap()
        .unwrap();
    assert_eq!(first_reply.into_text().unwrap(), payload);

    // Replaying the identical nonce should be silently dropped (no echo).
    write.send(WsMessage::Text(payload.clone().into())).await.unwrap();
    let second_reply = tokio::time::timeout(Duration::from_millis(500), read.next()).await;
    assert!(
        second_reply.is_err(),
        "replayed message nonce should not produce a reply"
    );
}

#[tokio::test]
async fn expired_message_timestamp_is_dropped_silently() {
    let (_server, addr) = start_test_server().await;

    let handshake_nonce = NonceManager::generate_nonce();
    let (ws_stream, _) = tokio_tungstenite::connect_async(client_request(
        addr,
        &handshake_nonce,
        &chrono::Utc::now().to_rfc3339(),
    ))
    .await
    .unwrap();
    let (mut write, mut read) = ws_stream.split();

    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "ping",
        "nonce": NonceManager::generate_nonce(),
        "timestamp": (chrono::Utc::now() - chrono::Duration::seconds(120)).to_rfc3339(),
    })
    .to_string();

    write.send(WsMessage::Text(payload.into())).await.unwrap();
    let reply = tokio::time::timeout(Duration::from_millis(500), read.next()).await;
    assert!(
        reply.is_err(),
        "message with an expired timestamp should not produce a reply"
    );
}
