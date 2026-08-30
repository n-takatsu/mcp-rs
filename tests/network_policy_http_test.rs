//! Integration tests proving `NetworkPolicy` bind enforcement actually
//! prevents `HttpTransport`/`WebSocketServer` from starting on a
//! non-loopback address, rather than just logging a warning.

use mcp_rs::security::NetworkPolicy;
use mcp_rs::transport::http::{HttpConfig, HttpTransport};
use mcp_rs::transport::websocket::{ServerConfig, WebSocketServer};
use std::net::SocketAddr;

#[tokio::test]
async fn http_start_server_rejects_unspecified_bind_by_default() {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let config = HttpConfig {
        bind_addr,
        ..HttpConfig::default()
    };
    let transport = HttpTransport::new(config).unwrap();

    let result = transport.start_server().await;
    assert!(
        result.is_err(),
        "expected start_server() to reject an unspecified bind under the default policy"
    );
}

#[tokio::test]
async fn http_start_server_allows_unspecified_bind_when_policy_disabled() {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let config = HttpConfig {
        bind_addr,
        network_policy: NetworkPolicy {
            reject_external_connections: false,
            ..NetworkPolicy::default()
        },
        ..HttpConfig::default()
    };
    let transport = HttpTransport::new(config).unwrap();

    let result = transport.start_server().await;
    assert!(
        result.is_ok(),
        "expected start_server() to succeed once reject_external_connections is disabled: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn websocket_start_rejects_unspecified_bind_by_default() {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:0".parse().unwrap(),
        ..ServerConfig::default()
    };
    let mut server = WebSocketServer::new(config);

    let result = server.start().await;
    assert!(
        result.is_err(),
        "expected WebSocketServer::start() to reject an unspecified bind under the default policy"
    );
}

#[tokio::test]
async fn websocket_start_allows_unspecified_bind_when_policy_disabled() {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:0".parse().unwrap(),
        network_policy: NetworkPolicy {
            reject_external_connections: false,
            ..NetworkPolicy::default()
        },
        ..ServerConfig::default()
    };
    let mut server = WebSocketServer::new(config);

    let result = server.start().await;
    assert!(
        result.is_ok(),
        "expected WebSocketServer::start() to succeed once reject_external_connections is disabled: {:?}",
        result.err()
    );
}
