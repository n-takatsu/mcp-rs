//! HTTP Transport implementation for MCP-RS
//!
//! This module provides HTTP-based JSON-RPC transport for MCP communication.

use crate::{
    error::{Error, Result},
    security::{
        AntiReplayConfig, AntiReplayMiddleware, ApiKeyManager, AuditLogger, IDSConfig,
        IntrusionDetectionSystem, NetworkPolicy, RecommendedAction, ReplayError, RequestData,
        SecurityHeaders,
    },
    transport::{
        oauth::{www_authenticate, OAuthConfig, OAuthValidator, TokenError},
        ConnectionStats, Transport, TransportCapabilities, TransportError, TransportInfo,
        TransportType,
    },
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use axum::{
    extract::{
        rejection::JsonRejection,
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
use ipnet::IpNet;
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
    /// Addresses (IP addresses or CIDR ranges) of the reverse proxies in
    /// front of this transport (#543). Headers that a proxy sets - the
    /// protocol (`X-Forwarded-Proto`, `Forwarded`), the TLS version, the
    /// certificate fingerprint (`certificate_pin_header`), and the client
    /// address (`X-Forwarded-For`, `X-Real-IP`) - are used only on
    /// connections from these addresses, and ignored on others, since any
    /// client can send them. Required by `enforce_https` without
    /// `tls_enabled`, by certificate pinning without `tls_enabled`, and by
    /// `ids_trust_forwarded_for`. Defaults to empty.
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
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
    /// closes. A `/ws` connection counts only until its upgrade: after that,
    /// it is limited by `websocket_max_connections` instead (#596; this
    /// comment used to say WebSocket connections were included).
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
            trusted_proxies: Vec::new(),
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

/// `HttpStats`を共有するもの。同期のロックで持つ（#760）。同期の
/// `connection_stats()`をTokioのタスクから呼んでも、パニックしないように
/// する（以前は`blocking_read()`で、非同期の文脈ではパニックした）。ロックは
/// 数を足す間だけ持ち、持ったまま`.await`しない
#[derive(Debug, Clone, Default)]
struct SharedStats(Arc<std::sync::Mutex<HttpStats>>);

impl SharedStats {
    fn new(stats: HttpStats) -> Self {
        Self(Arc::new(std::sync::Mutex::new(stats)))
    }

    /// 値は数だけで、書き込み中にパニックしても壊れた状態にはならないため、
    /// ポイズンは無視する
    fn lock(&self) -> std::sync::MutexGuard<'_, HttpStats> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
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

    /// `handle_jsonrpc_with_protocol_version`と同じ。`session`は、この
    /// リクエストに使う接続の状態（`initialize`で決まった版、#570・#596）。
    /// `/mcp`はリクエストごとに新しいものを、`/ws`は接続ごとのものを渡す。
    /// 既定の実装は`session`を使わない。
    async fn handle_jsonrpc_in_session(
        &self,
        message: &str,
        caller: &crate::mcp::Caller,
        protocol_version: Option<&'static str>,
        session: &crate::mcp::Session,
    ) -> std::result::Result<Option<Value>, String> {
        let _ = session;
        self.handle_jsonrpc_with_protocol_version(message, caller, protocol_version)
            .await
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

/// `Origin`を検証するミドルウェア（#484・#488）。`/`・`/mcp`・`/ws`の
/// いちばん外側（CORSより外、#741）に付ける。ハンドラーの引数（`Json`の
/// 取り出し等）やCORSのpreflightより先に動くので、許可していないオリジンには、
/// 本文が壊れていても、preflightでも、必ず403で答える。Protected Resource
/// Metadataは公開の情報なので付けない。
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
    Err(unsupported_protocol_version(requested, request))
}

/// 対応していない版への400の応答（2026-07-28の`UnsupportedProtocolVersion`、
/// #570。以前は`-32600`だった）。`data`で、対応する版（`initialize`で交渉
/// する版も）を知らせる
fn unsupported_protocol_version(requested: &str, request: &Value) -> Box<Response> {
    warn!(
        "Rejected request with unsupported MCP-Protocol-Version '{}'",
        requested
    );
    let supported = crate::mcp::all_supported_protocol_versions();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": crate::mcp::UNSUPPORTED_PROTOCOL_VERSION_CODE,
            "message": format!(
                "Unsupported MCP-Protocol-Version: '{}' (supported: {})",
                requested,
                supported.join(", ")
            ),
            "data": { "supported": supported, "requested": requested }
        },
        "id": error_response_id(request)
    });
    Box::new((StatusCode::BAD_REQUEST, Json(body)).into_response())
}

/// 断るリクエストへのエラー応答に返す`id`。文字列・数値だけを返し、ほか
/// （オブジェクト・配列・真偽値・無い）は`null`にする（#759）
fn error_response_id(request: &Value) -> Value {
    request
        .get("id")
        .filter(|id| id.is_string() || id.is_number())
        .cloned()
        .unwrap_or(Value::Null)
}

/// 2026-07-28のリクエスト（`is_modern_request`でヘッダーと本文が合うことを
/// 確かめた後）の版が、対応するものか。ハンドラーに任せず、ここで断る。
/// 組み込みの`RequestDispatcher`は自分でも確かめるが、ライブラリとして
/// 独自のハンドラーを使う場合に、対応していない版を通さないため（#697）
fn check_modern_protocol_version(
    headers: &HeaderMap,
    request: &Value,
) -> std::result::Result<(), Box<Response>> {
    let requested = header_value(headers, "mcp-protocol-version")
        .map(str::trim)
        .unwrap_or("");
    if crate::mcp::is_modern_protocol_version(requested) {
        Ok(())
    } else {
        Err(unsupported_protocol_version(requested, request))
    }
}

/// ヘッダーと本文が合わないときのエラー（2026-07-28の`HeaderMismatch`）
const HEADER_MISMATCH_CODE: i32 = -32020;

/// リクエストが2026-07-28のものか（#570）。本文の`params._meta`に版が
/// あるか、`MCP-Protocol-Version`ヘッダーが2026-07-28の版なら、そう扱う。
/// `initialize`は今までどおり（legacy）。
///
/// 2026-07-28のリクエストでは、仕様（Streamable HTTP）どおり、ヘッダーと
/// 本文が合うかを確かめ、合わなければ400と`-32020`を返す。
/// - `MCP-Protocol-Version`と`_meta`の`io.modelcontextprotocol/protocolVersion`
/// - `Mcp-Method`と`method`
/// - `tools/call`・`prompts/get`では`Mcp-Name`と`params.name`、
///   `resources/read`では`Mcp-Name`と`params.uri`
///
/// ヘッダーが無いのも、合わないのと同じに扱う。
fn is_modern_request(
    headers: &HeaderMap,
    request: &Value,
) -> std::result::Result<bool, Box<Response>> {
    let method = request.get("method").and_then(Value::as_str);
    if method == Some("initialize") {
        return Ok(false);
    }
    let body_version = request
        .get("params")
        .and_then(|params| params.get("_meta"))
        .and_then(|meta| meta.get(crate::mcp::META_PROTOCOL_VERSION));
    let header_version = header_value(headers, "mcp-protocol-version").map(str::trim);
    if body_version.is_none() && !header_version.is_some_and(crate::mcp::is_modern_protocol_version)
    {
        return Ok(false);
    }

    let mismatch = |message: String| -> std::result::Result<bool, Box<Response>> {
        warn!("Rejected an MCP request: {}", message);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "error": { "code": HEADER_MISMATCH_CODE, "message": message },
            "id": error_response_id(request)
        });
        Err(Box::new(
            (StatusCode::BAD_REQUEST, Json(body)).into_response(),
        ))
    };
    let check =
        |header: &str, actual: Option<&str>, expected: Option<&str>| match (actual, expected) {
            (Some(actual), Some(expected)) if actual == expected => Ok(()),
            (None, _) => Err(format!("Header mismatch: {} header is missing", header)),
            (Some(actual), expected) => Err(format!(
                "Header mismatch: {} header value '{}' does not match body value '{}'",
                header,
                actual,
                expected.unwrap_or("")
            )),
        };

    if let Err(message) = check(
        "MCP-Protocol-Version",
        header_version,
        body_version.and_then(Value::as_str),
    ) {
        return mismatch(message);
    }
    if let Err(message) = check("Mcp-Method", header_value(headers, "mcp-method"), method) {
        return mismatch(message);
    }
    let name_field = match method {
        Some("tools/call") | Some("prompts/get") => Some("name"),
        Some("resources/read") => Some("uri"),
        _ => None,
    };
    if let Some(field) = name_field {
        let name = headers
            .get("mcp-name")
            .and_then(|value| decode_mcp_header_value(value.as_bytes()));
        let expected = request
            .get("params")
            .and_then(|params| params.get(field))
            .and_then(Value::as_str);
        if let Err(message) = check("Mcp-Name", name.as_deref(), expected) {
            return mismatch(message);
        }
    }
    Ok(true)
}

/// `Mcp-Name`等のヘッダーの値を読む（2026-07-28）。ASCIIで送れない値は、
/// UTF-8のバイト列をBase64にして`=?base64?…?=`で囲んで送られる。
/// 読めなければ`None`（ヘッダーが無いのと同じに扱う）。
fn decode_mcp_header_value(raw: &[u8]) -> Option<String> {
    use base64::Engine as _;
    let value = std::str::from_utf8(raw).ok()?;
    match value
        .strip_prefix("=?base64?")
        .and_then(|rest| rest.strip_suffix("?="))
    {
        Some(encoded) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()?;
            String::from_utf8(bytes).ok()
        }
        None if value.is_ascii() => Some(value.to_string()),
        None => None,
    }
}

/// 2026-07-28のリクエストへの応答のHTTPの状態（#570）。仕様
/// （Streamable HTTP）どおり、知らないメソッド（`-32601`）は404、
/// 版・`_meta`・能力の誤り（`-32022`・`-32602`・`-32021`）は400。
/// ほかは200。
fn modern_response_status(response: &Value) -> StatusCode {
    match response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64)
    {
        Some(-32601) => StatusCode::NOT_FOUND,
        Some(-32602) | Some(-32021) | Some(-32022) => StatusCode::BAD_REQUEST,
        _ => StatusCode::OK,
    }
}

/// `id`の無いJSON-RPCのエラー（JSONとして読めない等、#543）を、400で返す。
/// MCPの仕様（Streamable HTTP）は、受け付けられない入力に400を勧め、本文に
/// `id`の無いJSON-RPCのエラーを置いてよいとしている
fn jsonrpc_error_response(code: i32, message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(jsonrpc_error_body(code, message)),
    )
        .into_response()
}

/// `id`の無いJSON-RPCのエラー（#543）を、`/ws`のメッセージにする
fn jsonrpc_error_message(code: i32, message: &str) -> Message {
    Message::Text(jsonrpc_error_body(code, message).to_string().into())
}

fn jsonrpc_error_body(code: i32, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "error": { "code": code, "message": message },
        "id": Value::Null
    })
}

/// チャンネルに渡す要求の`id`が正しいか（PR #771のレビュー）。`id`が
/// 無ければ通知で、正しい。あれば文字列か数値でなければならない（`null`は
/// 通知ではなく、不正な要求。`RequestDispatcher`の確かめ方と同じ、#759）
fn channel_request_id_is_valid(request: &Value) -> bool {
    request
        .get("id")
        .is_none_or(|id| id.is_string() || id.is_number())
}

/// `id`が`null`のJSON-RPCのエラー（リクエストとして読めなかったもの）か
fn is_error_without_id(response: &Value) -> bool {
    response.get("error").is_some() && response.get("id").is_none_or(Value::is_null)
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

/// これより短いAPIキーは、起動時に警告する（#596）
const MIN_RECOMMENDED_API_KEY_CHARS: usize = 16;

/// Shared state for HTTP transport
#[derive(Clone)]
struct HttpTransportState {
    request_handler: Option<Arc<dyn JsonRpcRequestHandler>>,
    /// `with_request_handler`を使わないときに、要求を渡すチャンネル
    channel: RequestChannel,
    stats: SharedStats,
    /// `stop()`で`true`になる（#760）。`/ws`の接続を閉じる
    shutdown: tokio::sync::watch::Receiver<bool>,
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
    /// 転送のヘッダーを信じるプロキシ（#543）
    trusted_proxies: Arc<[IpNet]>,
    api_key_manager: Option<Arc<RwLock<ApiKeyManager>>>,
    /// APIキー（`ApiKey::id`）→ 名前と使えるツール（#52）
    api_key_callers: Arc<HashMap<String, crate::mcp::Caller>>,
    oauth: Option<Arc<OAuthValidator>>,
    /// `/ws`の接続 → 呼び出し元（接続時の認証で決まる、#52）と接続の状態
    ws_callers: WsClients,
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
    /// アドレスごとの認証の失敗の数（#596）
    auth_failures: Arc<AuthFailureLimiter>,
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

/// `with_request_handler`を使わないときに、チャンネルで運ぶ要求（#749・
/// #760）
#[derive(Debug)]
struct QueuedRequest {
    /// 本文。`id`は、内部の相関ID（`RequestChannel::forward_request`）に
    /// 置き換えてある
    message: String,
    caller: crate::mcp::Caller,
    /// `MCP-Protocol-Version`ヘッダーで選んだ版（検証済み）
    protocol_version: Option<&'static str>,
    /// 応答を待っている要求なら、その期限と取り消しの印（PR #771の
    /// レビュー）。通知は`None`（いつでも渡す）
    waiter: Option<QueuedWaiter>,
}

/// キューの中の要求が、まだ応答を待たれているか（PR #771のレビュー）
#[derive(Debug)]
struct QueuedWaiter {
    /// 応答を待つ期限。過ぎたら、HTTPは408を返している
    deadline: tokio::time::Instant,
    /// 待つのをやめたら（期限切れ・クライアントが切った等）`true`
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl QueuedRequest {
    /// もう誰も応答を待っていない要求か。受信側に渡さずに捨てる（処理を
    /// 始めると、408の後に実行され、クライアントがやり直すと副作用が重なる）
    fn is_abandoned(&self) -> bool {
        self.waiter.as_ref().is_some_and(|waiter| {
            waiter.cancelled.load(Ordering::SeqCst)
                || tokio::time::Instant::now() >= waiter.deadline
        })
    }
}

/// `HttpTransport::receive_request_with_context`が返す要求（#760）
#[derive(Debug, Clone)]
pub struct ReceivedRequest {
    /// 要求。`id`は、`HttpTransport`が要求ごとに付けた内部の相関IDで、
    /// クライアントの付けた`id`ではない。応答の`id`には、これをそのまま
    /// 入れる。`send_message`が、クライアントの`id`に戻して返す
    pub request: JsonRpcRequest,
    /// 呼び出し元（APIキー・トークンの名前と使えるツール、接続元の
    /// IPアドレス、#749）
    pub caller: crate::mcp::Caller,
    /// `MCP-Protocol-Version`ヘッダーで選んだ版（検証済み、#484）。
    /// ヘッダーが無い要求・`initialize`・2026-07-28版の要求（版は本文の
    /// `_meta`で決まる）・WebSocketでは`None`。
    ///
    /// `RequestDispatcher::handle_message_with_protocol_version`に渡すと:
    /// - `Some(版)`: `with_request_handler`と同じく、その版で応答の項目が
    ///   決まる
    /// - `None`: ディスパッチャーの共有の版（それまでの`initialize`で決まった
    ///   版、例: 2025-11-25）になる。`with_request_handler`の`/mcp`は、要求
    ///   ごとに新しい接続の状態を使い、対応する一番古い版
    ///   （`DEFAULT_PROTOCOL_VERSION`）で答えるので、ツールの`title`の有無等が
    ///   違うことがある。同じにするなら、`handle_message_in_session`に要求ごとの
    ///   新しい`Session`を渡す（PR #771のレビュー）
    pub protocol_version: Option<&'static str>,
}

/// チャンネル経由の要求の、応答を待つ時間（キューの枠を待つ時間を含む）
const CHANNEL_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// チャンネル経由の要求に付ける、内部の相関IDの頭（#760）
const CORRELATION_ID_PREFIX: &str = "mcp-rs-";

/// チャンネル経由の要求1件の、応答の待ち先（#760）
#[derive(Debug)]
struct PendingResponse {
    /// クライアントの付けた`id`。応答を送るときに戻す
    client_id: Value,
    response_tx: tokio::sync::oneshot::Sender<Value>,
}

/// 内部の相関ID → 応答の待ち先（#760）。以前はクライアントの付けた`id`で
/// 照合し、別のクライアントが同じ`id`を使うと、応答が入れ替わった。
/// 同期のロックで持つ（`Drop`で登録を消すため。持ったまま`.await`しない）
#[derive(Debug, Clone, Default)]
struct PendingResponses(Arc<std::sync::Mutex<HashMap<String, PendingResponse>>>);

impl PendingResponses {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingResponse>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// チャンネル経由の要求が処理できなかった理由（#760）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelError {
    /// 期限までにキューの枠が空かなかった（受信側が処理を止めている）
    Busy,
    /// チャンネルが閉じている
    Closed,
    /// 応答の待ち先が、応答を受けずに消えた
    NoResponse,
    /// 期限までに応答が無かった
    TimedOut,
}

impl ChannelError {
    fn status(self) -> StatusCode {
        match self {
            ChannelError::Busy => StatusCode::SERVICE_UNAVAILABLE,
            ChannelError::Closed | ChannelError::NoResponse => StatusCode::INTERNAL_SERVER_ERROR,
            ChannelError::TimedOut => StatusCode::REQUEST_TIMEOUT,
        }
    }
}

/// `with_request_handler`を使わないときに、HTTP・WebSocketで受けた要求を
/// 受信側（`receive_request_with_context`）に渡し、応答を待つ（#760）
#[derive(Debug, Clone)]
struct RequestChannel {
    sender: tokio::sync::mpsc::Sender<QueuedRequest>,
    pending: PendingResponses,
    next_correlation_id: Arc<AtomicU64>,
}

impl RequestChannel {
    /// `deadline`までにキューの枠を取る。以前は`send().await`で期限なしに
    /// 待ち、キューが埋まると、応答の期限が始まる前に止まった
    async fn reserve(
        &self,
        deadline: tokio::time::Instant,
    ) -> std::result::Result<tokio::sync::mpsc::Permit<'_, QueuedRequest>, ChannelError> {
        match tokio::time::timeout_at(deadline, self.sender.reserve()).await {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err(ChannelError::Closed),
            Err(_) => Err(ChannelError::Busy),
        }
    }

    /// 通知（`id`なし）を渡す
    fn forward_notification(
        permit: tokio::sync::mpsc::Permit<'_, QueuedRequest>,
        message: String,
        caller: crate::mcp::Caller,
        protocol_version: Option<&'static str>,
    ) {
        permit.send(QueuedRequest {
            message,
            caller,
            protocol_version,
            waiter: None,
        });
    }

    /// 要求の`id`を内部の相関IDに置き換えて渡し、応答の待ち先を登録する。
    /// 登録は渡す前にする（受信側が速く応答しても、待ち先がある）。
    /// `deadline`を過ぎるか、待つのをやめたら、受信側はこの要求を受け取らない
    fn forward_request(
        &self,
        permit: tokio::sync::mpsc::Permit<'_, QueuedRequest>,
        mut request: Value,
        caller: crate::mcp::Caller,
        protocol_version: Option<&'static str>,
        deadline: tokio::time::Instant,
    ) -> PendingRequest {
        let correlation_id = format!(
            "{}{}",
            CORRELATION_ID_PREFIX,
            self.next_correlation_id.fetch_add(1, Ordering::Relaxed)
        );
        let client_id = request.get("id").cloned().unwrap_or(Value::Null);
        if let Some(object) = request.as_object_mut() {
            object.insert("id".to_string(), Value::String(correlation_id.clone()));
        }
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.pending.lock().insert(
            correlation_id.clone(),
            PendingResponse {
                client_id,
                response_tx,
            },
        );
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let registration = PendingRegistration {
            pending: self.pending.clone(),
            correlation_id,
            cancelled: cancelled.clone(),
        };
        permit.send(QueuedRequest {
            message: request.to_string(),
            caller,
            protocol_version,
            waiter: Some(QueuedWaiter {
                deadline,
                cancelled,
            }),
        });
        PendingRequest {
            registration,
            response_rx,
        }
    }
}

/// 応答の待ち先の登録。捨てるときに、この要求の登録だけを消す（#760）。
/// HTTPの要求が途中で捨てられても（クライアントが切った等）、登録が
/// 残らないようにする
struct PendingRegistration {
    pending: PendingResponses,
    correlation_id: String,
    /// キューの中の要求に、待つのをやめたことを伝える（PR #771のレビュー）
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for PendingRegistration {
    fn drop(&mut self) {
        // 応答を受けた後なら、要求はもう受信側に渡っているので、影響しない
        self.cancelled.store(true, Ordering::SeqCst);
        self.pending.lock().remove(&self.correlation_id);
    }
}

/// 渡した要求の応答を待つもの（#760）
struct PendingRequest {
    registration: PendingRegistration,
    response_rx: tokio::sync::oneshot::Receiver<Value>,
}

impl PendingRequest {
    /// `deadline`まで応答を待つ。応答の`id`は、クライアントの`id`に戻してある
    async fn response(
        self,
        deadline: tokio::time::Instant,
    ) -> std::result::Result<Value, ChannelError> {
        let PendingRequest {
            registration: _registration,
            response_rx,
        } = self;
        match tokio::time::timeout_at(deadline, response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(ChannelError::NoResponse),
            Err(_) => Err(ChannelError::TimedOut),
        }
    }
}

/// 起動したサーバー（#760）
#[derive(Debug)]
struct RunningServer {
    /// `true`を送ると、待ち受けをやめ、接続を閉じる
    shutdown: tokio::sync::watch::Sender<bool>,
    /// 待ち受けるタスクが終わると`true`になる（パニックで終わると、送る側が
    /// 無くなる）。止めるのを待つ`stop_server()`が、途中で捨てられて
    /// 呼び直されても、同じタスクを待てるようにする（PR #771のレビュー）
    finished: tokio::sync::watch::Receiver<bool>,
    /// 待ち受けるタスク。接続のタスクを終えてから終わる
    task: tokio::task::JoinHandle<()>,
}

/// HTTP Transport implementation
#[derive(Debug)]
pub struct HttpTransport {
    config: HttpConfig,
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<QueuedRequest>>>,
    channel: RequestChannel,
    stats: SharedStats,
    /// `start_server()`で実際に待ち受けたアドレス。同期の`transport_info()`
    /// からも待たずに読めるよう、async版ではなく`std::sync::RwLock`を使う
    /// （ロックを持ったまま`.await`しない）（#265）
    bound_addr: Arc<std::sync::RwLock<Option<SocketAddr>>>,
    request_handler: RequestHandlerSlot,
    /// 認証の失敗等を記録する監査ログ（#55）。既定はメモリにだけ残すもの
    audit_logger: Arc<AuditLogger>,
    /// 起動・停止を1つずつにする（#760）
    lifecycle: Arc<tokio::sync::Mutex<()>>,
    /// 動いているサーバー（#760）。同期の`is_connected()`からも読むため、
    /// `std::sync::Mutex`を使う（ロックを持ったまま`.await`しない）
    server: Arc<std::sync::Mutex<Option<RunningServer>>>,
}

impl HttpTransport {
    /// 認証の失敗・リプレイ攻撃の検出等を記録する監査ログを設定する（#55）。
    /// 本体は、ツール呼び出しと同じ監査ログ（設定の`[audit]`）を渡す
    pub fn with_audit_logger(mut self, audit_logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = audit_logger;
        self
    }

    /// `with_request_handler`を使わないときに、HTTP・WebSocketで受けた要求を、
    /// その呼び出し元と、`MCP-Protocol-Version`ヘッダーで選んだ版と一緒に
    /// 返す（#760）。チャンネルが閉じたら`None`。
    ///
    /// 要求は、`RequestDispatcher::handle_message_with_protocol_version`に、
    /// 返った`caller`と`protocol_version`を渡して処理する。要求の`id`は
    /// 内部の相関IDで、応答の`id`にそのまま入れて`send_message`で返す
    pub async fn receive_request_with_context(
        &self,
    ) -> std::result::Result<Option<ReceivedRequest>, TransportError> {
        let mut receiver = self.receiver.lock().await;

        // もう誰も応答を待っていない要求（期限切れ・クライアントが切った）は
        // 渡さずに、次を待つ（PR #771のレビュー）
        let next = loop {
            match receiver.recv().await {
                Some(queued) if queued.is_abandoned() => {
                    debug!("Skipped a request that is no longer waited for (it timed out or was cancelled)");
                }
                next => break next,
            }
        };
        match next {
            Some(queued) => {
                debug!("HTTP transport received message: {}", queued.message);
                let request: JsonRpcRequest =
                    serde_json::from_str(&queued.message).map_err(|e| {
                        TransportError::Internal(format!("Failed to parse request: {}", e))
                    })?;
                Ok(Some(ReceivedRequest {
                    request,
                    caller: queued.caller,
                    protocol_version: queued.protocol_version,
                }))
            }
            None => Ok(None), // Channel closed, no more messages
        }
    }

    /// `with_request_handler`を使わないときに、HTTP・WebSocketで受けた要求を、
    /// その呼び出し元（APIキー・トークンの名前と使えるツール、接続元の
    /// IPアドレス）と一緒に返す（#749）。チャンネルが閉じたら`None`。
    ///
    /// 要求は、`RequestDispatcher::handle_message_as`に、返った`Caller`を
    /// 渡して処理する。認証していない呼び出し元は、名前が無く、IPアドレスの
    /// ある`Caller`になり、`mcp_rs_status`はバージョンだけを返す。
    /// `Transport::receive_message`は、呼び出し元を捨てて要求だけを返す。
    /// `MCP-Protocol-Version`ヘッダーで選んだ版も要るなら、
    /// `receive_request_with_context`を使う（#760）
    pub async fn receive_request_with_caller(
        &self,
    ) -> std::result::Result<Option<(JsonRpcRequest, crate::mcp::Caller)>, TransportError> {
        Ok(self
            .receive_request_with_context()
            .await?
            .map(|received| (received.request, received.caller)))
    }

    /// 受け付けたJSON-RPCメッセージを直接処理するハンドラーを設定する
    /// （#400）。`start_server()`の前に呼ぶ。
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
            channel: RequestChannel {
                sender,
                pending: PendingResponses::default(),
                next_correlation_id: Arc::new(AtomicU64::new(1)),
            },
            stats: SharedStats::new(stats),
            bound_addr: Arc::new(std::sync::RwLock::new(None)),
            request_handler: RequestHandlerSlot::default(),
            audit_logger: Arc::new(AuditLogger::with_defaults()),
            lifecycle: Arc::new(tokio::sync::Mutex::new(())),
            server: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Start the HTTP server
    ///
    /// 動いている間に呼ぶと、待ち受けるアドレスを変えずにエラーを返す
    /// （#760）。以前は、`bound_addr()`が消えるか、別のリスナーのアドレスで
    /// 上書きされた。`stop_server()`の後は、また起動できる
    pub async fn start_server(&self) -> Result<()> {
        // 起動・停止を1つずつにする（#760）
        let _lifecycle = self.lifecycle.lock().await;
        if self.is_running() {
            return Err(Error::TransportError(TransportError::Internal(format!(
                "HTTP server is already running on {}",
                self.current_bound_addr()
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| self.config.bind_addr.to_string())
            ))));
        }
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
        // 上限は`Semaphore`の許す数まで（#724。超えると、起動の後にパニック
        // して、何も受け付けなかった）
        let max_permits = tokio::sync::Semaphore::MAX_PERMITS;
        if self.config.max_connections == 0 || self.config.max_connections > max_permits {
            return Err(Error::TransportError(TransportError::Configuration(
                format!("max_connections must be between 1 and {max_permits}"),
            )));
        }

        // WebSocketを使わなくても、`Semaphore`は作るので、上は確かめる
        if self.config.websocket_max_connections > max_permits
            || (self.config.enable_websocket_upgrade && self.config.websocket_max_connections == 0)
        {
            return Err(Error::TransportError(TransportError::Configuration(format!(
                "websocket_max_connections must be between 1 and {max_permits} when enable_websocket_upgrade is enabled"
            ))));
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

            // 短いキーは、当て推量で当たりやすい（#596）。起動は止めずに警告する
            for entry in &self.config.api_keys {
                if entry.key.chars().count() < MIN_RECOMMENDED_API_KEY_CHARS {
                    warn!(
                        "API key '{}' is shorter than {} characters, so it is easier to guess. Use a long random key, for example: openssl rand -hex 32",
                        entry.name, MIN_RECOMMENDED_API_KEY_CHARS
                    );
                }
            }

            // Two entries sharing a `key` would otherwise import silently
            // (the later one overwriting the earlier one in the manager's
            // hash-keyed storage), misattributing that key's usage to
            // whichever entry happened to be imported last.
            let mut seen_keys = HashSet::new();
            // 名前も重ねない（#724）。名前は呼び出し元を区別する値になり
            // （監査ログ・リプレイ攻撃対策・IDS）、重なると別のキーの記録が
            // 混ざる
            let mut seen_names = HashSet::new();
            for entry in &self.config.api_keys {
                if !seen_keys.insert(&entry.key) {
                    return Err(Error::TransportError(TransportError::Configuration(
                        format!("api_keys contains a duplicate key (entry '{}')", entry.name),
                    )));
                }
                if !seen_names.insert(&entry.name) {
                    return Err(Error::TransportError(TransportError::Configuration(
                        format!(
                            "api_keys contains two entries named '{}'. Give each key its own name",
                            entry.name
                        ),
                    )));
                }
            }
        }

        let mut api_key_callers = HashMap::new();
        let api_key_manager = if self.config.auth_enabled {
            let mut manager = ApiKeyManager::new();
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
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let state = HttpTransportState {
            channel: self.channel.clone(),
            stats: self.stats.clone(),
            shutdown: shutdown_rx.clone(),
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
            trusted_proxies: parse_trusted_proxies(&self.config.trusted_proxies)
                .map_err(|e| Error::TransportError(TransportError::Configuration(e)))?
                .into(),
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
                channel: self.channel.clone(),
                stats: self.stats.clone(),
            }),
            websocket_max_message_size: self.config.max_request_size,
            websocket_idle_timeout: (self.config.websocket_idle_timeout_secs > 0)
                .then(|| std::time::Duration::from_secs(self.config.websocket_idle_timeout_secs)),
            auth_failure_audit: Arc::new(AuthFailureAuditLimiter::new()),
            // 1分に20回失敗したアドレスは、その1分の残りを断る（#596）
            auth_failures: Arc::new(AuthFailureLimiter::new(
                20,
                std::time::Duration::from_secs(60),
                10_000,
            )),
        };
        // IDSの古い記録を、5分ごとに消す（#543）。以前は、掃除の関数が
        // あっても、どこからも呼ばれていなかった。サーバーが無くなったら
        // （IDSを持つ`Weak`が切れたら）終わる
        if let Some(ids) = &state.ids {
            let ids = Arc::downgrade(ids);
            tokio::spawn(async move {
                let period = std::time::Duration::from_secs(5 * 60);
                let mut tick =
                    tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                loop {
                    tick.tick().await;
                    let Some(ids) = ids.upgrade() else {
                        return;
                    };
                    ids.cleanup().await;
                }
            });
        }
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
            .layer(DefaultBodyLimit::max(self.config.max_request_size))
            .layer(RequestBodyTimeoutLayer::new(request_timeout))
            .layer(mcp_cors_layer(
                self.config.cors_enabled,
                origin_policy.clone(),
            ))
            // `Origin`の検証は、CORSより外側（#741）。CORSは`OPTIONS`の
            // preflightに内側を呼ばずに答えるので、内側では、許可していない
            // オリジンのpreflightにも200を返していた
            .layer(axum::middleware::from_fn_with_state(
                origin_policy,
                origin_middleware,
            ));
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
        let (finished_tx, finished) = tokio::sync::watch::channel(false);
        let task = tokio::spawn(async move {
            serve_connections(listener, app, tls_acceptor, limits, shutdown_rx).await;
            let _ = finished_tx.send(true);
        });
        *self.lock_server() = Some(RunningServer {
            shutdown: shutdown_tx,
            finished,
            task,
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

    /// サーバーを止める（#760）。待ち受けをやめ、HTTPの接続を閉じ、
    /// `/ws`の接続にも閉じるよう伝える。待ち受けのタスクが終わってから返す。
    /// 動いていなければ何もしない。以前の`Transport::stop()`は何もせず、
    /// 新しい接続を受け付け続けた。`HttpTransport`を捨てるだけでは止まらない
    ///
    /// 止まるまで、動いているサーバーの記録を消さない（PR #771のレビュー）。
    /// 待つ途中でこのfutureが捨てられても、`is_connected()`は止まるまで
    /// `true`のままで、呼び直せば同じタスクを待つ
    pub async fn stop_server(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        let finished = self.lock_server().as_ref().map(|running| {
            let _ = running.shutdown.send(true);
            running.finished.clone()
        });
        if let Some(mut finished) = finished {
            // パニックで終わったとき（送る側が無くなった）も、終わったとする。
            // `finished`は、リスナーを閉じ、接続のタスクを終えた後に`true`に
            // なるので、ここで記録を消してよい。`JoinHandle`は待たない（待つ
            // 途中で捨てられると、終わる前に記録が消えるため、PR #771の
            // レビュー）。記録を消すまでに`.await`は無い
            if finished.wait_for(|finished| *finished).await.is_err() {
                warn!("HTTP server task ended unexpectedly");
            }
            self.lock_server().take();
        }
        self.set_bound_addr(None);
    }

    fn lock_server(&self) -> std::sync::MutexGuard<'_, Option<RunningServer>> {
        // 値は`Option`だけで、書き込み中にパニックしても壊れた状態には
        // ならないため、ポイズンは無視する
        self.server
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 待ち受けるタスクが動いているか（#760）
    fn is_running(&self) -> bool {
        self.lock_server()
            .as_ref()
            .is_some_and(|running| !running.task.is_finished())
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
/// で明示して許したときは、警告だけ出す。
///
/// `trusted_proxies`を書いたとき（リバースプロキシの後ろ）は、ループバックで
/// 待ち受けていても同じにする（#593）。同じマシンのプロキシから来る通信は、
/// すべてループバックからに見えるので、待ち受けるアドレス・ネットワークの
/// 方針では、プロキシの先（インターネット等）の相手を止められない。
///
/// プロキシの後ろでは、mTLSを認証に数えない（PR #601のレビュー）。TLSを
/// 終えるのはプロキシなので、mcp-rsに証明書を出すのはプロキシで、確かめ
/// られるのは「つないできたのがプロキシか」だけ。プロキシの先の
/// クライアントは確かめられない
fn unauthenticated_external_bind(config: &HttpConfig) -> Option<String> {
    let behind_proxy = !config.trusted_proxies.is_empty();
    let authenticated =
        config.auth_enabled || config.oauth.is_some() || (config.mtls_enabled && !behind_proxy);
    if authenticated || (config.bind_addr.ip().is_loopback() && !behind_proxy) {
        return None;
    }
    let where_from = if behind_proxy {
        format!(
            "{} behind a reverse proxy (trusted_proxies is set)",
            config.bind_addr
        )
    } else {
        config.bind_addr.to_string()
    };
    if config.allow_unauthenticated_external {
        warn!(
            "⚠️  Listening on {} without authentication (allow_unauthenticated_external = true): \
             anyone who can reach it can use every tool, unless something in front of mcp-rs \
             authenticates clients",
            where_from
        );
        return None;
    }
    let proxy_note = if behind_proxy {
        " Requests through a proxy on the same machine come from loopback, so listening on \
         loopback does not keep out the clients the proxy lets in. Behind a proxy, client \
         certificates (mTLS) check only the proxy, not its clients, so they do not count."
    } else {
        ""
    };
    Some(format!(
        "refusing to listen on {} without authentication: anyone who can reach it could use \
         every tool.{} Configure API keys (auth_enabled = true and [[transport.http.api_keys]]) or \
         OAuth ([transport.http.oauth]). Only when something in front of mcp-rs authenticates \
         clients, set allow_unauthenticated_external = true",
        where_from, proxy_note
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
/// - `shutdown`が`true`になったら、待ち受けをやめ（リスナーを閉じ）、
///   接続のタスクを終えてから返す（#760）
async fn serve_connections(
    listener: TcpListener,
    app: Router,
    tls_acceptor: Option<TlsAcceptor>,
    limits: ConnectionLimits,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut make_service = app.into_make_service_with_connect_info::<SocketAddr>();
    let connections = Arc::new(tokio::sync::Semaphore::new(limits.max_connections));
    let mut tasks = tokio::task::JoinSet::new();

    loop {
        // 終わった接続のタスクを片付ける
        while tasks.try_join_next().is_some() {}
        let accepted = tokio::select! {
            biased;
            _ = wait_for_shutdown(&mut shutdown) => None,
            accepted = async {
                let permit = connections.clone().acquire_owned().await.ok()?;
                Some((permit, listener.accept().await))
            } => accepted,
        };
        let Some((permit, accepted)) = accepted else {
            break;
        };
        let (tcp_stream, remote_addr) = match accepted {
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

        tasks.spawn(async move {
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

    // 待ち受けをやめ（ポートを放し）、接続のタスクを終える（#760）。
    // `/ws`の接続は、`HttpTransportState::shutdown`で閉じる
    drop(listener);
    tasks.shutdown().await;
}

/// `stop_server()`が呼ばれるまで待つ（#760）。`HttpTransport`が捨てられた
/// だけ（送る側が無くなった）なら、終わらない（これまでどおり動き続ける）
async fn wait_for_shutdown(shutdown: &mut tokio::sync::watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow_and_update() {
            return;
        }
        if shutdown.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// 1つの接続の、処理中のリクエストの数と、最後に変わった時刻（#592）
struct ConnectionActivity {
    in_flight: std::sync::atomic::AtomicUsize,
    last_change: std::sync::Mutex<std::time::Instant>,
    /// 送っている応答ごとの、送信が次に進むべき期限（#759）。キーは
    /// `ActivityGuard`の番号
    send_deadlines: std::sync::Mutex<HashMap<u64, std::time::Instant>>,
    next_guard_id: AtomicU64,
}

impl ConnectionActivity {
    fn new() -> Self {
        Self {
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            last_change: std::sync::Mutex::new(std::time::Instant::now()),
            send_deadlines: std::sync::Mutex::new(HashMap::new()),
            next_guard_id: AtomicU64::new(0),
        }
    }

    fn send_deadlines(&self) -> std::sync::MutexGuard<'_, HashMap<u64, std::time::Instant>> {
        self.send_deadlines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 送っている応答のうち、いちばん早い送信の期限（#759）。送っている
    /// 応答が無ければ`None`
    fn send_deadline(&self) -> Option<std::time::Instant> {
        self.send_deadlines().values().min().copied()
    }

    fn touch(&self) {
        *self
            .last_change
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = std::time::Instant::now();
    }

    /// 処理中のリクエストが無い時間。処理中なら0
    fn idle_for(&self) -> std::time::Duration {
        if self.in_flight.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            return std::time::Duration::ZERO;
        }
        self.last_change
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .elapsed()
    }
}

/// リクエストを受けてから、応答の本文を送り終える（または捨てる）までの
/// 間、処理中として数える（#592）
struct ActivityGuard {
    activity: Arc<ConnectionActivity>,
    id: u64,
}

impl ActivityGuard {
    fn new(activity: Arc<ConnectionActivity>) -> Self {
        activity
            .in_flight
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        activity.touch();
        let id = activity.next_guard_id.fetch_add(1, Ordering::Relaxed);
        Self { activity, id }
    }

    /// この応答の送信が次に進むべき期限を決める。`None`なら外す（#759）
    fn set_send_deadline(&self, deadline: Option<std::time::Instant>) {
        let mut deadlines = self.activity.send_deadlines();
        match deadline {
            Some(deadline) => {
                deadlines.insert(self.id, deadline);
            }
            None => {
                deadlines.remove(&self.id);
            }
        }
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.set_send_deadline(None);
        self.activity.touch();
        self.activity
            .in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 応答の本文を包み、送信が進んでいるかを見る（#759）。
///
/// HTTP/2では、相手が応答を読まず、送信のウィンドウが空かないと、hyperは
/// 本文を読み出さないまま待ち続ける。その間は本文（と`ActivityGuard`）が
/// 残るので、処理中として数え、何もしていない接続として閉じることもない。
/// pingは送信のウィンドウに縛られないので、keep-aliveでも見つからない。
///
/// そこで、本文のある応答を作った時点で、送信の期限（`timeout`後）を決め、
/// 本文の各フレームを渡すたびに、その大きさに合わせて延ばす（`/ws`の応答と
/// 同じ最低の速さ）。本文を送り終えるか捨てれば外す。期限を過ぎた接続は、
/// `serve_connection`が閉じる。
///
/// 最後のデータを送れるようになるまで、期限を外さない（PR #765の
/// レビュー）。hyper（1.8）は、フレームを渡すたびに、次を読む前に送信の
/// 余地が1バイト以上空くのを待つ。h2は渡されたデータを相手のウィンドウが
/// 空くまで溜めておくので、余地が空くのは、溜めたデータとさらに1バイトを
/// 送れるようになってから。そこで、長さの決まった本文（`Full`等。終わりが
/// 前もって分かる）の最後のデータは、最後の1バイトを残して渡し、余地が
/// 空いて次に読まれたときに、その1バイトを終わり（END_STREAM）と一緒に
/// 渡す。その時点で、それまでのデータと最後の1バイトは、どれも相手の
/// ウィンドウに収まっている。本文はそこで捨てられ、期限も外れる。
///
/// 以前（1回目）は、最後のフレームを渡した時点で終わりを知らせたので、
/// データがh2に溜まったままでも期限が外れた。2回目は、最後のフレームの後も
/// 終わりを知らせず、空のDATAフレーム（END_STREAM）を次に送ったが、それにも
/// 1バイトの余地を待つので、ウィンドウを本文の長さちょうどにして、
/// END_STREAMまでWINDOW_UPDATEを送らない相手（RFC 9113で許される）には、
/// 終わりを送れずに期限で切っていた。1バイトを残せば、その相手にも最後まで
/// 送れる（最後の1バイトの分の余地は、ウィンドウに残っている）。
///
/// 限界:
/// - 終わりが前もって分からない本文（ストリーム等）は、最後のフレームの後、
///   もう一度読まれて終わりを返すまで期限を外さない。その終わりにも1バイトの
///   余地を待つので、上の相手には送り切れない。mcp-rsの応答は、どれも
///   長さの決まった本文
/// - 「送れる」はh2の流量制御の上でのこと。ソケットへの書き込み自体が
///   止まった相手（TCPで読まない相手）は、ここでは見ない。その場合は
///   keep-aliveのpingも送れず、`keep_alive_timeout`で接続が閉じる
/// - HTTP/1.1では、hyper（1.8）は、書き込みのバッファが埋まっている間は
///   本文を読まないので、そこまでは期限が働く。ただし、長さの決まった本文
///   （`Content-Length`）は、最後のデータを書き込みのバッファに入れた時点で、
///   もう一度読まずに捨てる（PR #765のレビュー）。データがソケットに出る前に
///   期限が外れ、処理中のリクエストも無くなる。相手がその後も読まなければ、
///   何もしていない接続として、`timeout`の後に閉じる手続きを始め、さらに
///   `timeout`の後に切る。相手がリクエストを送り続けると、本文のある応答は
///   書き込みのバッファに入らずに期限で閉じるが、本文の無い応答（404・202
///   等）だけを返すリクエストを送り続ける相手は、閉じられないことがある。
///   hyperの外から、バッファのデータがソケットに出たかを知る方法が無い
struct SendProgressBody {
    inner: axum::body::Body,
    guard: ActivityGuard,
    timeout: std::time::Duration,
    /// データのフレームを1つ以上渡した
    sent_data: bool,
    /// 最後のデータの、まだ渡していない最後の1バイト
    tail: Option<axum::body::Bytes>,
    /// 本文の最後まで渡した（次は終わりを返す）
    finished: bool,
}

impl SendProgressBody {
    fn new(inner: axum::body::Body, guard: ActivityGuard, timeout: std::time::Duration) -> Self {
        use hyper::body::Body as _;
        // 本文の無い応答（101・204等）はヘッダーだけで、ウィンドウを待たない
        if !inner.is_end_stream() {
            guard.set_send_deadline(Some(std::time::Instant::now() + timeout));
        }
        Self {
            inner,
            guard,
            timeout,
            sent_data: false,
            tail: None,
            finished: false,
        }
    }

    /// `len`バイトを渡したので、送信の期限を延ばす
    fn extend_deadline(&self, len: usize) {
        let limit =
            websocket_response_send_timeout(len, self.timeout, WEBSOCKET_MIN_SEND_BYTES_PER_SEC);
        self.guard
            .set_send_deadline(Some(std::time::Instant::now() + limit));
    }
}

impl hyper::body::Body for SendProgressBody {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<std::result::Result<hyper::body::Frame<Self::Data>, Self::Error>>>
    {
        use std::task::Poll;
        let this = &mut *self;
        if let Some(tail) = this.tail.take() {
            this.extend_deadline(tail.len());
            this.finished = true;
            return Poll::Ready(Some(Ok(hyper::body::Frame::data(tail))));
        }
        if this.finished {
            this.guard.set_send_deadline(None);
            return Poll::Ready(None);
        }
        match std::pin::Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                let frame = match frame.into_data() {
                    Ok(mut data) => {
                        this.sent_data = true;
                        if this.inner.is_end_stream() {
                            if data.len() > 1 {
                                // 最後の1バイトは、余地が空いてから渡す
                                this.tail = Some(data.split_off(data.len() - 1));
                            } else {
                                this.finished = true;
                            }
                        }
                        this.extend_deadline(data.len());
                        hyper::body::Frame::data(data)
                    }
                    Err(frame) => {
                        this.extend_deadline(0);
                        frame
                    }
                };
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(other) => {
                this.guard.set_send_deadline(None);
                this.finished = true;
                Poll::Ready(other)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        if self.tail.is_some() {
            return false;
        }
        if self.finished {
            return true;
        }
        // データを渡した後、終わりが分からない本文は、もう一度読まれて
        // 終わりを返すまで`false`
        !self.sent_data && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        // 残してある最後の1バイトも、まだ渡していない分に数える（PR #765の
        // レビュー）
        let inner = self.inner.size_hint();
        let held = self.tail.as_ref().map_or(0, |tail| tail.len() as u64);
        let mut hint = hyper::body::SizeHint::new();
        hint.set_lower(inner.lower() + held);
        if let Some(upper) = inner.upper() {
            hint.set_upper(upper + held);
        }
        hint
    }
}

/// 1つの接続で、HTTP/1.1かHTTP/2のリクエストを処理する。ヘッダーを読む
/// 時間（HTTP/1.1）と、応答の無い接続を見つけるping（HTTP/2）に`timeout`を
/// 使う（#540）。
///
/// 処理中のリクエストが無いまま`timeout`が過ぎた接続は、閉じる（#592）。
/// HTTP/2には、HTTP/1.1の`header_read_timeout`にあたる上限が無く、pingに
/// 答える相手は、何も送らなくても接続を持ち続けられた。接続を開いて何も
/// 送らない相手で、`max_connections`を認証無しに埋められた。
///
/// 応答の送信が期限までに進まない接続も、閉じる（`SendProgressBody`、
/// #759）。応答を読まない相手は、処理中のリクエストがあるように見えるので、
/// 上の上限が働かなかった
async fn serve_connection<I>(
    io: I,
    service: ConnectionService,
    remote_addr: SocketAddr,
    timeout: std::time::Duration,
) where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use hyper::service::Service as _;

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
    let activity = Arc::new(ConnectionActivity::new());
    let inner = TowerToHyperService::new(service);
    let tracked = activity.clone();
    let hyper_service = hyper::service::service_fn(move |request| {
        let guard = ActivityGuard::new(tracked.clone());
        let response = inner.call(request);
        async move {
            let response = response.await?;
            // 本文を送り終えるか捨てるまで、処理中として数える。送信が
            // 進まなければ、接続を閉じる（#759）
            Ok::<_, std::convert::Infallible>(
                response
                    .map(|body| axum::body::Body::new(SendProgressBody::new(body, guard, timeout))),
            )
        }
    });
    let connection = builder.serve_connection_with_upgrades(io, hyper_service);
    tokio::pin!(connection);
    let mut closing = false;
    loop {
        let wait = timeout.saturating_sub(activity.idle_for());
        let send_deadline = activity.send_deadline();
        tokio::select! {
            result = connection.as_mut() => {
                if let Err(e) = result {
                    // 時間切れ・相手が切った等。よくあることなので、エラーにはしない
                    debug!("HTTP connection error for {}: {}", remote_addr, e);
                }
                return;
            }
            _ = sleep_until_or_forever(send_deadline.map(tokio::time::Instant::from_std)) => {
                if activity
                    .send_deadline()
                    .is_some_and(|deadline| deadline <= std::time::Instant::now())
                {
                    // 相手が応答を読まない。接続ごと捨てる（本文と
                    // `ActivityGuard`も放され、接続の枠が返る）
                    warn!("Closing connection from {}: a response was not sent within the time limit (the client is not reading it)", remote_addr);
                    return;
                }
                // 送信が進み、期限が延びた
            }
            _ = tokio::time::sleep(wait.max(std::time::Duration::from_millis(10))) => {
                if activity.idle_for() < timeout {
                    // 処理中のリクエストがあるか、最近まであった。閉じる手続きの
                    // 開始とほぼ同時に届いて受け付けたリクエストも、最後まで
                    // 処理させる（PR #605のレビュー）
                    continue;
                }
                if closing {
                    // 閉じる手続きに応じない相手は、そのまま切る
                    debug!("Closed idle connection from {} without waiting for it", remote_addr);
                    return;
                }
                debug!("Closing idle connection from {} (no request for {:?})", remote_addr, timeout);
                connection.as_mut().graceful_shutdown();
                closing = true;
                // 閉じる手続きに応じるのを、さらに`timeout`まで待つ
                activity.touch();
            }
        }
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
        self.start_server().await.map_err(TransportError::from)?;
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        debug!("Stopping HTTP transport server");
        self.stop_server().await;
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

        // 待っている要求への応答なら、内部の相関IDで照合し、クライアントの
        // `id`に戻して返す（#760）
        // （クライアントの`id`は`RequestId`で表せない値（小数等）のことも
        // あるので、JSONの値で戻す）
        let correlation_id = match &message.id {
            crate::types::RequestId::String(id) => Some(id.as_str()),
            _ => None,
        };
        let pending = correlation_id.and_then(|id| self.channel.pending.lock().remove(id));
        if let Some(pending) = pending {
            let mut response = serde_json::to_value(&message).map_err(|e| {
                TransportError::Internal(format!("Failed to serialize response: {}", e))
            })?;
            if let Some(object) = response.as_object_mut() {
                object.insert("id".to_string(), pending.client_id);
            }
            if pending.response_tx.send(response).is_err() {
                error!(
                    "Failed to send response - receiver dropped for ID: {:?}",
                    correlation_id
                );
            }
            return Ok(());
        }
        if correlation_id.is_some_and(|id| id.starts_with(CORRELATION_ID_PREFIX)) {
            // 内部の相関IDの応答なのに待ち先が無い（期限を過ぎた・HTTPの
            // 要求が捨てられた）。捨てる（PR #771のレビュー）。以前は下の
            // 通知の道で要求のキューに入り、キューが埋まっていると、受信と
            // 送信を1つのタスクで回す受信側が止まり、`receive_request_*`は
            // （`method`が無いので）読めないエラーを返した
            warn!(
                "Dropped a response for {:?}: the request is no longer waiting (it timed out or was cancelled)",
                correlation_id
            );
            return Ok(());
        }

        // Fallback: send through regular channel for notifications
        // （このプロセス自身が送るものなので、制限の無い呼び出し元とする）
        self.channel
            .sender
            .send(QueuedRequest {
                message: message_str,
                caller: crate::mcp::Caller::unrestricted(),
                protocol_version: None,
                waiter: None,
            })
            .await
            .map_err(|e| TransportError::Internal(format!("HTTP send failed: {}", e)))?;

        Ok(())
    }

    /// `with_request_handler`を使わないときに、HTTP・WebSocketで受けた要求を返す。
    ///
    /// 返す要求には、呼び出し元の情報（APIキー・トークンの名前、使えるツール、
    /// 接続元のIPアドレス）が無い（#746）。呼び出し元の情報が要るなら、
    /// `HttpTransport::receive_request_with_caller`を使い、返った`Caller`で
    /// `RequestDispatcher::handle_message_as`を呼ぶ（#749）。
    /// `handle_message`（制限の無い呼び出し元）を使うと、`mcp_rs_status`は
    /// ファイルのパス等を返し、ツールの制限も効かない
    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        Ok(self
            .receive_request_with_caller()
            .await?
            .map(|(request, _caller)| request))
    }

    /// サーバーが待ち受けているか（#760）。以前は、起動の前や、起動に
    /// 失敗した後も`true`だった
    fn is_connected(&self) -> bool {
        self.is_running()
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
        // 短い同期のロックで写しを取る（#760）。Tokioのタスクから呼んでも
        // パニックしない
        let stats = self.stats.lock().clone();

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
    request: std::result::Result<Json<Value>, JsonRejection>,
) -> std::result::Result<Response, StatusCode> {
    // ネットワークの制限と認証は、`auth_middleware`が本文を読む前に確かめた
    let expires_at = authenticated.expires_at;
    let caller = authenticated
        .caller
        .with_ip_address(Some(remote_addr.ip().to_string()));
    let request = match request {
        Ok(Json(request)) => request,
        // JSONとして読めない本文には、JSON-RPCの`-32700`を返す（#543）。
        // 以前はaxumのテキストの400だった。Content-Typeの誤り（415）・
        // 大きすぎる本文（413）等は、これまでどおり
        Err(JsonRejection::JsonSyntaxError(_)) => {
            state.stats.lock().total_errors += 1;
            return Ok(jsonrpc_error_response(-32700, "Parse error"));
        }
        Err(rejection) => return Err(rejection.status()),
    };
    // 本文を読んでいる間に、OAuthのトークンの期限が切れていれば、処理しない
    // （#724。認証は本文を読む前に確かめるので、ゆっくり送ると、期限の後に
    // 処理を始められた）。この後の確認（リプレイ攻撃対策・IDS等）で待つ間に
    // 切れることもあるので、振り分ける直前にも確かめる（PR #726のレビュー）
    if let Some(response) = expired_token_response(&state, expires_at) {
        return Ok(response);
    }
    // プロキシの付けるヘッダーは、信頼するプロキシからのものだけを使う（#543）
    let no_headers = HeaderMap::new();
    let forwarded = forwarded_headers(&state, remote_addr, &headers, &no_headers);
    check_https_enforcement(
        state.enforce_https,
        state.tls_terminated_locally,
        forwarded,
        &state.trusted_proxies,
        remote_addr,
    )?;
    check_min_tls_version(
        state.min_tls_version.as_deref(),
        state.enforce_https,
        state.tls_terminated_locally,
        forwarded,
        remote_addr,
    )?;
    check_certificate_pin(
        state.certificate_pinning_enabled,
        &state.certificate_pin_header,
        &state.pinned_certificates_sha256,
        forwarded,
        remote_addr,
    )?;
    check_anti_replay_handshake(
        state.anti_replay.as_ref(),
        &state.audit_logger,
        &headers,
        remote_addr,
        &caller,
        anti_replay_client_ip(&state, remote_addr, &headers),
    )
    .await?;
    check_ids(
        state.ids.as_ref(),
        remote_addr,
        ids_trusted_proxies(&state),
        &caller,
        &uri,
        &headers,
        Some(&request),
    )
    .await?;
    // 2026-07-28のリクエストは、版を本文の`_meta`で決める（#570）
    let modern = match is_modern_request(&headers, &request) {
        Ok(modern) => modern,
        Err(response) => return Ok(*response),
    };
    let protocol_version = if modern {
        if let Err(response) = check_modern_protocol_version(&headers, &request) {
            return Ok(*response);
        }
        None
    } else {
        match requested_protocol_version(&headers, &request) {
            Ok(version) => version,
            Err(response) => return Ok(*response),
        }
    };

    let start_time = Instant::now();

    debug!(
        "Received HTTP JSON-RPC request from {}: {}",
        remote_addr, request
    );

    // Update statistics - request received
    {
        let request_size = serde_json::to_string(&request).unwrap_or_default().len() as u64;
        let mut stats = state.stats.lock();
        stats.total_requests += 1;
        stats.total_bytes_received += request_size;
    }

    // Convert request to string and send through channel
    // `blocking_write()` must not be used here: this is an async handler on
    // a tokio worker, where tokio's blocking lock methods panic (#374).
    let request_str = match serde_json::to_string(&request) {
        Ok(request_str) => request_str,
        Err(_) => {
            state.stats.lock().total_errors += 1;
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // 振り分ける直前に、もう一度期限を確かめる（PR #726のレビュー）。ここより
    // 後は、受け付けたリクエストとして最後まで処理する（ツールの実行中に
    // 期限が切れても、途中で止めない）
    if let Some(response) = expired_token_response(&state, expires_at) {
        return Ok(response);
    }

    if let Some(handler) = state.request_handler.clone() {
        return respond_with_request_handler(
            &state,
            handler.as_ref(),
            &request_str,
            &caller,
            protocol_version,
            modern,
            start_time,
        )
        .await;
    }

    // `id`は文字列か数値（PR #771のレビュー）。内部の相関IDに置き換える
    // 前に確かめる。以前は`id: true`等も通し、応答にそのまま戻していた
    if !channel_request_id_is_valid(&request) {
        state.stats.lock().total_errors += 1;
        return Ok(jsonrpc_error_response(
            -32600,
            "Invalid Request: id must be a string or a number",
        ));
    }

    // チャンネル経由（#760）: キューの枠は期限付きで取る。期限には、枠を
    // 待つ時間も含める（応答を待つのは、その残り）
    let deadline = tokio::time::Instant::now() + CHANNEL_RESPONSE_TIMEOUT;
    let permit = match state.channel.reserve(deadline).await {
        Ok(permit) => permit,
        Err(e) => {
            warn!(
                "Could not forward a JSON-RPC request to the channel: {:?}",
                e
            );
            state.stats.lock().total_errors += 1;
            return Err(e.status());
        }
    };
    // 枠を待つ間に期限が切れていれば、渡さずに断る（PR #726のレビュー）
    if let Some(response) = expired_token_response(&state, expires_at) {
        return Ok(response);
    }

    if request.get("id").is_none() {
        // Notification (no ID)
        RequestChannel::forward_notification(permit, request_str, caller, protocol_version);

        // JSON-RPC 2.0: the server MUST NOT reply to a notification. MCP's
        // HTTP transport acknowledges an accepted notification with
        // `202 Accepted` and no body (#374) - previously this returned a
        // `{"jsonrpc":"2.0","result":{"status":"accepted",...}}` body with no
        // `id`, which is not a valid JSON-RPC response either.
        state.stats.lock().total_responses += 1;

        return Ok((StatusCode::ACCEPTED, response_headers(&state)).into_response());
    }

    // 内部の相関IDで応答を待つ（#760）。この future が途中で捨てられても
    // （クライアントが切った等）、待ち先の登録は`PendingRegistration`が消す
    let pending =
        state
            .channel
            .forward_request(permit, request, caller, protocol_version, deadline);
    match pending.response(deadline).await {
        Ok(response_value) => {
            let response_size = response_value.to_string().len() as u64;
            {
                let mut stats = state.stats.lock();
                stats.total_responses += 1;
                stats.total_response_time_ms += start_time.elapsed().as_millis() as u64;
                stats.total_bytes_sent += response_size;
            }
            // 直接処理（`respond_with_request_handler`）と同じHTTPの
            // ステータスにする（#760。以前は常に200）
            let status = if is_error_without_id(&response_value) {
                StatusCode::BAD_REQUEST
            } else if modern {
                modern_response_status(&response_value)
            } else {
                StatusCode::OK
            };
            Ok((status, response_headers(&state), Json(response_value)).into_response())
        }
        Err(e) => {
            warn!(
                "No response from the channel for a JSON-RPC request: {:?}",
                e
            );
            state.stats.lock().total_errors += 1;
            Err(e.status())
        }
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
        audit_auth_failure(state, remote_addr, reason).await;
        return Err(Box::new(StatusCode::UNAUTHORIZED.into_response()));
    };

    let Some(token) = extract_bearer_token(headers) else {
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
                    audit_auth_failure(
                        state,
                        remote_addr,
                        &format!("invalid access token: {}", reason),
                    )
                    .await;
                }
                TokenError::InsufficientScope => {
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
    // 認証の失敗が続く相手は、しばらく確かめずに断る（#596）。プロキシの
    // 後ろでは、`trusted_proxies`のヘッダーの相手で数える
    let client = forwarded_client_ip(
        remote_addr,
        request.headers(),
        Some(&state.trusted_proxies[..]),
    );
    // 確かめる前に1回分を数え、通ったら戻す（PR #608のレビュー）。「止めて
    // いるか」の確認と数えるのを1回のロックで行うので、同時に送られた
    // リクエストも、上限を越えて確かめない
    let Some(attempt) = state.auth_failures.begin(client, Instant::now()) else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    match check_auth(&state, request.headers(), remote_addr).await {
        Ok(authenticated) => {
            state.auth_failures.succeeded(&attempt);
            request.extensions_mut().insert(authenticated);
            next.run(request).await
        }
        Err(response) => *response,
    }
}

/// 呼び出し元のアドレスごとの、認証の失敗の数（#596）。`window`の間に
/// `limit`回失敗したアドレスは、その間の残りを、確かめずに断る。確かめて
/// いる最中のものも数える（同時に送られたリクエストで、上限を越えさせない）。
/// 覚えるアドレスの数に上限を付ける（たくさんのアドレスからの失敗で、
/// メモリーを使い切らせない）。覚えきれないアドレスは、まとめて1つとして
/// 数える（#705。以前は数えずに通したので、たくさんのアドレスで表を
/// 埋めると、後のアドレスは上限なく試せた）
struct AuthFailureLimiter {
    limit: u32,
    window: std::time::Duration,
    max_clients: usize,
    failures: std::sync::Mutex<AuthFailures>,
}

#[derive(Default)]
struct AuthFailures {
    /// アドレスごとの、数と、時間枠の始まり
    clients: HashMap<IpAddr, (u32, Instant)>,
    /// 覚えきれないアドレスの、まとめた数と、時間枠の始まり
    overflow: Option<(u32, Instant)>,
}

/// `begin`で数えた1回分。どこで、どの時間枠で数えたか
struct AuthAttempt {
    client: Option<IpAddr>,
    window_started: Instant,
}

impl AuthFailureLimiter {
    fn new(limit: u32, window: std::time::Duration, max_clients: usize) -> Self {
        Self {
            limit,
            window,
            max_clients,
            failures: std::sync::Mutex::new(AuthFailures::default()),
        }
    }

    /// 認証を確かめる前に呼ぶ。上限に達していれば`None`（確かめずに断る）。
    /// そうでなければ1回分を数えて返す。認証が通ったら`succeeded`で戻す
    fn begin(&self, client: IpAddr, now: Instant) -> Option<AuthAttempt> {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let AuthFailures { clients, overflow } = &mut *failures;
        if clients.len() >= self.max_clients && !clients.contains_key(&client) {
            clients.retain(|_, (_, started)| now.duration_since(*started) < self.window);
        }
        let (key, entry) = if clients.len() >= self.max_clients && !clients.contains_key(&client) {
            // 覚えきれないアドレスは、まとめて数える（メモリーの上限は
            // 保ったまま、上限なく試させない。#705）
            (None, overflow.get_or_insert((0, now)))
        } else {
            (Some(client), clients.entry(client).or_insert((0, now)))
        };
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        if entry.0 >= self.limit {
            return None;
        }
        entry.0 += 1;
        Some(AuthAttempt {
            client: key,
            window_started: entry.1,
        })
    }

    /// 認証が通ったとき、`begin`で数えた1回分を戻す。数えたときと同じ
    /// 時間枠のときだけ（#705。前の時間枠で始めた認証が、新しい時間枠の
    /// 失敗を減らさないように）
    fn succeeded(&self, attempt: &AuthAttempt) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = match attempt.client {
            Some(client) => failures.clients.get_mut(&client),
            None => failures.overflow.as_mut(),
        };
        if let Some(entry) = entry.filter(|entry| entry.1 == attempt.window_started) {
            entry.0 = entry.0.saturating_sub(1);
        }
    }
}

/// 認証の失敗を監査ログに記録する（#55）。提示された認証情報（キー・
/// トークン）そのものは記録しない。1分あたり`AUTH_FAILURE_AUDIT_PER_MINUTE`
/// 件まで（#540）
async fn audit_auth_failure(state: &HttpTransportState, remote_addr: SocketAddr, reason: &str) {
    let (record, report) = state.auth_failure_audit.admit(Instant::now());
    if let Some(report) = report {
        // このリクエストの処理とは別に書く（#724）。取り出した件数は、ここで
        // 書かないと失われるので、接続が切れて処理が破棄されても書き終える
        let audit_logger = state.audit_logger.clone();
        tokio::spawn(async move {
            audit_suppressed_auth_failures(&audit_logger, report).await;
        });
    }
    if !record {
        return;
    }
    // ログにも、監査ログと同じ件数まで書く（#596）。以前は件数の上限が無く、
    // 認証の無い相手が、大きな`kid`等でログを埋められた
    warn!("Rejected request from {} due to {}", remote_addr, reason);
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
    trusted_proxies: &[IpNet],
    remote_addr: SocketAddr,
) -> std::result::Result<(), StatusCode> {
    if enforce_https && !tls_terminated_locally && !is_forwarded_https(headers, trusted_proxies) {
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

/// リプレイ攻撃対策の検査。nonceは、認証した呼び出し元ごと（認証して
/// いなければクライアントのIPごと）に覚える（#543）
async fn check_anti_replay_handshake(
    anti_replay: Option<&Arc<AntiReplayMiddleware>>,
    audit_logger: &AuditLogger,
    headers: &HeaderMap,
    remote_addr: SocketAddr,
    caller: &crate::mcp::Caller,
    client_ip: IpAddr,
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
        ip_address: Some(client_ip),
    };

    // 名前ではなく、利用者まで区別する値で、nonceを分ける（#724）
    let caller_identity = caller.identity();
    if let Err(e) = anti_replay
        .validate_headers(&security_headers, caller_identity.as_deref())
        .await
    {
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
/// by their conventional title-cased names (`"User-Agent"`,
/// `"X-Session-ID"`, `"Referer"`, `"Cookie"`) via an exact-match
/// `HashMap::get`/`contains_key`. Map a lowercase name to that expected form
/// so those lookups actually see real request headers instead of always
/// missing. Keep this in sync with every such exact-match lookup under
/// `src/security/ids/` (searched recursively - some live in submodules like
/// `ml/`), not just the top-level files.
fn canonical_ids_header_name(lowercase_name: &str) -> Option<&'static str> {
    match lowercase_name {
        "user-agent" => Some("User-Agent"),
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
/// into a map from each key to its values. `None` (no query string at all)
/// yields an empty map.
fn parse_query_params(query: Option<&str>) -> HashMap<String, Vec<String>> {
    let Some(query) = query else {
        return HashMap::new();
    };
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    // A repeated key (e.g. "a=malicious&a=benign") must not silently keep
    // only one of the values. Each value stays separate (PR #764 review):
    // joining them with ", " turned `q=x;id&q=benign` into `x;id, benign`,
    // which patterns anchored on the end of a value (CMD-001) missed.
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        map.entry(key.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    map
}

/// The client's IP address, for IDS detection/blocking and for keeping
/// anti-replay nonces apart (#543, PR #555 review). `remote_addr` is the raw
/// TCP peer address, which behind a reverse proxy/load balancer is the
/// proxy's own IP, not the actual client's - blocking on it would ban every
/// client behind that proxy over one bad request. `X-Forwarded-For` and
/// `X-Real-IP` are consulted only when `trusted_proxies` is given
/// (`ids_trust_forwarded_for`) and the peer is one of them (#543):
/// otherwise any client could forge them, either evading IP blocking or
/// getting an arbitrary victim IP blocklisted.
///
/// A proxy appends the address it received the request from to
/// `X-Forwarded-For`, after whatever the client sent, so the entries on the
/// left are the client's own claims. The client's address is the rightmost
/// entry that is not one of the trusted proxies (#543); previously the
/// leftmost one was used. If the header has an entry that is not an IP
/// address, or neither header is usable, the peer address is used.
fn forwarded_client_ip(
    remote_addr: SocketAddr,
    headers: &HeaderMap,
    trusted_proxies: Option<&[IpNet]>,
) -> IpAddr {
    // IPv4をIPv6で表したアドレス（`::ffff:192.0.2.1`）は、IPv4にそろえる
    // （#724。ブロックリスト等で、同じアドレスが別のものとして数えられた）
    forwarded_client_ip_as_sent(remote_addr, headers, trusted_proxies).to_canonical()
}

/// `forwarded_client_ip`の、表記をそろえる前の値
fn forwarded_client_ip_as_sent(
    remote_addr: SocketAddr,
    headers: &HeaderMap,
    trusted_proxies: Option<&[IpNet]>,
) -> IpAddr {
    let peer = remote_addr.ip();
    let Some(trusted_proxies) = trusted_proxies else {
        return peer;
    };
    if !is_trusted_proxy(trusted_proxies, peer) {
        return peer;
    }
    // 複数の行に分かれたヘッダーは、順につないだ1つの一覧と同じ
    let entries: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .collect();
    if !entries.is_empty() {
        let mut leftmost = None;
        for entry in entries.iter().rev() {
            let Ok(ip) = entry.parse::<IpAddr>() else {
                return peer;
            };
            if !is_trusted_proxy(trusted_proxies, ip) {
                return ip;
            }
            leftmost = Some(ip);
        }
        // すべてが信頼するプロキシ: いちばん左（最初のプロキシ）
        return leftmost.unwrap_or(peer);
    }
    header_value(headers, "x-real-ip")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer)
}

/// リプレイ攻撃対策で、nonceを分けるクライアントのIP（#543、PR #555の
/// レビュー）。接続元が信頼するプロキシなら、プロキシの付けるヘッダーから
/// 取る。接続元のIPのままだと、プロキシの後ろの全員が1つの枠を分け合い、
/// 1つのクライアントが埋めると、全員が失敗する。IDSと違い、
/// `ids_trust_forwarded_for`は見ない
fn anti_replay_client_ip(
    state: &HttpTransportState,
    remote_addr: SocketAddr,
    headers: &HeaderMap,
) -> IpAddr {
    forwarded_client_ip(remote_addr, headers, Some(&state.trusted_proxies))
}

/// IDSが、接続元のIPをプロキシの付けるヘッダーから取るときの、信頼する
/// プロキシ（`ids_trust_forwarded_for`、#543）。取らなければ`None`
fn ids_trusted_proxies(state: &HttpTransportState) -> Option<&[IpNet]> {
    state
        .ids_trust_forwarded_for
        .then_some(&state.trusted_proxies[..])
}

/// Runs the intrusion detection/prevention system against an incoming
/// request. Already-blocked source IPs are rejected before any of the more
/// expensive signature/behavioral/network analysis runs. `body` is the
/// parsed JSON-RPC payload when one is available (`handle_jsonrpc_request`)
/// or `None` when it isn't yet (`handle_ws_upgrade`, where only the
/// blocklist check applies since there's no request body to analyze at
/// upgrade time). `caller` is the authenticated caller, which the IDS uses
/// to tell callers apart (by IP when unauthenticated), #543. The `X-User-ID`
/// header that it used before could be set to anything by the client.
async fn check_ids(
    ids: Option<&Arc<IntrusionDetectionSystem>>,
    remote_addr: SocketAddr,
    trusted_proxies: Option<&[IpNet]>,
    caller: &crate::mcp::Caller,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: Option<&Value>,
) -> std::result::Result<(), StatusCode> {
    let Some(ids) = ids else {
        return Ok(());
    };
    let ip = forwarded_client_ip(remote_addr, headers, trusted_proxies);

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
        // 名前ではなく、利用者まで区別する値（#724）
        caller_id: caller.identity(),
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
    // リプレイ攻撃対策でnonceを分けるクライアントのアドレス（#543）
    let client_ip = anti_replay_client_ip(&state, remote_addr, &headers);
    // プロキシの付けるヘッダーは、信頼するプロキシからのものだけを使う（#543）
    let no_headers = HeaderMap::new();
    let forwarded = forwarded_headers(&state, remote_addr, &headers, &no_headers);
    if let Err(status) = check_https_enforcement(
        state.enforce_https,
        state.tls_terminated_locally,
        forwarded,
        &state.trusted_proxies,
        remote_addr,
    ) {
        return status.into_response();
    }
    if let Err(status) = check_min_tls_version(
        state.min_tls_version.as_deref(),
        state.enforce_https,
        state.tls_terminated_locally,
        forwarded,
        remote_addr,
    ) {
        return status.into_response();
    }
    if let Err(status) = check_certificate_pin(
        state.certificate_pinning_enabled,
        &state.certificate_pin_header,
        &state.pinned_certificates_sha256,
        forwarded,
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
        &caller,
        client_ip,
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
        ids_trusted_proxies(&state),
        &caller,
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
            handle_ws_socket(
                socket,
                state,
                remote_addr,
                client_ip,
                caller,
                expires_at,
                permit,
                WsRequest { uri, headers },
            )
        });
    response.headers_mut().extend(hsts_headers);
    response
}

/// `/ws`のアップグレードのリクエスト（メッセージをIDSで確かめるときに使う、
/// #596）
struct WsRequest {
    uri: axum::http::Uri,
    headers: HeaderMap,
}

/// Runs the per-connection message loop for a `/ws` client. Holds `_permit` for the connection's
/// lifetime, releasing the reserved capacity slot back to
/// `websocket_semaphore` when the connection ends.
#[allow(clippy::too_many_arguments)]
async fn handle_ws_socket(
    socket: WebSocket,
    state: HttpTransportState,
    addr: SocketAddr,
    client_ip: IpAddr,
    caller: crate::mcp::Caller,
    expires_at: Option<u64>,
    _permit: tokio::sync::OwnedSemaphorePermit,
    request: WsRequest,
) {
    let conn_id = Uuid::new_v4();
    let ids_caller = caller.clone();
    // リプレイ攻撃対策のnonceを、呼び出し元ごとに覚えるため（#543）。
    // 名前ではなく、利用者まで区別する値で分ける（#724）
    let caller_name = caller.identity();
    state.ws_callers.lock().await.insert(
        conn_id,
        WsClient {
            caller,
            token_deadline: expires_at.map(unix_time_to_instant),
            session: Arc::new(crate::mcp::Session::default()),
        },
    );

    let conn_info = WebSocketConnectionInfo::new();
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
    let mut shutdown = state.shutdown.clone();

    loop {
        let idle_deadline = state
            .websocket_idle_timeout
            .map(|idle| last_received + idle);
        // `stop_server()`で閉じる（#760）
        let event = tokio::select! {
            biased;
            _ = wait_for_shutdown(&mut shutdown) => WsEvent::Shutdown,
            event = next_ws_event(&mut receiver, idle_deadline, token_deadline, ping.as_mut()) => event,
        };
        let msg_result = match event {
            WsEvent::Shutdown => {
                debug!(
                    "Closing WebSocket connection {} from {}: the server is stopping",
                    conn_id, addr
                );
                close_ws(&sender, close_code::AWAY, "server stopping").await;
                break;
            }
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
                // top-level fields instead.
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
                                ip_address: Some(client_ip),
                            };

                            if let Err(e) = anti_replay
                                .validate_headers(&security_headers, caller_name.as_deref())
                                .await
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

                // メッセージも、HTTPのリクエストと同じくIDSで確かめる（#596）。
                // 以前は、アップグレードのときにしか確かめなかった
                if state.ids.is_some() {
                    if let Message::Text(text) = &msg {
                        let body = serde_json::from_str::<Value>(text).ok();
                        if let Err(status) = check_ids(
                            state.ids.as_ref(),
                            addr,
                            ids_trusted_proxies(&state),
                            &ids_caller,
                            &request.uri,
                            &request.headers,
                            body.as_ref(),
                        )
                        .await
                        {
                            info!(
                                "Closing WebSocket connection {} from {}: intrusion detection ({})",
                                conn_id, addr, status
                            );
                            close_ws(
                                &sender,
                                close_code::POLICY,
                                "blocked by intrusion detection",
                            )
                            .await;
                            break;
                        }
                    }
                }

                // 期限の直前に受け取ったフレームが、リプレイ攻撃対策・IDSで
                // 待つ間に期限を過ぎることもある。処理を始める直前に確かめ
                // 直す（#759）。始めた処理は、そのまま終える
                if token_expired(token_deadline) {
                    info!(
                        "Closing WebSocket connection {} from {}: the access token expired before the message was processed",
                        conn_id, addr
                    );
                    close_ws(&sender, close_code::POLICY, "access token expired").await;
                    break;
                }

                match state.websocket_handler.handle_message(conn_id, msg).await {
                    // ハンドラーが閉じると決めた（キューの枠を待つ間に
                    // トークンの期限が切れた等、PR #771のレビュー）
                    Ok(Some(Message::Close(frame))) => {
                        info!(
                            "Closing WebSocket connection {} from {}: {}",
                            conn_id,
                            addr,
                            frame
                                .as_ref()
                                .map(|frame| frame.reason.to_string())
                                .unwrap_or_default()
                        );
                        let _ = tokio::time::timeout(
                            WEBSOCKET_SEND_TIMEOUT,
                            sender.lock().await.send(Message::Close(frame)),
                        )
                        .await;
                        break;
                    }
                    Ok(Some(response)) => {
                        // 相手が受信を止めていると、送信で止まり続け、接続の枠を
                        // 持ち続ける（#543）。大きさに合わせた時間内に送れなければ
                        // 閉じる
                        let limit = websocket_response_send_timeout(
                            websocket_message_len(&response),
                            WEBSOCKET_RESPONSE_SEND_BASE_TIMEOUT,
                            WEBSOCKET_MIN_SEND_BYTES_PER_SEC,
                        );
                        let mut s = sender.lock().await;
                        match tokio::time::timeout(limit, s.send(response)).await {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                error!("Failed to send WebSocket response: {}", e);
                                break;
                            }
                            Err(_) => {
                                warn!(
                                    "Closing WebSocket connection {} from {}: the response could not be sent within {} seconds",
                                    conn_id,
                                    addr,
                                    limit.as_secs()
                                );
                                break;
                            }
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
    /// サーバーを止める（`stop_server()`、#760）
    Shutdown,
}

/// pingを送る間隔。無通信の時間の上限の間に、少なくとも2回送る
fn websocket_ping_interval(idle_timeout: std::time::Duration) -> std::time::Duration {
    (idle_timeout / 2)
        .min(std::time::Duration::from_secs(30))
        .max(std::time::Duration::from_millis(100))
}

/// アクセストークンの期限（`/ws`、`unix_time_to_instant`で変えたもの）を
/// 過ぎたか。期限が無ければ`false`（#759）
fn token_expired(token_deadline: Option<tokio::time::Instant>) -> bool {
    token_deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
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
    // トークンの期限を先に見る（#724）。期限とメッセージが同時に準備できた
    // とき、メッセージを選ぶと、期限の後のメッセージを処理してしまう
    tokio::select! {
        biased;
        _ = sleep_until_or_forever(token_deadline) => WsEvent::TokenExpired,
        next = receiver.next() => match next {
            Some(msg_result) => WsEvent::Message(msg_result),
            None => WsEvent::Closed,
        },
        _ = sleep_until_or_forever(idle_deadline) => WsEvent::IdleTimeout,
        _ = ping_due => WsEvent::PingDue,
    }
}

/// `/ws`でpingやCloseフレームを送る時間の上限。相手が受信を止めていても、
/// ここで止まらないようにする
const WEBSOCKET_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// `/ws`で応答を送る時間の上限の、大きさによらない部分（#543）
#[cfg(not(test))]
const WEBSOCKET_RESPONSE_SEND_BASE_TIMEOUT: std::time::Duration = WEBSOCKET_SEND_TIMEOUT;
/// テストでは短くする（止まった相手を閉じるまで待つため）
#[cfg(test)]
const WEBSOCKET_RESPONSE_SEND_BASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// `/ws`で応答を送るときの最低の速さ（バイト/秒、#543）。これより遅い
/// 相手は、受信を止めているとみなして閉じる。64KB/秒（約512kbps）
#[cfg(not(test))]
const WEBSOCKET_MIN_SEND_BYTES_PER_SEC: u64 = 64 * 1024;
/// テストでは速くする（大きな応答で、送信のバッファを埋めるため）
#[cfg(test)]
const WEBSOCKET_MIN_SEND_BYTES_PER_SEC: u64 = 8 * 1024 * 1024;

/// `/ws`で`len`バイトの応答を送る時間の上限（#543）: `base`と、最低の速さ
/// （`min_bytes_per_sec`）で送る時間の和。大きな応答を、遅い回線で正常に
/// 受け取っている相手は切らない
fn websocket_response_send_timeout(
    len: usize,
    base: std::time::Duration,
    min_bytes_per_sec: u64,
) -> std::time::Duration {
    let millis = (len as u64).saturating_mul(1000) / min_bytes_per_sec.max(1);
    base + std::time::Duration::from_millis(millis)
}

/// WebSocketのメッセージの本文の大きさ
fn websocket_message_len(message: &Message) -> usize {
    match message {
        Message::Text(text) => text.len(),
        Message::Binary(data) | Message::Ping(data) | Message::Pong(data) => data.len(),
        Message::Close(_) => 0,
    }
}

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

/// OAuthのトークンの期限（`expires_at`）が過ぎていれば、ほかの無効な
/// トークンと同じ401（`WWW-Authenticate`付き、RFC 6750 §3）。過ぎていなければ
/// `None`（#724、PR #726のレビュー）
fn expired_token_response(state: &HttpTransportState, expires_at: Option<u64>) -> Option<Response> {
    let expired = expires_at.is_some_and(|expires_at| unix_now_secs() > expires_at);
    if !expired {
        return None;
    }
    Some(match &state.oauth {
        Some(oauth) => oauth_error_response(
            oauth.config(),
            Some(&crate::transport::oauth::TokenError::Invalid(
                "the access token expired before the request was processed".to_string(),
            )),
        ),
        None => StatusCode::UNAUTHORIZED.into_response(),
    })
}

/// 今のUNIX時刻（秒）
fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// UNIX時刻（秒）を、tokioの時刻にする。過ぎていれば今
fn unix_time_to_instant(unix_seconds: u64) -> tokio::time::Instant {
    let now = unix_now_secs();
    tokio::time::Instant::now() + std::time::Duration::from_secs(unix_seconds.saturating_sub(now))
}

/// `JsonRpcRequestHandler`でリクエストを処理し、HTTPの応答にする（#400）。
/// 応答があれば200でJSON-RPCの応答を、通知なら202（本文なし、#374）を返す。
/// 2026-07-28のリクエスト（`modern`）へのエラーは、仕様どおり404・400で
/// 返す（`modern_response_status`、#570）。
///
/// 接続の状態（`initialize`で決まった版）は、リクエストごとに新しく作る
/// （#570・#596）。以前は全クライアントで1つを共有していたため、ある
/// 呼び出し元の`initialize`が、ほかの呼び出し元の版を変えていた。
/// 2025-06-18以降のクライアントは、`initialize`の後のリクエストに
/// `MCP-Protocol-Version`ヘッダーを付ける。ヘッダーの無いリクエストは、
/// 対応する一番古い版（`DEFAULT_PROTOCOL_VERSION`）で答える。
#[allow(clippy::too_many_arguments)]
async fn respond_with_request_handler(
    state: &HttpTransportState,
    handler: &dyn JsonRpcRequestHandler,
    request_str: &str,
    caller: &crate::mcp::Caller,
    protocol_version: Option<&'static str>,
    modern: bool,
    start_time: Instant,
) -> std::result::Result<Response, StatusCode> {
    let session = crate::mcp::Session::default();
    match tokio::time::timeout(
        REQUEST_HANDLER_TIMEOUT,
        handler.handle_jsonrpc_in_session(request_str, caller, protocol_version, &session),
    )
    .await
    {
        Ok(Ok(Some(response_value))) => {
            {
                // 大きさは、ロックの外で数える（PR #771のレビュー）
                let response_size = response_value.to_string().len() as u64;
                let mut stats = state.stats.lock();
                stats.total_responses += 1;
                stats.total_response_time_ms += start_time.elapsed().as_millis() as u64;
                stats.total_bytes_sent += response_size;
            }
            // リクエストとして読めなかったもの（バッチ等、#543）は400
            let status = if is_error_without_id(&response_value) {
                StatusCode::BAD_REQUEST
            } else if modern {
                modern_response_status(&response_value)
            } else {
                StatusCode::OK
            };
            Ok((status, response_headers(state), Json(response_value)).into_response())
        }
        Ok(Ok(None)) => {
            state.stats.lock().total_responses += 1;
            Ok((StatusCode::ACCEPTED, response_headers(state)).into_response())
        }
        Ok(Err(e)) => {
            warn!("Rejected an invalid JSON-RPC message: {}", e);
            state.stats.lock().total_errors += 1;
            Ok(jsonrpc_error_response(-32600, "Invalid Request"))
        }
        Err(_) => {
            warn!(
                "JSON-RPC request did not finish within {} seconds",
                REQUEST_HANDLER_TIMEOUT.as_secs()
            );
            state.stats.lock().total_errors += 1;
            Err(StatusCode::REQUEST_TIMEOUT)
        }
    }
}

/// `/ws`の接続ID
type ConnectionId = Uuid;

/// `/ws`の接続1つ分の、呼び出し元と状態
#[derive(Clone)]
struct WsClient {
    /// 接続時の認証で決まる呼び出し元（#52）
    caller: crate::mcp::Caller,
    /// アクセストークンの期限（`unix_time_to_instant`で変えたもの）。
    /// キューの枠を待つ間に切れたら、要求を渡さない（PR #771のレビュー）
    token_deadline: Option<tokio::time::Instant>,
    /// `initialize`で決まったプロトコルの版。接続ごとに持つ（#570・#596）。
    /// 以前は全クライアントで1つを共有していた
    session: Arc<crate::mcp::Session>,
}

/// `/ws`の接続 → `WsClient`
type WsClients = Arc<tokio::sync::Mutex<HashMap<ConnectionId, WsClient>>>;

/// `/ws`の接続の情報
#[derive(Debug, Clone)]
struct WebSocketConnectionInfo {
    /// 最終活動時刻
    last_activity: Instant,
    /// 受信メッセージ数
    messages_received: u64,
    /// 送信メッセージ数
    messages_sent: u64,
}

impl WebSocketConnectionInfo {
    fn new() -> Self {
        Self {
            last_activity: Instant::now(),
            messages_received: 0,
            messages_sent: 0,
        }
    }
}

/// `/ws`で受け取ったメッセージを処理する。以前は
/// `transport::websocket::server`にあった（#543で、使われていなかった
/// 単独のWebSocketサーバーと一緒に、そこから移した）
#[async_trait]
trait MessageHandler: Send + Sync {
    /// メッセージを処理し、返すメッセージがあれば返す
    async fn handle_message(
        &self,
        conn_id: ConnectionId,
        message: Message,
    ) -> Result<Option<Message>>;

    /// 接続時のコールバック
    async fn on_connect(&self, conn_id: ConnectionId, addr: SocketAddr) -> Result<()>;

    /// 切断時のコールバック
    async fn on_disconnect(&self, conn_id: ConnectionId) -> Result<()>;
}

/// Bridges `/ws` text frames into the same JSON-RPC request/response
/// correlation machinery `handle_jsonrpc_request` uses (`RequestChannel`),
/// so `/ws` speaks MCP JSON-RPC rather than echoing
/// frames back.
struct JsonRpcWsHandler {
    /// 設定されていれば、フレームをチャンネルに流さず直接処理する（#400）
    request_handler: Option<Arc<dyn JsonRpcRequestHandler>>,
    /// 接続 → 呼び出し元（#52）と接続の状態。見つからない接続は、どの
    /// ツールも使えない
    callers: WsClients,
    channel: RequestChannel,
    stats: SharedStats,
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
                // 以前は何も返さなかった（#543）
                warn!("Failed to parse WebSocket JSON-RPC request: {}", e);
                self.stats.lock().total_errors += 1;
                return Ok(Some(jsonrpc_error_message(-32700, "Parse error")));
            }
        };

        {
            let mut stats = self.stats.lock();
            stats.total_requests += 1;
            stats.total_bytes_received += text.len() as u64;
        }

        let client = self
            .callers
            .lock()
            .await
            .get(&conn_id)
            .cloned()
            .unwrap_or_else(|| WsClient {
                token_deadline: None,
                caller: crate::mcp::Caller::new(
                    "unknown-websocket-connection",
                    crate::mcp::ToolAccess::any_of(Vec::new()),
                ),
                session: Arc::new(crate::mcp::Session::default()),
            });
        if let Some(handler) = &self.request_handler {
            // `/ws`は、今までどおり`initialize`で版を決める（legacy、#570）。
            // 版は接続ごと
            let response = match tokio::time::timeout(
                REQUEST_HANDLER_TIMEOUT,
                handler.handle_jsonrpc_in_session(&text, &client.caller, None, &client.session),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(e)) => {
                    warn!("Rejected an invalid WebSocket JSON-RPC message: {}", e);
                    self.stats.lock().total_errors += 1;
                    return Ok(Some(jsonrpc_error_message(-32600, "Invalid Request")));
                }
                Err(_) => {
                    warn!(
                        "WebSocket JSON-RPC request did not finish within {} seconds",
                        REQUEST_HANDLER_TIMEOUT.as_secs()
                    );
                    self.stats.lock().total_errors += 1;
                    return Ok(None);
                }
            };
            let Some(response) = response else {
                return Ok(None); // 通知には応答しない
            };
            let response_json = response.to_string();
            {
                let mut stats = self.stats.lock();
                stats.total_responses += 1;
                stats.total_bytes_sent += response_json.len() as u64;
            }
            return Ok(Some(Message::Text(response_json.into())));
        }

        // `id`は文字列か数値（PR #771のレビュー）
        if !channel_request_id_is_valid(&request) {
            self.stats.lock().total_errors += 1;
            return Ok(Some(jsonrpc_error_message(
                -32600,
                "Invalid Request: id must be a string or a number",
            )));
        }

        // チャンネル経由（#760）: キューの枠は期限付きで取る。期限には、枠を
        // 待つ時間も含める。期限を過ぎたら、この応答だけを返さない（接続は
        // 閉じない）
        let deadline = tokio::time::Instant::now() + CHANNEL_RESPONSE_TIMEOUT;
        let permit = match self.channel.reserve(deadline).await {
            Ok(permit) => permit,
            Err(ChannelError::Closed) => {
                return Err(Error::Internal(
                    "Failed to forward WebSocket request: the channel is closed".to_string(),
                ));
            }
            Err(e) => {
                self.stats.lock().total_errors += 1;
                warn!(
                    "Could not forward a WebSocket request to the channel: {:?}",
                    e
                );
                return Ok(None);
            }
        };
        // 枠を待つ間にアクセストークンの期限が切れていれば、渡さずに
        // （枠を返して）接続を閉じる（PR #771のレビュー。HTTPと同じ）
        if token_expired(client.token_deadline) {
            drop(permit);
            return Ok(Some(Message::Close(Some(CloseFrame {
                code: close_code::POLICY,
                reason: "access token expired".into(),
            }))));
        }

        if request.get("id").is_none() {
            // Notification (no `id`) - no response frame expected
            RequestChannel::forward_notification(
                permit,
                text.to_string(),
                client.caller.clone(),
                None,
            );
            return Ok(None);
        }

        // 内部の相関IDで応答を待つ（#760）。別の接続が同じ`id`を使っても、
        // 応答が入れ替わらない
        let pending =
            self.channel
                .forward_request(permit, request, client.caller.clone(), None, deadline);
        match pending.response(deadline).await {
            Ok(response) => {
                let response_json = response.to_string();
                {
                    let mut stats = self.stats.lock();
                    stats.total_responses += 1;
                    stats.total_bytes_sent += response_json.len() as u64;
                }
                Ok(Some(Message::Text(response_json.into())))
            }
            Err(e) => {
                self.stats.lock().total_errors += 1;
                warn!(
                    "No response from the channel for a WebSocket request: {:?}",
                    e
                );
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
        // mcp-rsが自分でTLSを終えるとき（`build_tls_acceptor`）はTLS 1.3
        // だけを使う。それより新しい版を求めても満たせないので、起動しない
        // （#759。以前は起動し、TLS 1.3の接続を受け付けていた）
        if config.tls_enabled && !is_tls_version_at_least("1.3", min_tls_version) {
            return Err(format!(
                "min_tls_version {} cannot be met with tls_enabled = true: mcp-rs serves TLS 1.3 only (use \"1.3\")",
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

    // プロキシの付けるヘッダーを使う設定は、信頼するプロキシが要る（#543）。
    // 無いと、どのリクエストもヘッダーを信じてもらえず、拒否されるか、
    // 設定が働かない
    if config.trusted_proxies.is_empty() {
        let needs_proxy = if config.enforce_https && !config.tls_enabled {
            Some("enforce_https without tls_enabled")
        } else if config.certificate_pinning_enabled && !config.tls_enabled {
            Some("certificate pinning without tls_enabled")
        } else if config.ids_enabled && config.ids_trust_forwarded_for {
            Some("ids_trust_forwarded_for")
        } else {
            None
        };
        if let Some(setting) = needs_proxy {
            return Err(format!(
                "{setting} uses headers set by a reverse proxy: set trusted_proxies to the proxy's addresses (headers from other addresses are ignored)"
            ));
        }
    }
    parse_trusted_proxies(&config.trusted_proxies)?;

    Ok(())
}

/// `trusted_proxies`を読む（#543）。IPアドレスは、そのアドレスだけの範囲にする
fn parse_trusted_proxies(items: &[String]) -> std::result::Result<Vec<IpNet>, String> {
    items
        .iter()
        .map(|item| {
            let item = item.trim();
            let net = item
                .parse::<IpNet>()
                .or_else(|_| item.parse::<IpAddr>().map(IpNet::from))
                .map_err(|_| {
                    format!("trusted_proxies: '{item}' is not an IP address or a CIDR range")
                })?;
            canonical_net(net).map_err(|e| format!("trusted_proxies: '{item}' {e}"))
        })
        .collect()
}

/// IPv4をIPv6で表した範囲（`::ffff:10.0.0.5/128`）を、IPv4の範囲にする
/// （#724）。接続元は`to_canonical()`でIPv4にして比べるので、IPv6のままでは
/// 一致しなかった
fn canonical_net(net: IpNet) -> std::result::Result<IpNet, String> {
    let IpNet::V6(v6) = net else {
        return Ok(net);
    };
    // 96ビットより短い範囲（`::/64`等）は、IPv6の範囲のまま。接続元は
    // IPv4にそろえてから比べるので、その中のIPv4を表すアドレスには当たらない
    let Some(v4) = v6
        .network()
        .to_ipv4_mapped()
        .filter(|_| v6.prefix_len() >= 96)
    else {
        return Ok(net);
    };
    ipnet::Ipv4Net::new(v4, v6.prefix_len() - 96)
        .map(|v4| IpNet::V4(v4.trunc()))
        .map_err(|e| e.to_string())
}

/// 接続元が、信頼するプロキシか（#543）。IPv4をIPv6で表したアドレス
/// （`::ffff:192.0.2.1`）も、IPv4として比べる
fn is_trusted_proxy(trusted_proxies: &[IpNet], ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    trusted_proxies.iter().any(|net| net.contains(&ip))
}

/// プロキシの付けたヘッダーとして使ってよいヘッダー（#543）。接続元が
/// 信頼するプロキシでなければ、空（`empty`）にする
fn forwarded_headers<'a>(
    state: &HttpTransportState,
    remote_addr: SocketAddr,
    headers: &'a HeaderMap,
    empty: &'a HeaderMap,
) -> &'a HeaderMap {
    if is_trusted_proxy(&state.trusted_proxies, remote_addr.ip()) {
        headers
    } else {
        empty
    }
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

fn is_forwarded_https(headers: &HeaderMap, trusted_proxies: &[IpNet]) -> bool {
    header_eq_ignore_ascii_case(headers, "x-forwarded-proto", "https")
        || forwarded_proto_is_https(headers, trusted_proxies)
}

/// `Forwarded`ヘッダー（RFC 7239）で、クライアントから受けたプロキシが
/// 付けた要素の`proto`が`https`か（#759）。
///
/// `headers`は、接続元が信頼するプロキシのときだけ渡される
/// （`forwarded_headers`）。プロキシは要素を右に足していくので、右から順に
/// 見て、`for`が信頼するプロキシの要素（プロキシの間の区間）を飛ばし、
/// 最初のそれ以外の要素を使う（`forwarded_client_ip`の`X-Forwarded-For`と
/// 同じ）。それより左はクライアントが送ったものなので見ない。すべての要素の
/// `for`が信頼するプロキシなら、いちばん右の要素（mcp-rsにつないだ
/// プロキシが足したもの）を使う（PR #765のレビュー。いちばん左は、
/// クライアントが送ったものでありうる）。
///
/// 形の正しくないヘッダー（閉じていない引用符、`=`の無いパラメーター、
/// 1つの要素に同じ名前が2つ等）は、HTTPSとみなさない（PR #765の
/// レビュー。閉じていない引用符が、プロキシの足した要素を飲み込んだ）。
///
/// 以前はヘッダー全体から`proto=https`を部分文字列で探したため、
/// クライアントが`Forwarded: for=192.0.2.1;proto=https`を送ると、プロキシが
/// `proto=http`の要素を足しても通った
fn forwarded_proto_is_https(headers: &HeaderMap, trusted_proxies: &[IpNet]) -> bool {
    let Some(elements) = parse_forwarded(headers) else {
        warn!("Ignored a malformed Forwarded header");
        return false;
    };
    let chosen = elements
        .iter()
        .rev()
        .find(|element| {
            !element
                .get("for")
                .and_then(|node| parse_forwarded_node_ip(node))
                .is_some_and(|ip| is_trusted_proxy(trusted_proxies, ip))
        })
        .or_else(|| elements.last());
    chosen
        .and_then(|element| element.get("proto"))
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

/// `Forwarded`ヘッダーを、要素ごとの（小文字の名前 → 値）にする。複数の
/// 行に分かれたヘッダーは、順につないだ1つの一覧と同じ。形が正しくなければ
/// `None`
fn parse_forwarded(headers: &HeaderMap) -> Option<Vec<HashMap<String, String>>> {
    let mut elements = Vec::new();
    for value in headers.get_all("forwarded") {
        let value = value.to_str().ok()?;
        for element in split_unquoted(value, ',')? {
            let element = element.trim();
            if element.is_empty() {
                continue;
            }
            let mut params = HashMap::new();
            for pair in split_unquoted(element, ';')? {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let (key, value) = pair.split_once('=')?;
                let key = key.trim();
                if key.is_empty() || !key.chars().all(is_http_token_char) {
                    return None;
                }
                let value = parse_forwarded_value(value.trim())?;
                if params.insert(key.to_ascii_lowercase(), value).is_some() {
                    return None; // 同じ名前が2つ
                }
            }
            elements.push(params);
        }
    }
    Some(elements)
}

/// `Forwarded`のパラメーターの値。引用符で囲んだ値は、囲みを外し、
/// quoted-pair（`\x`）を`x`にする。囲んでいない値に、引用符・空白は使えない
fn parse_forwarded_value(value: &str) -> Option<String> {
    let Some(quoted) = value.strip_prefix('"') else {
        if value.is_empty() || value.contains(['"', ' ', '\t']) {
            return None;
        }
        return Some(value.to_string());
    };
    let mut unescaped = String::with_capacity(quoted.len());
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => unescaped.push(chars.next()?),
            // 閉じる引用符の後には何も無いこと
            '"' => return chars.next().is_none().then_some(unescaped),
            c => unescaped.push(c),
        }
    }
    None // 閉じていない
}

/// HTTPのtoken（RFC 9110）に使える文字か
fn is_http_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// `text`を`separator`で分ける。引用符（`"`）の中の区切りでは分けない。
/// 引用符が閉じていなければ`None`
fn split_unquoted(text: &str, separator: char) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == separator && !quoted {
            parts.push(&text[start..index]);
            start = index + c.len_utf8();
        }
    }
    if quoted || escaped {
        return None;
    }
    parts.push(&text[start..]);
    Some(parts)
}
/// `Forwarded`の`for`の値（`192.0.2.1`・`192.0.2.1:4711`・
/// `[2001:db8::1]:4711`等）のIPアドレス。`unknown`・`_hidden`等は`None`
fn parse_forwarded_node_ip(node: &str) -> Option<IpAddr> {
    if let Some(rest) = node.strip_prefix('[') {
        return rest.split_once(']')?.0.parse().ok();
    }
    node.parse::<IpAddr>()
        .ok()
        .or_else(|| node.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
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

/// `actual`が`minimum`以上か。どちらかを読めなければ`false`（断る側。PR #729の
/// レビュー。以前は読めないと`true`で、nginxの`$ssl_protocol`が送る
/// `TLSv1.2`を読めず、`min_tls_version = "1.3"`でも通していた）
fn is_tls_version_at_least(actual: &str, minimum: &str) -> bool {
    match (parse_tls_version(actual), parse_tls_version(minimum)) {
        (Some(actual_num), Some(min_num)) => actual_num >= min_num,
        _ => false,
    }
}

/// TLSの版を数にする。`1.3`・`TLS1.3`・`TLSv1.3`（nginxの`$ssl_protocol`）・
/// `TLS 1.3`・`TLSv1_3`等を読む
fn parse_tls_version(version: &str) -> Option<f32> {
    let lower = version.trim().to_ascii_lowercase();
    let number = lower.strip_prefix("tls").unwrap_or(&lower);
    let number = number.trim_start_matches(['v', ' ', '_']).replace('_', ".");
    number.parse::<f32>().ok()
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
        // プロキシの書き方（nginxの`$ssl_protocol`等）も読む（PR #729のレビュー。
        // 以前は`TLSv1.2`を読めず、通していた）
        assert!(!is_tls_version_at_least("TLSv1.2", "1.3"));
        assert!(is_tls_version_at_least("TLSv1.3", "1.3"));
        assert!(!is_tls_version_at_least("TLS 1.2", "1.3"));
        assert!(!is_tls_version_at_least("TLSv1_2", "1.3"));
        assert!(is_tls_version_at_least("tls1.3", "1.3"));
        // 読めない値は断る
        assert!(!is_tls_version_at_least("SSLv3", "1.3"));
        assert!(!is_tls_version_at_least("", "1.3"));
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

    /// mcp-rsがTLSを終えるとき（TLS 1.3だけ）は、それより新しい
    /// `min_tls_version`で起動しない（#759）。プロキシの後ろでは、
    /// プロキシの知らせる版と比べるので、そのまま受け付ける
    #[test]
    fn test_tls_validation_rejects_min_version_above_direct_tls() {
        let direct = |version: &str| HttpConfig {
            tls_enabled: true,
            tls_cert_path: Some("cert.pem".to_string()),
            tls_key_path: Some("key.pem".to_string()),
            min_tls_version: Some(version.to_string()),
            ..HttpConfig::default()
        };
        let error = validate_tls_security_settings(&direct("1.4")).unwrap_err();
        assert!(error.contains("TLS 1.3 only"), "{error}");
        assert!(validate_tls_security_settings(&direct("1.3")).is_ok());
        assert!(validate_tls_security_settings(&direct("TLSv1.3")).is_ok());
        let behind_proxy = HttpConfig {
            min_tls_version: Some("1.4".to_string()),
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&behind_proxy).is_ok());
    }

    /// 対応していない版を断る応答に、文字列・数値でない`id`を返さない（#759）
    #[tokio::test]
    async fn test_unsupported_protocol_version_does_not_echo_an_invalid_id() {
        let id_of = |request: Value| async move {
            let response = unsupported_protocol_version("2099-01-01", &request);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice::<Value>(&body).unwrap()["id"].clone()
        };
        assert_eq!(
            id_of(serde_json::json!({ "id": { "nested": 1 } })).await,
            Value::Null
        );
        assert_eq!(id_of(serde_json::json!({ "id": [1] })).await, Value::Null);
        assert_eq!(id_of(serde_json::json!({ "id": true })).await, Value::Null);
        assert_eq!(id_of(serde_json::json!({})).await, Value::Null);
        assert_eq!(id_of(serde_json::json!({ "id": 7 })).await, 7);
        assert_eq!(id_of(serde_json::json!({ "id": "a" })).await, "a");
    }

    /// `Forwarded`は、クライアントから受けたプロキシが足した要素の`proto`
    /// だけを、完全一致で見る（#759）。以前はヘッダー全体の部分文字列で、
    /// クライアントの送った`proto=https`でも通った
    #[test]
    fn test_forwarded_proto_uses_the_element_added_by_the_trusted_proxy() {
        let trusted = vec!["127.0.0.1/32".parse::<IpNet>().unwrap()];
        let https = |values: &[&str], trusted: &[IpNet]| {
            let mut headers = HeaderMap::new();
            for value in values {
                headers.append("forwarded", value.parse().unwrap());
            }
            is_forwarded_https(&headers, trusted)
        };
        assert!(https(&["for=198.51.100.7;proto=https"], &trusted));
        assert!(https(&["For=198.51.100.7;PROTO=HTTPS"], &trusted));
        assert!(https(
            &["for=\"[2001:db8::1]:4711\";proto=\"https\""],
            &trusted
        ));
        // クライアントが送った要素の後ろに、プロキシが`proto=http`を足した
        assert!(!https(
            &["for=192.0.2.1;proto=https, for=198.51.100.7;proto=http"],
            &trusted
        ));
        assert!(!https(
            &["for=192.0.2.1;proto=https", "for=198.51.100.7;proto=http"],
            &trusted
        ));
        // 引用符の中の区切りで、要素を偽れない
        assert!(!https(
            &["for=198.51.100.7;proto=http;x=\"a, for=1.2.3.4;proto=https\""],
            &trusted
        ));
        // 部分文字列では一致しない
        assert!(!https(&["for=198.51.100.7;proto=httpsx"], &trusted));
        assert!(!https(&["for=198.51.100.7;xproto=https"], &trusted));
        assert!(!https(&["for=198.51.100.7"], &trusted));
        // 信頼するプロキシの間の要素は飛ばし、クライアントから受けた
        // プロキシの要素を使う
        let chain = vec![
            "127.0.0.1/32".parse::<IpNet>().unwrap(),
            "10.0.0.2/32".parse::<IpNet>().unwrap(),
        ];
        assert!(https(
            &["for=198.51.100.7;proto=https, for=10.0.0.2;proto=http"],
            &chain
        ));
        assert!(!https(
            &["for=198.51.100.7;proto=http, for=10.0.0.2;proto=https"],
            &chain
        ));
        // すべての要素の`for`が信頼するプロキシなら、いちばん右（mcp-rsに
        // つないだプロキシが足したもの）を使う。いちばん左は、クライアントが
        // 送ったものでありうる（PR #765のレビュー）
        assert!(!https(
            &["for=10.0.0.2;proto=https, for=127.0.0.1;proto=http"],
            &chain
        ));
        assert!(https(
            &["for=10.0.0.2;proto=http, for=127.0.0.1;proto=https"],
            &chain
        ));
        // 形の正しくないヘッダーは、HTTPSとみなさない。閉じていない引用符が、
        // プロキシの足した要素を飲み込まない（PR #765のレビュー）
        assert!(!https(
            &["for=192.0.2.1;proto=https;x=\"a, for=198.51.100.7;proto=http"],
            &trusted
        ));
        assert!(!https(
            &[
                "for=192.0.2.1;proto=https;x=\"a",
                "for=198.51.100.7;proto=http"
            ],
            &trusted
        ));
        assert!(!https(&["for=198.51.100.7;proto=https;junk"], &trusted));
        assert!(!https(
            &["for=198.51.100.7;proto=https;proto=https"],
            &trusted
        ));
        assert!(!https(&["for=198.51.100.7;proto=\"https\"x"], &trusted));
        assert!(!https(&["for=198.51.100.7;proto=htt ps"], &trusted));
        assert!(!https(&[], &trusted));
    }

    /// `/ws`で、処理を始める直前にトークンの期限を確かめ直す（#759）
    #[test]
    fn test_token_expired() {
        let now = tokio::time::Instant::now();
        assert!(!token_expired(None));
        assert!(token_expired(Some(now)));
        assert!(!token_expired(Some(
            now + std::time::Duration::from_secs(60)
        )));
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
            None,
            &crate::mcp::Caller::default(),
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
            None,
            &crate::mcp::Caller::default(),
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

    /// ブラウザのUser-Agent、複数のCookie、`&`・`<`・`;`・`base64`を含む
    /// 本文は、IDSで断らない（#759。以前はCMD-001・XSS-011に当たり、
    /// HTTP 403になって、送り元のアドレスもブロックした）
    #[tokio::test]
    async fn check_ids_allows_ordinary_browser_headers_and_bodies() {
        let ids = Arc::new(
            IntrusionDetectionSystem::new(IDSConfig {
                auto_block_enabled: true,
                ..IDSConfig::default()
            })
            .await
            .unwrap(),
        );
        let remote_addr: SocketAddr = "203.0.113.7:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                .parse()
                .unwrap(),
        );
        headers.insert("Cookie", "a=b; c=d; theme=dark".parse().unwrap());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "create_post",
                "arguments": {
                    "title": "Q&A; tips",
                    "content": "<p>a < b; c > d & e</p>",
                    "encoding": "base64"
                }
            }
        });
        let uri: axum::http::Uri = "/mcp".parse().unwrap();

        let result = check_ids(
            Some(&ids),
            remote_addr,
            None,
            &crate::mcp::Caller::default(),
            &uri,
            &headers,
            Some(&body),
        )
        .await;

        assert_eq!(result, Ok(()));
        assert!(!ids.is_blocked(remote_addr.ip()).await);
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
        // Each value stays separate (PR #764 review).
        let map = parse_query_params(Some("a=malicious&a=benign"));

        assert_eq!(
            map["a"],
            vec!["malicious".to_string(), "benign".to_string()]
        );
    }

    /// A repeated query key must not hide an attack in an earlier value by
    /// joining it with a later benign one (PR #764 review: `x;id, benign`
    /// did not match CMD-001).
    #[tokio::test]
    async fn check_ids_checks_each_value_of_a_repeated_query_key() {
        let ids = Arc::new(
            IntrusionDetectionSystem::new(IDSConfig {
                auto_block_enabled: true,
                ..IDSConfig::default()
            })
            .await
            .unwrap(),
        );
        let remote_addr: SocketAddr = "203.0.113.8:12345".parse().unwrap();
        let benign_body = serde_json::json!({"jsonrpc": "2.0", "method": "tools/list"});
        let uri: axum::http::Uri = "/mcp?q=x%3Bid&q=benign".parse().unwrap();

        let result = check_ids(
            Some(&ids),
            remote_addr,
            None,
            &crate::mcp::Caller::default(),
            &uri,
            &HeaderMap::new(),
            Some(&benign_body),
        )
        .await;

        assert_eq!(result, Err(StatusCode::FORBIDDEN));
    }

    #[test]
    fn forwarded_client_ip_uses_the_peer_address_when_forwarding_is_not_trusted() {
        // Default posture: a proxy in front of us could set X-Forwarded-For
        // to anything, so without an explicit trust decision it must be
        // ignored and the real TCP peer used - otherwise any client can
        // evade IP blocking, or get an arbitrary victim IP blocklisted, by
        // just setting this header themselves.
        let remote_addr: SocketAddr = "203.0.113.9:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());

        let ip = forwarded_client_ip(remote_addr, &headers, None);

        assert_eq!(ip, remote_addr.ip());
    }

    fn proxies(items: &[&str]) -> Vec<IpNet> {
        parse_trusted_proxies(&items.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn forwarded_client_ip_ignores_the_header_from_a_peer_that_is_not_a_trusted_proxy() {
        // #543: ids_trust_forwarded_for alone used to trust the header from
        // any client
        let remote_addr: SocketAddr = "203.0.113.9:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());
        headers.insert("x-real-ip", "198.51.100.1".parse().unwrap());

        let trusted = proxies(&["10.0.0.0/8"]);
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));

        assert_eq!(ip, remote_addr.ip());
    }

    #[test]
    fn forwarded_client_ip_uses_the_forwarded_header_when_trusted() {
        let remote_addr: SocketAddr = "10.0.0.5:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.1".parse().unwrap());

        let trusted = proxies(&["10.0.0.5"]);
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));

        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn forwarded_client_ip_takes_the_rightmost_address_that_is_not_a_trusted_proxy() {
        // #543: the proxy appends the address it got the request from, so
        // the entries on the left are whatever the client sent. The client
        // here claims to be 192.0.2.66 to get that address blocked
        let remote_addr: SocketAddr = "10.0.0.5:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "192.0.2.66, 198.51.100.1, 10.0.0.7".parse().unwrap(),
        );
        let trusted = proxies(&["10.0.0.0/24"]);

        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));
        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());

        // ヘッダーが2行に分かれていても、同じ
        let mut headers = HeaderMap::new();
        headers.append("x-forwarded-for", "192.0.2.66".parse().unwrap());
        headers.append("x-forwarded-for", "198.51.100.1".parse().unwrap());
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));
        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());

        // IPv4をIPv6で表した接続元も、信頼するプロキシとして比べる
        let mapped: SocketAddr = "[::ffff:10.0.0.5]:12345".parse().unwrap();
        let ip = forwarded_client_ip(mapped, &headers, Some(&trusted));
        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn forwarded_client_ip_falls_back_to_the_peer_address_when_trusted_but_header_is_unparseable() {
        let remote_addr: SocketAddr = "10.0.0.5:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());

        let trusted = proxies(&["10.0.0.5"]);
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));

        assert_eq!(ip, remote_addr.ip());
    }

    #[test]
    fn parse_trusted_proxies_accepts_addresses_and_ranges() {
        let parsed = proxies(&["10.0.0.1", " 192.168.0.0/16 ", "::1", "fd00::/8"]);
        assert_eq!(parsed.len(), 4);
        assert!(is_trusted_proxy(&parsed, "10.0.0.1".parse().unwrap()));
        assert!(!is_trusted_proxy(&parsed, "10.0.0.2".parse().unwrap()));
        assert!(is_trusted_proxy(&parsed, "192.168.3.4".parse().unwrap()));
        assert!(is_trusted_proxy(&parsed, "fd12::1".parse().unwrap()));
        assert!(parse_trusted_proxies(&["proxy.example.com".to_string()]).is_err());
        // IPv4をIPv6で表した範囲は、IPv4の範囲にする（#724）
        let mapped = proxies(&["::ffff:10.0.0.5/128", "::ffff:192.168.0.0/112"]);
        assert!(is_trusted_proxy(&mapped, "10.0.0.5".parse().unwrap()));
        assert!(is_trusted_proxy(
            &mapped,
            "::ffff:10.0.0.5".parse().unwrap()
        ));
        assert!(is_trusted_proxy(&mapped, "192.168.7.8".parse().unwrap()));
        assert!(!is_trusted_proxy(&mapped, "10.0.0.6".parse().unwrap()));
        // 96ビットより短い範囲は、IPv6の範囲のまま
        let v6 = proxies(&["::/64"]);
        assert!(is_trusted_proxy(&v6, "::1".parse().unwrap()));
        assert!(!is_trusted_proxy(&v6, "10.0.0.5".parse().unwrap()));
    }

    /// プロキシのヘッダーのIPv4をIPv6で表したアドレスは、IPv4にそろえる
    /// （#724。ブロックリスト等で、同じアドレスを別のものとして数えない）
    #[test]
    fn forwarded_client_ip_is_canonical() {
        let remote_addr: SocketAddr = "10.0.0.5:12345".parse().unwrap();
        let trusted = proxies(&["10.0.0.5"]);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "::ffff:198.51.100.1".parse().unwrap());
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));
        assert_eq!(ip, "198.51.100.1".parse::<IpAddr>().unwrap());
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "::ffff:198.51.100.2".parse().unwrap());
        let ip = forwarded_client_ip(remote_addr, &headers, Some(&trusted));
        assert_eq!(ip, "198.51.100.2".parse::<IpAddr>().unwrap());
        // 接続元そのものも
        let mapped: SocketAddr = "[::ffff:203.0.113.9]:1".parse().unwrap();
        assert_eq!(
            forwarded_client_ip(mapped, &HeaderMap::new(), None),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
    }

    /// 接続数の上限は、`Semaphore`の許す数まで（#724）
    #[tokio::test]
    async fn test_start_server_rejects_too_many_connections() {
        for config in [
            HttpConfig {
                max_connections: tokio::sync::Semaphore::MAX_PERMITS + 1,
                ..HttpConfig::default()
            },
            HttpConfig {
                websocket_max_connections: tokio::sync::Semaphore::MAX_PERMITS + 1,
                ..HttpConfig::default()
            },
        ] {
            let config = HttpConfig {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                ..config
            };
            let transport = HttpTransport::new(config).unwrap();
            let error = transport.start_server().await.err().unwrap();
            assert!(
                error.to_string().contains("must be between 1 and"),
                "{error}"
            );
        }
    }

    #[test]
    fn test_settings_that_read_proxy_headers_require_trusted_proxies() {
        // #543
        let enforce = HttpConfig {
            enforce_https: true,
            ..HttpConfig::default()
        };
        let error = validate_tls_security_settings(&enforce).unwrap_err();
        assert!(error.contains("trusted_proxies"), "got: {error}");
        assert!(validate_tls_security_settings(&HttpConfig {
            trusted_proxies: vec!["127.0.0.1".to_string()],
            ..enforce.clone()
        })
        .is_ok());

        let ids = HttpConfig {
            ids_enabled: true,
            ids_trust_forwarded_for: true,
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&ids).is_err());

        let bad = HttpConfig {
            trusted_proxies: vec!["not-an-address".to_string()],
            ..HttpConfig::default()
        };
        assert!(validate_tls_security_settings(&bad).is_err());
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
            trusted_proxies: vec!["127.0.0.1".to_string()],
            // 認証を確かめるテストではない（#593: trusted_proxiesには認証が要る）
            allow_unauthenticated_external: true,
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

    /// 信頼するプロキシでない接続元の転送のヘッダーは、使わない（#543）。
    /// 以前は、どの接続元のヘッダーも信じたので、平文のHTTPで
    /// `X-Forwarded-Proto: https`を付ければ、`enforce_https`を通れた
    #[tokio::test]
    async fn test_forwarded_headers_from_an_untrusted_peer_are_ignored() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            enforce_https: true,
            min_tls_version: None,
            // テストの接続元（127.0.0.1）ではないプロキシ
            trusted_proxies: vec!["10.0.0.1".to_string()],
            // 認証を確かめるテストではない（#593: trusted_proxiesには認証が要る）
            allow_unauthenticated_external: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("x-forwarded-proto", "https")
            .header("forwarded", "proto=https")
            .json(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);

        let mut request = format!("ws://localhost:{}/ws", server_addr.port())
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("x-forwarded-proto", "https".parse().unwrap());
        match tokio_tungstenite::connect_async(request).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
            }
            other => panic!("expected the upgrade to be rejected with 426, got: {other:?}"),
        }
    }

    /// プロキシの後ろでは、リプレイ攻撃対策のnonceを、プロキシのIPでは
    /// なく、クライアントのアドレスごとに覚える（#543、PR #555のレビュー）
    #[tokio::test]
    async fn test_anti_replay_nonces_are_kept_per_client_behind_a_trusted_proxy() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            anti_replay_enabled: true,
            trusted_proxies: vec!["127.0.0.1".to_string()],
            // 認証を確かめるテストではない（#593: trusted_proxiesには認証が要る）
            allow_unauthenticated_external: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let send = |client_ip: &'static str| {
            client
                .post(&url)
                .header("x-forwarded-for", client_ip)
                .header("x-nonce", "the-same-nonce")
                .header("x-timestamp", chrono::Utc::now().to_rfc3339())
                .json(&serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .send()
        };

        assert_eq!(
            send("198.51.100.1").await.unwrap().status(),
            StatusCode::ACCEPTED
        );
        // 同じnonceでも、別のクライアントなら受け付ける
        assert_eq!(
            send("198.51.100.2").await.unwrap().status(),
            StatusCode::ACCEPTED
        );
        // 同じクライアントが同じnonceを使えば、リプレイ
        assert_eq!(
            send("198.51.100.1").await.unwrap().status(),
            StatusCode::CONFLICT
        );
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
            trusted_proxies: vec!["127.0.0.1".to_string()],
            // 認証を確かめるテストではない（#593: trusted_proxiesには認証が要る）
            allow_unauthenticated_external: true,
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

    /// `/ws`のメッセージも、IDSで確かめる（#596）。以前は、アップグレードの
    /// ときにしか確かめなかった
    #[tokio::test]
    async fn test_ids_checks_websocket_messages() {
        use futures::{SinkExt, StreamExt};
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ids_enabled: true,
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let url = format!("ws://localhost:{}/ws", server_addr.port());
        let (mut stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                sql_injection_jsonrpc_request().to_string().into(),
            ))
            .await
            .unwrap();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(tokio_tungstenite::tungstenite::Message::Close(frame)) => {
                        return frame.map(|f| f.reason.to_string());
                    }
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
            None
        })
        .await
        .expect("the connection was not closed");
        assert_eq!(closed.as_deref(), Some("blocked by intrusion detection"));
    }

    /// 認証の失敗が続く相手は、しばらく確かめずに429で断る（#596）
    #[tokio::test]
    async fn test_repeated_authentication_failures_are_throttled() {
        let port = start_test_server(api_key_config).await;
        let client = reqwest::Client::new();
        let post = |key: &'static str| {
            client
                .post(format!("http://127.0.0.1:{}/mcp", port))
                .header("X-API-Key", key)
                .json(&notification_jsonrpc_request())
                .send()
        };
        for _ in 0..20 {
            assert_eq!(
                post("wrong-key").await.unwrap().status(),
                StatusCode::UNAUTHORIZED
            );
        }
        // 正しいキーでも、しばらくは断る（当て推量を続けさせない）
        assert_eq!(
            post("wrong-key").await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            post("correct-key").await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn test_auth_failure_limiter() {
        let limiter = AuthFailureLimiter::new(3, std::time::Duration::from_secs(60), 2);
        let start = std::time::Instant::now();
        let a: IpAddr = "192.0.2.1".parse().unwrap();
        let b: IpAddr = "192.0.2.2".parse().unwrap();
        let c: IpAddr = "192.0.2.3".parse().unwrap();
        // 確かめている最中のもの（同時に送られたリクエスト）も数えるので、
        // 4つ目は、前の3つの結果を待たずに断る（PR #608のレビュー）
        let mut attempts = Vec::new();
        for _ in 0..3 {
            attempts.push(limiter.begin(a, start).unwrap());
        }
        assert!(limiter.begin(a, start).is_none());
        // 通ったものは戻すので、正しいキーの呼び出し元は止まらない
        limiter.succeeded(&attempts[0]);
        assert!(limiter.begin(a, start).is_some());
        assert!(limiter.begin(a, start).is_none());
        // ほかのアドレスは別に数える
        assert!(limiter.begin(b, start).is_some());
        // 窓が過ぎれば、また確かめる
        let later = start + std::time::Duration::from_secs(61);
        assert!(limiter.begin(a, later).is_some());
        // 前の窓で始めた認証が通っても、新しい窓の数は減らさない（#705）
        assert!(limiter.begin(a, later).is_some());
        assert!(limiter.begin(a, later).is_some());
        assert!(limiter.begin(a, later).is_none());
        limiter.succeeded(&attempts[1]);
        assert!(limiter.begin(a, later).is_none());

        // 覚えるアドレスの数（2）を超えたアドレスは、まとめて数える（#705。
        // 以前は数えずに通した）
        let d: IpAddr = "192.0.2.4".parse().unwrap();
        let overflow = limiter.begin(c, start).unwrap();
        assert_eq!(overflow.client, None);
        for _ in 0..2 {
            assert!(limiter.begin(c, start).is_some());
        }
        assert!(limiter.begin(c, start).is_none());
        assert!(limiter.begin(d, start).is_none());
        // まとめた数も、通ったものは戻す
        limiter.succeeded(&overflow);
        assert!(limiter.begin(d, start).is_some());
        // 窓が過ぎたアドレスは忘れるので、空いた分は、アドレスごとに数える
        assert_eq!(limiter.begin(d, later).unwrap().client, Some(d));
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
            trusted_proxies: vec!["127.0.0.1".to_string()],
            // 認証を確かめるテストではない（#593: trusted_proxiesには認証が要る）
            allow_unauthenticated_external: true,
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

    /// 名前の重なるAPIキーは、起動時に断る（#724。名前は呼び出し元を区別する
    /// 値になり、重なると記録が混ざる）
    #[tokio::test]
    async fn test_start_server_rejects_duplicate_api_key_names() {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![
                ApiKeyEntry {
                    key: "first-key-0123456789abcdef0123456789".to_string(),
                    name: "shared-name".to_string(),
                    user_id: "user-1".to_string(),
                    tools: None,
                },
                ApiKeyEntry {
                    key: "second-key-0123456789abcdef012345678".to_string(),
                    name: "shared-name".to_string(),
                    user_id: "user-1".to_string(),
                    tools: None,
                },
            ],
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let error = transport.start_server().await.err().unwrap();
        assert!(
            error
                .to_string()
                .contains("two entries named 'shared-name'"),
            "{error}"
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

    /// `with_request_handler`を使わないとき、`receive_request_with_caller`は
    /// 要求と一緒に呼び出し元を返す（#749）。APIキーの名前、認証していない
    /// 呼び出し元（名前が無く、IPアドレスがある）、WebSocketの接続も
    #[tokio::test]
    async fn test_channel_requests_carry_the_caller() {
        use futures::{SinkExt as _, StreamExt as _};

        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        })
        .unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        // 受け取った呼び出し元を、そのまま応答に入れて返す
        let mut responder = transport;
        tokio::spawn(async move {
            while let Ok(Some((request, caller))) = responder.receive_request_with_caller().await {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(serde_json::json!({
                        "name": caller.name,
                        "ip_address": caller.ip_address,
                        "unauthenticated_remote": caller.is_unauthenticated_remote(),
                    })),
                    error: None,
                    id: request.id,
                };
                if responder.send_message(response).await.is_err() {
                    break;
                }
            }
        });

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            body["result"],
            serde_json::json!({
                "name": null,
                "ip_address": "127.0.0.1",
                "unauthenticated_remote": true,
            })
        );

        let url = format!("ws://127.0.0.1:{}/ws", server_addr.port());
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(body["id"], serde_json::json!(2));
        assert_eq!(body["result"]["ip_address"], serde_json::json!("127.0.0.1"));
        assert_eq!(
            body["result"]["unauthenticated_remote"],
            serde_json::json!(true)
        );
    }

    /// APIキーで認証した要求は、チャンネルでもキーの名前を運ぶ（#749）
    #[tokio::test]
    async fn test_channel_requests_carry_the_api_key_name() {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![api_key("reader-key", "reader", None)],
            ..HttpConfig::default()
        })
        .unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        let mut responder = transport;
        tokio::spawn(async move {
            while let Ok(Some((request, caller))) = responder.receive_request_with_caller().await {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(serde_json::json!({ "name": caller.name })),
                    error: None,
                    id: request.id,
                };
                if responder.send_message(response).await.is_err() {
                    break;
                }
            }
        });

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .header("Authorization", "Bearer reader-key")
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["result"]["name"], serde_json::json!("api_key:reader"));
    }

    /// チャンネルで受けた要求に、`answer`の結果で答え続けるサーバー（#760）
    async fn start_channel_server(
        config: HttpConfig,
        answer: impl Fn(&ReceivedRequest) -> JsonRpcResponse + Send + 'static,
    ) -> SocketAddr {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..config
        })
        .unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        let mut responder = transport;
        tokio::spawn(async move {
            while let Ok(Some(received)) = responder.receive_request_with_context().await {
                if responder.send_message(answer(&received)).await.is_err() {
                    break;
                }
            }
        });
        server_addr
    }

    fn channel_result(received: &ReceivedRequest, result: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id: received.request.id.clone(),
        }
    }

    fn test_request_channel(
        capacity: usize,
    ) -> (RequestChannel, tokio::sync::mpsc::Receiver<QueuedRequest>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
        (
            RequestChannel {
                sender,
                pending: PendingResponses::default(),
                next_correlation_id: Arc::new(AtomicU64::new(1)),
            },
            receiver,
        )
    }

    /// 別々のクライアントが同じ`id`を使っても、チャンネル経由の応答が
    /// 入れ替わらない（#760）。受信側には内部の相関IDが渡り、応答の`id`は
    /// クライアントの`id`に戻る
    #[tokio::test]
    async fn test_channel_responses_are_not_mixed_up_between_clients_using_the_same_id() {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            auth_enabled: true,
            api_keys: vec![
                api_key("first-key-0123456789", "first", None),
                api_key("second-key-0123456789", "second", None),
            ],
            ..HttpConfig::default()
        })
        .unwrap();
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        // 2件そろってから、後に届いた方から答える
        let mut responder = transport;
        let responder_task = tokio::spawn(async move {
            let mut held = Vec::new();
            while held.len() < 2 {
                held.push(
                    responder
                        .receive_request_with_context()
                        .await
                        .unwrap()
                        .unwrap(),
                );
            }
            let ids: Vec<_> = held.iter().map(|r| r.request.id.clone()).collect();
            for received in held.into_iter().rev() {
                let response = channel_result(
                    &received,
                    serde_json::json!({ "name": received.caller.name }),
                );
                responder.send_message(response).await.unwrap();
            }
            ids
        });

        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let send = |key: &'static str| {
            let request = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
            async move {
                let response = request.send().await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                response.json::<Value>().await.unwrap()
            }
        };
        let (first, second) =
            tokio::join!(send("first-key-0123456789"), send("second-key-0123456789"));
        assert_eq!(first["id"], serde_json::json!(1));
        assert_eq!(first["result"]["name"], serde_json::json!("api_key:first"));
        assert_eq!(second["id"], serde_json::json!(1));
        assert_eq!(
            second["result"]["name"],
            serde_json::json!("api_key:second")
        );

        let ids = tokio::time::timeout(Duration::from_secs(10), responder_task)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(ids[0], ids[1]);
        for id in ids {
            assert!(
                matches!(&id, crate::types::RequestId::String(s) if s.starts_with("mcp-rs-")),
                "{id:?}"
            );
        }
    }

    /// キューが埋まっていると、期限で諦める（#760。以前は`send().await`で
    /// 期限なしに待った）。チャンネルが閉じていれば、すぐに分かる
    #[tokio::test]
    async fn test_request_channel_reserves_a_slot_with_a_deadline() {
        let (channel, receiver) = test_request_channel(1);
        let permit = channel
            .reserve(Instant::now() + Duration::from_secs(1))
            .await
            .unwrap();
        RequestChannel::forward_notification(
            permit,
            "{}".to_string(),
            crate::mcp::Caller::unrestricted(),
            None,
        );

        let started = Instant::now();
        let full = channel
            .reserve(Instant::now() + Duration::from_millis(200))
            .await;
        assert!(matches!(full, Err(ChannelError::Busy)));
        assert!(started.elapsed() < Duration::from_secs(5));

        drop(receiver);
        let closed = channel
            .reserve(Instant::now() + Duration::from_secs(5))
            .await;
        assert!(matches!(closed, Err(ChannelError::Closed)));
    }

    /// 応答を待つものを捨てると（HTTPの要求が途中で捨てられた等）、その
    /// 要求の登録だけが消える（#760）
    #[tokio::test]
    async fn test_dropping_a_pending_channel_request_removes_only_its_registration() {
        let (channel, mut receiver) = test_request_channel(4);
        let deadline = Instant::now() + Duration::from_secs(5);
        let request = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let first = channel.forward_request(
            channel.reserve(deadline).await.unwrap(),
            request.clone(),
            crate::mcp::Caller::unrestricted(),
            None,
            deadline,
        );
        let second = channel.forward_request(
            channel.reserve(deadline).await.unwrap(),
            request,
            crate::mcp::Caller::unrestricted(),
            Some("2025-11-25"),
            deadline,
        );
        assert_eq!(channel.pending.lock().len(), 2);

        let queued_first: Value =
            serde_json::from_str(&receiver.recv().await.unwrap().message).unwrap();
        let queued_second = receiver.recv().await.unwrap();
        assert_eq!(queued_second.protocol_version, Some("2025-11-25"));
        let queued_second: Value = serde_json::from_str(&queued_second.message).unwrap();
        assert_ne!(queued_first["id"], serde_json::json!(1));
        assert_ne!(queued_first["id"], queued_second["id"]);

        drop(first);
        let pending = channel.pending.lock();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains_key(queued_second["id"].as_str().unwrap()));
        drop(pending);

        drop(second);
        assert!(channel.pending.lock().is_empty());
    }

    /// チャンネル経由でも、2026-07-28版の要求の応答に、直接処理と同じ
    /// HTTPのステータスを付ける（#760。`-32601`は404）
    #[tokio::test]
    async fn test_channel_modern_responses_get_the_same_http_status() {
        let server_addr = start_channel_server(HttpConfig::default(), |received| {
            if received.request.method == "tools/list" {
                return channel_result(received, serde_json::json!({ "tools": [] }));
            }
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(crate::types::JsonRpcError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
                id: received.request.id.clone(),
            }
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let version = ("MCP-Protocol-Version", "2026-07-28");

        let (status, body) = post_modern(
            &client,
            &url,
            &[version, ("Mcp-Method", "no/such")],
            &modern_body("no/such", serde_json::json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["id"], serde_json::json!(9));
        assert_eq!(body["error"]["code"], serde_json::json!(-32601));

        let (status, body) = post_modern(
            &client,
            &url,
            &[version, ("Mcp-Method", "tools/list")],
            &modern_body("tools/list", serde_json::json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        // 2026-07-28版でない要求は、これまでどおり200
        let response = client
            .post(&url)
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 3, "method": "no/such" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// チャンネル経由の要求にも、`MCP-Protocol-Version`ヘッダーで選んだ版が
    /// 渡る（#760）。ヘッダーの無い要求は`None`（共有の版）
    #[tokio::test]
    async fn test_channel_requests_carry_the_protocol_version() {
        let server_addr = start_channel_server(HttpConfig::default(), |received| {
            channel_result(
                received,
                serde_json::json!({ "protocol_version": received.protocol_version }),
            )
        })
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let send = |header: Option<&'static str>| {
            let mut request = client.post(&url).json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 7, "method": "tools/list"
            }));
            if let Some(header) = header {
                request = request.header("MCP-Protocol-Version", header);
            }
            async move { request.send().await.unwrap().json::<Value>().await.unwrap() }
        };

        let body = send(None).await;
        assert_eq!(body["result"]["protocol_version"], Value::Null);
        let body = send(Some("2025-11-25")).await;
        assert_eq!(
            body["result"]["protocol_version"],
            serde_json::json!("2025-11-25")
        );
    }

    /// `is_connected()`は待ち受けの状態を返し、動いている間の
    /// `start_server()`はアドレスを変えずに断り、`stop_server()`は待ち受けと
    /// 接続を止める（#760）
    #[tokio::test]
    async fn test_server_start_and_stop() {
        use futures::StreamExt as _;

        let failing = HttpTransport::new(HttpConfig {
            auth_enabled: true,
            ..HttpConfig::default()
        })
        .unwrap();
        assert!(!failing.is_connected());
        assert!(failing.start_server().await.is_err());
        assert!(!failing.is_connected());

        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        })
        .unwrap()
        .with_request_handler(Arc::new(EchoRequestHandler));
        assert!(!transport.is_connected());
        transport.start_server().await.unwrap();
        assert!(transport.is_connected());
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        // 動いている間は、アドレスを変えずに断る
        assert!(transport.start_server().await.is_err());
        assert_eq!(transport.bound_addr().await, Some(server_addr));
        assert!(transport.is_connected());

        let url = format!("ws://127.0.0.1:{}/ws", server_addr.port());
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        transport.stop_server().await;
        assert!(!transport.is_connected());
        assert_eq!(transport.bound_addr().await, None);

        // 新しい接続を受け付けず、ポートも放す
        assert!(reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mcp", server_addr.port()))
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))
            .send()
            .await
            .is_err());

        // `/ws`の接続も閉じる。接続が終わる（読み切る）まで待ってから、
        // ポートを確かめる（`stop_server()`は`/ws`のタスクを待たないので、
        // 残った接続がポートを持つことがある。PR #771のレビュー）
        let mut saw_close = false;
        while let Some(Ok(frame)) = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("the WebSocket connection was not closed")
        {
            saw_close |= frame.is_close();
        }
        assert!(saw_close);
        drop(ws);
        drop(std::net::TcpListener::bind(server_addr).unwrap());

        // 止めた後は、また起動できる
        transport.start_server().await.unwrap();
        assert!(transport.is_connected());
        let restarted = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(restarted).await;
        transport.stop_server().await;
    }

    /// `stop_server()`が途中で捨てられても、止まるまでサーバーの記録を
    /// 消さず、呼び直すと同じタスクを待って止める（PR #771のレビュー）
    #[tokio::test]
    async fn test_cancelled_stop_server_can_be_retried() {
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..HttpConfig::default()
        })
        .unwrap()
        .with_request_handler(Arc::new(EchoRequestHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;

        // 1回だけ進めて捨てる（待ち受けのタスクは、まだ終わっていない）
        {
            let mut stop = Box::pin(transport.stop_server());
            assert!(futures::poll!(stop.as_mut()).is_pending());
        }
        assert!(transport.is_connected());
        assert_eq!(transport.bound_addr().await, Some(server_addr));

        tokio::time::timeout(Duration::from_secs(10), transport.stop_server())
            .await
            .unwrap();
        assert!(!transport.is_connected());
        assert_eq!(transport.bound_addr().await, None);
        drop(std::net::TcpListener::bind(server_addr).unwrap());
    }

    /// チャンネルに渡す要求の`id`は、文字列か数値（PR #771のレビュー）。
    /// ほか（真偽値・`null`等）は、内部の相関IDに置き換えずに、`id: null`の
    /// Invalid Requestで断る。`id`の無い通知は、これまでどおり受け付ける
    #[tokio::test]
    async fn test_channel_rejects_a_request_with_an_invalid_id() {
        use futures::{SinkExt as _, StreamExt as _};

        let server_addr = start_channel_server(
            HttpConfig {
                enable_websocket_upgrade: true,
                ..HttpConfig::default()
            },
            |received| channel_result(received, serde_json::json!({})),
        )
        .await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        for id in [serde_json::json!(true), Value::Null, serde_json::json!({})] {
            let response = client
                .post(&url)
                .json(&serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": "ping" }))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "id {id}");
            let body: Value = response.json().await.unwrap();
            assert_eq!(body["id"], Value::Null, "id {id}");
            assert_eq!(body["error"]["code"], serde_json::json!(-32600), "id {id}");
        }
        // 正しい`id`と通知は、これまでどおり
        let response = client
            .post(&url)
            .json(&serde_json::json!({ "jsonrpc": "2.0", "id": "a", "method": "ping" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.unwrap()["id"],
            serde_json::json!("a")
        );
        let response = client
            .post(&url)
            .json(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let ws_url = format!("ws://127.0.0.1:{}/ws", server_addr.port());
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({ "jsonrpc": "2.0", "id": true, "method": "ping" })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let body: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        assert_eq!(body["id"], Value::Null);
        assert_eq!(body["error"]["code"], serde_json::json!(-32600));
    }

    /// 内部の相関IDの応答で、待ち先がもう無いもの（期限切れ等）は捨てる
    /// （PR #771のレビュー）。要求のキューに入れない（キューが埋まっていても
    /// 止まらない）。内部の相関IDでないものは、これまでどおりキューに入る
    #[tokio::test]
    async fn test_late_channel_response_is_dropped_instead_of_queued() {
        let mut transport = HttpTransport::new(HttpConfig::default()).unwrap();
        // キューを埋める
        while transport
            .channel
            .sender
            .try_send(QueuedRequest {
                message: "{}".to_string(),
                caller: crate::mcp::Caller::unrestricted(),
                protocol_version: None,
                waiter: None,
            })
            .is_ok()
        {}
        let late = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({})),
            error: None,
            id: crate::types::RequestId::String("mcp-rs-42".to_string()),
        };
        tokio::time::timeout(Duration::from_secs(5), transport.send_message(late))
            .await
            .expect("send_message waited for the full queue")
            .unwrap();

        // 空けてから、内部の相関IDでないものを送ると、これまでどおり入る
        let mut drained = 0;
        {
            let mut receiver = transport.receiver.lock().await;
            while receiver.try_recv().is_ok() {
                drained += 1;
            }
        }
        assert_eq!(drained, 1000);
        let other = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({})),
            error: None,
            id: crate::types::RequestId::Number(1),
        };
        transport.send_message(other).await.unwrap();
        assert!(transport.receiver.lock().await.try_recv().is_ok());
        assert!(transport.receiver.lock().await.try_recv().is_err());
    }

    /// もう誰も応答を待っていない要求（クライアントが切った・期限を過ぎた）は、
    /// 受信側に渡さない（PR #771のレビュー）。408の後に処理されて、
    /// やり直しで副作用が重ならないようにする。待たれている要求は
    /// これまでどおり渡す
    #[tokio::test]
    async fn test_abandoned_channel_requests_are_not_handed_to_the_receiver() {
        let transport = HttpTransport::new(HttpConfig::default()).unwrap();
        let channel = transport.channel.clone();
        let forward = |id: i64, deadline| {
            let channel = channel.clone();
            async move {
                let permit = channel.reserve(deadline).await.unwrap();
                channel.forward_request(
                    permit,
                    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": "ping" }),
                    crate::mcp::Caller::unrestricted(),
                    None,
                    deadline,
                )
            }
        };
        let long = Instant::now() + Duration::from_secs(30);

        // 1: クライアントが切った（待つものを捨てた）
        drop(forward(1, long).await);
        // 2: 期限を過ぎた（待つものは、まだ残っている）
        let expired = forward(2, Instant::now() + Duration::from_millis(50)).await;
        // 4: 待たれている要求
        let waiting = forward(4, long).await;
        sleep(Duration::from_millis(100)).await;

        let received = transport
            .receive_request_with_context()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.request.method, "ping");
        let crate::types::RequestId::String(id) = &received.request.id else {
            panic!("{:?}", received.request.id);
        };
        assert!(channel.pending.lock().contains_key(id));
        assert!(transport.receiver.lock().await.try_recv().is_err());
        drop((expired, waiting));
    }

    /// `/ws`で、キューの枠を待つ間にアクセストークンの期限が切れたら、
    /// 要求も通知も渡さずに、接続を閉じる（PR #771のレビュー）
    #[tokio::test]
    async fn test_websocket_request_is_not_queued_after_the_token_expired() {
        let (channel, mut receiver) = test_request_channel(4);
        let callers: WsClients = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let conn_id = Uuid::new_v4();
        callers.lock().await.insert(
            conn_id,
            WsClient {
                caller: crate::mcp::Caller::unrestricted(),
                token_deadline: Some(Instant::now() - Duration::from_secs(1)),
                session: Arc::new(crate::mcp::Session::default()),
            },
        );
        let handler = JsonRpcWsHandler {
            request_handler: None,
            callers,
            channel,
            stats: SharedStats::default(),
        };
        for message in [
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
            serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        ] {
            let reply = handler
                .handle_message(conn_id, Message::Text(message.to_string().into()))
                .await
                .unwrap();
            assert!(
                matches!(
                    &reply,
                    Some(Message::Close(Some(frame))) if frame.code == close_code::POLICY
                ),
                "{reply:?}"
            );
        }
        assert!(receiver.try_recv().is_err());
    }

    /// `connection_stats()`をTokioのタスクから呼んでも、パニックしない
    /// （#760。以前は`blocking_read()`だった）
    #[tokio::test]
    async fn test_connection_stats_can_be_read_from_a_tokio_task() {
        let transport = HttpTransport::new(HttpConfig::default()).unwrap();
        let stats = tokio::spawn(async move { transport.connection_stats() })
            .await
            .unwrap();
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.messages_sent, 0);
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

    /// 2026-07-28のリクエストの版は、ハンドラーに渡す前に確かめる。独自の
    /// ハンドラー（版を確かめない）でも、対応していない版は400と`-32022`
    /// （#697）
    #[tokio::test]
    async fn test_modern_request_with_unsupported_version_is_rejected_before_the_handler() {
        let (_transport, server_addr) = start_version_echo_server(HttpConfig::default()).await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let post = |version: &'static str| {
            let request = client
                .post(&url)
                .header("MCP-Protocol-Version", version)
                .header("Mcp-Method", "tools/list")
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 7,
                    "method": "tools/list",
                    "params": { "_meta": {
                        "io.modelcontextprotocol/protocolVersion": version,
                        "io.modelcontextprotocol/clientCapabilities": {}
                    } }
                }));
            async move {
                let response = request.send().await.unwrap();
                let status = response.status();
                (status, response.json::<Value>().await.unwrap())
            }
        };

        let (status, body) = post("2099-01-01").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], serde_json::json!(-32022));
        assert_eq!(body["error"]["data"]["requested"], "2099-01-01");
        assert_eq!(body["id"], 7);

        // 対応する版は、今までどおりハンドラーに届く
        let (status, body) = post(crate::mcp::MODERN_PROTOCOL_VERSION).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.get("result").is_some(), "{body}");
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

        // 許可していないオリジンのpreflightは403、許可したオリジンは成功
        // （#741。以前は、CORSが先に答えて200だった）
        let preflight_status = |origin: &'static str| {
            let request = client
                .request(reqwest::Method::OPTIONS, &url)
                .header("Origin", origin)
                .header("Access-Control-Request-Method", "POST");
            async move { request.send().await.unwrap().status() }
        };
        assert_eq!(
            preflight_status("http://attacker.example").await,
            reqwest::StatusCode::FORBIDDEN
        );
        assert!(preflight_status("https://app.example.com")
            .await
            .is_success());
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
        // 2026-07-28の`UnsupportedProtocolVersion`（#570。以前は-32600）
        assert_eq!(body["error"]["code"], serde_json::json!(-32022));
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("2099-01-01"));
        assert_eq!(
            body["error"]["data"]["requested"],
            serde_json::json!("2099-01-01")
        );
        assert_eq!(
            body["error"]["data"]["supported"][0],
            serde_json::json!("2026-07-28")
        );

        // initializeでは、ヘッダーを見ない（版は本文で交渉する）
        let (status, body) = send("initialize", Some("2099-01-01")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["protocol_version"], Value::Null);
    }

    /// 実際の`RequestDispatcher`（ハンドラー無し。`mcp_rs_status`だけ）で
    /// 答えるサーバー（#570）
    async fn start_dispatcher_server(config: HttpConfig) -> (HttpTransport, SocketAddr) {
        let server = crate::mcp::McpServer::new();
        let transport = HttpTransport::new(HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..config
        })
        .unwrap()
        .with_request_handler(Arc::new(server.dispatcher()));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        (transport, server_addr)
    }

    fn modern_body(method: &str, params: Value) -> Value {
        let mut params = params;
        params["_meta"] = serde_json::json!({
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": method, "params": params })
    }

    async fn post_modern(
        client: &reqwest::Client,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> (StatusCode, Value) {
        let mut request = client.post(url).json(body);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        (status, response.json::<Value>().await.unwrap())
    }

    /// 2026-07-28のリクエストを、`initialize`無しで受ける。ヘッダーと本文が
    /// 合わなければ400と-32020（#570の段階2）
    #[tokio::test]
    async fn test_modern_requests_over_http() {
        let (_transport, server_addr) = start_dispatcher_server(HttpConfig::default()).await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let version = ("MCP-Protocol-Version", "2026-07-28");

        // server/discover
        let (status, body) = post_modern(
            &client,
            &url,
            &[version, ("Mcp-Method", "server/discover")],
            &modern_body("server/discover", serde_json::json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["resultType"], serde_json::json!("complete"));
        assert_eq!(
            body["result"]["supportedVersions"],
            serde_json::json!(["2026-07-28"])
        );

        // tools/call。Mcp-Nameは、Base64で送ってもよい
        let call = modern_body(
            "tools/call",
            serde_json::json!({ "name": "mcp_rs_status", "arguments": {} }),
        );
        for name in ["mcp_rs_status", "=?base64?bWNwX3JzX3N0YXR1cw==?="] {
            let (status, body) = post_modern(
                &client,
                &url,
                &[version, ("Mcp-Method", "tools/call"), ("Mcp-Name", name)],
                &call,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            // 認証していない呼び出し元（接続元のIPアドレスがあり、名前が無い）
            // なので、バージョンだけが返る（#746）
            // （ほかの情報が足されたら失敗するよう、全体で比べる。PR #747の
            // レビュー）
            assert_eq!(
                body["result"]["content"][0]["text"],
                serde_json::json!(format!(
                    "mcp-rs {}\nOther details are shown only to authenticated callers in HTTP mode (set auth_enabled or [transport.http.oauth]).",
                    env!("CARGO_PKG_VERSION")
                ))
            );
        }

        // ヘッダーと本文が合わない・ヘッダーが無い
        let tools_list = modern_body("tools/list", serde_json::json!({}));
        let mut legacy_body = tools_list.clone();
        legacy_body["params"] = serde_json::json!({});
        let cases: Vec<(Vec<(&str, &str)>, &Value)> = vec![
            // Mcp-Methodが無い
            (vec![version], &tools_list),
            // Mcp-Methodが違う
            (vec![version, ("Mcp-Method", "tools/call")], &tools_list),
            // 版のヘッダーが無い（本文には版がある）
            (vec![("Mcp-Method", "tools/list")], &tools_list),
            // 版のヘッダーが本文と違う
            (
                vec![
                    ("MCP-Protocol-Version", "2025-11-25"),
                    ("Mcp-Method", "tools/list"),
                ],
                &tools_list,
            ),
            // ヘッダーは2026-07-28で、本文に版が無い
            (vec![version, ("Mcp-Method", "tools/list")], &legacy_body),
            // Mcp-Nameが無い
            (vec![version, ("Mcp-Method", "tools/call")], &call),
            // Mcp-Nameが違う
            (
                vec![
                    version,
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "delete_post"),
                ],
                &call,
            ),
        ];
        for (headers, body) in cases {
            let (status, response) = post_modern(&client, &url, &headers, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{headers:?}: {response}");
            assert_eq!(response["error"]["code"], serde_json::json!(-32020));
            assert_eq!(response["id"], serde_json::json!(9));
            assert!(response["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("Header mismatch"));
        }

        // 知らないメソッドは404と-32601
        let (status, body) = post_modern(
            &client,
            &url,
            &[version, ("Mcp-Method", "ping")],
            &modern_body("ping", serde_json::json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], serde_json::json!(-32601));

        // 対応していない版は400と-32022
        let mut unsupported = tools_list.clone();
        unsupported["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] =
            serde_json::json!("2099-01-01");
        let (status, body) = post_modern(
            &client,
            &url,
            &[
                ("MCP-Protocol-Version", "2099-01-01"),
                ("Mcp-Method", "tools/list"),
            ],
            &unsupported,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], serde_json::json!(-32022));

        // クライアントの能力が無ければ400と-32602
        let mut no_capabilities = tools_list.clone();
        no_capabilities["params"]["_meta"]
            .as_object_mut()
            .unwrap()
            .remove("io.modelcontextprotocol/clientCapabilities");
        let (status, body) = post_modern(
            &client,
            &url,
            &[version, ("Mcp-Method", "tools/list")],
            &no_capabilities,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], serde_json::json!(-32602));

        // ツールの失敗は、今までどおり200（CallToolResultのisError）
        let missing = modern_body("tools/call", serde_json::json!({ "name": "no_such_tool" }));
        let (status, body) = post_modern(
            &client,
            &url,
            &[
                version,
                ("Mcp-Method", "tools/call"),
                ("Mcp-Name", "no_such_tool"),
            ],
            &missing,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["isError"], serde_json::json!(true));
    }

    /// `/mcp`の`initialize`は、ほかのリクエストの版を変えない（#570・
    /// #596）。版は`MCP-Protocol-Version`ヘッダーで決まり、無ければ
    /// 対応する一番古い版
    #[tokio::test]
    async fn test_http_initialize_does_not_change_the_version_of_other_requests() {
        let (_transport, server_addr) = start_dispatcher_server(HttpConfig::default()).await;
        let url = format!("http://127.0.0.1:{}/mcp", server_addr.port());
        let client = reqwest::Client::new();
        let (status, body) = post_modern(
            &client,
            &url,
            &[],
            &serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "a", "version": "1" }
                }
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["result"]["protocolVersion"],
            serde_json::json!("2025-11-25")
        );

        let tools_list = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let (_, body) = post_modern(&client, &url, &[], &tools_list).await;
        assert!(body["result"]["tools"][0].get("title").is_none(), "{body}");
        let (_, body) = post_modern(
            &client,
            &url,
            &[("MCP-Protocol-Version", "2025-11-25")],
            &tools_list,
        )
        .await;
        assert!(body["result"]["tools"][0].get("title").is_some(), "{body}");
        assert!(body["result"].get("resultType").is_none());
    }

    /// `/ws`の版は、接続ごと（#570・#596）。以前は全接続で1つを共有して
    /// いた
    #[tokio::test]
    async fn test_websocket_protocol_version_is_per_connection() {
        let (_transport, server_addr) = start_dispatcher_server(HttpConfig {
            enable_websocket_upgrade: true,
            ..HttpConfig::default()
        })
        .await;
        let url = format!("ws://127.0.0.1:{}/ws", server_addr.port());
        let (mut old, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let (mut new, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        async fn exchange<S>(ws: &mut S, request: Value) -> Value
        where
            S: futures::Sink<tokio_tungstenite::tungstenite::Message>
                + futures::Stream<
                    Item = std::result::Result<
                        tokio_tungstenite::tungstenite::Message,
                        tokio_tungstenite::tungstenite::Error,
                    >,
                > + Unpin,
            <S as futures::Sink<tokio_tungstenite::tungstenite::Message>>::Error: std::fmt::Debug,
        {
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                request.to_string().into(),
            ))
            .await
            .unwrap();
            let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
                panic!("expected a text frame");
            };
            serde_json::from_str(&text).unwrap()
        }
        let initialize = |version: &str| {
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": version,
                    "capabilities": {},
                    "clientInfo": { "name": "ws", "version": "1" }
                }
            })
        };
        exchange(&mut new, initialize("2025-11-25")).await;
        // 後から別の接続が古い版でinitializeしても、先の接続の版は変わらない
        exchange(&mut old, initialize("2024-11-05")).await;
        let tools_list = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let body = exchange(&mut new, tools_list.clone()).await;
        assert!(body["result"]["tools"][0].get("title").is_some(), "{body}");
        let body = exchange(&mut old, tools_list).await;
        assert!(body["result"]["tools"][0].get("title").is_none(), "{body}");
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

    /// 何も送らない接続は、`timeout_ms`で閉じる（#592）。以前は、
    /// プロトコルを決める前（最初のバイトを待つ間）に上限が無かった
    #[tokio::test]
    async fn test_connection_that_sends_nothing_is_closed() {
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let started = std::time::Instant::now();
        let (_, closed) = raw_exchange(port, b"", std::time::Duration::from_secs(5)).await;
        assert!(closed, "the connection was not closed");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    /// HTTP/2で、pingには答えるが、リクエストを送らない接続も閉じる
    /// （#592）。リクエストの後に何もしなくなった接続も同じ
    #[tokio::test]
    async fn test_idle_http2_connection_is_closed() {
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        // hyperのクライアントは、サーバーのpingに自動で答える
        let (mut sender, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
                .await
                .unwrap();
        let connection = tokio::spawn(connection);

        // 1回は使える
        let request = hyper::Request::builder()
            .uri(format!("http://127.0.0.1:{port}/nothing-here"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        drop(response);

        // その後、何もしなければ閉じられる
        let started = std::time::Instant::now();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), connection).await;
        assert!(closed.is_ok(), "the HTTP/2 connection was not closed");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    /// 処理中のリクエストがある間は、何もしていない接続として閉じない
    /// （#592）
    #[test]
    fn test_connection_activity_counts_requests_in_flight() {
        let activity = Arc::new(ConnectionActivity::new());
        let guard = ActivityGuard::new(activity.clone());
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(activity.idle_for(), std::time::Duration::ZERO);
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(activity.idle_for() >= std::time::Duration::from_millis(20));
    }

    /// HTTP/2で応答を読まない相手は、送信が進まないまま`timeout`を過ぎると
    /// 切る（#759）。以前は、本文が送信のウィンドウを待つ間、処理中として
    /// 数え続け、接続（と接続の枠）を持ち続けられた
    #[tokio::test]
    async fn test_http2_client_that_does_not_read_the_response_is_closed() {
        use http_body_util::BodyExt;
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let request = || {
            hyper::Request::builder()
                .method("POST")
                .uri(format!("http://127.0.0.1:{port}/mcp"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from("not json"))
                .unwrap()
        };

        // 読む相手には、本文のある応答を最後まで送る
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let (mut sender, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
                .await
                .unwrap();
        tokio::spawn(connection);
        let response = sender.send_request(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!body.is_empty());

        // 受信のウィンドウを0にした相手（応答を読まないのと同じ）は、切る
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let (mut sender, connection) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .initial_stream_window_size(0)
                .handshake(TokioIo::new(stream))
                .await
                .unwrap();
        let connection = tokio::spawn(connection);
        let response = sender.send_request(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let started = std::time::Instant::now();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), connection).await;
        assert!(closed.is_ok(), "the HTTP/2 connection was not closed");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
        drop(response);
    }

    /// 最後のフレームをh2に渡した後も、溜まったデータを送れるようになるまで
    /// 期限を外さない（PR #765のレビュー）。以前は、データが送信の
    /// ウィンドウを待って溜まったままでも本文が捨てられ、期限が外れた。
    /// 相手が中身の無い応答（404）のリクエストを送り続ければ、接続は
    /// 何もしていない接続にもならず、閉じられなかった
    #[tokio::test]
    async fn test_http2_response_buffered_in_h2_keeps_the_send_deadline() {
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        // 本文（JSON-RPCのエラー）より小さいウィンドウ: 最初のフレームは
        // h2に渡されるが、送り切れずに溜まる
        let (mut sender, connection) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .initial_stream_window_size(10)
                .handshake(TokioIo::new(stream))
                .await
                .unwrap();
        let connection = tokio::spawn(connection);
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://127.0.0.1:{port}/mcp"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from("not json"))
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // 中身の無い応答のリクエストを送り続ける
        let started = std::time::Instant::now();
        let busy = async {
            loop {
                let request = hyper::Request::builder()
                    .uri(format!("http://127.0.0.1:{port}/nothing-here"))
                    .body(axum::body::Body::empty())
                    .unwrap();
                if sender.send_request(request).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        };
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(busy, connection)
        })
        .await;
        assert!(closed.is_ok(), "the HTTP/2 connection was not closed");
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
        drop(response);
    }

    /// 長さの決まった本文は、最後の1バイトを残して渡し、もう一度読まれた
    /// ときに、その1バイトを終わりと一緒に渡す。それまで期限を持ち続ける
    /// （PR #765のレビュー）
    #[tokio::test]
    async fn test_send_progress_body_holds_back_the_last_byte() {
        use http_body_util::BodyExt;
        use hyper::body::Body as _;
        let activity = Arc::new(ConnectionActivity::new());
        let mut body = SendProgressBody::new(
            axum::body::Body::from("hello"),
            ActivityGuard::new(activity.clone()),
            std::time::Duration::from_secs(1),
        );
        assert!(!body.is_end_stream());
        assert!(activity.send_deadline().is_some());
        assert_eq!(body.size_hint().exact(), Some(5));

        let frame = body.frame().await.unwrap().unwrap();
        assert_eq!(frame.into_data().unwrap(), "hell");
        // 残した1バイトも、残りの大きさに数える
        assert_eq!(body.size_hint().exact(), Some(1));
        // 最後の1バイトがまだあるので、終わりとは言わず、期限も残す
        assert!(!body.is_end_stream());
        assert!(activity.send_deadline().is_some());

        let frame = body.frame().await.unwrap().unwrap();
        assert_eq!(frame.into_data().unwrap(), "o");
        assert_eq!(body.size_hint().exact(), Some(0));
        // ここで終わり（hyperは、このフレームと一緒にEND_STREAMを送り、
        // 本文を捨てる。捨てれば期限も外れる）
        assert!(body.is_end_stream());
        assert!(activity.send_deadline().is_some());
        drop(body);
        assert_eq!(activity.send_deadline(), None);

        // 1バイトの本文は、そのまま終わりと一緒に渡す
        let mut one = SendProgressBody::new(
            axum::body::Body::from("x"),
            ActivityGuard::new(activity.clone()),
            std::time::Duration::from_secs(1),
        );
        let frame = one.frame().await.unwrap().unwrap();
        assert_eq!(frame.into_data().unwrap(), "x");
        assert!(one.is_end_stream());
        assert!(one.frame().await.is_none());
        assert_eq!(activity.send_deadline(), None);

        // 本文の無い応答には、期限を付けず、すぐ終わりと言う
        let empty = SendProgressBody::new(
            axum::body::Body::empty(),
            ActivityGuard::new(activity.clone()),
            std::time::Duration::from_secs(1),
        );
        assert!(empty.is_end_stream());
        assert_eq!(activity.send_deadline(), None);
    }

    /// 受信のウィンドウを本文の長さちょうどにして、END_STREAMまで
    /// WINDOW_UPDATEを送らない相手にも、最後まで送る（PR #765のレビュー。
    /// 以前は、データを送り切った後、終わりを送る1バイトの余地を待ち続け、
    /// 期限で切っていた）
    #[tokio::test]
    async fn test_http2_response_fits_a_window_of_exactly_its_length() {
        use http_body_util::BodyExt;
        let port = start_test_server(|c| c.timeout_ms = 500).await;
        let request = || {
            hyper::Request::builder()
                .method("POST")
                .uri(format!("http://127.0.0.1:{port}/mcp"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from("not json"))
                .unwrap()
        };
        let handshake = |window: Option<u32>| async move {
            let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            let (sender, connection) =
                hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                    .initial_stream_window_size(window)
                    .handshake::<_, axum::body::Body>(TokioIo::new(stream))
                    .await
                    .unwrap();
            tokio::spawn(connection);
            sender
        };

        // 本文の長さを知る
        let mut sender = handshake(None).await;
        let response = sender.send_request(request()).await.unwrap();
        let expected = response.into_body().collect().await.unwrap().to_bytes();
        assert!(expected.len() > 1);

        // ウィンドウを本文の長さちょうどにする。hyperのクライアントは、
        // 本文を読むまでWINDOW_UPDATEを送らない
        let mut sender = handshake(Some(expected.len() as u32)).await;
        let response = sender.send_request(request()).await.unwrap();
        // 期限（500ms）を何度も過ぎるまで、読まずに待つ
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let body = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            response.into_body().collect(),
        )
        .await
        .expect("the body did not end")
        .expect("the connection was cut before the body ended")
        .to_bytes();
        assert_eq!(body, expected);
    }
    /// 送っている応答の期限は、本文を送り終えるか捨てれば外す（#759）
    #[test]
    fn test_send_deadline_is_cleared_when_the_response_is_done() {
        let activity = Arc::new(ConnectionActivity::new());
        let first = ActivityGuard::new(activity.clone());
        let second = ActivityGuard::new(activity.clone());
        assert_eq!(activity.send_deadline(), None);
        let now = std::time::Instant::now();
        first.set_send_deadline(Some(now + std::time::Duration::from_secs(5)));
        second.set_send_deadline(Some(now + std::time::Duration::from_secs(1)));
        assert_eq!(
            activity.send_deadline(),
            Some(now + std::time::Duration::from_secs(1))
        );
        drop(second);
        assert_eq!(
            activity.send_deadline(),
            Some(now + std::time::Duration::from_secs(5))
        );
        first.set_send_deadline(None);
        assert_eq!(activity.send_deadline(), None);
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
        // 期限は、接続できるだけの余裕を持たせる。`now`は秒で切り捨てるので、
        // 実際に残る時間は1秒ほど短い。以前の2秒では、混んだCI（Windows）で
        // 接続する前に切れ、401になることがあった
        let token = auth.token(OAUTH_RESOURCE, serde_json::json!({ "exp": now + 6 }));
        let mut request = format!("ws://localhost:{}/ws", port)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let code = ws_close_code(&mut ws, std::time::Duration::from_secs(15)).await;
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

        // リバースプロキシの後ろ（trusted_proxies）では、ループバックで
        // 待ち受けていても、認証が要る（#593）
        let behind_proxy = HttpConfig {
            bind_addr: "127.0.0.1:8081".parse().unwrap(),
            trusted_proxies: vec!["127.0.0.1".to_string()],
            ..HttpConfig::default()
        };
        let reason = unauthenticated_external_bind(&behind_proxy).unwrap();
        assert!(reason.contains("reverse proxy"), "{reason}");
        for config in [
            HttpConfig {
                auth_enabled: true,
                ..behind_proxy.clone()
            },
            HttpConfig {
                allow_unauthenticated_external: true,
                ..behind_proxy.clone()
            },
        ] {
            assert_eq!(unauthenticated_external_bind(&config), None, "{config:?}");
        }
        // プロキシの後ろのmTLSは、プロキシを確かめるだけなので、認証に
        // 数えない（PR #601のレビュー）
        let mtls_behind_proxy = HttpConfig {
            mtls_enabled: true,
            ..behind_proxy.clone()
        };
        let reason = unauthenticated_external_bind(&mtls_behind_proxy).unwrap();
        assert!(reason.contains("mTLS"), "{reason}");
    }

    #[test]
    fn test_websocket_response_send_timeout_grows_with_the_size() {
        // #543: 本番の値。10MBの応答なら、10秒 + 160秒
        let base = std::time::Duration::from_secs(10);
        let rate = 64 * 1024;
        assert_eq!(websocket_response_send_timeout(0, base, rate), base);
        assert_eq!(
            websocket_response_send_timeout(10 * 1024 * 1024, base, rate),
            std::time::Duration::from_secs(170)
        );
        assert_eq!(
            websocket_response_send_timeout(32 * 1024, base, rate),
            std::time::Duration::from_millis(10_500)
        );
    }

    /// `params.size`バイトの文字列を結果に入れて返す（#543、大きな応答）
    struct LargeResponseHandler;

    #[async_trait]
    impl JsonRpcRequestHandler for LargeResponseHandler {
        async fn handle_jsonrpc(
            &self,
            message: &str,
            _caller: &crate::mcp::Caller,
        ) -> std::result::Result<Option<Value>, String> {
            let request: Value = serde_json::from_str(message).map_err(|e| e.to_string())?;
            let size = request["params"]["size"].as_u64().unwrap_or(0) as usize;
            Ok(Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": { "data": "x".repeat(size) }
            })))
        }
    }

    /// テストの応答の大きさ。送信と受信のバッファ（OSによって数MB）より、
    /// 十分に大きくする。上限は`1秒 + 32MB ÷ 8MB/秒` = 5秒（テストの値）
    const LARGE_RESPONSE_SIZE: usize = 32 * 1024 * 1024;

    async fn start_large_response_server() -> SocketAddr {
        let config = HttpConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            enable_websocket_upgrade: true,
            websocket_max_connections: 1,
            ..HttpConfig::default()
        };
        let transport = HttpTransport::new(config)
            .unwrap()
            .with_request_handler(Arc::new(LargeResponseHandler));
        transport.start_server().await.unwrap();
        let server_addr = transport.bound_addr().await.unwrap();
        wait_for_plain_server_ready(server_addr).await;
        // `transport`を落としてもサーバーは動き続ける
        server_addr
    }

    /// 受信の速さを決められるよう、TCPの上で直接、`/ws`につなぐ
    async fn raw_ws_connect(addr: SocketAddr) -> tokio::net::TcpStream {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!(
                    "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
                    addr.port()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(stream.read_u8().await.unwrap());
        }
        let head = String::from_utf8_lossy(&head);
        assert!(head.starts_with("HTTP/1.1 101"), "got: {head}");
        stream
    }

    /// クライアントからのテキストのフレーム（マスクは0なので、本文はそのまま）
    async fn raw_ws_send_text(stream: &mut tokio::net::TcpStream, text: &str) {
        use tokio::io::AsyncWriteExt;
        let payload = text.as_bytes();
        assert!(payload.len() < 126);
        let mut frame = vec![0x81, 0x80 | payload.len() as u8, 0, 0, 0, 0];
        frame.extend_from_slice(payload);
        stream.write_all(&frame).await.unwrap();
    }

    fn large_response_request() -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "size": LARGE_RESPONSE_SIZE }
        })
        .to_string()
    }

    /// 応答を読まないクライアントは、時間内に送れないので閉じられ、接続の枠が
    /// 戻る（#543）。以前は送信で止まり続け、枠を持ち続けた。
    ///
    /// Windowsでは、OSが1回の送信を大きさによらずすべて受け取る（相手が
    /// 読まなくても、32MBの送信がすぐ終わる）ので、送信で止まらない。
    /// そのため、Windowsでは試さない（読まない相手は、pingに応答しないので、
    /// 無通信の上限で閉じられる）
    #[tokio::test]
    #[cfg_attr(
        windows,
        ignore = "Windows accepts a large send at once, so the send does not stall"
    )]
    async fn test_websocket_client_that_does_not_read_is_closed() {
        let addr = start_large_response_server().await;
        let mut stalled = raw_ws_connect(addr).await;
        raw_ws_send_text(&mut stalled, &large_response_request()).await;

        // 枠（websocket_max_connections = 1）は、読まないクライアントが持っている
        let url = format!("ws://127.0.0.1:{}/ws", addr.port());
        match tokio_tungstenite::connect_async(&url).await {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            }
            other => panic!("expected 503 while the slot is held, got: {other:?}"),
        }

        // 上限（5秒）を過ぎると閉じられ、つなげるようになる
        let started = Instant::now();
        loop {
            match tokio_tungstenite::connect_async(&url).await {
                Ok(_) => break,
                Err(WsError::Http(response))
                    if response.status() == StatusCode::SERVICE_UNAVAILABLE =>
                {
                    assert!(
                        started.elapsed() < Duration::from_secs(60),
                        "the connection that does not read was not closed"
                    );
                    sleep(Duration::from_millis(200)).await;
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
        drop(stalled);
    }

    /// 遅くても読み続けているクライアントは、上限の大きさによらない部分
    /// （テストでは1秒）より長くかかっても、閉じられない（#543）
    #[tokio::test]
    async fn test_websocket_client_that_reads_slowly_is_not_closed() {
        use tokio::io::AsyncReadExt;
        let addr = start_large_response_server().await;
        let mut client = raw_ws_connect(addr).await;
        raw_ws_send_text(&mut client, &large_response_request()).await;

        // フレームの頭: テキスト、マスク無し、64ビットの長さ
        let mut head = [0u8; 2];
        client.read_exact(&mut head).await.unwrap();
        assert_eq!(head[0], 0x81);
        assert_eq!(head[1], 127);
        let length = client.read_u64().await.unwrap() as usize;
        assert!(length > LARGE_RESPONSE_SIZE);

        // 2MB読むごとに100ミリ秒休む（20MB/秒より遅い。最低の速さの8MB/秒より速い）
        let started = Instant::now();
        let mut buffer = vec![0u8; 256 * 1024];
        let mut received = 0;
        let mut next_pause = 2 * 1024 * 1024;
        while received < length {
            let want = buffer.len().min(length - received);
            let n = client.read(&mut buffer[..want]).await.unwrap();
            assert!(n > 0, "closed after {received} of {length} bytes");
            received += n;
            if received >= next_pause {
                next_pause += 2 * 1024 * 1024;
                sleep(Duration::from_millis(100)).await;
            }
        }
        assert!(
            started.elapsed() > Duration::from_secs(1),
            "the response must take longer than the base limit"
        );
    }
}
