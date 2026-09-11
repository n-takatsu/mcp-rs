//! Integration tests proving `/ws` on `HttpTransport` (Issue #261) actually
//! shares the transport's security stack, rather than being a route that
//! merely looks connected. Mirrors `tests/anti_replay_websocket_test.rs`'s
//! four scenarios against the standalone `WebSocketServer`, replaying them
//! here against `HttpTransport`'s `/ws` to prove the anti-replay wiring
//! survived the move onto HTTP's shared listener/TLS stack.

use futures::{SinkExt, StreamExt};
use mcp_rs::security::anti_replay::NonceManager;
use mcp_rs::transport::http::{HttpConfig, HttpTransport};
use mcp_rs::transport::Transport;
use mcp_rs::types::JsonRpcResponse;
use std::net::SocketAddr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as WsMessage;

async fn start_test_transport() -> (HttpTransport, SocketAddr) {
    let config = HttpConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        enable_websocket_upgrade: true,
        anti_replay_enabled: true,
        ..HttpConfig::default()
    };
    let transport = HttpTransport::new(config).unwrap();
    transport.start_server().await.unwrap();
    let addr = transport.bound_addr().await.unwrap();

    // give the spawned serve task a moment to start accepting
    tokio::time::sleep(Duration::from_millis(100)).await;

    (transport, addr)
}

/// Simulates the real MCP handler loop: drains requests off the transport
/// via `receive_message()` and replies with a canned successful response
/// correlated by `id` via `send_message()`. Needed so the message-level
/// tests below can distinguish "accepted" (a reply arrives) from "silently
/// dropped" (no reply within the timeout).
fn spawn_responder(mut transport: HttpTransport) {
    tokio::spawn(async move {
        while let Ok(Some(request)) = transport.receive_message().await {
            let response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::json!({"ok": true})),
                error: None,
                id: request.id,
            };
            if transport.send_message(response).await.is_err() {
                break;
            }
        }
    });
}

fn client_request(
    addr: SocketAddr,
    nonce: &str,
    timestamp: &str,
) -> tokio_tungstenite::tungstenite::handshake::client::Request {
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
    let (_transport, addr) = start_test_transport().await;

    let nonce = NonceManager::generate_nonce();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let request = client_request(addr, &nonce, &timestamp);

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_ok(),
        "expected handshake to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn reused_handshake_nonce_is_rejected() {
    let (_transport, addr) = start_test_transport().await;

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
    let (transport, addr) = start_test_transport().await;
    spawn_responder(transport);

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
        "id": 1,
        "nonce": message_nonce,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
    .to_string();

    // First send should reach the responder and get a correlated reply.
    write
        .send(WsMessage::Text(payload.clone().into()))
        .await
        .unwrap();
    let first_reply = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("expected a reply to the first message")
        .unwrap()
        .unwrap();
    let reply_value: serde_json::Value =
        serde_json::from_str(&first_reply.into_text().unwrap()).unwrap();
    assert_eq!(reply_value["id"], serde_json::json!(1));

    // Replaying the identical nonce should be silently dropped (no reply).
    write.send(WsMessage::Text(payload.into())).await.unwrap();
    let second_reply = tokio::time::timeout(Duration::from_millis(500), read.next()).await;
    assert!(
        second_reply.is_err(),
        "replayed message nonce should not produce a reply"
    );
}

#[tokio::test]
async fn expired_message_timestamp_is_dropped_silently() {
    let (transport, addr) = start_test_transport().await;
    spawn_responder(transport);

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
        "id": 1,
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
