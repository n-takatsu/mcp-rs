//! HTTP Transport implementation for MCP-RS
//!
//! This module provides HTTP-based JSON-RPC transport for MCP communication.

use crate::{
    error::{Error, Result},
    security::{
        AntiReplayConfig, AntiReplayMiddleware, ApiKeyConfig, ApiKeyManager, AuditLogger,
        IDSConfig, IntrusionDetectionSystem, NetworkPolicy, RecommendedAction, ReplayError,
        RequestData, SecurityHeaders,
    },
    transport::{
        oauth::{www_authenticate, OAuthConfig, OAuthValidator, TokenError},
        websocket::server::{ConnectionId, MessageHandler, WebSocketConnectionInfo},
        ConnectionStats, Transport, TransportCapabilities, TransportError, TransportInfo,
        TransportType,
    },
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket},
        ConnectInfo, DefaultBodyLimit, State, WebSocketUpgrade,
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Extension, Router,
};
use futures::{SinkExt, StreamExt};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder as HyperConnectionBuilder,
    service::TowerToHyperService,
};
use rustls::{
    crypto::ring::default_provider,
    pki_types::{CertificateDer, PrivateKeyDer},
    server::WebPkiClientVerifier,
    version::TLS13,
    RootCertStore, ServerConfig as RustlsServerConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tower_http::{cors::CorsLayer, timeout::RequestBodyTimeoutLayer};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// HTTP Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Bind address for HTTP server
    pub bind_addr: SocketAddr,
    /// Enable CORS
    pub cors_enabled: bool,
    /// Maximum request size in bytes: the HTTP request body, and each
    /// WebSocket message and frame (#540). A larger body is answered with
    /// 413, and a larger WebSocket message closes the connection.
    pub max_request_size: usize,
    /// Time limit in milliseconds for receiving a request (#540): the TLS
    /// handshake and the request headers as a whole (also the wait for the
    /// next request on a kept-alive connection), and each wait for more of
    /// the request body (not the whole body). It does not limit how long a
    /// tool call takes.
    pub timeout_ms: u64,
    /// Network access policy
    #[serde(default)]
    pub network_policy: NetworkPolicy,
    /// Enable direct TLS termination in the application
    #[serde(default)]
    pub tls_enabled: bool,
    /// PEM-encoded certificate chain for HTTPS listener
    pub tls_cert_path: Option<String>,
    /// PEM-encoded private key for HTTPS listener
    pub tls_key_path: Option<String>,
    /// Require mTLS client certificate authentication
    #[serde(default)]
    pub mtls_enabled: bool,
    /// PEM-encoded CA certificate(s) used for mTLS client certificate validation
    pub mtls_ca_cert_path: Option<String>,
    /// Enforce HTTPS-only access via proxy-forwarded protocol headers
    #[serde(default)]
    pub enforce_https: bool,
    /// Minimum TLS version expected from forwarded TLS metadata
    pub min_tls_version: Option<String>,
    /// Add Strict-Transport-Security header to all successful responses
    #[serde(default)]
    pub hsts_enabled: bool,
    /// HSTS max-age in seconds
    pub hsts_max_age_seconds: u64,
    /// Add includeSubDomains to HSTS header
    #[serde(default)]
    pub hsts_include_subdomains: bool,
    /// Add preload token to HSTS header
    #[serde(default)]
    pub hsts_preload: bool,
    /// Enforce certificate pinning based on forwarded certificate fingerprint header
    #[serde(default)]
    pub certificate_pinning_enabled: bool,
    /// Allowed SHA-256 certificate fingerprints (hex, optional colon separators)
    #[serde(default)]
    pub pinned_certificates_sha256: Vec<String>,
    /// Header name containing the forwarded certificate SHA-256 fingerprint
    pub certificate_pin_header: String,
    /// Enforce nonce/timestamp replay protection on incoming requests
    /// (requires clients to send `X-Nonce`/`X-Timestamp` headers).
    /// Defaults to `false` since existing HTTP clients do not send these
    /// headers yet — enable explicitly once client-side support is in place.
    #[serde(default)]
    pub anti_replay_enabled: bool,
    /// Run incoming requests through the intrusion detection/prevention
    /// system (signature-based, behavioral, and network-pattern detection),
    /// automatically blocking a source IP for a period - or permanently,
    /// for the most severe detections - once it triggers one. Defaults to
    /// `false`: detection has a per-request cost and false positives would
    /// reject legitimate traffic, so this stays opt-in.
    #[serde(default)]
    pub ids_enabled: bool,
    /// When IDS/IPS is enabled, take the client IP for detection and
    /// blocking from the `X-Forwarded-For`/`X-Real-IP` headers instead of
    /// the raw TCP peer address. Only enable this when this transport sits
    /// directly behind a trusted reverse proxy that sets these headers
    /// itself - otherwise any client can forge them, either evading IP
    /// blocking entirely or getting an arbitrary victim IP blocklisted.
    /// Defaults to `false` (use the raw peer address), since without a
    /// known trusted proxy in front, that peer address behind a
    /// proxy/load balancer would otherwise be blocklisted instead of the
    /// actual attacker, blocking every client behind it.
    #[serde(default)]
    pub ids_trust_forwarded_for: bool,
    /// Mount a `/ws` WebSocket upgrade endpoint on this same listener, so it
    /// shares this transport's TLS termination, `enforce_https`, HSTS, and
    /// certificate pinning instead of running as a separate, unencrypted
    /// listener. Defaults to `false` (no new attack surface unless opted in).
    #[serde(default)]
    pub enable_websocket_upgrade: bool,
    /// Maximum concurrent `/ws` connections before new upgrades are rejected
    /// with 503. Only meaningful when `enable_websocket_upgrade` is `true`.
    #[serde(default = "default_websocket_max_connections")]
    pub websocket_max_connections: usize,
    /// Require a valid API key (`Authorization: Bearer <key>` or
    /// `X-Api-Key` header) on every HTTP/WebSocket request. STDIO
    /// transport is unaffected (local pipe, implicit trust model) - this
    /// only gates the HTTP/WS listener, which can be exposed on a real
    /// network interface. Defaults to `false` for backward compatibility,
    /// but strongly recommended whenever `bind_addr` is not loopback-only.
    #[serde(default)]
    pub auth_enabled: bool,
    /// Valid API keys when `auth_enabled = true`. Each `key` is hashed
    /// in-memory at startup and never logged or echoed back.
    #[serde(default)]
    pub api_keys: Vec<ApiKeyEntry>,
    /// Accept OAuth 2.1 access tokens (JWT) issued by an external
    /// authorization server, as described in the MCP specification
    /// (2025-06-18, "Authorization"), #420. When set, every HTTP/WebSocket
    /// request needs either a valid access token or (when `auth_enabled`)
    /// a valid API key. Also serves the Protected Resource Metadata
    /// (RFC 9728). `None` (the default) disables it.
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
    /// Origins allowed in the `Origin` header, in addition to loopback
    /// origins (`http(s)://localhost`, `127.0.0.1`, `[::1]`, any port), #484.
    /// Compared with the whole origin (`scheme://host[:port]`, ignoring
    /// case). `"*"` allows any origin. The MCP specification requires
    /// servers to validate `Origin` (DNS rebinding) and to answer an
    /// invalid one with 403. Requests without an `Origin` header -
    /// non-browser clients - are always accepted.
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Maximum number of TCP connections served at the same time (#540).
    /// Further connections wait in the operating system's queue until one
    /// closes. Includes WebSocket connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Close a `/ws` connection when nothing, not even an answer to the
    /// server's pings, arrives for this many seconds (#540). The server
    /// pings every 30 seconds (or every half of this, when shorter), so a
    /// live but quiet client stays connected. `0` disables the limit and
    /// the pings.
    #[serde(default = "default_websocket_idle_timeout_secs")]
    pub websocket_idle_timeout_secs: u64,
    /// Start even when `bind_addr` is not loopback and no authentication is
    /// configured (no API keys, OAuth, or mTLS), #540. Without it, such a
    /// configuration refuses to start, since anyone who can reach the
    /// address could use every tool. Only for deployments where something
    /// in front of mcp-rs (such as a reverse proxy) authenticates clients.
    #[serde(default)]
    pub allow_unauthenticated_external: bool,
}

fn default_websocket_max_connections() -> usize {
    1000
}

fn default_max_connections() -> usize {
    1024
}

fn default_websocket_idle_timeout_secs() -> u64 {
    300
}

/// A single API key accepted by this transport when `auth_enabled = true`.
/// Distinct from `security::auth::ApiKey` (the runtime representation,
/// which includes hash/usage/permission bookkeeping) - this is just the
/// minimal config-facing shape. `Debug` does not show `key` (#543).
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    /// The raw key value clients must present. Hashed on import; never
    /// stored or logged in plaintext after startup.
    pub key: String,
    /// Human-readable label for this key (e.g. for audit/log messages).
    pub name: String,
    /// Identifier for whoever this key was issued to.
    pub user_id: String,
    /// Tools this key may use (#52): tool names, patterns with `*` (e.g.
    /// `fs__*`), or `@read_only` (tools whose annotations say they are
    /// read-only). `None` (the default) allows every tool. Other tools are
    /// hidden from `tools/list`, and calls to them are refused.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

impl std::fmt::Debug for ApiKeyEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyEntry")
            .field("key", &"<redacted>")
            .field("name", &self.name)
            .field("user_id", &self.user_id)
            .field("tools", &self.tools)
            .finish()
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8081".parse().unwrap(),
            cors_enabled: true,
            max_request_size: 1024 * 1024, // 1MB
            timeout_ms: 30000,             // 30 seconds
            network_policy: NetworkPolicy::default(),
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            mtls_enabled: false,
            mtls_ca_cert_path: None,
            enforce_https: false,
            min_tls_version: Some("1.3".to_string()),
            hsts_enabled: true,
            hsts_max_age_seconds: 31536000,
            hsts_include_subdomains: true,
            hsts_preload: false,
            certificate_pinning_enabled: false,
            pinned_certificates_sha256: Vec::new(),
            certificate_pin_header: "x-tls-cert-sha256".to_string(),
            anti_replay_enabled: false,
            ids_enabled: false,
            ids_trust_forwarded_for: false,
            enable_websocket_upgrade: false,
            websocket_max_connections: default_websocket_max_connections(),
            auth_enabled: false,
            api_keys: Vec::new(),
            oauth: None,
            allowed_origins: Vec::new(),
            max_connections: default_max_connections(),
            websocket_idle_timeout_secs: default_websocket_idle_timeout_secs(),
            allow_unauthenticated_external: false,
        }
    }
}

/// Statistics for HTTP transport
#[derive(Debug, Clone, Default)]
struct HttpStats {
    total_requests: u64,
    total_responses: u64,
    total_errors: u64,
    total_bytes_sent: u64,
    total_bytes_received: u64,
    total_response_time_ms: u64,
    started_at: Option<Instant>,
}

/// HTTP/WebSocketで受け付けたJSON-RPCメッセージを、その場で処理する
/// ハンドラー（#400）。
///
/// 設定すると、受け付けたメッセージを内部のチャンネル（`receive_message` /
/// `send_message`）に流さず、リクエストごとに直接このハンドラーで処理する。
/// チャンネル経由では応答を`id`で待ち合わせるため、同じ`id`を使う複数の
/// クライアント（多くのクライアントは`id`を1から振る）の応答が入れ替わり
/// うる。直接処理すれば、各HTTPリクエストにその応答がそのまま返る。
#[async_trait]
pub trait JsonRpcRequestHandler: Send + Sync {
    /// 1件のJSON-RPCメッセージ（JSON文字列）を処理し、応答（JSON-RPCの
    /// 応答オブジェクト）を返す。通知（`id`なし）は`Ok(None)`。JSON-RPCの
    /// メッセージとして不正な場合は`Err`。
    ///
    /// `caller`は、認証で分かった呼び出し元と、その呼び出し元が使えるツール
    /// （#52）。ハンドラーは、使えないツールを一覧に出さず、呼び出しも
    /// 拒否する。
    async fn handle_jsonrpc(
        &self,
        message: &str,
        caller: &crate::mcp::Caller,
    ) -> std::result::Result<Option<Value>, String>;

    /// `handle_jsonrpc`と同じ。`protocol_version`は、リクエストの
    /// `MCP-Protocol-Version`ヘッダーの版（対応する版のどれか、#484）。
    /// ヘッダーが無いか、`initialize`では`None`。既定の実装は版を使わずに
    /// `handle_jsonrpc`を呼ぶ。
    async fn handle_jsonrpc_with_protocol_version(
        &self,
        message: &str,
        caller: &crate::mcp::Caller,
        protocol_version: Option<&'static str>,
    ) -> std::result::Result<Option<Value>, String> {
        let _ = protocol_version;
        self.handle_jsonrpc(message, caller).await
    }
}

/// `Origin`ヘッダーで受け付けるオリジン（#484）。MCPの仕様（Streamable
/// HTTP）は、DNS rebinding対策として`Origin`を検証し、不正なら403で答える
/// ことを求める。ループバック（`localhost`・`127.0.0.1`等・`[::1]`、ポートは
/// 問わない）と、設定の`allowed_origins`を受け付ける。`Origin`の無い
/// リクエスト（ブラウザー以外のクライアント）は受け付ける。
#[derive(Debug, Default)]
struct OriginPolicy {
    /// `"*"`があれば、どのオリジンも受け付ける
    allow_any: bool,
    /// 小文字にし、末尾の`/`を除いたオリジン
    allowed: HashSet<String>,
}

impl OriginPolicy {
    fn new(allowed_origins: &[String]) -> Self {
        let mut policy = Self::default();
        for origin in allowed_origins {
            let origin = normalize_origin(origin);
            if origin == "*" {
                policy.allow_any = true;
            } else if !origin.is_empty() {
                policy.allowed.insert(origin);
            }
        }
        policy
    }

    fn allows(&self, origin: &str) -> bool {
        if self.allow_any {
            return true;
        }
        let origin = normalize_origin(origin);
        is_loopback_origin(&origin) || self.allowed.contains(&origin)
    }
}

fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// `http(s)://`のループバックのオリジンか（`null`等は違う）
fn is_loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    if rest.contains(['/', '@', '?', '#']) {
        return false;
    }
    // ホスト（[IPv6]か、`:ポート`の前）
    let host = if let Some(v6) = rest.strip_prefix('[') {
        match v6.split_once(']') {
            Some((host, port)) if port.is_empty() || is_port_suffix(port) => host,
            _ => return false,
        }
    } else {
        match rest.split_once(':') {
            Some((host, port)) if is_port_suffix(&format!(":{port}")) => host,
            Some(_) => return false,
            None => rest,
        }
    };
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn is_port_suffix(text: &str) -> bool {
    text.strip_prefix(':')
        .is_some_and(|port| !port.is_empty() && port.parse::<u16>().is_ok())
}

/// `Origin`を検証するミドルウェア（#484・#488）。`/`・`/mcp`・`/ws`に
/// `route_layer`で付ける。ハンドラーの引数（`Json`の取り出し等）より先に
/// 動くので、許可していないオリジンには、本文が壊れていても必ず403で
/// 答える。Protected Resource Metadataは公開の情報なので付けない。
async fn origin_middleware(
    State(policy): State<Arc<OriginPolicy>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let remote_addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    match check_origin(&policy, request.headers(), remote_addr) {
        Ok(()) => next.run(request).await,
        Err(status) => status.into_response(),
    }
}

/// `/`・`/mcp`・`/ws`のCORS（#488）。許可するオリジンを`Origin`の検証と
/// 同じ`OriginPolicy`から決めるので、許可していないオリジンはブラウザーの
/// preflightの段階で止まる。`cors_enabled = false`ならCORSのヘッダーを
/// 付けない（別のオリジンのページからは、ブラウザーが応答を読ませない）。
fn mcp_cors_layer(cors_enabled: bool, policy: Arc<OriginPolicy>) -> CorsLayer {
    if !cors_enabled {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin: &HeaderValue, _| origin.to_str().is_ok_and(|o| policy.allows(o)),
        ))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
        .expose_headers(tower_http::cors::Any)
}

/// `Origin`ヘッダーを検証する（#484）。許可していないオリジンなら403。
fn check_origin(
    policy: &OriginPolicy,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
) -> std::result::Result<(), StatusCode> {
    let remote_addr = remote_addr
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        return Ok(());
    };
    match origin.to_str() {
        Ok(origin) if policy.allows(origin) => Ok(()),
        Ok(origin) => {
            warn!(
                "Rejected request from {} with Origin '{}' (not in allowed_origins)",
                remote_addr, origin
            );
            Err(StatusCode::FORBIDDEN)
        }
        Err(_) => {
            warn!(
                "Rejected request from {} with an invalid Origin header",
                remote_addr
            );
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// リクエストの`MCP-Protocol-Version`ヘッダーの版（#484）。ヘッダーが
/// 無ければ`None`（共有の版を使う）。`initialize`では見ない（版は本文で
/// 交渉する）。対応していない版なら、仕様どおり400で答える応答を返す。
fn requested_protocol_version(
    headers: &HeaderMap,
    request: &Value,
) -> std::result::Result<Option<&'static str>, Box<Response>> {
    let Some(value) = headers.get("mcp-protocol-version") else {
        return Ok(None);
    };
    if request.get("method").and_then(Value::as_str) == Some("initialize") {
        return Ok(None);
    }
    let requested = value.to_str().unwrap_or("").trim();
    if let Some(version) = crate::mcp::SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|v| *v == requested)
    {
        return Ok(Some(version));
    }
    warn!(
        "Rejected request with unsupported MCP-Protocol-Version '{}'",
        requested
    );
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": -32600,
            "message": format!(
                "Unsupported MCP-Protocol-Version: '{}' (supported: {})",
                requested,
                crate::mcp::SUPPORTED_PROTOCOL_VERSIONS.join(", ")
            )
        },
        "id": request.get("id").cloned().unwrap_or(Value::Null)
    });
    Err(Box::new(
        (StatusCode::BAD_REQUEST, Json(body)).into_response(),
    ))
}

/// `JsonRpcRequestHandler`で1件を処理する時間の上限。ツール側（WordPress・
/// プラグイン）にもそれぞれタイムアウトがあるため、それより長くとる。
const REQUEST_HANDLER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// `HttpTransport`が`#[derive(Debug)]`のままでいられるよう、ハンドラーを
/// 包む（`dyn JsonRpcRequestHandler`はDebugを実装しない）。
#[derive(Clone, Default)]
struct RequestHandlerSlot(Option<Arc<dyn JsonRpcRequestHandler>>);

impl std::fmt::Debug for RequestHandlerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "RequestHandler(set)"
        } else {
            "RequestHandler(unset)"
        })
    }
}

/// Shared state for HTTP transport
#[derive(Clone)]
struct HttpTransportState {
    request_handler: Option<Arc<dyn JsonRpcRequestHandler>>,
    request_sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
    network_policy: NetworkPolicy,
    tls_terminated_locally: bool,
    enforce_https: bool,
    min_tls_version: Option<String>,
    hsts_header_value: Option<String>,
    certificate_pinning_enabled: bool,
    pinned_certificates_sha256: Arc<HashSet<String>>,
    certificate_pin_header: String,
    anti_replay: Option<Arc<AntiReplayMiddleware>>,
    ids: Option<Arc<IntrusionDetectionSystem>>,
    ids_trust_forwarded_for: bool,
    api_key_manager: Option<Arc<RwLock<ApiKeyManager>>>,
    /// APIキー（`ApiKey::id`）→ 名前と使えるツール（#52）
    api_key_callers: Arc<HashMap<String, crate::mcp::Caller>>,
    oauth: Option<Arc<OAuthValidator>>,
    /// `/ws`の接続 → 呼び出し元（接続時の認証で決まる、#52）
    ws_callers: Arc<tokio::sync::Mutex<HashMap<ConnectionId, crate::mcp::Caller>>>,
    audit_logger: Arc<AuditLogger>,
    websocket_upgrade_enabled: bool,
    // Reserves capacity atomically at upgrade time (before the handshake
    // completes and the connection is actually inserted into
    // `websocket_connections`), unlike checking that map's length directly,
    // which leaves a TOCTOU window where concurrent upgrades can all pass
    // the check before any of them registers a connection.
    websocket_semaphore: Arc<tokio::sync::Semaphore>,
    websocket_connections: Arc<tokio::sync::Mutex<HashMap<ConnectionId, WebSocketConnectionInfo>>>,
    websocket_total_connections: Arc<AtomicU64>,
    websocket_handler: Arc<dyn MessageHandler>,
    /// WebSocketのメッセージ・フレームの大きさの上限（#540）
    websocket_max_message_size: usize,
    /// WebSocketの無通信の時間の上限（#540）。`None`は無制限
    websocket_idle_timeout: Option<std::time::Duration>,
    /// 認証の失敗を監査ログに書く数の制限（#540）
    auth_failure_audit: Arc<AuthFailureAuditLimiter>,
}

/// 認証を通った呼び出し元（#540）。本文を読む前のミドルウェアで決め、
/// リクエストの拡張に入れてハンドラーに渡す
#[derive(Clone)]
struct Authenticated {
    caller: crate::mcp::Caller,
    /// OAuthのアクセストークンの期限（UNIX時刻の秒、許容の秒数を足したもの）。
    /// `/ws`の接続は、この時刻に閉じる。APIキー・認証なしは`None`
    expires_at: Option<u64>,
}

/// 認証の失敗を監査ログに書くのは、1分あたりこの数まで（#540）
const AUTH_FAILURE_AUDIT_PER_MINUTE: u32 = 100;

/// 認証の失敗を監査ログに書く数の制限（#540）。大量の失敗で、監査ログが
/// ローテーションされ、本物の記録が消えないようにする。書かなかった数は、
/// その1分が終わったときに、1件の記録にまとめる（PR #548のレビュー: 以前は
/// 次の失敗が来るまで書かれなかった）
#[derive(Debug)]
struct AuthFailureAuditLimiter {
    window: std::sync::Mutex<AuthFailureWindow>,
}

#[derive(Debug)]
struct AuthFailureWindow {
    started: Instant,
    /// 監査ログに書く、1分の始まりの時刻
    started_at: chrono::DateTime<chrono::Utc>,
    recorded: u32,
    suppressed: u64,
}

/// 書かなかった認証の失敗の数と、その1分の始まりの時刻
#[derive(Debug, PartialEq)]
struct SuppressedAuthFailures {
    count: u64,
    since: chrono::DateTime<chrono::Utc>,
}

impl AuthFailureAuditLimiter {
    fn new() -> Self {
        Self {
            window: std::sync::Mutex::new(AuthFailureWindow {
                started: Instant::now(),
                started_at: chrono::Utc::now(),
                recorded: 0,
                suppressed: 0,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AuthFailureWindow> {
        self.window
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 1分が過ぎていれば、次の1分を始める。書かなかった数があれば返す
    fn roll(window: &mut AuthFailureWindow, now: Instant) -> Option<SuppressedAuthFailures> {
        if now.duration_since(window.started) < std::time::Duration::from_secs(60) {
            return None;
        }
        let report = (window.suppressed > 0).then_some(SuppressedAuthFailures {
            count: window.suppressed,
            since: window.started_at,
        });
        *window = AuthFailureWindow {
            started: now,
            started_at: chrono::Utc::now(),
            recorded: 0,
            suppressed: 0,
        };
        report
    }

    /// 失敗を1件数える。戻り値は（この失敗を書くか、前の1分に書かなかった数）
    fn admit(&self, now: Instant) -> (bool, Option<SuppressedAuthFailures>) {
        let mut window = self.lock();
        let report = Self::roll(&mut window, now);
        if window.recorded < AUTH_FAILURE_AUDIT_PER_MINUTE {
            window.recorded += 1;
            (true, report)
        } else {
            window.suppressed += 1;
            (false, report)
        }
    }

    /// 1分が過ぎていれば、書かなかった数を返す（定期的に呼ぶ）
    fn take_report(&self, now: Instant) -> Option<SuppressedAuthFailures> {
        Self::roll(&mut self.lock(), now)
    }
}

/// 書かなかった認証の失敗の数を、監査ログに1件で書く
async fn audit_suppressed_auth_failures(
    audit_logger: &AuditLogger,
    report: SuppressedAuthFailures,
) {
    let entry = crate::security::AuditLogEntry::new(
        crate::security::AuditLevel::Warning,
        crate::security::AuditCategory::Authentication,
        format!(
            "Authentication failed {} more times in the minute from {} (not recorded one by one: at most {} per minute are recorded, without their IP addresses)",
            report.count,
            report.since.format("%Y-%m-%dT%H:%M:%SZ"),
            AUTH_FAILURE_AUDIT_PER_MINUTE
        ),
    )
    .with_result("DENIED".to_string());
    if let Err(e) = audit_logger.log(entry).await {
        warn!(
            "Failed to write audit log entry for suppressed authentication failures: {}",
            e
        );
    }
}

/// HTTP Transport implementation
#[derive(Debug)]
pub struct HttpTransport {
    config: HttpConfig,
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<String>>>,
    sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
    /// `start_server()`で実際に待ち受けたアドレス。同期の`transport_info()`
    /// からも待たずに読めるよう、async版ではなく`std::sync::RwLock`を使う
    /// （ロックを持ったまま`.await`しない）（#265）
    bound_addr: Arc<std::sync::RwLock<Option<SocketAddr>>>,
    request_handler: RequestHandlerSlot,
    /// 認証の失敗等を記録する監査ログ（#55）。既定はメモリにだけ残すもの
    audit_logger: Arc<AuditLogger>,
}

impl HttpTransport {
    /// 受け付けたJSON-RPCメッセージを直接処理するハンドラーを設定する
    /// （#400）。`start_server()`の前に呼ぶ。
    /// 認証の失敗・リプレイ攻撃の検出等を記録する監査ログを設定する（#55）。
    /// 本体は、ツール呼び出しと同じ監査ログ（設定の`[audit]`）を渡す
    pub fn with_audit_logger(mut self, audit_logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = audit_logger;
        self
    }

    pub fn with_request_handler(mut self, handler: Arc<dyn JsonRpcRequestHandler>) -> Self {
        self.request_handler = RequestHandlerSlot(Some(handler));
        self
    }

    /// Create a new HTTP transport
    pub fn new(config: HttpConfig) -> Result<Self> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        let stats = HttpStats {
            started_at: Some(Instant::now()),
            ..Default::default()
        };

        Ok(Self {
            config,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
            sender,
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(stats)),
            bound_addr: Arc::new(std::sync::RwLock::new(None)),
            request_handler: RequestHandlerSlot::default(),
            audit_logger: Arc::new(AuditLogger::with_defaults()),
        })
    }

    /// Start the HTTP server
    pub async fn start_server(&self) -> Result<()> {
        self.set_bound_addr(None);

        // Validate bind address
        self.config
            .network_policy
            .validate_bind_address(&self.config.bind_addr)
            .map_err(|e| Error::TransportError(TransportError::Configuration(e.to_string())))?;

        validate_tls_security_settings(&self.config)
            .map_err(|e| Error::TransportError(TransportError::Configuration(e)))?;

        if let Some(reason) = unauthenticated_external_bind(&self.config) {
            return Err(Error::TransportError(TransportError::Configuration(reason)));
        }
        if self.config.max_connections == 0 {
            return Err(Error::TransportError(TransportError::Configuration(
                "max_connections must be greater than 0".to_string(),
            )));
        }

        if self.config.enable_websocket_upgrade && self.config.websocket_max_connections == 0 {
            return Err(Error::TransportError(TransportError::Configuration(
                "websocket_max_connections must be greater than 0 when enable_websocket_upgrade is enabled"
                    .to_string(),
            )));
        }

        if self.config.auth_enabled && self.config.api_keys.is_empty() {
            return Err(Error::TransportError(TransportError::Configuration(
                "auth_enabled requires at least one entry in api_keys".to_string(),
            )));
        }

        if self.config.auth_enabled {
            // An empty `key` would hash to the same value for every entry
            // configured this way, so a single blank entry (e.g. from a
            // template/env-substitution bug) would let any client
            // authenticate with an empty X-Api-Key, regardless of how many
            // real keys are also configured - reject that outright rather
            // than importing it.
            if self
                .config
                .api_keys
                .iter()
                .any(|entry| entry.key.is_empty())
            {
                return Err(Error::TransportError(TransportError::Configuration(
                    "api_keys entries must not have an empty key".to_string(),
                )));
            }

            // Two entries sharing a `key` would otherwise import silently
            // (the later one overwriting the earlier one in the manager's
            // hash-keyed storage), misattributing that key's usage to
            // whichever entry happened to be imported last.
            let mut seen_keys = HashSet::new();
            for entry in &self.config.api_keys {
                if !seen_keys.insert(&entry.key) {
                    return Err(Error::TransportError(TransportError::Configuration(
                        format!("api_keys contains a duplicate key (entry '{}')", entry.name),
                    )));
                }
            }
        }

        let mut api_key_callers = HashMap::new();
        let api_key_manager = if self.config.auth_enabled {
            let mut manager = ApiKeyManager::new(ApiKeyConfig::default());
            for entry in &self.config.api_keys {
                let access = crate::mcp::ToolAccess::from_patterns(entry.tools.as_deref())
                    .map_err(|e| {
                        Error::TransportError(TransportError::Configuration(format!(
                            "API key '{}' has invalid tools: {}",
                            entry.name, e
                        )))
                    })?;
                let api_key = manager
                    .import_key(&entry.key, entry.name.clone(), entry.user_id.clone(), None)
                    .map_err(|e| {
                        Error::TransportError(TransportError::Configuration(format!(
                            "Failed to import configured API key '{}': {}",
                            entry.name, e
                        )))
                    })?;
                api_key_callers.insert(
                    api_key.id,
                    crate::mcp::Caller::new(format!("api_key:{}", entry.name), access)
                        .with_subject(Some(entry.user_id.clone())),
                );
            }
            Some(Arc::new(RwLock::new(manager)))
        } else {
            None
        };

        // ツールの制限（#52）は、リクエストを直接処理する場合
        // （with_request_handler）だけ効く。チャンネル経由では、どの
        // 呼び出し元のメッセージかを渡せないため、起動しない
        let restricts_tools = (self.config.auth_enabled
            && self.config.api_keys.iter().any(|e| e.tools.is_some()))
            || self
                .config
                .oauth
                .as_ref()
                .is_some_and(|o| !o.scope_tools.is_empty());
        if restricts_tools && self.request_handler.0.is_none() {
            return Err(Error::TransportError(TransportError::Configuration(
                "tool restrictions (api_keys tools / oauth scope_tools) require a request handler (HttpTransport::with_request_handler)".to_string(),
            )));
        }

        let oauth = match &self.config.oauth {
            Some(config) => {
                let validator = Arc::new(
                    OAuthValidator::new(config.clone())
                        .map_err(|e| Error::TransportError(TransportError::Configuration(e)))?,
                );
                info!(
                    "🔐 OAuth: accepting access tokens from {} for {}",
                    config.issuer, config.resource
                );
                let prefetch = validator.clone();
                tokio::spawn(async move { prefetch.prefetch().await });
                Some(validator)
            }
            None => None,
        };

        let pinned_certificates_sha256: HashSet<String> = self
            .config
            .pinned_certificates_sha256
            .iter()
            .filter_map(|fingerprint| normalize_fingerprint(fingerprint))
            .collect();

        let ws_callers = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let state = HttpTransportState {
            request_sender: self.sender.clone(),
            pending_responses: self.pending_responses.clone(),
            stats: self.stats.clone(),
            network_policy: self.config.network_policy.clone(),
            tls_terminated_locally: self.config.tls_enabled,
            enforce_https: self.config.enforce_https,
            min_tls_version: self.config.min_tls_version.clone(),
            hsts_header_value: if self.config.hsts_enabled {
                Some(build_hsts_header_value(
                    self.config.hsts_max_age_seconds,
                    self.config.hsts_include_subdomains,
                    self.config.hsts_preload,
                ))
            } else {
                None
            },
            certificate_pinning_enabled: self.config.certificate_pinning_enabled
                && !self.config.tls_enabled,
            pinned_certificates_sha256: Arc::new(pinned_certificates_sha256.clone()),
            certificate_pin_header: self.config.certificate_pin_header.clone(),
            anti_replay: if self.config.anti_replay_enabled {
                Some(Arc::new(AntiReplayMiddleware::new(
                    AntiReplayConfig::default(),
                )))
            } else {
                None
            },
            ids: if self.config.ids_enabled {
                Some(Arc::new(
                    IntrusionDetectionSystem::new(IDSConfig {
                        // ids_enabled opting into this wiring at all means
                        // the caller wants real IPS enforcement, not just
                        // detection/alerting - IDSConfig::default() leaves
                        // auto_block_enabled false, which would otherwise
                        // silently make check_ids never block anything.
                        auto_block_enabled: true,
                        ..IDSConfig::default()
                    })
                    .await
                    .map_err(|e| {
                        Error::TransportError(TransportError::Configuration(format!(
                            "IDS initialization failed: {e}"
                        )))
                    })?,
                ))
            } else {
                None
            },
            ids_trust_forwarded_for: self.config.ids_trust_forwarded_for,
            api_key_manager,
            api_key_callers: Arc::new(api_key_callers),
            oauth: oauth.clone(),
            ws_callers: ws_callers.clone(),
            audit_logger: self.audit_logger.clone(),
            websocket_upgrade_enabled: self.config.enable_websocket_upgrade,
            websocket_semaphore: Arc::new(tokio::sync::Semaphore::new(
                self.config.websocket_max_connections,
            )),
            websocket_connections: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            websocket_total_connections: Arc::new(AtomicU64::new(0)),
            request_handler: self.request_handler.0.clone(),
            websocket_handler: Arc::new(JsonRpcWsHandler {
                request_handler: self.request_handler.0.clone(),
                callers: ws_callers,
                request_sender: self.sender.clone(),
                pending_responses: self.pending_responses.clone(),
                stats: self.stats.clone(),
            }),
            websocket_max_message_size: self.config.max_request_size,
            websocket_idle_timeout: (self.config.websocket_idle_timeout_secs > 0)
                .then(|| std::time::Duration::from_secs(self.config.websocket_idle_timeout_secs)),
            auth_failure_audit: Arc::new(AuthFailureAuditLimiter::new()),
        };
        let request_timeout = std::time::Duration::from_millis(self.config.timeout_ms.max(1));
        // 書かなかった認証の失敗の数を、1分ごとに監査ログに書く。失敗が
        // 止まっても、記録が残るようにする（PR #548のレビュー）。サーバーが
        // 無くなったら（状態を持つ`Weak`が切れたら）終わる
        {
            let limiter = Arc::downgrade(&state.auth_failure_audit);
            let audit_logger = state.audit_logger.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tick.tick().await;
                    let Some(limiter) = limiter.upgrade() else {
                        return;
                    };
                    if let Some(report) = limiter.take_report(Instant::now()) {
                        audit_suppressed_auth_failures(&audit_logger, report).await;
                    }
                }
            });
        }

        // MCPのエンドポイント: Originの検証（ミドルウェア）と、同じ
        // OriginPolicyから作るCORS（#488）
        let origin_policy = Arc::new(OriginPolicy::new(&self.config.allowed_origins));
        // ネットワークの制限と認証は、本文を読む前に確かめる（#540）。本文の
        // 大きさと、読む時間にも上限を付ける
        let mcp_routes = Router::new()
            .route("/", post(handle_jsonrpc_request))
            .route("/mcp", post(handle_jsonrpc_request))
            .route("/ws", get(handle_ws_upgrade))
            .route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .route_layer(axum::middleware::from_fn_with_state(
                origin_policy.clone(),
                origin_middleware,
            ))
            .layer(DefaultBodyLimit::max(self.config.max_request_size))
            .layer(RequestBodyTimeoutLayer::new(request_timeout))
            .layer(mcp_cors_layer(self.config.cors_enabled, origin_policy));
        // Protected Resource Metadata: 公開の情報なので、どのオリジンにも返す
        let mut metadata_routes = Router::new();
        if let Some(validator) = &oauth {
            // RFC 9728: resourceにパスがあればその後ろ。ルートにも置く
            // （パスを付けずに探すクライアントのため）
            let path = validator.config().resource_metadata_path();
            metadata_routes = metadata_routes.route(&path, get(handle_protected_resource_metadata));
            if path != crate::transport::oauth::PROTECTED_RESOURCE_METADATA_PATH {
                metadata_routes = metadata_routes.route(
                    crate::transport::oauth::PROTECTED_RESOURCE_METADATA_PATH,
                    get(handle_protected_resource_metadata),
                );
            }
        }
        let metadata_routes = metadata_routes.layer(if self.config.cors_enabled {
            CorsLayer::permissive()
        } else {
            CorsLayer::new()
        });
        let app = mcp_routes.merge(metadata_routes).with_state(state);

        info!(
            "Starting {} transport server on {}",
            if self.config.tls_enabled {
                "HTTPS"
            } else {
                "HTTP"
            },
            self.config.bind_addr
        );

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                Error::TransportError(TransportError::Configuration(format!(
                    "Failed to bind HTTP server: {}",
                    e
                )))
            })?;
        let local_addr = listener.local_addr().map_err(|e| {
            Error::TransportError(TransportError::Configuration(format!(
                "Failed to get bound HTTP address: {}",
                e
            )))
        })?;

        let tls_acceptor = if self.config.tls_enabled {
            Some(build_tls_acceptor(
                &self.config,
                &pinned_certificates_sha256,
            )?)
        } else {
            None
        };
        let limits = ConnectionLimits {
            network_policy: self.config.network_policy.clone(),
            max_connections: self.config.max_connections,
            timeout: request_timeout,
        };
        self.set_bound_addr(Some(local_addr));
        tokio::spawn(async move {
            serve_connections(listener, app, tls_acceptor, limits).await;
        });

        if self.config.bind_addr.port() == 0 {
            let protocol = if self.config.tls_enabled {
                "HTTPS"
            } else {
                "HTTP"
            };
            info!(
                "{} server bound to dynamic address {}",
                protocol, local_addr
            );
        }

        Ok(())
    }

    /// Returns the actual bound address once `start_server()` has completed,
    /// useful when `bind_addr` uses an OS-assigned ephemeral port (`:0`).
    pub async fn bound_addr(&self) -> Option<SocketAddr> {
        self.current_bound_addr()
    }

    fn current_bound_addr(&self) -> Option<SocketAddr> {
        // 値はOption<SocketAddr>だけで、書き込み中にパニックしても壊れた
        // 状態にはならないため、ポイズンは無視する
        *self
            .bound_addr
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_bound_addr(&self, addr: Option<SocketAddr>) {
        *self
            .bound_addr
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = addr;
    }
}

/// ループバック以外で待ち受けるのに、認証（APIキー・OAuth・mTLS）が
/// 無ければ、起動しない理由を返す（#540）。`allow_unauthenticated_external`
/// で明示して許したときは、警告だけ出す
fn unauthenticated_external_bind(config: &HttpConfig) -> Option<String> {
    let authenticated = config.auth_enabled || config.oauth.is_some() || config.mtls_enabled;
    if authenticated || config.bind_addr.ip().is_loopback() {
        return None;
    }
    if config.allow_unauthenticated_external {
        warn!(
            "⚠️  Listening on {} without authentication (allow_unauthenticated_external = true): \
             anyone who can reach it can use every tool, unless something in front of mcp-rs \
             authenticates clients",
            config.bind_addr
        );
        return None;
    }
    Some(format!(
        "refusing to listen on {} without authentication: anyone who can reach it could use \
         every tool. Configure API keys (auth_enabled = true and [[transport.http.api_keys]]) or \
         OAuth ([transport.http.oauth]). Only when something in front of mcp-rs authenticates \
         clients, set allow_unauthenticated_external = true",
        config.bind_addr
    ))
}

/// 接続の受け付けの制限（#540）
struct ConnectionLimits {
    network_policy: NetworkPolicy,
    max_connections: usize,
    /// TLSのハンドシェイク・ヘッダーを読む時間の上限
    timeout: std::time::Duration,
}

/// The service axum makes for one connection (the router with the peer
/// address as `ConnectInfo`)
type ConnectionService = axum::middleware::AddExtension<Router, ConnectInfo<SocketAddr>>;

/// 接続を受け付けて処理する（HTTPとHTTPS、#540）。以前は、HTTPは
/// `axum::serve`、HTTPSは自前のループで、どちらも時間と接続数の上限が
/// 無かった:
///
/// - 同時の接続は`max_connections`まで。超えた分は、OSの待ち行列で待つ
/// - ネットワークの制限（`reject_external_connections`・`ip_whitelist`）に
///   合わない接続は、TLSのハンドシェイクもヘッダーの読み取りもせずに閉じる
/// - TLSのハンドシェイクと、ヘッダーの読み取り（keep-aliveの次のリクエスト
///   を待つ時間も）に、`timeout`の上限を付ける
async fn serve_connections(
    listener: TcpListener,
    app: Router,
    tls_acceptor: Option<TlsAcceptor>,
    limits: ConnectionLimits,
) {
    let mut make_service = app.into_make_service_with_connect_info::<SocketAddr>();
    let connections = Arc::new(tokio::sync::Semaphore::new(limits.max_connections));

    loop {
        let Ok(permit) = connections.clone().acquire_owned().await else {
            return;
        };
        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                // ファイル記述子が足りない等。すぐにやり直すと空回りする
                error!("HTTP accept error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
        };
        if let Err(e) = limits.network_policy.validate_connection(&remote_addr) {
            debug!("Connection closed before reading it: {}", e);
            continue;
        }

        let tower_service = match make_service.call(remote_addr).await {
            Ok(service) => service,
            Err(e) => {
                error!("Failed to create HTTP service for {}: {}", remote_addr, e);
                continue;
            }
        };
        let tls_acceptor = tls_acceptor.clone();
        let timeout = limits.timeout;

        tokio::spawn(async move {
            let _permit = permit;
            match tls_acceptor {
                Some(acceptor) => {
                    let tls_stream =
                        match tokio::time::timeout(timeout, acceptor.accept(tcp_stream)).await {
                            Ok(Ok(stream)) => stream,
                            Ok(Err(e)) => {
                                warn!("TLS handshake failed for {}: {}", remote_addr, e);
                                return;
                            }
                            Err(_) => {
                                debug!("TLS handshake timed out for {}", remote_addr);
                                return;
                            }
                        };
                    serve_connection(tls_stream, tower_service, remote_addr, timeout).await;
                }
                None => {
                    serve_connection(tcp_stream, tower_service, remote_addr, timeout).await;
                }
            }
        });
    }
}

/// 1つの接続で、HTTP/1.1かHTTP/2のリクエストを処理する。ヘッダーを読む
/// 時間（HTTP/1.1）と、応答の無い接続を見つけるping（HTTP/2）に`timeout`を
/// 使う（#540）
async fn serve_connection<I>(
    io: I,
    service: ConnectionService,
    remote_addr: SocketAddr,
    timeout: std::time::Duration,
) where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(io);
    let mut builder = HyperConnectionBuilder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(timeout);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(timeout)
        .keep_alive_timeout(timeout);
    let hyper_service = TowerToHyperService::new(service);
    if let Err(e) = builder
        .serve_connection_with_upgrades(io, hyper_service)
        .await
    {
        // 時間切れ・相手が切った等。よくあることなので、エラーにはしない
        debug!("HTTP connection error for {}: {}", remote_addr, e);
    }
}

fn build_tls_acceptor(
    config: &HttpConfig,
    pinned_certificates_sha256: &HashSet<String>,
) -> Result<TlsAcceptor> {
    let _ = default_provider().install_default();

    let certificates = load_tls_certificates(config)?;
    let private_key = load_tls_private_key(config)?;

    validate_direct_certificate_pinning(config, pinned_certificates_sha256, &certificates)?;

    let mut server_config = if config.mtls_enabled {
        let client_roots = load_mtls_client_root_store(config)?;
        let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_roots))
            .build()
            .map_err(|e| {
                Error::TransportError(TransportError::Configuration(format!(
                    "Invalid mTLS client verifier configuration: {}",
                    e
                )))
            })?;

        RustlsServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certificates, private_key)
            .map_err(|e| {
                Error::TransportError(TransportError::Configuration(format!(
                    "Invalid TLS certificate configuration: {}",
                    e
                )))
            })?
    } else {
        RustlsServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)
            .map_err(|e| {
                Error::TransportError(TransportError::Configuration(format!(
                    "Invalid TLS certificate configuration: {}",
                    e
                )))
            })?
    };
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

fn load_tls_certificates(config: &HttpConfig) -> Result<Vec<CertificateDer<'static>>> {
    let cert_path = config.tls_cert_path.as_deref().ok_or_else(|| {
        Error::TransportError(TransportError::Configuration(
            "tls_cert_path is required when tls_enabled=true".to_string(),
        ))
    })?;

    let cert_file = File::open(cert_path).map_err(|e| {
        Error::TransportError(TransportError::Configuration(format!(
            "Failed to open TLS certificate {}: {}",
            cert_path, e
        )))
    })?;
    let mut cert_reader = BufReader::new(cert_file);
    let certificates = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::TransportError(TransportError::Configuration(format!(
                "Failed to parse TLS certificates: {}",
                e
            )))
        })?;

    if certificates.is_empty() {
        return Err(Error::TransportError(TransportError::Configuration(
            "TLS certificate chain is empty".to_string(),
        )));
    }

    Ok(certificates)
}

fn load_tls_private_key(config: &HttpConfig) -> Result<PrivateKeyDer<'static>> {
    let key_path = config.tls_key_path.as_deref().ok_or_else(|| {
        Error::TransportError(TransportError::Configuration(
            "tls_key_path is required when tls_enabled=true".to_string(),
        ))
    })?;

    let key_file = File::open(key_path).map_err(|e| {
        Error::TransportError(TransportError::Configuration(format!(
            "Failed to open TLS private key {}: {}",
            key_path, e
        )))
    })?;
    let mut key_reader = BufReader::new(key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| {
            Error::TransportError(TransportError::Configuration(format!(
                "Failed to parse TLS private key: {}",
                e
            )))
        })?
        .ok_or_else(|| {
            Error::TransportError(TransportError::Configuration(
                "TLS private key not found in key file".to_string(),
            ))
        })?;

    Ok(private_key)
}

fn load_mtls_client_root_store(config: &HttpConfig) -> Result<RootCertStore> {
    let ca_path = config.mtls_ca_cert_path.as_deref().ok_or_else(|| {
        Error::TransportError(TransportError::Configuration(
            "mtls_ca_cert_path is required when mtls_enabled=true".to_string(),
        ))
    })?;

    let ca_file = File::open(ca_path).map_err(|e| {
        Error::TransportError(TransportError::Configuration(format!(
            "Failed to open mTLS CA certificate {}: {}",
            ca_path, e
        )))
    })?;
    let mut ca_reader = BufReader::new(ca_file);
    let ca_certs = rustls_pemfile::certs(&mut ca_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            Error::TransportError(TransportError::Configuration(format!(
                "Failed to parse mTLS CA certificate(s): {}",
                e
            )))
        })?;

    if ca_certs.is_empty() {
        return Err(Error::TransportError(TransportError::Configuration(
            "mTLS CA certificate chain is empty".to_string(),
        )));
    }

    let mut root_store = RootCertStore::empty();
    let (added, ignored) = root_store.add_parsable_certificates(ca_certs);
    if added == 0 {
        return Err(Error::TransportError(TransportError::Configuration(
            "No valid CA certificate found for mTLS client authentication".to_string(),
        )));
    }
    if ignored > 0 {
        warn!(
            "Ignored {} unparsable mTLS CA certificate(s) while loading {}",
            ignored, ca_path
        );
    }

    Ok(root_store)
}

fn validate_direct_certificate_pinning(
    config: &HttpConfig,
    pinned_certificates_sha256: &HashSet<String>,
    certificates: &[CertificateDer<'static>],
) -> Result<()> {
    if !config.tls_enabled || !config.certificate_pinning_enabled {
        return Ok(());
    }

    let leaf_certificate = certificates
        .first()
        .ok_or_else(|| Error::Internal("TLS certificate chain is empty".to_string()))?;
    let leaf_fingerprint = certificate_fingerprint_sha256(leaf_certificate);

    if !pinned_certificates_sha256.contains(leaf_fingerprint.as_str()) {
        return Err(Error::TransportError(TransportError::Configuration(
            "Configured pinned_certificates_sha256 does not match the loaded TLS certificate"
                .to_string(),
        )));
    }

    info!("Direct TLS certificate pin validation succeeded");
    Ok(())
}

#[async_trait]
impl Transport for HttpTransport {
    type Error = TransportError;

    async fn start(&mut self) -> std::result::Result<(), Self::Error> {
        debug!("Starting HTTP transport server");
        self.start_server()
            .await
            .map_err(|e| TransportError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        debug!("Stopping HTTP transport server");
        // HTTP server stops automatically when dropped
        Ok(())
    }

    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error> {
        let message_str = serde_json::to_string(&message).map_err(|e| {
            TransportError::Internal(format!("Failed to serialize response: {}", e))
        })?;

        debug!("HTTP transport sending message: {}", message_str);

        // Check if this is a response to a pending request
        let id_value = serde_json::to_value(&message.id).ok();
        if let Some(id) = id_value {
            let mut pending = self.pending_responses.write().await;
            if let Some(response_tx) = pending.remove(&id) {
                // Send response through oneshot channel
                if response_tx.send(message).is_err() {
                    error!(
                        "Failed to send response - receiver dropped for ID: {:?}",
                        id
                    );
                }
                return Ok(());
            }
        }

        // Fallback: send through regular channel for notifications
        self.sender
            .send(message_str)
            .await
            .map_err(|e| TransportError::Internal(format!("HTTP send failed: {}", e)))?;

        Ok(())
    }

    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        let mut receiver = self.receiver.lock().await;

        match receiver.recv().await {
            Some(message_str) => {
                debug!("HTTP transport received message: {}", message_str);
                let request: JsonRpcRequest = serde_json::from_str(&message_str).map_err(|e| {
                    TransportError::Internal(format!("Failed to parse request: {}", e))
                })?;
                Ok(Some(request))
            }
            None => Ok(None), // Channel closed, no more messages
        }
    }

    fn is_connected(&self) -> bool {
        // HTTP server is always "connected" when running
        true
    }

    fn transport_info(&self) -> TransportInfo {
        // 起動前は設定のアドレス（ポート0のこともある）、起動後は実際に
        // 待ち受けたアドレス。ロックの競合で設定の値に戻ることはない（#265）
        let addr = self.current_bound_addr().unwrap_or(self.config.bind_addr);

        TransportInfo {
            transport_type: TransportType::Http { addr },
            description: "HTTP JSON-RPC transport for MCP communication".to_string(),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: false,
                max_message_size: Some(self.config.max_request_size),
                framing_methods: vec![],
            },
        }
    }

    fn connection_stats(&self) -> ConnectionStats {
        let stats = self.stats.blocking_read();

        let uptime = stats
            .started_at
            .map(|start| start.elapsed())
            .unwrap_or(std::time::Duration::from_secs(0));

        ConnectionStats {
            messages_sent: stats.total_responses,
            messages_received: stats.total_requests,
            bytes_sent: stats.total_bytes_sent,
            bytes_received: stats.total_bytes_received,
            uptime,
            last_activity: None, // TODO: Track last activity time
        }
    }
}

/// Handle incoming JSON-RPC HTTP requests
async fn handle_jsonrpc_request(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<HttpTransportState>,
    Extension(authenticated): Extension<Authenticated>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> std::result::Result<Response, StatusCode> {
    // ネットワークの制限と認証は、`auth_middleware`が本文を読む前に確かめた
    let caller = authenticated
        .caller
        .with_ip_address(Some(remote_addr.ip().to_string()));
    check_https_enforcement(
        state.enforce_https,
        state.tls_terminated_locally,
        &headers,
        remote_addr,
    )?;
    check_min_tls_version(
        state.min_tls_version.as_deref(),
        state.enforce_https,
        state.tls_terminated_locally,
        &headers,
        remote_addr,
    )?;
    check_certificate_pin(
        state.certificate_pinning_enabled,
        &state.certificate_pin_header,
        &state.pinned_certificates_sha256,
        &headers,
        remote_addr,
    )?;
    check_anti_replay_handshake(
        state.anti_replay.as_ref(),
        &state.audit_logger,
        &headers,
        remote_addr,
    )
    .await?;
    check_ids(
        state.ids.as_ref(),
        remote_addr,
        state.ids_trust_forwarded_for,
        &uri,
        &headers,
        Some(&request),
    )
    .await?;
    let protocol_version = match requested_protocol_version(&headers, &request) {
        Ok(version) => version,
        Err(response) => return Ok(*response),
    };

    let start_time = Instant::now();

    debug!(
        "Received HTTP JSON-RPC request from {}: {}",
        remote_addr, request
    );

    // Update statistics - request received
    {
        let mut stats = state.stats.write().await;
        stats.total_requests += 1;
        let request_size = serde_json::to_string(&request).unwrap_or_default().len() as u64;
        stats.total_bytes_received += request_size;
    }

    // Extract request ID for response correlation
    let request_id = request.get("id").cloned();

    // Convert request to string and send through channel
    // `blocking_write()` must not be used here: this is an async handler on
    // a tokio worker, where tokio's blocking lock methods panic (#374).
    let request_str = match serde_json::to_string(&request) {
        Ok(request_str) => request_str,
        Err(_) => {
            state.stats.write().await.total_errors += 1;
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    if let Some(handler) = state.request_handler.clone() {
        return respond_with_request_handler(
            &state,
            handler.as_ref(),
            &request_str,
            &caller,
            protocol_version,
            start_time,
        )
        .await;
    }

    // If request has ID, wait for actual response
    if let Some(id) = request_id {
        // Register the pending response *before* forwarding the request -
        // otherwise a core that processes and replies fast enough could call
        // send_message() before this entry exists, missing the pending map
        // and falling back to the general notification channel instead of
        // ever reaching this waiter (the same race JsonRpcWsHandler had).
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = state.pending_responses.write().await;
            pending.insert(id.clone(), response_tx);
        }

        if state.request_sender.send(request_str).await.is_err() {
            state.pending_responses.write().await.remove(&id);
            let mut stats = state.stats.write().await;
            stats.total_errors += 1;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        // Wait for response with timeout
        let result =
            match tokio::time::timeout(tokio::time::Duration::from_secs(30), response_rx).await {
                Ok(Ok(response)) => {
                    // Update response stats
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    {
                        let mut stats = state.stats.write().await;
                        stats.total_responses += 1;
                        stats.total_response_time_ms += elapsed;
                    }

                    // Convert JsonRpcResponse to JSON Value
                    let response_value = serde_json::to_value(&response)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let response_size = serde_json::to_string(&response_value)
                        .unwrap_or_default()
                        .len() as u64;

                    {
                        let mut stats = state.stats.write().await;
                        stats.total_bytes_sent += response_size;
                    }

                    Ok((response_headers(&state), Json(response_value)).into_response())
                }
                Ok(Err(_)) => {
                    // Channel closed without response
                    let mut stats = state.stats.write().await;
                    stats.total_errors += 1;
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
                Err(_) => {
                    // Timeout
                    let mut pending = state.pending_responses.write().await;
                    pending.remove(&id);
                    let mut stats = state.stats.write().await;
                    stats.total_errors += 1;
                    Err(StatusCode::REQUEST_TIMEOUT)
                }
            };

        result
    } else {
        // Notification (no ID) - no pending entry to race against, so
        // forwarding it here (rather than before the branch) is safe.
        // (Previously `blocking_write()` in this map_err closure, which
        // panics on a tokio worker whenever the send fails - #374.)
        if state.request_sender.send(request_str).await.is_err() {
            state.stats.write().await.total_errors += 1;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        // JSON-RPC 2.0: the server MUST NOT reply to a notification. MCP's
        // HTTP transport acknowledges an accepted notification with
        // `202 Accepted` and no body (#374) - previously this returned a
        // `{"jsonrpc":"2.0","result":{"status":"accepted",...}}` body with no
        // `id`, which is not a valid JSON-RPC response either.
        state.stats.write().await.total_responses += 1;

        Ok((StatusCode::ACCEPTED, response_headers(&state)).into_response())
    }
}

/// Security checks shared between `/`, `/mcp` (`handle_jsonrpc_request`) and
/// `/ws` (`handle_ws_upgrade`), extracted so both entry points enforce
/// exactly the same policy from the same `HttpConfig` - sharing the router
/// alone does not apply these checks to a new route, since they were
/// previously hand-rolled inline in `handle_jsonrpc_request` only.
fn check_network_policy(
    network_policy: &NetworkPolicy,
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    if let Err(e) = network_policy.validate_connection(&remote_addr) {
        error!("Connection rejected from {}: {}", remote_addr, e);
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Requires credentials when authentication is configured: a valid,
/// non-revoked, non-expired API key (`HttpConfig::auth_enabled`) and/or a
/// valid OAuth 2.1 access token (`HttpConfig::oauth`, #420). When both are
/// configured, either is accepted. Does nothing when neither is configured,
/// preserving existing behavior for deployments that haven't opted in.
///
/// API keys are read from `Authorization: Bearer <key>`, falling back to
/// `X-Api-Key`. Access tokens are read from `Authorization: Bearer` only
/// (RFC 6750; MCP forbids tokens in the query string). With OAuth, failures
/// carry a `WWW-Authenticate` header pointing at the Protected Resource
/// Metadata (RFC 9728), so that MCP clients can find the authorization
/// server.
///
/// Uses `ApiKeyManager` directly rather than the richer `MultiAuthProvider`/
/// `AuthMiddleware` (`security::auth`) - those require a matching
/// `UserRepository` entry per key. `verify_key` takes a write lock to record
/// usage stats, serializing concurrent API key checks on one lock; an
/// accepted tradeoff for this minimal scope.
async fn check_auth(
    state: &HttpTransportState,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
) -> std::result::Result<Authenticated, Box<Response>> {
    if state.api_key_manager.is_none() && state.oauth.is_none() {
        return Ok(Authenticated {
            caller: crate::mcp::Caller::unrestricted(),
            expires_at: None,
        });
    }

    if let Some(manager) = &state.api_key_manager {
        if let Some(key) = extract_api_key(headers) {
            if let Ok(api_key) = manager.write().await.verify_key(&key) {
                // 見つからないことは無い（同じ時に登録している）が、念のため
                // どのツールも使えない呼び出し元にする
                let caller = state
                    .api_key_callers
                    .get(&api_key.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::mcp::Caller::new(
                            format!("api_key:{}", api_key.name),
                            crate::mcp::ToolAccess::any_of(Vec::new()),
                        )
                    });
                return Ok(Authenticated {
                    caller,
                    expires_at: None,
                });
            }
        }
    }

    let Some(oauth) = &state.oauth else {
        let reason = if extract_api_key(headers).is_none() {
            "missing API key"
        } else {
            "invalid, revoked, or expired API key"
        };
        warn!("Rejected request from {} due to {}", remote_addr, reason);
        audit_auth_failure(state, remote_addr, reason).await;
        return Err(Box::new(StatusCode::UNAUTHORIZED.into_response()));
    };

    let Some(token) = extract_bearer_token(headers) else {
        warn!(
            "Rejected request from {} due to missing access token",
            remote_addr
        );
        audit_auth_failure(state, remote_addr, "missing access token").await;
        return Err(Box::new(oauth_error_response(oauth.config(), None)));
    };
    match oauth.validate(&token).await {
        Ok(info) => {
            debug!(
                "Accepted access token from {} (sub: {:?}, client: {:?})",
                remote_addr, info.subject, info.client_id
            );
            Ok(Authenticated {
                caller: oauth.caller_for(&info),
                // 検証と同じ許容の秒数を足す
                expires_at: info
                    .expires_at
                    .map(|exp| exp.saturating_add(oauth.config().leeway_seconds)),
            })
        }
        Err(error) => {
            match &error {
                TokenError::Invalid(reason) => {
                    warn!(
                        "Rejected request from {} due to an invalid access token: {}",
                        remote_addr, reason
                    );
                    audit_auth_failure(
                        state,
                        remote_addr,
                        &format!("invalid access token: {}", reason),
                    )
                    .await;
                }
                TokenError::InsufficientScope => {
                    warn!(
                        "Rejected request from {}: the access token lacks the required scopes",
                        remote_addr
                    );
                    audit_auth_failure(state, remote_addr, "insufficient scope").await;
                }
            }
            Err(Box::new(oauth_error_response(oauth.config(), Some(&error))))
        }
    }
}

/// ネットワークの制限と認証を、本文を読む前に確かめる（#540）。以前は
/// ハンドラーの中で、本文を読んでJSONとして解釈した後に確かめていたので、
/// 許していないIPや認証の無いクライアントも、大きな本文や遅い送信で負荷を
/// かけられた。通ったら、呼び出し元をリクエストの拡張に入れる
async fn auth_middleware(
    State(state): State<HttpTransportState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if let Err(status) = check_network_policy(&state.network_policy, remote_addr) {
        return status.into_response();
    }
    match check_auth(&state, request.headers(), remote_addr).await {
        Ok(authenticated) => {
            request.extensions_mut().insert(authenticated);
            next.run(request).await
        }
        Err(response) => *response,
    }
}

/// 認証の失敗を監査ログに記録する（#55）。提示された認証情報（キー・
/// トークン）そのものは記録しない。1分あたり`AUTH_FAILURE_AUDIT_PER_MINUTE`
/// 件まで（#540）
async fn audit_auth_failure(state: &HttpTransportState, remote_addr: SocketAddr, reason: &str) {
    let (record, report) = state.auth_failure_audit.admit(Instant::now());
    if let Some(report) = report {
        audit_suppressed_auth_failures(&state.audit_logger, report).await;
    }
    if !record {
        return;
    }
    let mut entry = crate::security::AuditLogEntry::new(
        crate::security::AuditLevel::Warning,
        crate::security::AuditCategory::Authentication,
        format!("Authentication failed: {}", reason),
    )
    .with_result("DENIED".to_string());
    entry.ip_address = Some(remote_addr.ip().to_string());
    if let Err(e) = state.audit_logger.log(entry).await {
        warn!(
            "Failed to write audit log entry for an authentication failure: {}",
            e
        );
    }
}

/// 401（トークンが無い・無効）か403（スコープが足りない）を、
/// `WWW-Authenticate`付きで返す
fn oauth_error_response(config: &OAuthConfig, error: Option<&TokenError>) -> Response {
    let status = match error {
        Some(TokenError::InsufficientScope) => StatusCode::FORBIDDEN,
        _ => StatusCode::UNAUTHORIZED,
    };
    let mut response = status.into_response();
    if let Ok(value) = HeaderValue::from_str(&www_authenticate(config, error)) {
        response
            .headers_mut()
            .insert(axum::http::header::WWW_AUTHENTICATE, value);
    }
    response
}

/// `Authorization: Bearer <token>`のトークン
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = header_value(headers, "authorization")?;
    let (scheme, token) = auth.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then(|| token.to_string())
}

/// Protected Resource Metadata（RFC 9728）。認証なしで返す（クライアントが
/// 認可サーバーを見つけるため）。ネットワークポリシーは適用する
async fn handle_protected_resource_metadata(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<HttpTransportState>,
) -> Response {
    if let Err(status) = check_network_policy(&state.network_policy, remote_addr) {
        return status.into_response();
    }
    match &state.oauth {
        Some(oauth) => Json(oauth.config().resource_metadata()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Extracts the client-supplied API key from `Authorization: Bearer <key>`,
/// falling back to `X-Api-Key` for clients that can't set an Authorization
/// header.
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers)
        .or_else(|| header_value(headers, "x-api-key").map(|v| v.to_string()))
}

fn check_https_enforcement(
    enforce_https: bool,
    tls_terminated_locally: bool,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    if enforce_https && !tls_terminated_locally && !is_forwarded_https(headers) {
        warn!(
            "Rejected non-HTTPS request from {} due to enforce_https policy",
            remote_addr
        );
        return Err(StatusCode::UPGRADE_REQUIRED);
    }
    Ok(())
}

fn check_min_tls_version(
    min_tls_version: Option<&str>,
    enforce_https: bool,
    tls_terminated_locally: bool,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    if let Some(min_tls_version) = min_tls_version {
        if let Err(reason) = validate_forwarded_tls_version(
            headers,
            min_tls_version,
            enforce_https && !tls_terminated_locally,
        ) {
            warn!(
                "Rejected request from {} due to TLS metadata validation failure: {}",
                remote_addr, reason
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

fn check_certificate_pin(
    certificate_pinning_enabled: bool,
    certificate_pin_header: &str,
    pinned_certificates_sha256: &HashSet<String>,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    if !certificate_pinning_enabled {
        return Ok(());
    }

    let Some(actual_fingerprint_raw) = header_value(headers, certificate_pin_header) else {
        warn!(
            "Rejected request from {} due to missing certificate pin header {}",
            remote_addr, certificate_pin_header
        );
        return Err(StatusCode::FORBIDDEN);
    };

    let Some(actual_fingerprint) = normalize_fingerprint(actual_fingerprint_raw) else {
        warn!(
            "Rejected request from {} due to invalid certificate fingerprint format",
            remote_addr
        );
        return Err(StatusCode::FORBIDDEN);
    };

    if !pinned_certificates_sha256.contains(actual_fingerprint.as_str()) {
        warn!(
            "Rejected request from {} due to certificate pin mismatch",
            remote_addr
        );
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(())
}

async fn check_anti_replay_handshake(
    anti_replay: Option<&Arc<AntiReplayMiddleware>>,
    audit_logger: &AuditLogger,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    let Some(anti_replay) = anti_replay else {
        return Ok(());
    };

    let user_agent = header_value(headers, "user-agent").map(str::to_string);
    let security_headers = SecurityHeaders {
        nonce: header_value(headers, "x-nonce").map(str::to_string),
        timestamp: header_value(headers, "x-timestamp").map(str::to_string),
        device_id: header_value(headers, "x-device-id").map(str::to_string),
        user_agent: user_agent.clone(),
        ip_address: Some(remote_addr.ip()),
    };

    if let Err(e) = anti_replay.validate_headers(&security_headers, None).await {
        warn!(
            "Anti-replay validation rejected request from {}: {}",
            remote_addr, e
        );
        let _ = audit_logger
            .log_security_attack(
                "replay_or_spoofing",
                &e.to_string(),
                Some(remote_addr.ip().to_string()),
                user_agent,
            )
            .await;

        return Err(match e {
            ReplayError::MissingNonce | ReplayError::MissingTimestamp => StatusCode::BAD_REQUEST,
            ReplayError::NonceAlreadyUsed => StatusCode::CONFLICT,
            ReplayError::TimestampOutOfWindow
            | ReplayError::FutureTimestamp
            | ReplayError::ExpiredTimestamp => StatusCode::REQUEST_TIMEOUT,
            ReplayError::UnknownDevice | ReplayError::DeviceMismatch(_) => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        });
    }

    Ok(())
}

/// `http::HeaderMap` always stores header names lowercased regardless of the
/// casing a client actually sent, but the IDS's own code (`detector.rs`,
/// `network.rs`, `behavioral.rs`, `ml/features.rs`) looks several of them up
/// by their conventional title-cased names (`"User-Agent"`, `"X-User-ID"`,
/// `"X-Session-ID"`, `"Referer"`, `"Cookie"`) via an exact-match
/// `HashMap::get`/`contains_key`. Map a lowercase name to that expected form
/// so those lookups actually see real request headers instead of always
/// missing. Keep this in sync with every such exact-match lookup under
/// `src/security/ids/` (searched recursively - some live in submodules like
/// `ml/`), not just the top-level files.
fn canonical_ids_header_name(lowercase_name: &str) -> Option<&'static str> {
    match lowercase_name {
        "user-agent" => Some("User-Agent"),
        "x-user-id" => Some("X-User-ID"),
        "x-session-id" => Some("X-Session-ID"),
        "referer" => Some("Referer"),
        "cookie" => Some("Cookie"),
        _ => None,
    }
}

/// Appends `value` under `key`, joining onto any value already there
/// instead of overwriting it - `HeaderMap::iter()` yields one entry per
/// occurrence of a repeated header, and a repeated query parameter key
/// behaves the same way, so a naive `insert` would silently keep only the
/// last one and let attacker-controlled content in an earlier occurrence
/// (or an earlier one hiding a later one, depending on iteration order)
/// go unanalyzed.
fn append_multivalue(map: &mut HashMap<String, String>, key: String, value: String) {
    map.entry(key)
        .and_modify(|existing| {
            existing.push_str(", ");
            existing.push_str(&value);
        })
        .or_insert(value);
}

fn header_map_to_string_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::with_capacity(headers.len());
    for (name, value) in headers.iter() {
        // A non-UTF-8 header value is still real header content an
        // attacker could use to evade detection - dropping it entirely
        // (as `to_str().ok()` would) is a bigger risk than lossily
        // decoding it, which still lets pattern matching see most of it.
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        if let Some(canonical) = canonical_ids_header_name(name.as_str()) {
            append_multivalue(&mut map, canonical.to_string(), value.clone());
        }
        append_multivalue(&mut map, name.as_str().to_string(), value);
    }
    map
}

/// Decodes a URL query string (e.g. `"a=1&b=2"`, without the leading `?`)
/// into a flat map. `None` (no query string at all) yields an empty map.
fn parse_query_params(query: Option<&str>) -> HashMap<String, String> {
    let Some(query) = query else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    // A repeated key (e.g. "a=malicious&a=benign") must not silently keep
    // only one of the values - see append_multivalue.
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        append_multivalue(&mut map, key.into_owned(), value.into_owned());
    }
    map
}

/// The IP to run IDS detection/blocking against. `remote_addr` is the raw
/// TCP peer address, which behind a reverse proxy/load balancer is the
/// proxy's own IP, not the actual client's - blocking on it would ban every
/// client behind that proxy over one bad request. Only consult
/// `X-Forwarded-For`/`X-Real-IP` when `trust_forwarded` is set (meaning the
/// operator has confirmed this transport sits directly behind a trusted
/// proxy that sets these itself); otherwise, or if neither header parses to
/// a valid IP, fall back to the raw peer address.
fn client_ip_for_ids(
    remote_addr: SocketAddr,
    headers: &HeaderMap,
    trust_forwarded: bool,
) -> IpAddr {
    if trust_forwarded {
        let forwarded_ip = header_value(headers, "x-forwarded-for")
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .and_then(|ip| ip.parse::<IpAddr>().ok())
            .or_else(|| {
                header_value(headers, "x-real-ip").and_then(|value| value.trim().parse().ok())
            });
        if let Some(ip) = forwarded_ip {
            return ip;
        }
    }
    remote_addr.ip()
}

/// Runs the intrusion detection/prevention system against an incoming
/// request. Already-blocked source IPs are rejected before any of the more
/// expensive signature/behavioral/network analysis runs. `body` is the
/// parsed JSON-RPC payload when one is available (`handle_jsonrpc_request`)
/// or `None` when it isn't yet (`handle_ws_upgrade`, where only the
/// blocklist check applies since there's no request body to analyze at
/// upgrade time).
async fn check_ids(
    ids: Option<&Arc<IntrusionDetectionSystem>>,
    remote_addr: SocketAddr,
    trust_forwarded_for: bool,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: Option<&Value>,
) -> std::result::Result<(), StatusCode> {
    let Some(ids) = ids else {
        return Ok(());
    };
    let ip = client_ip_for_ids(remote_addr, headers, trust_forwarded_for);

    if ids.is_blocked(ip).await {
        warn!("Rejected request from blocked IP {}", ip);
        return Err(StatusCode::FORBIDDEN);
    }

    let Some(body) = body else {
        return Ok(());
    };

    // A serialization failure here must not silently turn into "analyze an
    // empty body" - that would make the IDS reach a security decision on
    // different data than was actually received, risking a false negative
    // on the very request that failed to serialize. Treat it the same as
    // an IDS analysis failure: fail open with a warning, same as below.
    let body_bytes = match serde_json::to_vec(body) {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!(
                "Failed to serialize request body for IDS analysis, allowing request: {}",
                e
            );
            return Ok(());
        }
    };

    let request_data = RequestData {
        request_id: Uuid::new_v4().to_string(),
        method: "POST".to_string(),
        path: uri.path().to_string(),
        query_params: parse_query_params(uri.query()),
        headers: header_map_to_string_map(headers),
        body: Some(body_bytes),
        source_ip: Some(ip),
        timestamp: chrono::Utc::now(),
    };

    match ids.analyze_and_enforce(&request_data).await {
        Ok(result)
            if matches!(
                result.recommended_action,
                RecommendedAction::Block
                    | RecommendedAction::BlocklistIp
                    | RecommendedAction::EmergencyResponse
            ) =>
        {
            warn!(
                "Blocking request from {} due to detected {:?}",
                ip, result.detection_type
            );
            Err(StatusCode::FORBIDDEN)
        }
        Ok(_) => Ok(()),
        Err(e) => {
            // A bug or transient failure in IDS analysis must not take the
            // whole service down - fail open, the same way analyze_request
            // already tolerates one sub-detector failing without failing
            // the others (see IntrusionDetectionSystem::analyze_request).
            warn!("IDS analysis failed, allowing request: {}", e);
            Ok(())
        }
    }
}

/// Handles a `/ws` upgrade request, applying the exact same
/// network-policy/TLS-enforcement/certificate-pinning/anti-replay checks as
/// `handle_jsonrpc_request` before handing off to the WebSocket message loop.
async fn handle_ws_upgrade(
    ws: WebSocketUpgrade,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<HttpTransportState>,
    Extension(authenticated): Extension<Authenticated>,
    uri: axum::http::Uri,
    headers: HeaderMap,
) -> Response {
    if !state.websocket_upgrade_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }

    // ネットワークの制限と認証は、`auth_middleware`が確かめた
    let caller = authenticated
        .caller
        .with_ip_address(Some(remote_addr.ip().to_string()));
    let expires_at = authenticated.expires_at;
    if let Err(status) = check_https_enforcement(
        state.enforce_https,
        state.tls_terminated_locally,
        &headers,
        remote_addr,
    ) {
        return status.into_response();
    }
    if let Err(status) = check_min_tls_version(
        state.min_tls_version.as_deref(),
        state.enforce_https,
        state.tls_terminated_locally,
        &headers,
        remote_addr,
    ) {
        return status.into_response();
    }
    if let Err(status) = check_certificate_pin(
        state.certificate_pinning_enabled,
        &state.certificate_pin_header,
        &state.pinned_certificates_sha256,
        &headers,
        remote_addr,
    ) {
        return status.into_response();
    }

    let Ok(permit) = Arc::clone(&state.websocket_semaphore).try_acquire_owned() else {
        warn!(
            "Max WebSocket connections reached, rejecting {}",
            remote_addr
        );
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    if let Err(status) = check_anti_replay_handshake(
        state.anti_replay.as_ref(),
        &state.audit_logger,
        &headers,
        remote_addr,
    )
    .await
    {
        return status.into_response();
    }
    // No JSON-RPC body exists yet at upgrade time, so only the blocklist
    // fast-path check applies here. Unlike the HTTP JSON-RPC path (`/` and
    // `/mcp`), messages sent over an established `/ws` connection are not
    // currently run through full IDS analysis at all - there is no
    // per-message check_ids() call in the WebSocket message loop. That's a
    // real coverage gap versus HTTP, not something this upgrade-time check
    // makes up for.
    if let Err(status) = check_ids(
        state.ids.as_ref(),
        remote_addr,
        state.ids_trust_forwarded_for,
        &uri,
        &headers,
        None,
    )
    .await
    {
        return status.into_response();
    }

    // /ws shares the same security posture as / and /mcp, HSTS included -
    // capture it before `state` moves into the upgrade closure below, since
    // `ws.on_upgrade()`'s returned response is what actually carries
    // response headers back to the client (the 101 Switching Protocols
    // response), not something response_headers() gets applied to elsewhere.
    let hsts_headers = response_headers(&state);
    // メッセージ・フレームの大きさの上限（#540）。以前はtungsteniteの既定
    // （メッセージ64MiB・フレーム16MiB）のままだった
    let max_message_size = state.websocket_max_message_size;
    let mut response = ws
        .max_message_size(max_message_size)
        .max_frame_size(max_message_size)
        .on_upgrade(move |socket| {
            handle_ws_socket(socket, state, remote_addr, caller, expires_at, permit)
        });
    response.headers_mut().extend(hsts_headers);
    response
}

/// Runs the per-connection message loop for a `/ws` client, adapted from
/// `websocket::server::handle_socket` to use `HttpTransportState` instead of
/// a standalone `ServerState`. Holds `_permit` for the connection's
/// lifetime, releasing the reserved capacity slot back to
/// `websocket_semaphore` when the connection ends.
async fn handle_ws_socket(
    socket: WebSocket,
    state: HttpTransportState,
    addr: SocketAddr,
    caller: crate::mcp::Caller,
    expires_at: Option<u64>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let conn_id = Uuid::new_v4();
    state.ws_callers.lock().await.insert(conn_id, caller);

    let conn_info = WebSocketConnectionInfo::new(conn_id, addr);
    state
        .websocket_connections
        .lock()
        .await
        .insert(conn_id, conn_info);
    state
        .websocket_total_connections
        .fetch_add(1, Ordering::SeqCst);

    if let Err(e) = state.websocket_handler.on_connect(conn_id, addr).await {
        error!("WebSocket handler on_connect failed: {}", e);
    }

    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(tokio::sync::Mutex::new(sender));
    // アクセストークンの期限で閉じる（#540）。以前は接続のときにしか
    // 確かめず、期限が切れても接続を使い続けられた
    let token_deadline = expires_at.map(unix_time_to_instant);
    // 無通信の時間の上限があれば、サーバーからpingを送る（PR #548の
    // レビュー）。生きているクライアント（ブラウザも）は自動でpongを返し、
    // それも受け取ったものとして数えるので、静かなだけのクライアントは
    // 切れない。閉じるのは、何も返さない接続だけ
    let mut ping = state.websocket_idle_timeout.map(|idle| {
        let period = websocket_ping_interval(idle);
        tokio::time::interval_at(tokio::time::Instant::now() + period, period)
    });
    let mut last_received = tokio::time::Instant::now();

    loop {
        let idle_deadline = state
            .websocket_idle_timeout
            .map(|idle| last_received + idle);
        let msg_result = match next_ws_event(
            &mut receiver,
            idle_deadline,
            token_deadline,
            ping.as_mut(),
        )
        .await
        {
            WsEvent::Message(msg_result) => {
                last_received = tokio::time::Instant::now();
                msg_result
            }
            WsEvent::PingDue => {
                // 相手が受信を止めて送信のバッファが埋まっていると、送信で
                // 止まり、無通信の上限も働かなくなる。送れなければ閉じる
                // （PR #548の再レビュー）
                let sent = tokio::time::timeout(
                    WEBSOCKET_SEND_TIMEOUT,
                    sender.lock().await.send(Message::Ping(Vec::new().into())),
                )
                .await;
                if !matches!(sent, Ok(Ok(()))) {
                    debug!(
                        "Closing WebSocket connection {} from {}: a ping could not be sent",
                        conn_id, addr
                    );
                    break;
                }
                continue;
            }
            WsEvent::Closed => break,
            WsEvent::IdleTimeout => {
                debug!(
                    "Closing idle WebSocket connection {} from {}",
                    conn_id, addr
                );
                close_ws(&sender, close_code::AWAY, "idle timeout").await;
                break;
            }
            WsEvent::TokenExpired => {
                info!(
                    "Closing WebSocket connection {} from {}: the access token expired",
                    conn_id, addr
                );
                close_ws(&sender, close_code::POLICY, "access token expired").await;
                break;
            }
        };
        match msg_result {
            Ok(msg) => {
                if let Some(conn) = state.websocket_connections.lock().await.get_mut(&conn_id) {
                    conn.last_activity = Instant::now();
                    conn.messages_received += 1;
                }

                // Message-level anti-replay: WS frames carry no HTTP headers,
                // so nonce/timestamp are read from the JSON-RPC payload's own
                // top-level fields instead, matching the standalone
                // WebSocketServer's existing convention.
                if let Some(anti_replay) = &state.anti_replay {
                    if let Message::Text(text) = &msg {
                        if let Ok(value) = serde_json::from_str::<Value>(text) {
                            let security_headers = SecurityHeaders {
                                nonce: value
                                    .get("nonce")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
                                timestamp: value
                                    .get("timestamp")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
                                device_id: None,
                                user_agent: None,
                                ip_address: Some(addr.ip()),
                            };

                            if let Err(e) =
                                anti_replay.validate_headers(&security_headers, None).await
                            {
                                warn!(
                                    "Anti-replay validation rejected WS message from {} ({}): {}",
                                    addr, conn_id, e
                                );
                                let _ = state
                                    .audit_logger
                                    .log_security_attack(
                                        "replay_or_spoofing_ws_message",
                                        &e.to_string(),
                                        Some(addr.ip().to_string()),
                                        None,
                                    )
                                    .await;
                                continue;
                            }
                        }
                    }
                }

                match state.websocket_handler.handle_message(conn_id, msg).await {
                    Ok(Some(response)) => {
                        let mut s = sender.lock().await;
                        if let Err(e) = s.send(response).await {
                            error!("Failed to send WebSocket response: {}", e);
                            break;
                        }

                        if let Some(conn) =
                            state.websocket_connections.lock().await.get_mut(&conn_id)
                        {
                            conn.messages_sent += 1;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("WebSocket handler error: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
        // メッセージの処理（ツールの呼び出し、最長120秒）の間は受信もpingも
        // しないので、処理が終わった時刻から数え直す。無通信の上限より長い
        // 処理の直後に、閉じないようにする（PR #548の再レビュー）
        last_received = tokio::time::Instant::now();
    }

    state.websocket_connections.lock().await.remove(&conn_id);
    state.ws_callers.lock().await.remove(&conn_id);

    if let Err(e) = state.websocket_handler.on_disconnect(conn_id).await {
        error!("WebSocket handler on_disconnect failed: {}", e);
    }

    debug!("WebSocket connection {} closed", conn_id);
}

/// `/ws`の接続で、次に起きたこと（#540）
enum WsEvent {
    /// メッセージ（pongを含む）を受け取った
    Message(std::result::Result<Message, axum::Error>),
    /// 相手が接続を閉じた
    Closed,
    /// pingを送る時刻になった
    PingDue,
    /// 無通信の時間の上限を超えた（pingにも応答が無い）
    IdleTimeout,
    /// アクセストークンの期限が切れた
    TokenExpired,
}

/// pingを送る間隔。無通信の時間の上限の間に、少なくとも2回送る
fn websocket_ping_interval(idle_timeout: std::time::Duration) -> std::time::Duration {
    (idle_timeout / 2)
        .min(std::time::Duration::from_secs(30))
        .max(std::time::Duration::from_millis(100))
}

/// `deadline`まで待つ。`None`なら終わらない
async fn sleep_until_or_forever(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// 次に起きることを待つ（#540）: メッセージ、pingの時刻、無通信の時間の
/// 上限（最後に受け取った時刻から）、トークンの期限
async fn next_ws_event(
    receiver: &mut futures::stream::SplitStream<WebSocket>,
    idle_deadline: Option<tokio::time::Instant>,
    token_deadline: Option<tokio::time::Instant>,
    ping: Option<&mut tokio::time::Interval>,
) -> WsEvent {
    let ping_due = async {
        match ping {
            Some(ping) => {
                ping.tick().await;
            }
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        next = receiver.next() => match next {
            Some(msg_result) => WsEvent::Message(msg_result),
            None => WsEvent::Closed,
        },
        _ = sleep_until_or_forever(token_deadline) => WsEvent::TokenExpired,
        _ = sleep_until_or_forever(idle_deadline) => WsEvent::IdleTimeout,
        _ = ping_due => WsEvent::PingDue,
    }
}

/// `/ws`でpingやCloseフレームを送る時間の上限。相手が受信を止めていても、
/// ここで止まらないようにする
const WEBSOCKET_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 理由を付けて`/ws`の接続を閉じる。送れなくても、接続は閉じる
async fn close_ws(
    sender: &tokio::sync::Mutex<futures::stream::SplitSink<WebSocket, Message>>,
    code: u16,
    reason: &'static str,
) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = tokio::time::timeout(
        WEBSOCKET_SEND_TIMEOUT,
        sender.lock().await.send(Message::Close(Some(frame))),
    )
    .await;
}

/// UNIX時刻（秒）を、tokioの時刻にする。過ぎていれば今
fn unix_time_to_instant(unix_seconds: u64) -> tokio::time::Instant {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tokio::time::Instant::now() + std::time::Duration::from_secs(unix_seconds.saturating_sub(now))
}

/// `JsonRpcRequestHandler`でリクエストを処理し、HTTPの応答にする（#400）。
/// 応答があれば200でJSON-RPCの応答を、通知なら202（本文なし、#374）を返す。
async fn respond_with_request_handler(
    state: &HttpTransportState,
    handler: &dyn JsonRpcRequestHandler,
    request_str: &str,
    caller: &crate::mcp::Caller,
    protocol_version: Option<&'static str>,
    start_time: Instant,
) -> std::result::Result<Response, StatusCode> {
    match tokio::time::timeout(
        REQUEST_HANDLER_TIMEOUT,
        handler.handle_jsonrpc_with_protocol_version(request_str, caller, protocol_version),
    )
    .await
    {
        Ok(Ok(Some(response_value))) => {
            {
                let mut stats = state.stats.write().await;
                stats.total_responses += 1;
                stats.total_response_time_ms += start_time.elapsed().as_millis() as u64;
                stats.total_bytes_sent += response_value.to_string().len() as u64;
            }
            Ok((response_headers(state), Json(response_value)).into_response())
        }
        Ok(Ok(None)) => {
            state.stats.write().await.total_responses += 1;
            Ok((StatusCode::ACCEPTED, response_headers(state)).into_response())
        }
        Ok(Err(e)) => {
            warn!("Rejected an invalid JSON-RPC message: {}", e);
            state.stats.write().await.total_errors += 1;
            Err(StatusCode::BAD_REQUEST)
        }
        Err(_) => {
            warn!(
                "JSON-RPC request did not finish within {} seconds",
                REQUEST_HANDLER_TIMEOUT.as_secs()
            );
            state.stats.write().await.total_errors += 1;
            Err(StatusCode::REQUEST_TIMEOUT)
        }
    }
}

/// Bridges `/ws` text frames into the same JSON-RPC request/response
/// correlation machinery `handle_jsonrpc_request` uses (`request_sender` /
/// `pending_responses`), so `/ws` speaks MCP JSON-RPC rather than echoing
/// frames back.
struct JsonRpcWsHandler {
    /// 設定されていれば、フレームをチャンネルに流さず直接処理する（#400）
    request_handler: Option<Arc<dyn JsonRpcRequestHandler>>,
    /// 接続 → 呼び出し元（#52）。見つからない接続は、どのツールも使えない
    callers: Arc<tokio::sync::Mutex<HashMap<ConnectionId, crate::mcp::Caller>>>,
    request_sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
}

#[async_trait]
impl MessageHandler for JsonRpcWsHandler {
    async fn handle_message(
        &self,
        conn_id: ConnectionId,
        message: Message,
    ) -> Result<Option<Message>> {
        let text = match message {
            Message::Text(text) => text,
            _ => return Ok(None),
        };

        let request: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse WebSocket JSON-RPC request: {}", e);
                return Ok(None);
            }
        };

        {
            let mut stats = self.stats.write().await;
            stats.total_requests += 1;
            stats.total_bytes_received += text.len() as u64;
        }

        if let Some(handler) = &self.request_handler {
            let caller = self
                .callers
                .lock()
                .await
                .get(&conn_id)
                .cloned()
                .unwrap_or_else(|| {
                    crate::mcp::Caller::new(
                        "unknown-websocket-connection",
                        crate::mcp::ToolAccess::any_of(Vec::new()),
                    )
                });
            let response = match tokio::time::timeout(
                REQUEST_HANDLER_TIMEOUT,
                handler.handle_jsonrpc(&text, &caller),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(e)) => {
                    warn!("Rejected an invalid WebSocket JSON-RPC message: {}", e);
                    self.stats.write().await.total_errors += 1;
                    return Ok(None);
                }
                Err(_) => {
                    warn!(
                        "WebSocket JSON-RPC request did not finish within {} seconds",
                        REQUEST_HANDLER_TIMEOUT.as_secs()
                    );
                    self.stats.write().await.total_errors += 1;
                    return Ok(None);
                }
            };
            let Some(response) = response else {
                return Ok(None); // 通知には応答しない
            };
            let response_json = response.to_string();
            {
                let mut stats = self.stats.write().await;
                stats.total_responses += 1;
                stats.total_bytes_sent += response_json.len() as u64;
            }
            return Ok(Some(Message::Text(response_json.into())));
        }

        let request_id = request.get("id").cloned();

        let Some(id) = request_id else {
            // Notification (no `id`) - no response frame expected, so there
            // is no pending entry to race against.
            self.request_sender
                .send(text.to_string())
                .await
                .map_err(|e| {
                    Error::Internal(format!("Failed to forward WebSocket request: {e}"))
                })?;
            return Ok(None);
        };

        // Register the pending response *before* forwarding the request -
        // otherwise a core that processes and replies fast enough could call
        // send_message() before this entry exists, missing the pending map
        // and falling back to the general notification channel instead of
        // reaching this waiter at all.
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_responses.write().await;
            pending.insert(id.clone(), response_tx);
        }

        if let Err(e) = self.request_sender.send(text.to_string()).await {
            self.pending_responses.write().await.remove(&id);
            return Err(Error::Internal(format!(
                "Failed to forward WebSocket request: {e}"
            )));
        }

        // Matches handle_jsonrpc_request's timeout; a slow/failed request
        // drops just this one response frame rather than closing the socket.
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => {
                let response_json = serde_json::to_string(&response).map_err(|e| {
                    Error::Internal(format!("Failed to serialize WebSocket response: {e}"))
                })?;
                {
                    let mut stats = self.stats.write().await;
                    stats.total_responses += 1;
                    stats.total_bytes_sent += response_json.len() as u64;
                }
                Ok(Some(Message::Text(response_json.into())))
            }
            Ok(Err(_)) => {
                let mut stats = self.stats.write().await;
                stats.total_errors += 1;
                warn!("WebSocket response channel closed for request id {:?}", id);
                Ok(None)
            }
            Err(_) => {
                self.pending_responses.write().await.remove(&id);
                let mut stats = self.stats.write().await;
                stats.total_errors += 1;
                warn!("WebSocket request timed out for id {:?}", id);
                Ok(None)
            }
        }
    }

    async fn on_connect(&self, conn_id: ConnectionId, addr: SocketAddr) -> Result<()> {
        info!("WebSocket client connected: {} from {}", conn_id, addr);
        Ok(())
    }

    async fn on_disconnect(&self, conn_id: ConnectionId) -> Result<()> {
        info!("WebSocket client disconnected: {}", conn_id);
        Ok(())
    }
}

fn build_hsts_header_value(
    max_age_seconds: u64,
    include_subdomains: bool,
    preload: bool,
) -> String {
    let mut value = format!("max-age={}", max_age_seconds);
    if include_subdomains {
        value.push_str("; includeSubDomains");
    }
    if preload {
        value.push_str("; preload");
    }
    value
}

fn validate_tls_security_settings(config: &HttpConfig) -> std::result::Result<(), String> {
    if config.mtls_enabled && !config.tls_enabled {
        return Err("mtls_enabled requires tls_enabled=true".to_string());
    }

    if config.tls_enabled {
        if config.tls_cert_path.as_deref().is_none() {
            return Err("tls_cert_path is required when tls_enabled=true".to_string());
        }
        if config.tls_key_path.as_deref().is_none() {
            return Err("tls_key_path is required when tls_enabled=true".to_string());
        }
        if config.mtls_enabled && config.mtls_ca_cert_path.as_deref().is_none() {
            return Err("mtls_ca_cert_path is required when mtls_enabled=true".to_string());
        }
    }

    if let Some(min_tls_version) = config.min_tls_version.as_deref() {
        if !is_tls_version_at_least(min_tls_version, "1.3") {
            return Err(format!(
                "min_tls_version must be TLS 1.3 or higher, got {}",
                min_tls_version
            ));
        }
    }

    if config.enforce_https && !config.hsts_enabled {
        return Err("hsts_enabled must be true when enforce_https is enabled".to_string());
    }

    if config.hsts_enabled && config.hsts_max_age_seconds == 0 {
        return Err("hsts_max_age_seconds must be greater than 0".to_string());
    }

    if config.hsts_preload {
        if !config.hsts_include_subdomains {
            return Err("hsts_preload requires hsts_include_subdomains=true".to_string());
        }
        if config.hsts_max_age_seconds < 31_536_000 {
            return Err("hsts_preload requires hsts_max_age_seconds >= 31536000".to_string());
        }
    }

    if config.certificate_pinning_enabled {
        if !config.enforce_https {
            return Err("certificate pinning requires enforce_https=true".to_string());
        }
        if config.pinned_certificates_sha256.is_empty() {
            return Err(
                "pinned_certificates_sha256 must contain at least one fingerprint when certificate pinning is enabled"
                    .to_string(),
            );
        }
        for fingerprint in &config.pinned_certificates_sha256 {
            if normalize_fingerprint(fingerprint).is_none() {
                return Err(format!(
                    "Invalid SHA-256 certificate fingerprint: {}",
                    fingerprint
                ));
            }
        }
    }

    Ok(())
}

fn response_headers(state: &HttpTransportState) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(hsts_header_value) = state.hsts_header_value.as_deref() {
        if let Ok(value) = HeaderValue::from_str(hsts_header_value) {
            headers.insert("Strict-Transport-Security", value);
        }
    }
    headers
}

fn is_forwarded_https(headers: &HeaderMap) -> bool {
    header_eq_ignore_ascii_case(headers, "x-forwarded-proto", "https")
        || header_contains_token(headers, "forwarded", "proto=https")
}

fn extract_forwarded_tls_version(headers: &HeaderMap) -> Option<&str> {
    header_value(headers, "x-forwarded-tls-version")
        .or_else(|| header_value(headers, "x-ssl-protocol"))
}

fn validate_forwarded_tls_version(
    headers: &HeaderMap,
    min_tls_version: &str,
    require_header: bool,
) -> std::result::Result<(), &'static str> {
    match extract_forwarded_tls_version(headers) {
        Some(actual_tls_version) => {
            if is_tls_version_at_least(actual_tls_version, min_tls_version) {
                Ok(())
            } else {
                Err("tls_version_too_low")
            }
        }
        None => {
            if require_header {
                Err("missing_tls_version_header")
            } else {
                Ok(())
            }
        }
    }
}

fn is_tls_version_at_least(actual: &str, minimum: &str) -> bool {
    parse_tls_version(actual)
        .zip(parse_tls_version(minimum))
        .map(|(actual_num, min_num)| actual_num >= min_num)
        .unwrap_or(true)
}

fn parse_tls_version(version: &str) -> Option<f32> {
    let normalized = version.trim().to_ascii_lowercase().replace("tls", "");
    normalized.parse::<f32>().ok()
}

fn normalize_fingerprint(fingerprint: &str) -> Option<String> {
    let normalized = fingerprint.trim().replace(':', "").to_ascii_lowercase();
    if normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(normalized)
    } else {
        None
    }
}

fn certificate_fingerprint_sha256(certificate: &CertificateDer<'_>) -> String {
    let digest = Sha256::digest(certificate.as_ref());
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim())
}

fn header_eq_ignore_ascii_case(headers: &HeaderMap, name: &str, expected: &str) -> bool {
    header_value(headers, name)
        .map(|v| v.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn header_contains_token(headers: &HeaderMap, name: &str, token: &str) -> bool {
    header_value(headers, name)
        .map(|v| v.to_ascii_lowercase().contains(&token.to_ascii_lowercase()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use rcgen::{
        generate_simple_self_signed, BasicConstraints, Certificate, CertificateParams,
        ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
    };
    use std::error::Error as StdError;
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use tokio::time::{sleep, Duration, Instant};

    /// Polls `addr` until a plain TCP connection succeeds (or panics after a
    /// timeout), instead of a fixed sleep, so tests only proceed once the
    /// spawned `axum::serve` task is actually accepting - avoids flakiness
    /// under load or slow CI where a fixed delay might not be enough.
    async fn wait_for_plain_server_ready(addr: SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                return;
            }
            if Instant::now() >= deadline {
                panic!("server did not become ready on {addr} within timeout");
            }
            sleep(Duration::from_millis(20)).await;
        }
    }

    async fn wait_for_https_server_ready(
        bind_addr: SocketAddr,
        root_ca: reqwest::Certificate,
        client_identity_pem: Option<&[u8]>,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let url = format!("https://localhost:{}/", bind_addr.port());

        loop {
            let mut builder = reqwest::Client::builder()
                .add_root_certificate(root_ca.clone())
                .resolve("localhost", bind_addr)
                .connect_timeout(Duration::from_millis(300))
                .timeout(Duration::from_millis(500));

            if let Some(identity_pem) = client_identity_pem {
                let identity = reqwest::Identity::from_pem(identity_pem)
                    .expect("valid client identity PEM for readiness probe");
                builder = builder.identity(identity);
            }

            let client = builder
                .build()
                .expect("failed to build readiness probe HTTP client");

            match client.get(&url).send().await {
                Ok(_) => {
                    return;
                }
                Err(err) if is_connectivity_probe_error(&err) => {}
                Err(err) => {
                    panic!("failed to probe HTTPS readiness for {bind_addr}: {err:?}");
                }
            }

            if Instant::now() >= deadline {
                panic!("HTTPS server did not become ready on {bind_addr} within timeout");
            }

            sleep(Duration::from_millis(50)).await;
        }
    }

    fn is_connectivity_probe_error(error: &reqwest::Error) -> bool {
        error.is_timeout()
            || reqwest_error_has_io_kind(error, std::io::ErrorKind::ConnectionRefused)
            || reqwest_error_has_io_kind(error, std::io::ErrorKind::AddrNotAvailable)
            || reqwest_error_has_io_kind(error, std::io::ErrorKind::TimedOut)
            || reqwest_error_has_io_kind(error, std::io::ErrorKind::ConnectionReset)
            || reqwest_error_has_io_kind(error, std::io::ErrorKind::ConnectionAborted)
    }

    fn reqwest_error_has_io_kind(error: &reqwest::Error, kind: std::io::ErrorKind) -> bool {
        let mut source = StdError::source(error);
        while let Some(err) = source {
            if let Some(io_error) = err.downcast_ref::<std::io::Error>() {
                if io_error.kind() == kind {
                    return true;
                }
            }
            source = err.source();
        }
        false
    }

    fn assert_mtls_rejection_error(error: &reqwest::Error, context: &str) {
        assert!(
            error.is_request() || error.is_connect(),
            "unexpected error kind while {context}: {error:?}"
        );
        assert!(
            !error.is_timeout(),
            "error may indicate connectivity issue instead of mTLS rejection while {context}: {error:?}"
        );
        assert!(
            !reqwest_error_has_io_kind(error, std::io::ErrorKind::ConnectionRefused)
                && !reqwest_error_has_io_kind(error, std::io::ErrorKind::AddrNotAvailable)
                && !reqwest_error_has_io_kind(error, std::io::ErrorKind::TimedOut),
            "error may indicate connectivity issue instead of mTLS rejection while {context}: {error:?}"
        );
    }

    fn generate_ca_cert() -> (Certificate, KeyPair) {
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let ca_key = KeyPair::generate().unwrap();
        let ca_cert = params.self_signed(&ca_key).unwrap();
        (ca_cert, ca_key)
    }

    fn generate_signed_cert(
        dns_name: &str,
        ca_cert: &Certificate,
        ca_key: &KeyPair,
        eku: ExtendedKeyUsagePurpose,
    ) -> (Certificate, KeyPair) {
        let mut params = CertificateParams::new(vec![dns_name.to_string()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![eku];
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, ca_cert, ca_key).unwrap();
        (cert, key)
    }

    #[test]
    fn test_http_config_default() {
        let config = HttpConfig::default();
        assert_eq!(config.bind_addr.port(), 8081);
        assert!(config.cors_enabled);
        assert_eq!(config.max_request_size, 1024 * 1024);
        assert_eq!(config.timeout_ms, 30000);
        assert!(!config.tls_enabled);
        assert!(!config.mtls_enabled);
        assert!(config.hsts_enabled);
        assert_eq!(config.min_tls_version.as_deref(), Some("1.3"));
        assert!(!config.certificate_pinning_enabled);
    }

    #[tokio::test]
    async fn test_http_transport_creation() {
        let config = HttpConfig::default();
        let transport = HttpTransport::new(config);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_build_hsts_header_value() {
        let value = build_hsts_header_value(31536000, true, true);
        assert!(value.contains("max-age=31536000"));
        assert!(value.contains("includeSubDomains"));
        assert!(value.contains("preload"));
    }

    #[test]
    fn test_tls_version_comparison() {
        assert!(is_tls_version_at_least("1.3", "1.3"));
        assert!(is_tls_version_at_least("1.4", "1.3"));
        assert!(!is_tls_version_at_least("1.2", "1.3"));
    }

    #[test]
    fn test_validate_forwarded_tls_version_missing_header_when_required() {
        let headers = HeaderMap::new();
        let result = validate_forwarded_tls_version(&headers, "1.3", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_forwarded_tls_version_accepts_ssl_protocol_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ssl-protocol", HeaderValue::from_static("TLS1.3"));
        let result = validate_forwarded_tls_version(&headers, "1.3", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_normalize_fingerprint() {
        let value = normalize_fingerprint(
            "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
        );
        assert!(value.is_some());
        assert_eq!(value.unwrap().len(), 64);
    }

    #[test]
    fn test_tls_validation_rejects_low_version() {
        let config = HttpConfig {
            min_tls_version: Some("1.2".to_string()),
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&config).is_err());
    }

    #[test]
    fn test_tls_validation_requires_cert_and_key_paths() {
        let config = HttpConfig {
            tls_enabled: true,
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&config).is_err());
    }

    #[test]
    fn test_tls_validation_rejects_mtls_without_tls() {
        let config = HttpConfig {
            mtls_enabled: true,
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&config).is_err());
    }

    #[test]
    fn test_tls_validation_requires_mtls_ca_path() {
        let config = HttpConfig {
            tls_enabled: true,
            tls_cert_path: Some("./certs/server.crt".to_string()),
            tls_key_path: Some("./certs/server.key".to_string()),
            mtls_enabled: true,
            mtls_ca_cert_path: None,
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&config).is_err());
    }

    #[tokio::test]
    async fn test_https_server_start_and_request_roundtrip() {
        let cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert_pem = cert.cert.pem();
        let key_pem = cert.key_pair.serialize_pem();
        let cert_fingerprint = certificate_fingerprint_sha256(cert.cert.der());

        let temp_dir = tempdir().unwrap();
        let cert_path = temp_dir.path().join("server.crt");
        let key_path = temp_dir.path().join("server.key");
        std::fs::write(&cert_path, cert_pem).unwrap();
        std::fs::write(&key_path, key_pem).unwrap();

        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let config = HttpConfig {
            bind_addr,
            tls_enabled: true,
            tls_cert_path: Some(cert_path.to_string_lossy().to_string()),
            tls_key_path: Some(key_path.to_string_lossy().to_string()),
            enforce_https: true,
            certificate_pinning_enabled: true,
            pinned_certificates_sha256: vec![cert_fingerprint],
            ..HttpConfig::default()
        };

        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        let root_ca = reqwest::Certificate::from_pem(cert.cert.pem().as_bytes()).unwrap();
        wait_for_https_server_ready(server_addr, root_ca.clone(), None).await;

        let client = reqwest::Client::builder()
            .add_root_certificate(root_ca)
            .resolve("localhost", server_addr)
            .build()
            .unwrap();
        let response = client
            .post(format!("https://localhost:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert!(response.headers().contains_key("strict-transport-security"));
    }

    #[tokio::test]
    async fn test_https_server_rejects_client_without_mtls_certificate() {
        let (ca_cert, ca_key) = generate_ca_cert();
        let (server_cert, server_key) = generate_signed_cert(
            "localhost",
            &ca_cert,
            &ca_key,
            ExtendedKeyUsagePurpose::ServerAuth,
        );

        let temp_dir = tempdir().unwrap();
        let ca_path = temp_dir.path().join("ca.crt");
        let cert_path = temp_dir.path().join("server.crt");
        let key_path = temp_dir.path().join("server.key");
        std::fs::write(&ca_path, ca_cert.pem()).unwrap();
        std::fs::write(&cert_path, server_cert.pem()).unwrap();
        std::fs::write(&key_path, server_key.serialize_pem()).unwrap();

        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let config = HttpConfig {
            bind_addr,
            tls_enabled: true,
            tls_cert_path: Some(cert_path.to_string_lossy().to_string()),
            tls_key_path: Some(key_path.to_string_lossy().to_string()),
            mtls_enabled: true,
            mtls_ca_cert_path: Some(ca_path.to_string_lossy().to_string()),
            enforce_https: true,
            ..HttpConfig::default()
        };

        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        let (probe_client_cert, probe_client_key) = generate_signed_cert(
            "mcp-probe-client",
            &ca_cert,
            &ca_key,
            ExtendedKeyUsagePurpose::ClientAuth,
        );
        let probe_identity_pem = format!(
            "{}\n{}",
            probe_client_cert.pem(),
            probe_client_key.serialize_pem()
        );
        let root_ca = reqwest::Certificate::from_pem(ca_cert.pem().as_bytes()).unwrap();
        wait_for_https_server_ready(
            server_addr,
            root_ca.clone(),
            Some(probe_identity_pem.as_bytes()),
        )
        .await;

        let client = reqwest::Client::builder()
            .add_root_certificate(root_ca)
            .resolve("localhost", server_addr)
            .build()
            .unwrap();

        let response = client
            .post(format!("https://localhost:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await;

        let error = response.expect_err("request without client certificate should fail");
        assert_mtls_rejection_error(&error, "client certificate is missing");
    }

    #[tokio::test]
    async fn test_https_server_accepts_client_with_valid_mtls_certificate() {
        let (ca_cert, ca_key) = generate_ca_cert();
        let (server_cert, server_key) = generate_signed_cert(
            "localhost",
            &ca_cert,
            &ca_key,
            ExtendedKeyUsagePurpose::ServerAuth,
        );
        let (client_cert, client_key) = generate_signed_cert(
            "mcp-client",
            &ca_cert,
            &ca_key,
            ExtendedKeyUsagePurpose::ClientAuth,
        );

        let temp_dir = tempdir().unwrap();
        let ca_path = temp_dir.path().join("ca.crt");
        let cert_path = temp_dir.path().join("server.crt");
        let key_path = temp_dir.path().join("server.key");
        std::fs::write(&ca_path, ca_cert.pem()).unwrap();
        std::fs::write(&cert_path, server_cert.pem()).unwrap();
        std::fs::write(&key_path, server_key.serialize_pem()).unwrap();

        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let config = HttpConfig {
            bind_addr,
            tls_enabled: true,
            tls_cert_path: Some(cert_path.to_string_lossy().to_string()),
            tls_key_path: Some(key_path.to_string_lossy().to_string()),
            mtls_enabled: true,
            mtls_ca_cert_path: Some(ca_path.to_string_lossy().to_string()),
            enforce_https: true,
            ..HttpConfig::default()
        };

        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        let root_ca = reqwest::Certificate::from_pem(ca_cert.pem().as_bytes()).unwrap();
        let client_identity_pem = format!("{}\n{}", client_cert.pem(), client_key.serialize_pem());
        wait_for_https_server_ready(
            server_addr,
            root_ca.clone(),
            Some(client_identity_pem.as_bytes()),
        )
        .await;

        let client_identity = reqwest::Identity::from_pem(client_identity_pem.as_bytes()).unwrap();

        let client = reqwest::Client::builder()
            .add_root_certificate(root_ca)
            .identity(client_identity)
            .resolve("localhost", server_addr)
            .build()
            .unwrap();

        let response = client
            .post(format!("https://localhost:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_https_server_rejects_client_with_invalid_mtls_certificate_chain() {
        let (trusted_ca_cert, trusted_ca_key) = generate_ca_cert();
        let (untrusted_ca_cert, untrusted_ca_key) = generate_ca_cert();
        let (server_cert, server_key) = generate_signed_cert(
            "localhost",
            &trusted_ca_cert,
            &trusted_ca_key,
            ExtendedKeyUsagePurpose::ServerAuth,
        );
        let (client_cert, client_key) = generate_signed_cert(
            "mcp-client",
            &untrusted_ca_cert,
            &untrusted_ca_key,
            ExtendedKeyUsagePurpose::ClientAuth,
        );

        let temp_dir = tempdir().unwrap();
        let ca_path = temp_dir.path().join("ca.crt");
        let cert_path = temp_dir.path().join("server.crt");
        let key_path = temp_dir.path().join("server.key");
        std::fs::write(&ca_path, trusted_ca_cert.pem()).unwrap();
        std::fs::write(&cert_path, server_cert.pem()).unwrap();
        std::fs::write(&key_path, server_key.serialize_pem()).unwrap();

        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let config = HttpConfig {
            bind_addr,
            tls_enabled: true,
            tls_cert_path: Some(cert_path.to_string_lossy().to_string()),
            tls_key_path: Some(key_path.to_string_lossy().to_string()),
            mtls_enabled: true,
            mtls_ca_cert_path: Some(ca_path.to_string_lossy().to_string()),
            enforce_https: true,
            ..HttpConfig::default()
        };

        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        let (probe_client_cert, probe_client_key) = generate_signed_cert(
            "mcp-probe-client",
            &trusted_ca_cert,
            &trusted_ca_key,
            ExtendedKeyUsagePurpose::ClientAuth,
        );
        let probe_identity_pem = format!(
            "{}\n{}",
            probe_client_cert.pem(),
            probe_client_key.serialize_pem()
        );
        let root_ca = reqwest::Certificate::from_pem(trusted_ca_cert.pem().as_bytes()).unwrap();
        wait_for_https_server_ready(
            server_addr,
            root_ca.clone(),
            Some(probe_identity_pem.as_bytes()),
        )
        .await;

        let client_identity_pem = format!("{}\n{}", client_cert.pem(), client_key.serialize_pem());
        let client_identity = reqwest::Identity::from_pem(client_identity_pem.as_bytes()).unwrap();

        let client = reqwest::Client::builder()
            .add_root_certificate(root_ca)
            .identity(client_identity)
            .resolve("localhost", server_addr)
            .build()
            .unwrap();

        let response = client
            .post(format!("https://localhost:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await;

        let error =
            response.expect_err("request with client certificate signed by wrong CA should fail");
        assert_mtls_rejection_error(&error, "client certificate chain is invalid");
    }

    #[test]
    fn test_tls_validation_rejects_invalid_pin_config() {
        let config = HttpConfig {
            certificate_pinning_enabled: true,
            enforce_https: true,
            pinned_certificates_sha256: vec!["invalid-fingerprint".to_string()],
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&config).is_err());
    }

    // --- /ws-on-HttpTransport tests (Issue #261) ---

    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::Error as WsError;
    use tokio_tungstenite::Connector;

    #[test]
    fn test_check_network_policy_rejects_non_loopback_when_configured() {
        // check_network_policy is the exact function shared by both
        // handle_jsonrpc_request (/mcp) and handle_ws_upgrade (/ws) - a live
        // end-to-end test can't exercise its rejection path, because
        // NetworkPolicy::is_ip_whitelisted (security/network_policy.rs)
        // returns true for any loopback address before it even looks at
        // ip_whitelist, and the reject_external_connections check is
        // likewise gated on `!ip.is_loopback()`. So neither check
        // validate_connection performs can ever fail for a loopback address
        // regardless of configuration, and a real test process can only
        // originate connections from loopback - this proves the shared
        // function's own rejection behavior directly instead, with a
        // fabricated non-loopback SocketAddr.
        let policy = NetworkPolicy::default();
        let non_loopback: SocketAddr = "203.0.113.1:12345".parse().unwrap();
        assert!(check_network_policy(&policy, non_loopback).is_err());

        let loopback: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        assert!(check_network_policy(&policy, loopback).is_ok());
    }

    #[tokio::test]
    async fn check_ids_analyzes_the_actual_request_path_not_a_hardcoded_one() {
        // Regression test: check_ids used to always build RequestData with
        // a hardcoded path of "/mcp", so a path-only attack (nothing in the
        // body) could never be detected regardless of the real request
        // path. "/files/../../../etc/passwd" is a known-detected pattern
        // (see SignatureDetector's path traversal patterns; also exercised
        // directly against SignatureDetector in
        // tests/ids_integration_test.rs's
        // test_signature_detector_path_traversal) that lives only in the
        // path, not in the (benign) body below.
        let ids = Arc::new(
            IntrusionDetectionSystem::new(IDSConfig {
                auto_block_enabled: true,
                ..IDSConfig::default()
            })
            .await
            .unwrap(),
        );
        let remote_addr: SocketAddr = "203.0.113.5:12345".parse().unwrap();
        let headers = HeaderMap::new();
        let benign_body = serde_json::json!({"jsonrpc": "2.0", "method": "tools/list"});

        let uri: axum::http::Uri = "/files/../../../etc/passwd".parse().unwrap();
        let result = check_ids(
            Some(&ids),
            remote_addr,
            false,
            &uri,
            &headers,
            Some(&benign_body),
        )
        .await;

        assert_eq!(
            result,
            Err(StatusCode::FORBIDDEN),
            "a path-traversal path must be analyzed and rejected, not silently replaced with a hardcoded path"
        );
    }

    #[tokio::test]
    async fn check_ids_analyzes_the_actual_query_string_not_an_empty_one() {
        // Regression test: check_ids used to always build RequestData with
        // an empty query_params map, so a query-string-only attack (nothing
        // in the path or body) could never be detected -
        // SignatureDetector::extract_check_strings() specifically scans
        // request.query_params (see also
        // tests/ids_integration_test.rs's
        // test_signature_detector_command_injection, which detects this
        // same pattern via query_params directly).
        let ids = Arc::new(
            IntrusionDetectionSystem::new(IDSConfig {
                auto_block_enabled: true,
                ..IDSConfig::default()
            })
            .await
            .unwrap(),
        );
        let remote_addr: SocketAddr = "203.0.113.6:12345".parse().unwrap();
        let headers = HeaderMap::new();
        let benign_body = serde_json::json!({"jsonrpc": "2.0", "method": "tools/list"});

        let uri: axum::http::Uri = "/mcp?cmd=ls%3B%20rm%20-rf%20%2F".parse().unwrap();
        let result = check_ids(
            Some(&ids),
            remote_addr,
            false,
            &uri,
            &headers,
            Some(&benign_body),
        )
        .await;

        assert_eq!(
            result,
            Err(StatusCode::FORBIDDEN),
            "a query-string-only attack must be analyzed and rejected, not silently dropped"
        );
    }

    #[test]
    fn header_map_to_string_map_exposes_canonical_names_and_keeps_non_utf8_values() {
        let mut headers = HeaderMap::new();
        // http::HeaderMap always stores this as "user-agent" regardless of
        // how it's inserted - detector.rs/network.rs look it up as the
        // exact string "User-Agent", so both forms must be present.
        headers.insert("User-Agent", "sqlmap/1.0".parse().unwrap());
        // A non-UTF-8 value (0xFF is not valid UTF-8 on its own) must not
        // be silently dropped - lossily decoding still keeps most of an
        // attacker-controlled value available to pattern matching.
        headers.insert(
            "x-weird",
            axum::http::HeaderValue::from_bytes(b"bad\xffvalue").unwrap(),
        );

        let map = header_map_to_string_map(&headers);

        assert_eq!(
            map.get("user-agent").map(String::as_str),
            Some("sqlmap/1.0")
        );
        assert_eq!(
            map.get("User-Agent").map(String::as_str),
            Some("sqlmap/1.0")
        );
        assert!(
            map.contains_key("x-weird"),
            "a non-UTF-8 header value must not be dropped entirely"
        );
        assert!(map["x-weird"].contains("bad") && map["x-weird"].contains("value"));
    }

    #[test]
    fn header_map_to_string_map_exposes_cookie_in_title_case() {
        // src/security/ids/ml/features.rs checks
        // request.headers.contains_key("Cookie") - an exact-match lookup
        // that was missing from canonical_ids_header_name's list even
        // though http::HeaderMap always stores this as lowercase "cookie".
        // (The ml feature extractor isn't currently called from
        // IntrusionDetectionSystem::analyze_request at all - see the "ml
        // implementation" scope note in this PR's description - so this is
        // a forward-looking correctness fix for header_map_to_string_map
        // itself, not a live detection gap today.)
        let mut headers = HeaderMap::new();
        headers.insert("Cookie", "session=abc123".parse().unwrap());

        let map = header_map_to_string_map(&headers);

        assert_eq!(
            map.get("cookie").map(String::as_str),
            Some("session=abc123")
        );
        assert_eq!(
            map.get("Cookie").map(String::as_str),
            Some("session=abc123")
        );
    }

    #[test]
    fn header_map_to_string_map_keeps_all_values_of_a_repeated_header() {
        // http::HeaderMap can store multiple values under one name (a
        // client can send the same header more than once); a naive
        // insert-per-occurrence would keep only the last one and let
        // attacker-controlled content in an earlier occurrence go
        // unanalyzed.
        let mut headers = HeaderMap::new();
        headers.append("x-custom", "first".parse().unwrap());
        headers.append("x-custom", "second".parse().unwrap());

        let map = header_map_to_string_map(&headers);

        let value = &map["x-custom"];
        assert!(value.contains("first"), "got: {value}");
        assert!(value.contains("second"), "got: {value}");
    }

    #[test]
    fn parse_query_params_keeps_all_values_of_a_repeated_key() {
        // "a=malicious&a=benign" must not silently collapse to whichever
        // value HashMap::collect() happens to keep - both must be visible
        // to detection.
        let map = parse_query_params(Some("a=malicious&a=benign"));

        let value = &map["a"];
        assert!(value.contains("malicious"), "got: {value}");
        assert!(value.contains("benign"), "got: {value}");
    }

    #[test]
    fn client_ip_for_ids_uses_the_peer_address_when_forwarding_is_not_trusted() {
        // Default posture: a proxy in front of us could set X-Forwarded-For
        // to anything, so without an explicit trust decision it must be
        // ignored and the real TCP peer used - otherwise any client can
        // evade IP blocking, or get an arbitrary victim IP blocklisted, by
        // just setting this header themselves.
        let remote_addr: SocketAddr = "203.0.113.9:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());

        let ip = client_ip_for_ids(remote_addr, &headers, false);

        assert_eq!(ip, remote_addr.ip());
    }

    #[test]
    fn client_ip_for_ids_uses_the_forwarded_header_when_trusted() {
        let remote_addr: SocketAddr = "203.0.113.9:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        // The first entry is the original client in a single-trusted-proxy
        // deployment (the only topology this opt-in is meant for).
        headers.insert(
            "x-forwarded-for",
            "198.51.100.1, 203.0.113.9".parse().unwrap(),
        );

        let ip = client_ip_for_ids(remote_addr, &headers, true);

        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn client_ip_for_ids_falls_back_to_the_peer_address_when_trusted_but_header_is_unparseable() {
        let remote_addr: SocketAddr = "203.0.113.9:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());

        let ip = client_ip_for_ids(remote_addr, &headers, true);

        assert_eq!(ip, remote_addr.ip());
    }

    #[tokio::test]
    async fn test_ws_upgrade_returns_404_when_disabled() {
        // enable_websocket_upgrade defaults to false.
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::NOT_FOUND);
            }
            other => panic!("expected the /ws upgrade to be rejected with 404, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_ws_enforce_https_rejects_plain_ws_without_forwarded_header() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            enforce_https: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
            }
            other => panic!(
                "expected the plain ws:// upgrade to be rejected under enforce_https, got: {other:?}"
            ),
        }
    }

    #[tokio::test]
    async fn test_ws_enforce_https_accepts_with_forwarded_proto_header() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            enforce_https: true,
            // Isolate the enforce_https/is_forwarded_https check this test
            // targets from the separate min_tls_version forwarded-header
            // check, which also applies once enforce_https is set.
            min_tls_version: None,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let mut request = format!("ws://localhost:{}/ws", server_addr.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("x-forwarded-proto", "https".parse().unwrap());

        let result = tokio_tungstenite::connect_async(request).await;
        assert!(
            result.is_ok(),
            "expected the upgrade to succeed with X-Forwarded-Proto: https, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_ws_max_connections_rejects_second_connection() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            websocket_max_connections: 1,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        // The semaphore permit backing websocket_max_connections is acquired
        // synchronously inside handle_ws_upgrade before the upgrade response
        // is even returned, so by the time connect_async() resolves here the
        // capacity is already reserved - no extra delay needed before the
        // second attempt.
        let (first_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("first connection should succeed");

        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            }
            other => {
                panic!("expected the second connection to be rejected with 503, got: {other:?}")
            }
        }

        drop(first_stream);
    }

    #[tokio::test]
    async fn test_ws_max_connections_enforced_under_concurrent_upgrades() {
        // A len()-based guard checked before the handshake completes (and
        // the connection is actually inserted into websocket_connections)
        // leaves a TOCTOU window where many concurrent upgrades can all pass
        // the check before any of them registers - a fixed, sequential
        // "first, then second" test can't exercise that window. Firing many
        // upgrades at once does.
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            websocket_max_connections: 1,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        let attempts = 10;
        let handles: Vec<_> = (0..attempts)
            .map(|_| {
                let url = url.clone();
                tokio::spawn(async move { tokio_tungstenite::connect_async(&url).await })
            })
            .collect();

        let mut successes = Vec::new();
        let mut rejections = 0;
        for handle in handles {
            match handle.await.unwrap() {
                Ok(stream) => successes.push(stream),
                Err(WsError::Http(response)) => {
                    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                    rejections += 1;
                }
                other => panic!("unexpected connect_async result: {other:?}"),
            }
        }

        assert_eq!(
            successes.len(),
            1,
            "expected exactly one connection to be accepted under {attempts} concurrent upgrade attempts"
        );
        assert_eq!(rejections, attempts - 1);
    }

    #[tokio::test]
    async fn test_wss_handshake_and_json_rpc_roundtrip_over_tls() {
        let cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert_pem = cert.cert.pem();
        let key_pem = cert.key_pair.serialize_pem();

        let temp_dir = tempdir().unwrap();
        let cert_path = temp_dir.path().join("server.crt");
        let key_path = temp_dir.path().join("server.key");
        std::fs::write(&cert_path, cert_pem).unwrap();
        std::fs::write(&key_path, key_pem).unwrap();

        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            tls_enabled: true,
            tls_cert_path: Some(cert_path.to_string_lossy().to_string()),
            tls_key_path: Some(key_path.to_string_lossy().to_string()),
            enforce_https: true,
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        };

        let mut transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();

        let root_ca = reqwest::Certificate::from_pem(cert.cert.pem().as_bytes()).unwrap();
        wait_for_https_server_ready(server_addr, root_ca, None).await;

        // Simulate the MCP core: drain requests off the transport and reply
        // with a canned response correlated by the same request id, exactly
        // like a real handler loop would via receive_message()/send_message().
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

        let mut root_store = rustls::RootCertStore::empty();
        root_store.add(cert.cert.der().clone()).unwrap();
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let url = format!("wss://localhost:{}/ws", server_addr.port());
        let (mut ws_stream, upgrade_response) = tokio_tungstenite::connect_async_tls_with_config(
            &url,
            None,
            false,
            Some(Connector::Rustls(Arc::new(client_config))),
        )
        .await
        .expect("wss:// handshake should succeed");

        // /ws shares HttpTransportState's response_headers() (HSTS) with the
        // rest of the transport - it must appear on the 101 Switching
        // Protocols response itself, not just on ordinary JSON-RPC replies.
        assert!(
            upgrade_response
                .headers()
                .contains_key("strict-transport-security"),
            "expected the /ws upgrade response to carry the Strict-Transport-Security header"
        );

        let request_json = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 1
        });
        ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                request_json.to_string().into(),
            ))
            .await
            .unwrap();

        let response_msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("timed out waiting for a WebSocket response")
            .expect("stream ended without a response")
            .expect("WebSocket error while awaiting response");

        let response_text = match response_msg {
            tokio_tungstenite::tungstenite::Message::Text(text) => text,
            other => panic!("expected a text frame in response, got: {other:?}"),
        };
        let response_value: Value = serde_json::from_str(&response_text).unwrap();
        assert_eq!(response_value["id"], serde_json::json!(1));
        assert_eq!(response_value["result"]["ok"], serde_json::json!(true));
    }

    /// A notification (no `id`) so a request that gets past `check_ids`
    /// gets the immediate `202 Accepted` `handle_jsonrpc_request` sends for
    /// notifications, rather than waiting on the pending-response channel
    /// these tests have no consumer draining.
    fn sql_injection_jsonrpc_request() -> Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "query",
                "arguments": {
                    "filter": "1 UNION SELECT FROM users"
                }
            }
        })
    }

    #[tokio::test]
    async fn test_ids_blocks_request_with_sql_injection_payload() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ids_enabled: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&sql_injection_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_ids_blocklist_persists_for_later_benign_requests_from_same_ip() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ids_enabled: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());

        let first_response = client
            .post(&url)
            .json(&sql_injection_jsonrpc_request())
            .send()
            .await
            .unwrap();
        assert_eq!(first_response.status(), StatusCode::FORBIDDEN);

        // A second, entirely benign request from the same source IP must
        // still be rejected - the source IP itself is now blocklisted,
        // independent of this specific request's content.
        let second_response = client
            .post(&url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(second_response.status(), StatusCode::FORBIDDEN);
    }

    /// Regression test for blocking the wrong IP behind a trusted reverse
    /// proxy: with `ids_trust_forwarded_for: true`, a request that spoofs
    /// `X-Forwarded-For` as a client IP different from the real TCP peer
    /// (loopback, since that's all a test process can originate from) must
    /// get *that* forwarded IP blocklisted, not the peer - a later, benign
    /// request from the same real peer with no forwarded header must still
    /// go through, since the peer itself was never the one at fault.
    #[tokio::test]
    async fn test_ids_blocklists_the_trusted_forwarded_ip_not_the_proxy_peer() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ids_enabled: true,
            ids_trust_forwarded_for: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());

        let attack_response = client
            .post(&url)
            .header("x-forwarded-for", "198.51.100.55")
            .json(&sql_injection_jsonrpc_request())
            .send()
            .await
            .unwrap();
        assert_eq!(attack_response.status(), StatusCode::FORBIDDEN);

        // Same real peer (127.0.0.1, the only address a test client can
        // connect from), no forwarded header this time, benign payload:
        // must not be blocked, because the peer address itself was never
        // blocklisted - only the spoofed-but-trusted forwarded IP was.
        let later_response = client
            .post(&url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .unwrap();
        assert!(later_response.status().is_success());
    }

    #[tokio::test]
    async fn test_ids_disabled_by_default_does_not_affect_normal_requests() {
        // ids_enabled defaults to false - this must behave exactly as it
        // did before IDS wiring existed, even for a payload that would
        // otherwise be flagged.
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&sql_injection_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    fn notification_jsonrpc_request() -> Value {
        // No `id` field, so handle_jsonrpc_request takes the notification
        // path and returns an immediate `202 Accepted` instead of waiting on
        // the internal channel for a response that nothing in these tests
        // ever produces (see http.rs's notification branch).
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {}
        })
    }

    /// #374: JSON-RPC 2.0 forbids replying to a notification, and MCP's HTTP
    /// transport acknowledges one with `202 Accepted` and no body. This used
    /// to be `200` with a `{"jsonrpc":"2.0","result":{"status":"accepted"}}`
    /// body that had no `id`.
    #[tokio::test]
    async fn test_notification_gets_202_accepted_with_empty_body() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert!(response.text().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_jsonrpc_rejects_request_without_api_key_when_auth_enabled() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "test-key-12345".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jsonrpc_rejects_invalid_api_key() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("Authorization", "Bearer wrong-key")
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jsonrpc_accepts_valid_api_key() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("Authorization", "Bearer correct-key")
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_jsonrpc_accepts_valid_api_key_via_x_api_key_header() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("X-Api-Key", "correct-key")
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_jsonrpc_allows_request_without_credentials_when_auth_disabled() {
        // auth_enabled defaults to false - must behave exactly as before
        // this feature existed.
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    /// OAuth（#420）を有効にしてサーバーを起動する。戻り値は（ポート、
    /// テスト用の認可サーバー）
    async fn start_oauth_server(
        configure: impl FnOnce(&mut HttpConfig),
    ) -> (u16, crate::transport::oauth::test_support::TestAuthServer) {
        let auth = crate::transport::oauth::test_support::TestAuthServer::start().await;
        let mut config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            oauth: Some(auth.config(OAUTH_RESOURCE)),
            ..HttpConfig::default()
        };
        configure(&mut config);
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        // サーバーを止めないように、テストの間は残す
        std::mem::forget(transport);
        (server_addr.port(), auth)
    }

    const OAUTH_RESOURCE: &str = "https://mcp.example.com/mcp";
    const OAUTH_METADATA_URL: &str =
        "https://mcp.example.com/.well-known/oauth-protected-resource/mcp";

    async fn post_mcp(port: u16, authorization: Option<String>) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", port))
            .json(&notification_jsonrpc_request());
        if let Some(authorization) = authorization {
            request = request.header("Authorization", authorization);
        }
        request.send().await.unwrap()
    }

    fn www_authenticate_of(response: &reqwest::Response) -> String {
        response
            .headers()
            .get("www-authenticate")
            .expect("WWW-Authenticate header")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn test_oauth_rejects_missing_and_invalid_tokens_with_www_authenticate() {
        let (port, auth) = start_oauth_server(|_| {}).await;

        let response = post_mcp(port, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            www_authenticate_of(&response),
            format!("Bearer resource_metadata=\"{}\"", OAUTH_METADATA_URL)
        );

        // 別のサービス向けのトークン（対象が違う）
        let other = auth.token("https://other.example.com", serde_json::json!({}));
        let response = post_mcp(port, Some(format!("Bearer {}", other))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let header = www_authenticate_of(&response);
        assert!(header.contains("error=\"invalid_token\""), "{}", header);
        assert!(header.contains(OAUTH_METADATA_URL), "{}", header);

        let response = post_mcp(port, Some("Bearer garbage".to_string())).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oauth_accepts_valid_token() {
        let (port, auth) = start_oauth_server(|_| {}).await;
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({}));
        let response = post_mcp(port, Some(format!("Bearer {}", token))).await;
        assert!(response.status().is_success(), "{}", response.status());
    }

    #[tokio::test]
    async fn test_oauth_insufficient_scope_returns_403() {
        let (port, auth) = start_oauth_server(|config| {
            config.oauth.as_mut().unwrap().required_scopes = vec!["mcp:tools".to_string()];
        })
        .await;
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({ "scope": "openid" }));
        let response = post_mcp(port, Some(format!("Bearer {}", token))).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let header = www_authenticate_of(&response);
        assert!(
            header.contains("error=\"insufficient_scope\""),
            "{}",
            header
        );
        assert!(header.contains("scope=\"mcp:tools\""), "{}", header);

        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({ "scope": "mcp:tools" }));
        let response = post_mcp(port, Some(format!("Bearer {}", token))).await;
        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn test_oauth_protected_resource_metadata_is_served_without_auth() {
        let (port, auth) = start_oauth_server(|_| {}).await;
        for path in [
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-protected-resource",
        ] {
            let response = reqwest::get(format!("http://127.0.0.1:{}{}", port, path))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{}", path);
            let metadata: Value = response.json().await.unwrap();
            assert_eq!(metadata["resource"], serde_json::json!(OAUTH_RESOURCE));
            assert_eq!(
                metadata["authorization_servers"],
                serde_json::json!([auth.issuer])
            );

            // 公開の情報なので、許可していないオリジンにも返し、CORSでも
            // 読ませる（#488）
            let response = reqwest::Client::new()
                .get(format!("http://127.0.0.1:{}{}", port, path))
                .header("Origin", "https://other.example")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{}", path);
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .and_then(|v| v.to_str().ok()),
                Some("*"),
                "{}",
                path
            );
        }
    }

    #[tokio::test]
    async fn test_protected_resource_metadata_is_not_served_without_oauth() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        let response = reqwest::get(format!(
            "http://127.0.0.1:{}/.well-known/oauth-protected-resource",
            server_addr.port()
        ))
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_oauth_and_api_keys_can_be_used_together() {
        let (port, auth) = start_oauth_server(|config| {
            config.auth_enabled = true;
            config.api_keys = vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }];
        })
        .await;
        let response = post_mcp(port, Some("Bearer correct-key".to_string())).await;
        assert!(response.status().is_success());
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({}));
        let response = post_mcp(port, Some(format!("Bearer {}", token))).await;
        assert!(response.status().is_success());
        let response = post_mcp(port, Some("Bearer wrong-key".to_string())).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(www_authenticate_of(&response).contains("invalid_token"));
    }

    #[tokio::test]
    async fn test_oauth_ws_upgrade_requires_token() {
        let (port, auth) = start_oauth_server(|_| {}).await;
        let url = format!("ws://localhost:{}/ws", port);
        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            other => panic!("expected 401 without a token, got: {other:?}"),
        }

        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = url.into_client_request().unwrap();
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({}));
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
        assert!(tokio_tungstenite::connect_async(request).await.is_ok());
    }

    /// 実際の認可サーバー（Keycloak）が発行したトークンで確かめる（#420）。
    /// `MCP_RS_KEYCLOAK_TESTS=1`のときだけ動く（CIの「OAuth Keycloak Test」）。
    /// Keycloakは`MCP_RS_KEYCLOAK_URL`（既定`http://localhost:8080`）で、
    /// 管理者admin/adminで起動しておく
    #[tokio::test]
    async fn test_keycloak_access_tokens() {
        if std::env::var("MCP_RS_KEYCLOAK_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let base = std::env::var("MCP_RS_KEYCLOAK_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());
        let client = reqwest::Client::new();
        let token_of = |realm: &str, form: Vec<(&str, &str)>| {
            let client = client.clone();
            let url = format!("{}/realms/{}/protocol/openid-connect/token", base, realm);
            let form: Vec<(String, String)> = form
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            async move {
                let response = client.post(url).form(&form).send().await.unwrap();
                let status = response.status();
                let body: Value = response.json().await.unwrap();
                assert!(status.is_success(), "token request failed: {}", body);
                body["access_token"].as_str().unwrap().to_string()
            }
        };

        // 準備: realm「mcp」と、2つのクライアント（mcp-rs向けの対象（aud）を
        // 入れるものと、入れないもの）
        let admin = token_of(
            "master",
            vec![
                ("grant_type", "password"),
                ("client_id", "admin-cli"),
                ("username", "admin"),
                ("password", "admin"),
            ],
        )
        .await;
        let admin_post = |path: &str, body: Value| {
            let request = client
                .post(format!("{}/admin/realms{}", base, path))
                .bearer_auth(&admin)
                .json(&body);
            async move {
                let response = request.send().await.unwrap();
                let status = response.status();
                assert!(
                    status.is_success() || status == StatusCode::CONFLICT,
                    "admin request failed: {} {}",
                    status,
                    response.text().await.unwrap_or_default()
                );
            }
        };
        admin_post("", serde_json::json!({ "realm": "mcp", "enabled": true })).await;
        let new_client = |client_id: &str, secret: &str, audience: Option<&str>| {
            let mut body = serde_json::json!({
                "clientId": client_id,
                "enabled": true,
                "publicClient": false,
                "serviceAccountsEnabled": true,
                "standardFlowEnabled": false,
                "directAccessGrantsEnabled": false,
                "secret": secret,
            });
            if let Some(audience) = audience {
                body["protocolMappers"] = serde_json::json!([{
                    "name": "mcp-rs-audience",
                    "protocol": "openid-connect",
                    "protocolMapper": "oidc-audience-mapper",
                    "config": {
                        "included.custom.audience": audience,
                        "access.token.claim": "true",
                        "id.token.claim": "false"
                    }
                }]);
            }
            body
        };
        admin_post(
            "/mcp/clients",
            new_client("mcp-client", "mcp-secret", Some(OAUTH_RESOURCE)),
        )
        .await;
        admin_post(
            "/mcp/clients",
            new_client("other-client", "other-secret", None),
        )
        .await;

        let credentials = |id: &'static str, secret: &'static str| {
            vec![
                ("grant_type", "client_credentials"),
                ("client_id", id),
                ("client_secret", secret),
            ]
        };
        let token = token_of("mcp", credentials("mcp-client", "mcp-secret")).await;
        let other = token_of("mcp", credentials("other-client", "other-secret")).await;

        // 発行者だけを設定し、JWKSはメタデータから探す
        let issuer = format!("{}/realms/mcp", base);
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            oauth: Some(OAuthConfig {
                issuer: issuer.clone(),
                resource: OAUTH_RESOURCE.to_string(),
                audiences: Vec::new(),
                jwks_uri: None,
                required_scopes: vec!["profile".to_string()],
                leeway_seconds: 60,
                scope_tools: Default::default(),
            }),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        let port = server_addr.port();

        let response = post_mcp(port, Some(format!("Bearer {}", token))).await;
        assert!(
            response.status().is_success(),
            "Keycloak token was rejected: {}",
            response.status()
        );

        // 対象（aud）がmcp-rsでないトークン
        let response = post_mcp(port, Some(format!("Bearer {}", other))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(www_authenticate_of(&response).contains("invalid_token"));

        // 署名を改ざんしたトークン
        // （署名の途中の1文字を変える。末尾は、使われないビットのことがある）
        let mut tampered = token.clone().into_bytes();
        let i = tampered.len() - 10;
        tampered[i] = if tampered[i] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(tampered).unwrap();
        let response = post_mcp(port, Some(format!("Bearer {}", tampered))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = post_mcp(port, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(www_authenticate_of(&response).contains("resource_metadata="));

        let metadata: Value = reqwest::get(format!(
            "http://127.0.0.1:{}/.well-known/oauth-protected-resource/mcp",
            port
        ))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
        assert_eq!(
            metadata["authorization_servers"],
            serde_json::json!([issuer])
        );
    }

    #[tokio::test]
    async fn test_start_server_rejects_invalid_oauth_config() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            oauth: Some(OAuthConfig {
                issuer: "http://auth.example.com".to_string(),
                resource: OAUTH_RESOURCE.to_string(),
                audiences: Vec::new(),
                jwks_uri: None,
                required_scopes: Vec::new(),
                leeway_seconds: 60,
                scope_tools: Default::default(),
            }),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        assert!(transport.start_server().await.is_err());
    }

    #[tokio::test]
    async fn test_ws_upgrade_rejects_without_api_key_when_auth_enabled() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            other => panic!(
                "expected the /ws upgrade to be rejected with 401 without an API key, got: {other:?}"
            ),
        }
    }

    #[tokio::test]
    async fn test_ws_upgrade_accepts_valid_api_key() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let mut request = format!("ws://localhost:{}/ws", server_addr.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Authorization", "Bearer correct-key".parse().unwrap());

        let result = tokio_tungstenite::connect_async(request).await;
        assert!(
            result.is_ok(),
            "expected the upgrade to succeed with a valid API key, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_start_server_rejects_auth_enabled_without_api_keys() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: Vec::new(),
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();

        let result = transport.start_server().await;
        assert!(
            result.is_err(),
            "expected start_server() to reject auth_enabled=true with no configured api_keys"
        );
    }

    #[tokio::test]
    async fn test_start_server_rejects_empty_api_key_entry() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: String::new(),
                name: "blank".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();

        let result = transport.start_server().await;
        assert!(
            result.is_err(),
            "expected start_server() to reject an api_keys entry with an empty key \
             (would otherwise let any client authenticate with an empty X-Api-Key)"
        );
    }

    #[tokio::test]
    async fn test_start_server_rejects_duplicate_api_keys() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![
                ApiKeyEntry {
                    key: "shared-key".to_string(),
                    name: "first".to_string(),
                    user_id: "user-1".to_string(),
                    tools: None,
                },
                ApiKeyEntry {
                    key: "shared-key".to_string(),
                    name: "second".to_string(),
                    user_id: "user-2".to_string(),
                    tools: None,
                },
            ],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();

        let result = transport.start_server().await;
        assert!(
            result.is_err(),
            "expected start_server() to reject duplicate api_keys entries instead of \
             silently keeping only the last one"
        );
    }

    /// `with_request_handler`のテスト用。`params.sleep_ms`だけ待ってから、
    /// 受けた`id`・`method`・`params.tag`をそのまま結果に入れて返す。呼び出し
    /// 元（#52）の名前と、読み取り専用のツール（`get_post`）・そうでない
    /// ツール（`delete_post`）を使えるかも返す。
    struct EchoRequestHandler;

    #[async_trait]
    impl JsonRpcRequestHandler for EchoRequestHandler {
        async fn handle_jsonrpc(
            &self,
            message: &str,
            caller: &crate::mcp::Caller,
        ) -> std::result::Result<Option<Value>, String> {
            let request: Value = serde_json::from_str(message).map_err(|e| e.to_string())?;
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .ok_or("missing method")?
                .to_string();
            let Some(id) = request.get("id").cloned() else {
                return Ok(None);
            };
            if let Some(ms) = request["params"]["sleep_ms"].as_u64() {
                sleep(Duration::from_millis(ms)).await;
            }
            Ok(Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "method": method,
                    "tag": request["params"]["tag"],
                    "caller": caller.name,
                    "can_get": caller.access.allows(&annotated_tool("get_post", true)),
                    "can_delete": caller.access.allows(&annotated_tool("delete_post", false)),
                }
            })))
        }
    }

    fn annotated_tool(name: &str, read_only: bool) -> crate::mcp::Tool {
        crate::mcp::Tool {
            name: name.to_string(),
            annotations: Some(crate::mcp::ToolAnnotations {
                read_only_hint: Some(read_only),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// 呼び出し元（#52）を確かめる。`authorization`で`tools/list`を送り、
    /// （名前、読み取り専用を使えるか、変更するツールを使えるか）を返す
    async fn caller_seen_by_handler(
        server_addr: SocketAddr,
        authorization: &str,
    ) -> (Value, bool, bool) {
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("Authorization", authorization)
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let body: Value = response.json().await.unwrap();
        let result = &body["result"];
        (
            result["caller"].clone(),
            result["can_get"].as_bool().unwrap(),
            result["can_delete"].as_bool().unwrap(),
        )
    }

    fn api_key(key: &str, name: &str, tools: Option<&[&str]>) -> ApiKeyEntry {
        ApiKeyEntry {
            key: key.to_string(),
            name: name.to_string(),
            user_id: "user-1".to_string(),
            tools: tools.map(|t| t.iter().map(|s| s.to_string()).collect()),
        }
    }

    /// 認証の失敗を、設定した監査ログに記録する。提示されたキーそのものは
    /// 記録しない（#55）
    #[tokio::test]
    async fn test_authentication_failures_are_audited() {
        let audit_logger = Arc::new(AuditLogger::with_defaults());
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![api_key("correct-key", "reader", None)],
            ..HttpConfig::default()
        })
        .unwrap()
        .with_audit_logger(audit_logger.clone())
        .with_request_handler(Arc::new(EchoRequestHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        for authorization in [None, Some("Bearer secret-wrong-key")] {
            let mut request = client.post(&url).json(&notification_jsonrpc_request());
            if let Some(authorization) = authorization {
                request = request.header("Authorization", authorization);
            }
            assert_eq!(
                request.send().await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
        }
        // 成功した認証は記録しない（ツール呼び出しの記録に呼び出し元が残る）
        let response = client
            .post(&url)
            .header("Authorization", "Bearer correct-key")
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());

        let entries = audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 2, "{:?}", entries);
        assert_eq!(entries[0].message, "Authentication failed: missing API key");
        assert_eq!(
            entries[1].message,
            "Authentication failed: invalid, revoked, or expired API key"
        );
        for entry in &entries {
            assert_eq!(entry.ip_address.as_deref(), Some("127.0.0.1"));
            assert_eq!(entry.result.as_deref(), Some("DENIED"));
            let text = serde_json::to_string(entry).unwrap();
            assert!(!text.contains("secret-wrong-key"), "{}", text);
        }
    }

    #[tokio::test]
    async fn test_api_key_tools_are_passed_to_the_request_handler() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![
                api_key("reader-key", "reader", Some(&["@read_only"])),
                api_key("deleter-key", "deleter", Some(&["delete_*"])),
                api_key("admin-key", "admin", None),
            ],
            ..HttpConfig::default()
        })
        .await;
        assert_eq!(
            caller_seen_by_handler(server_addr, "Bearer reader-key").await,
            (serde_json::json!("api_key:reader"), true, false)
        );
        assert_eq!(
            caller_seen_by_handler(server_addr, "Bearer deleter-key").await,
            (serde_json::json!("api_key:deleter"), false, true)
        );
        assert_eq!(
            caller_seen_by_handler(server_addr, "Bearer admin-key").await,
            (serde_json::json!("api_key:admin"), true, true)
        );
    }

    #[tokio::test]
    async fn test_oauth_scope_tools_are_passed_to_the_request_handler() {
        let auth = crate::transport::oauth::test_support::TestAuthServer::start().await;
        let mut oauth = auth.config(OAUTH_RESOURCE);
        oauth.scope_tools = [
            ("mcp:read".to_string(), vec!["@read_only".to_string()]),
            ("mcp:write".to_string(), vec!["*".to_string()]),
        ]
        .into_iter()
        .collect();
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            oauth: Some(oauth),
            ..HttpConfig::default()
        })
        .await;
        let token = |scope: &str| {
            format!(
                "Bearer {}",
                auth.token(OAUTH_RESOURCE, serde_json::json!({ "scope": scope }))
            )
        };
        assert_eq!(
            caller_seen_by_handler(server_addr, &token("openid mcp:read")).await,
            (serde_json::json!("oauth:test-client"), true, false)
        );
        assert_eq!(
            caller_seen_by_handler(server_addr, &token("mcp:read mcp:write")).await,
            (serde_json::json!("oauth:test-client"), true, true)
        );
        // 対応するスコープが無いトークンは、どのツールも使えない
        assert_eq!(
            caller_seen_by_handler(server_addr, &token("openid")).await,
            (serde_json::json!("oauth:test-client"), false, false)
        );
    }

    #[tokio::test]
    async fn test_websocket_connection_keeps_its_caller() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            auth_enabled: true,
            api_keys: vec![api_key("reader-key", "reader", Some(&["@read_only"]))],
            ..HttpConfig::default()
        })
        .await;
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://localhost:{}/ws", server_addr.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Authorization", "Bearer reader-key".parse().unwrap());
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let message = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
            panic!("expected a text frame");
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["result"]["caller"],
            serde_json::json!("api_key:reader")
        );
        assert_eq!(value["result"]["can_get"], serde_json::json!(true));
        assert_eq!(value["result"]["can_delete"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn test_tool_restrictions_require_a_request_handler_and_valid_patterns() {
        // チャンネル経由（ハンドラー無し）では制限できないので、起動しない
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![api_key("reader-key", "reader", Some(&["@read_only"]))],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config.clone()).unwrap();
        assert!(transport.start_server().await.is_err());

        // 知らないプリセット
        let transport = HttpTransport::new(HttpConfig {
            api_keys: vec![api_key("reader-key", "reader", Some(&["@admin"]))],
            ..config
        })
        .unwrap()
        .with_request_handler(Arc::new(EchoRequestHandler));
        assert!(transport.start_server().await.is_err());
    }

    async fn start_with_echo_handler(config: HttpConfig) -> SocketAddr {
        let transport = HttpTransport::new(config)
            .unwrap()
            .with_request_handler(Arc::new(EchoRequestHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        server_addr
    }

    #[tokio::test]
    async fn test_request_handler_answers_requests_and_accepts_notifications() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();

        let response = client
            .post(&url)
            .json(&serde_json::json!({"jsonrpc": "2.0", "id": 7, "method": "tools/list"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["id"], serde_json::json!(7));
        assert_eq!(body["result"]["method"], serde_json::json!("tools/list"));

        // 通知には本文なしの202（#374）
        let response = client
            .post(&url)
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert!(response.text().await.unwrap().is_empty());

        // JSON-RPCのメッセージとして不正なものは400
        let response = client
            .post(&url)
            .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// 多くのクライアントは`id`を1から振る。チャンネル経由の待ち合わせは
    /// `id`で応答を探すため、同時に来た同じ`id`の応答が入れ替わりえた。
    /// 直接処理なら、各リクエストに自分の応答が返る（#400）。
    #[tokio::test]
    async fn test_request_handler_keeps_concurrent_requests_with_same_id_apart() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();

        let send = |tag: &'static str, sleep_ms: u64| {
            let client = client.clone();
            let url = url.clone();
            async move {
                let body: Value = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "tools/call",
                        "params": { "tag": tag, "sleep_ms": sleep_ms }
                    }))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                body["result"]["tag"].as_str().unwrap().to_string()
            }
        };
        // 先に送ったAの方が遅く終わる
        let (a, b) = tokio::join!(send("A", 300), send("B", 0));
        assert_eq!(a, "A");
        assert_eq!(b, "B");
    }

    /// `transport_info()`は同期のメソッドだが、asyncの中から呼んでも、
    /// 起動後は実際に待ち受けたアドレスを返す（#265）
    #[tokio::test]
    async fn test_transport_info_reports_bound_address() {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        })
        .unwrap();
        let addr_of = |transport: &HttpTransport| match transport.transport_info().transport_type {
            TransportType::Http { addr } => addr,
            other => panic!("unexpected transport type: {other:?}"),
        };

        // 起動前は設定のアドレス
        assert_eq!(addr_of(&transport).port(), 0);

        transport.start_server().await.unwrap();
        let bound = transport.bound_addr().await.unwrap();
        assert_ne!(bound.port(), 0);
        assert_eq!(addr_of(&transport), bound);
    }

    #[tokio::test]
    async fn test_request_handler_still_requires_api_key() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![ApiKeyEntry {
                key: "correct-key".to_string(),
                name: "test".to_string(),
                user_id: "user-1".to_string(),
                tools: None,
            }],
            ..HttpConfig::default()
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let request = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});

        let response = client.post(&url).json(&request).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = client
            .post(&url)
            .header("Authorization", "Bearer correct-key")
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_request_handler_answers_websocket_frames() {
        let server_addr = start_with_echo_handler(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        })
        .await;
        let url = format!("ws://localhost:{}/ws", server_addr.port());
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        // 通知には応答しないので、続くリクエストの応答が最初に届く
        for frame in [
            serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
        ] {
            ws_stream
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    frame.to_string().into(),
                ))
                .await
                .unwrap();
        }

        let response_msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("timed out waiting for a WebSocket response")
            .expect("stream ended without a response")
            .expect("WebSocket error while awaiting response");
        let response_text = match response_msg {
            tokio_tungstenite::tungstenite::Message::Text(text) => text,
            other => panic!("expected a text frame in response, got: {other:?}"),
        };
        let response_value: Value = serde_json::from_str(&response_text).unwrap();
        assert_eq!(response_value["id"], serde_json::json!(3));
        assert_eq!(
            response_value["result"]["method"],
            serde_json::json!("tools/list")
        );
    }

    #[test]
    fn test_origin_policy_allows_loopback_and_configured_origins() {
        let policy = OriginPolicy::new(&["https://App.Example.com/".to_string()]);
        for allowed in [
            "http://localhost",
            "http://localhost:3000",
            "https://127.0.0.1:8081",
            "http://127.0.0.2",
            "http://[::1]:6274",
            "HTTP://LOCALHOST:3000",
            "https://app.example.com",
        ] {
            assert!(policy.allows(allowed), "{allowed}");
        }
        for rejected in [
            "null",
            "https://example.com",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "http://localhost:3000@evil.example",
            "http://localhost:abc",
            "file://",
            "https://app.example.com:8443",
            "http://app.example.com",
        ] {
            assert!(!policy.allows(rejected), "{rejected}");
        }
        assert!(OriginPolicy::new(&["*".to_string()]).allows("https://anything.example"));
    }

    /// `MCP-Protocol-Version`ヘッダーの版（#484）を、結果の`protocol_version`
    /// で返す
    struct VersionEchoHandler;

    #[async_trait]
    impl JsonRpcRequestHandler for VersionEchoHandler {
        async fn handle_jsonrpc(
            &self,
            message: &str,
            caller: &crate::mcp::Caller,
        ) -> std::result::Result<Option<Value>, String> {
            self.handle_jsonrpc_with_protocol_version(message, caller, None)
                .await
        }

        async fn handle_jsonrpc_with_protocol_version(
            &self,
            message: &str,
            _caller: &crate::mcp::Caller,
            protocol_version: Option<&'static str>,
        ) -> std::result::Result<Option<Value>, String> {
            let request: Value = serde_json::from_str(message).map_err(|e| e.to_string())?;
            Ok(Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": { "protocol_version": protocol_version }
            })))
        }
    }

    async fn start_version_echo_server(config: HttpConfig) -> (HttpTransport, SocketAddr) {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..config
        })
        .unwrap()
        .with_request_handler(Arc::new(VersionEchoHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        (transport, server_addr)
    }

    /// `Origin`が許可していないオリジンなら403。`Origin`が無いリクエストと、
    /// ループバック・`allowed_origins`のオリジンは受け付ける（#484）
    #[tokio::test]
    async fn test_jsonrpc_rejects_disallowed_origin() {
        let (_transport, server_addr) = start_version_echo_server(HttpConfig {
            allowed_origins: vec!["https://app.example.com".to_string()],
            ..HttpConfig::default()
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let status = |origin: Option<&'static str>| {
            let mut request = client.post(&url).json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list"
            }));
            if let Some(origin) = origin {
                request = request.header("Origin", origin);
            }
            async move { request.send().await.unwrap().status() }
        };
        assert_eq!(status(None).await, StatusCode::OK);
        assert_eq!(status(Some("http://localhost:6274")).await, StatusCode::OK);
        assert_eq!(
            status(Some("https://app.example.com")).await,
            StatusCode::OK
        );
        assert_eq!(
            status(Some("http://attacker.example")).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(status(Some("null")).await, StatusCode::FORBIDDEN);
    }

    /// `Origin`の検証は本文を読む前に行うので、許可していないオリジンからの
    /// 壊れた本文にも403で答える（#488）
    #[tokio::test]
    async fn test_disallowed_origin_gets_403_even_with_a_malformed_body() {
        let (_transport, server_addr) = start_version_echo_server(HttpConfig::default()).await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        for (content_type, body) in [("application/json", "{not json"), ("text/plain", "hello")] {
            let response = client
                .post(&url)
                .header("Origin", "http://attacker.example")
                .header("Content-Type", content_type)
                .body(body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{content_type}");
        }
        // 許可しているオリジン（ループバック）なら、これまでどおり本文の誤り
        let response = client
            .post(&url)
            .header("Origin", "http://localhost:3000")
            .header("Content-Type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// CORSのpreflightは、`Origin`の検証と同じオリジンだけを許可する（#488）
    #[tokio::test]
    async fn test_cors_preflight_follows_allowed_origins() {
        let (_transport, server_addr) = start_version_echo_server(HttpConfig {
            allowed_origins: vec!["https://app.example.com".to_string()],
            ..HttpConfig::default()
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let allow_origin = |origin: &'static str| {
            let request = client
                .request(reqwest::Method::OPTIONS, &url)
                .header("Origin", origin)
                .header("Access-Control-Request-Method", "POST")
                .header("Access-Control-Request-Headers", "content-type");
            async move {
                let response = request.send().await.unwrap();
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .map(|v| v.to_str().unwrap().to_string())
            }
        };
        assert_eq!(
            allow_origin("http://localhost:3000").await.as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(
            allow_origin("https://app.example.com").await.as_deref(),
            Some("https://app.example.com")
        );
        assert_eq!(allow_origin("http://attacker.example").await, None);
    }

    /// `/ws`のアップグレードも、`Origin`を検証する（#484）
    #[tokio::test]
    async fn test_ws_upgrade_rejects_disallowed_origin() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let (_transport, server_addr) = start_version_echo_server(HttpConfig {
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        })
        .await;
        let url = format!("ws://127.0.0.1:{}/ws", server_addr.port());
        let mut request = url.clone().into_client_request().unwrap();
        request.headers_mut().insert(
            "Origin",
            HeaderValue::from_static("http://attacker.example"),
        );
        match tokio_tungstenite::connect_async(request).await {
            Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::FORBIDDEN),
            other => panic!("expected the /ws upgrade to be rejected with 403, got: {other:?}"),
        }

        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_static("http://localhost:3000"));
        assert!(tokio_tungstenite::connect_async(request).await.is_ok());
    }

    /// `MCP-Protocol-Version`ヘッダーの版を、ハンドラーに渡す。対応して
    /// いない版なら400。`initialize`では見ない（#484）
    #[tokio::test]
    async fn test_jsonrpc_uses_mcp_protocol_version_header() {
        let (_transport, server_addr) = start_version_echo_server(HttpConfig::default()).await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let send = |method: &'static str, header: Option<&'static str>| {
            let mut request = client.post(&url).json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 7, "method": method
            }));
            if let Some(header) = header {
                request = request.header("MCP-Protocol-Version", header);
            }
            async move {
                let response = request.send().await.unwrap();
                let status = response.status();
                (status, response.json::<Value>().await.unwrap())
            }
        };

        let (status, body) = send("tools/list", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["protocol_version"], Value::Null);

        for version in ["2025-11-25", "2025-06-18", "2024-11-05"] {
            let (status, body) = send("tools/list", Some(version)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                body["result"]["protocol_version"],
                serde_json::json!(version)
            );
        }

        let (status, body) = send("tools/list", Some("2099-01-01")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["id"], serde_json::json!(7));
        assert_eq!(body["error"]["code"], serde_json::json!(-32600));
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("2099-01-01"));

        // initializeでは、ヘッダーを見ない（版は本文で交渉する）
        let (status, body) = send("initialize", Some("2099-01-01")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["protocol_version"], Value::Null);
    }

    // ---- #540: limits on requests, connections, and WebSocket ----

    /// 設定でサーバーを起動し、ポートを返す（テストの間は止めない）
    async fn start_test_server(configure: impl FnOnce(&mut HttpConfig)) -> u16 {
        let mut config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        };
        configure(&mut config);
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        std::mem::forget(transport);
        server_addr.port()
    }

    fn api_key_config(config: &mut HttpConfig) {
        config.auth_enabled = true;
        config.api_keys = vec![ApiKeyEntry {
            key: "correct-key".to_string(),
            name: "test".to_string(),
            user_id: "user-1".to_string(),
            tools: None,
        }];
    }

    /// 生のTCPで送り、相手が閉じるか`limit`が過ぎるまで受け取る。
    /// 戻り値は（受け取った文字列、閉じられたか）
    async fn raw_exchange(port: u16, request: &[u8], limit: std::time::Duration) -> (String, bool) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request).await.unwrap();
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        let closed = tokio::time::timeout(limit, async {
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => received.extend_from_slice(&buf[..n]),
                }
            }
        })
        .await
        .is_ok();
        (String::from_utf8_lossy(&received).into_owned(), closed)
    }

    /// 本文の大きさの上限（`max_request_size`）を超えたら413（#540）。
    /// 以前は、axumの既定の2MBのままだった
    #[tokio::test]
    async fn test_request_body_over_max_request_size_is_rejected() {
        let port = start_test_server(|c| c.max_request_size = 1000).await;
        let client = reqwest::Client::new();
        let big = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": { "padding": "x".repeat(2000) }
        });
        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", port))
            .json(&big)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let response = client
            .post(format!("http://127.0.0.1:{}/mcp", port))
            .json(&notification_jsonrpc_request())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
    }

    /// 認証は、本文を読む前に確かめる（#540）。本文を送らなくても、すぐに
    /// 401が返る
    #[tokio::test]
    async fn test_authentication_is_checked_before_reading_the_body() {
        let port = start_test_server(api_key_config).await;
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 100000\r\n\r\n"
        );
        let (received, _) =
            raw_exchange(port, request.as_bytes(), std::time::Duration::from_secs(3)).await;
        assert!(received.starts_with("HTTP/1.1 401"), "{received}");
    }

    /// ヘッダーを送り終えない接続は、`timeout_ms`で閉じる（#540）。以前は
    /// `axum::serve`にタイマーを渡しておらず、時間の制限が無かった
    #[tokio::test]
    async fn test_slow_headers_are_closed_after_the_timeout() {
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let started = std::time::Instant::now();
        let (_, closed) = raw_exchange(
            port,
            format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n").as_bytes(),
            std::time::Duration::from_secs(5),
        )
        .await;
        assert!(closed, "the connection was not closed");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    /// 本文を送り終えないリクエストも、`timeout_ms`で打ち切る（#540）
    #[tokio::test]
    async fn test_slow_body_is_cut_off_after_the_timeout() {
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let started = std::time::Instant::now();
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{\"jsonrpc\""
        );
        let (received, closed) =
            raw_exchange(port, request.as_bytes(), std::time::Duration::from_secs(5)).await;
        assert!(
            closed || !received.is_empty(),
            "no response and the connection stayed open"
        );
        assert!(!received.starts_with("HTTP/1.1 2"), "{received}");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    /// 同時の接続は`max_connections`まで（#540）。超えた接続は、前の接続が
    /// 閉じるまで待つ
    #[tokio::test]
    async fn test_connections_over_max_connections_wait() {
        let port = start_test_server(|c| c.max_connections = 1).await;
        // 何も送らない接続で、枠を1つ使う
        let idle = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mcp", port);
        let waiting = client
            .post(&url)
            .json(&notification_jsonrpc_request())
            .timeout(std::time::Duration::from_millis(700))
            .send()
            .await;
        assert!(waiting.is_err(), "the second connection was served at once");

        drop(idle);
        let response = client
            .post(&url)
            .json(&notification_jsonrpc_request())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
    }

    /// 次のWebSocketの受信で、接続が閉じられたかを確かめる。戻り値は閉じた
    /// ときのコード（Closeフレームが無ければ`None`）
    async fn ws_close_code<S>(ws: &mut S, limit: std::time::Duration) -> Option<Option<u16>>
    where
        S: futures::Stream<
                Item = std::result::Result<
                    tokio_tungstenite::tungstenite::Message,
                    tokio_tungstenite::tungstenite::Error,
                >,
            > + Unpin,
    {
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        tokio::time::timeout(limit, async {
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Close(frame))) => {
                        return frame.map(|f| u16::from(f.code));
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => return None,
                }
            }
        })
        .await
        .ok()
    }

    /// WebSocketのメッセージの大きさの上限（#540）。以前はtungsteniteの
    /// 既定（64MiB）のままだった
    #[tokio::test]
    async fn test_websocket_message_over_max_request_size_closes_the_connection() {
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        let port = start_test_server(|c| {
            c.enable_websocket_upgrade = true;
            c.max_request_size = 1000;
        })
        .await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .unwrap();
        ws.send(WsMessage::Text("x".repeat(5000).into()))
            .await
            .unwrap();
        assert!(
            ws_close_code(&mut ws, std::time::Duration::from_secs(5))
                .await
                .is_some(),
            "the connection was not closed"
        );
    }

    /// pingに応答しない（何も読まない）WebSocketは、
    /// `websocket_idle_timeout_secs`で閉じる（#540）
    #[tokio::test]
    async fn test_unresponsive_websocket_is_closed() {
        let port = start_test_server(|c| {
            c.enable_websocket_upgrade = true;
            c.websocket_idle_timeout_secs = 1;
        })
        .await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .unwrap();
        // 読まない間は、届いたpingにpongを返さない
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        // 閉じられたこと。クライアントは、読むときに溜まったpingへpongを
        // 返そうとするので、Closeフレームを読む前に、閉じた接続への送信で
        // エラーになることがある（CIで起きた）。Closeフレームを読めたなら、
        // コードは1001
        let code = ws_close_code(&mut ws, std::time::Duration::from_secs(5))
            .await
            .expect("the connection was not closed");
        assert!(
            matches!(code, None | Some(close_code::AWAY)),
            "close code: {code:?}"
        );
    }

    /// 何も送らないが、pingにpongを返すクライアントは閉じない（PR #548の
    /// レビュー: 考えている間のMCPクライアント等）
    #[tokio::test]
    async fn test_quiet_websocket_answering_pings_stays_open() {
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        let port = start_test_server(|c| {
            c.enable_websocket_upgrade = true;
            c.websocket_idle_timeout_secs = 1;
        })
        .await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .unwrap();
        // 読み続ける（tungsteniteは、読むときにpongを返す）。上限の3倍の間
        let mut pings = 0;
        let quiet = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Ping(_))) => pings += 1,
                    Some(Ok(WsMessage::Close(frame))) => return Some(frame),
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return None,
                }
            }
        })
        .await;
        assert!(quiet.is_err(), "the connection was closed: {quiet:?}");
        assert!(pings >= 2, "pings: {pings}");
        // まだ使える
        ws.send(WsMessage::Text(
            serde_json::to_string(&notification_jsonrpc_request())
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    }

    /// OAuthのトークンの期限で、WebSocketの接続を閉じる（#540）。以前は
    /// 接続のときにしか見なかった
    #[tokio::test]
    async fn test_websocket_is_closed_when_the_access_token_expires() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let (port, auth) = start_oauth_server(|c| {
            if let Some(oauth) = c.oauth.as_mut() {
                oauth.leeway_seconds = 0;
            }
        })
        .await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({ "exp": now + 2 }));
        let mut request = format!("ws://localhost:{}/ws", port)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let code = ws_close_code(&mut ws, std::time::Duration::from_secs(6)).await;
        assert_eq!(code, Some(Some(close_code::POLICY)));
    }

    /// 認証の失敗は、1分あたり上限の数まで記録し、残りの数をまとめる（#540）
    #[test]
    fn test_auth_failure_audit_limiter() {
        let limiter = AuthFailureAuditLimiter::new();
        let start = std::time::Instant::now();
        for _ in 0..AUTH_FAILURE_AUDIT_PER_MINUTE {
            assert_eq!(limiter.admit(start), (true, None));
        }
        assert_eq!(limiter.admit(start), (false, None));
        assert_eq!(limiter.admit(start), (false, None));
        // 1分の間は、まとめない
        assert_eq!(limiter.take_report(start), None);
        // 1分が過ぎたら、次の失敗を待たずに、書かなかった数を返す
        let later = start + std::time::Duration::from_secs(61);
        let report = limiter
            .take_report(later)
            .expect("a report after the minute");
        assert_eq!(report.count, 2);
        assert_eq!(limiter.take_report(later), None);
        // 次の1分は、記録を再開する
        assert_eq!(limiter.admit(later), (true, None));
        // 失敗が来て1分が過ぎた場合も、返す
        let limiter = AuthFailureAuditLimiter::new();
        for _ in 0..=AUTH_FAILURE_AUDIT_PER_MINUTE {
            limiter.admit(start);
        }
        let (record, report) = limiter.admit(later);
        assert!(record);
        assert_eq!(report.map(|r| r.count), Some(1));
    }

    /// 無通信の上限より長くかかる処理の直後に、閉じない（PR #548の
    /// 再レビュー）。処理の間は受信もpingもしないので、処理が終わった時刻から
    /// 数え直す
    #[tokio::test]
    async fn test_websocket_is_not_closed_right_after_a_long_request() {
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            websocket_idle_timeout_secs: 1,
            ..HttpConfig::default()
        })
        .unwrap()
        .with_request_handler(Arc::new(EchoRequestHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", server_addr.port()))
                .await
                .unwrap();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": { "sleep_ms": 1500 }
        });
        ws.send(WsMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        // 応答を受け取り、その後も上限の間は閉じられない
        let after_response = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Text(_))) => break,
                    Some(Ok(WsMessage::Close(frame))) => panic!("closed: {frame:?}"),
                    Some(Ok(_)) => {}
                    other => panic!("unexpected: {other:?}"),
                }
            }
        })
        .await;
        assert!(after_response.is_ok(), "no response");
        let quiet = tokio::time::timeout(Duration::from_millis(800), async {
            loop {
                match ws.next().await {
                    Some(Ok(WsMessage::Close(frame))) => return Some(frame),
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return None,
                }
            }
        })
        .await;
        assert!(quiet.is_err(), "closed right after the response: {quiet:?}");
    }

    /// pingの間隔は、無通信の上限の半分以下で、30秒まで
    #[test]
    fn test_websocket_ping_interval() {
        use std::time::Duration;
        assert_eq!(
            websocket_ping_interval(Duration::from_secs(300)),
            Duration::from_secs(30)
        );
        assert_eq!(
            websocket_ping_interval(Duration::from_secs(40)),
            Duration::from_secs(20)
        );
        assert_eq!(
            websocket_ping_interval(Duration::from_secs(1)),
            Duration::from_millis(500)
        );
    }

    /// ループバック以外で、認証が無ければ起動しない（#540）
    #[test]
    fn test_unauthenticated_external_bind() {
        let external = HttpConfig {
            bind_addr: "0.0.0.0:8081".parse().unwrap(),
            ..HttpConfig::default()
        };
        assert!(unauthenticated_external_bind(&external).is_some());
        for config in [
            HttpConfig {
                bind_addr: "127.0.0.1:8081".parse().unwrap(),
                ..HttpConfig::default()
            },
            HttpConfig {
                bind_addr: "[::1]:8081".parse().unwrap(),
                ..HttpConfig::default()
            },
            HttpConfig {
                auth_enabled: true,
                ..external.clone()
            },
            HttpConfig {
                mtls_enabled: true,
                ..external.clone()
            },
            HttpConfig {
                allow_unauthenticated_external: true,
                ..external.clone()
            },
        ] {
            assert_eq!(unauthenticated_external_bind(&config), None, "{config:?}");
        }
    }
}
