//! Integration tests proving `NetworkPolicy` bind enforcement actually
//! prevents `HttpTransport`/`WebSocketServer` from starting on a
//! non-loopback address, rather than just logging a warning.

use mcp_rs::security::NetworkPolicy;
use mcp_rs::transport::http::{ApiKeyEntry, HttpConfig, HttpTransport};
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

fn external_config() -> HttpConfig {
    HttpConfig {
        bind_addr: "0.0.0.0:0".parse().unwrap(),
        network_policy: NetworkPolicy {
            reject_external_connections: false,
            ..NetworkPolicy::default()
        },
        ..HttpConfig::default()
    }
}

/// ループバック以外で待ち受けるには、認証が要る（#540）。以前は、認証が
/// 無くても警告だけで起動し、ネットワークの誰でも全部のツールを使えた
#[tokio::test]
async fn http_start_server_rejects_unspecified_bind_without_authentication() {
    let transport = HttpTransport::new(external_config()).unwrap();
    let err = transport
        .start_server()
        .await
        .expect_err("an external bind without authentication must not start")
        .to_string();
    assert!(err.contains("without authentication"), "{err}");
    assert!(err.contains("allow_unauthenticated_external"), "{err}");
}

#[tokio::test]
async fn http_start_server_allows_unspecified_bind_with_an_api_key() {
    let config = HttpConfig {
        auth_enabled: true,
        api_keys: vec![ApiKeyEntry {
            key: "a-long-enough-test-key".to_string(),
            name: "test".to_string(),
            user_id: "user-1".to_string(),
            tools: None,
        }],
        ..external_config()
    };
    let transport = HttpTransport::new(config).unwrap();
    let result = transport.start_server().await;
    assert!(
        result.is_ok(),
        "expected start_server() to succeed with reject_external_connections disabled and an API key: {:?}",
        result.err()
    );
}

/// 認証は前に置いたもの（リバースプロキシ等）がするとき、明示して許せる
#[tokio::test]
async fn http_start_server_allows_unspecified_bind_when_explicitly_allowed() {
    let config = HttpConfig {
        allow_unauthenticated_external: true,
        ..external_config()
    };
    let transport = HttpTransport::new(config).unwrap();
    let result = transport.start_server().await;
    assert!(
        result.is_ok(),
        "expected start_server() to succeed with allow_unauthenticated_external: {:?}",
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
