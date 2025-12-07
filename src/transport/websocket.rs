//! WebSocket transport implementation for MCP-RS
//!
//! Provides WebSocket-based communication for MCP protocol with full
//! bidirectional support, TLS/WSS, automatic reconnection, and heartbeat.

use crate::security::AuditLogger;
use crate::security::RateLimiter;
use crate::session::{SessionId, SessionManager, SessionState};
use crate::transport::{Transport, TransportError};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, timeout};
use tokio_tungstenite::{
    accept_async, accept_hdr_async, connect_async,
    tungstenite::handshake::server::{Request, Response},
    tungstenite::protocol::Message,
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, info, warn};

/// TLS configuration for WebSocket connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file (PEM format)
    pub cert_path: Option<PathBuf>,
    /// Path to private key file (PEM format)
    pub key_path: Option<PathBuf>,
    /// Path to CA certificate for client verification
    pub ca_cert_path: Option<PathBuf>,
    /// Whether to verify the server certificate (client mode)
    pub verify_server: bool,
    /// Accept invalid certificates (for testing only)
    pub accept_invalid_certs: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert_path: None,
            key_path: None,
            ca_cert_path: None,
            verify_server: true,
            accept_invalid_certs: false,
        }
    }
}

/// JWT algorithm for token verification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JwtAlgorithm {
    /// HMAC with SHA-256
    HS256,
    /// HMAC with SHA-384
    HS384,
    /// HMAC with SHA-512
    HS512,
    /// RSA with SHA-256
    RS256,
    /// RSA with SHA-384
    RS384,
    /// RSA with SHA-512
    RS512,
    /// ECDSA with SHA-256
    ES256,
    /// ECDSA with SHA-384
    ES384,
}

impl Default for JwtAlgorithm {
    fn default() -> Self {
        Self::HS256
    }
}

/// JWT authentication configuration for WebSocket connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWT secret for HMAC algorithms or public key for RSA/ECDSA
    pub secret: String,
    /// JWT algorithm
    pub algorithm: JwtAlgorithm,
    /// Required claims that must be present in the token
    pub required_claims: Vec<String>,
    /// Allowed roles (if empty, any role is allowed)
    pub allowed_roles: Vec<String>,
    /// Token expiration validation enabled
    pub validate_exp: bool,
    /// Token not-before validation enabled
    pub validate_nbf: bool,
    /// Token issued-at validation enabled
    pub validate_iat: bool,
    /// Allowed clock skew in seconds for time-based validations
    pub leeway_seconds: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            algorithm: JwtAlgorithm::default(),
            required_claims: vec!["sub".to_string()],
            allowed_roles: vec![],
            validate_exp: true,
            validate_nbf: true,
            validate_iat: false,
            leeway_seconds: 60,
        }
    }
}

/// Origin validation policy for WebSocket connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OriginValidationPolicy {
    /// Allow any origin (insecure, development only)
    AllowAny,
    /// Reject all origins (most secure, same-origin only)
    RejectAll,
    /// Allow specific origins (whitelist)
    AllowList(Vec<String>),
    /// Allow origins matching regex patterns
    AllowPattern(Vec<String>),
}

impl Default for OriginValidationPolicy {
    fn default() -> Self {
        Self::RejectAll
    }
}

/// WebSocket transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocket URL to connect to or bind address for server
    pub url: String,
    /// Whether to act as server (bind) or client (connect)
    pub server_mode: bool,
    /// Connection timeout in seconds
    pub timeout_seconds: Option<u64>,
    /// Enable TLS/WSS
    pub use_tls: bool,
    /// TLS configuration (required if use_tls is true)
    pub tls_config: Option<TlsConfig>,
    /// Origin validation policy (server mode only)
    pub origin_validation: OriginValidationPolicy,
    /// Strict Origin validation (reject if Origin header is missing)
    pub require_origin_header: bool,
    /// JWT authentication configuration (server mode only)
    pub jwt_config: Option<JwtConfig>,
    /// Require JWT authentication for all connections
    pub require_authentication: bool,
    /// Authentication timeout in seconds (time limit for client to send valid token)
    pub auth_timeout_seconds: Option<u64>,
    /// Enable session management (creates session on successful authentication)
    pub enable_session_management: bool,
    /// Session time-to-live in seconds (default: 3600 = 1 hour)
    pub session_ttl_seconds: u64,
    /// Enable rate limiting
    pub enable_rate_limiting: bool,
    /// Maximum requests per IP per minute
    pub max_requests_per_minute: u32,
    /// Maximum authentication failures before blocking
    pub max_auth_failures: u32,
    /// Duration to block after max auth failures (seconds)
    pub auth_failure_block_duration_secs: u64,
    /// Heartbeat interval in seconds (0 to disable)
    pub heartbeat_interval: u64,
    /// Maximum reconnection attempts (0 for infinite)
    pub max_reconnect_attempts: u32,
    /// Reconnection delay in seconds
    pub reconnect_delay: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Maximum concurrent connections (server mode only)
    pub max_connections: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:8082".to_string(),
            server_mode: true,
            timeout_seconds: Some(30),
            use_tls: false,
            tls_config: None,
            origin_validation: OriginValidationPolicy::default(),
            require_origin_header: false,
            jwt_config: None,
            require_authentication: false,
            auth_timeout_seconds: Some(30),
            enable_session_management: false,
            session_ttl_seconds: 3600,
            enable_rate_limiting: true,
            max_requests_per_minute: 60,
            max_auth_failures: 5,
            auth_failure_block_duration_secs: 300, // 5 minutes
            heartbeat_interval: 30,
            max_reconnect_attempts: 5,
            reconnect_delay: 5,
            max_message_size: 16 * 1024 * 1024, // 16MB
            max_connections: 100,
        }
    }
}

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    ShuttingDown,
}

/// WebSocket client/server connection wrapper
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Message queue item
#[derive(Debug)]
struct QueuedMessage {
    response: JsonRpcResponse,
    retry_count: u32,
}

/// WebSocket transport implementation
#[derive(Debug)]
pub struct WebSocketTransport {
    config: WebSocketConfig,
    state: Arc<RwLock<ConnectionState>>,
    // Client mode: single connection
    client_connection: Arc<RwLock<Option<WsStream>>>,
    // Server mode: multiple connections (simplified - first connection only for now)
    server_connection: Arc<RwLock<Option<WsStream>>>,
    // Message queues
    outgoing_tx: mpsc::UnboundedSender<JsonRpcResponse>,
    outgoing_rx: Arc<RwLock<mpsc::UnboundedReceiver<JsonRpcResponse>>>,
    incoming_tx: mpsc::UnboundedSender<JsonRpcRequest>,
    incoming_rx: Arc<RwLock<mpsc::UnboundedReceiver<JsonRpcRequest>>>,
    // Shutdown signal
    shutdown_tx: Arc<RwLock<Option<mpsc::Sender<()>>>>,
    // Statistics
    messages_sent: Arc<RwLock<u64>>,
    messages_received: Arc<RwLock<u64>>,
    bytes_sent: Arc<RwLock<u64>>,
    bytes_received: Arc<RwLock<u64>>,
    reconnect_count: Arc<RwLock<u32>>,
    // Audit logger for security events
    audit_logger: Option<Arc<AuditLogger>>,
    // Session manager for authenticated connections
    session_manager: Option<Arc<SessionManager>>,
    // Rate limiter for request throttling and auth failure tracking
    rate_limiter: Option<Arc<RateLimiter>>,
    // Session to connection mapping (SessionId -> SocketAddr)
    session_connections: Arc<RwLock<HashMap<SessionId, SocketAddr>>>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport with the given configuration
    pub fn new(config: WebSocketConfig) -> Result<Self, TransportError> {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();

        // Create session manager if session management is enabled
        let session_manager = if config.enable_session_management {
            Some(Arc::new(SessionManager::with_ttl(
                (config.session_ttl_seconds / 3600) as i64,
            )))
        } else {
            None
        };

        // Create rate limiter if rate limiting is enabled
        let rate_limiter = if config.enable_rate_limiting {
            use crate::config::RateLimitConfig;
            use std::time::Duration;

            let rate_config = RateLimitConfig {
                requests_per_second: (config.max_requests_per_minute as f64 / 60.0) as u32,
                burst_size: config.max_requests_per_minute,
                enabled: true,
            };
            Some(Arc::new(RateLimiter::new(rate_config)))
        } else {
            None
        };

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            client_connection: Arc::new(RwLock::new(None)),
            server_connection: Arc::new(RwLock::new(None)),
            outgoing_tx,
            outgoing_rx: Arc::new(RwLock::new(outgoing_rx)),
            incoming_tx,
            incoming_rx: Arc::new(RwLock::new(incoming_rx)),
            shutdown_tx: Arc::new(RwLock::new(None)),
            messages_sent: Arc::new(RwLock::new(0)),
            messages_received: Arc::new(RwLock::new(0)),
            bytes_sent: Arc::new(RwLock::new(0)),
            bytes_received: Arc::new(RwLock::new(0)),
            reconnect_count: Arc::new(RwLock::new(0)),
            audit_logger: None,
            session_manager,
            rate_limiter,
            session_connections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Set audit logger for security event logging
    pub fn with_audit_logger(mut self, audit_logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    /// Validate Origin header against configured policy
    fn validate_origin(
        origin: Option<&str>,
        policy: &OriginValidationPolicy,
        require_header: bool,
        audit_logger: &Option<Arc<AuditLogger>>,
        peer_addr: &str,
    ) -> Result<(), TransportError> {
        // Check if Origin header is required
        if require_header && origin.is_none() {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Warning,
                    AuditCategory::SecurityAttack,
                    "WebSocket connection rejected: Missing required Origin header".to_string(),
                )
                .with_request_info(peer_addr.to_string(), String::new());

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            return Err(TransportError::Unauthorized(
                "Origin header is required".to_string(),
            ));
        }

        // If Origin header is not required and not present, allow
        if origin.is_none() {
            return Ok(());
        }

        let origin_value = origin.unwrap();

        // Validate based on policy
        let is_valid = match policy {
            OriginValidationPolicy::AllowAny => true,
            OriginValidationPolicy::RejectAll => false,
            OriginValidationPolicy::AllowList(allowed) => allowed.iter().any(|o| o == origin_value),
            OriginValidationPolicy::AllowPattern(patterns) => {
                use regex::Regex;
                patterns.iter().any(|pattern| {
                    Regex::new(pattern)
                        .map(|re| re.is_match(origin_value))
                        .unwrap_or(false)
                })
            }
        };

        if !is_valid {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Warning,
                    AuditCategory::SecurityAttack,
                    format!(
                        "WebSocket connection rejected: Invalid Origin '{}'",
                        origin_value
                    ),
                )
                .with_request_info(peer_addr.to_string(), String::new())
                .add_metadata("origin".to_string(), origin_value.to_string())
                .add_metadata("policy".to_string(), format!("{:?}", policy));

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            return Err(TransportError::Unauthorized(format!(
                "Origin '{}' is not allowed",
                origin_value
            )));
        }

        // Log successful validation
        if let Some(logger) = audit_logger {
            use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
            let entry = AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::NetworkActivity,
                format!(
                    "WebSocket connection accepted: Valid Origin '{}'",
                    origin_value
                ),
            )
            .with_request_info(peer_addr.to_string(), String::new())
            .add_metadata("origin".to_string(), origin_value.to_string());

            let logger_clone = Arc::clone(logger);
            tokio::spawn(async move {
                let _ = logger_clone.log(entry).await;
            });
        }

        Ok(())
    }

    /// Extract IP address from SocketAddr
    fn extract_ip_from_socket_addr(addr: &SocketAddr) -> std::net::IpAddr {
        addr.ip()
    }

    /// Get session ID by peer address (if exists)
    async fn get_session_by_peer(
        &self,
        peer_addr: &SocketAddr,
    ) -> Option<SessionId> {
        let sessions = self.session_connections.read().await;
        for (session_id, addr) in sessions.iter() {
            if addr == peer_addr {
                return Some(session_id.clone());
            }
        }
        None
    }

    /// Invalidate session for a specific peer address
    async fn invalidate_session_for_peer(
        &self,
        peer_addr: &SocketAddr,
    ) -> Result<(), TransportError> {
        // Find session ID for this peer
        let session_id = {
            let sessions = self.session_connections.read().await;
            sessions
                .iter()
                .find_map(|(sid, addr)| if addr == peer_addr { Some(sid.clone()) } else { None })
        };

        if let Some(session_id) = session_id {
            // Delete from session manager
            if let Some(ref session_mgr) = self.session_manager {
                session_mgr.delete_session(&session_id).await.map_err(|e| {
                    TransportError::Internal(format!("Failed to delete session: {}", e))
                })?;

                info!(
                    "Session {} invalidated for peer {}",
                    session_id, peer_addr
                );

                // Log to audit log
                if let Some(ref logger) = self.audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::NetworkActivity,
                        format!("Session invalidated for peer disconnect: {}", session_id),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("session_id".to_string(), session_id.to_string());

                    let logger_clone = Arc::clone(logger);
                    tokio::spawn(async move {
                        let _ = logger_clone.log(entry).await;
                    });
                }
            }

            // Remove from mapping
            self.session_connections.write().await.remove(&session_id);
            debug!("Removed session mapping: {} -> {}", session_id, peer_addr);
        }

        Ok(())
    }

    /// Convert JwtAlgorithm to jsonwebtoken::Algorithm
    fn jwt_algorithm_to_jsonwebtoken(alg: &JwtAlgorithm) -> Algorithm {
        match alg {
            JwtAlgorithm::HS256 => Algorithm::HS256,
            JwtAlgorithm::HS384 => Algorithm::HS384,
            JwtAlgorithm::HS512 => Algorithm::HS512,
            JwtAlgorithm::RS256 => Algorithm::RS256,
            JwtAlgorithm::RS384 => Algorithm::RS384,
            JwtAlgorithm::RS512 => Algorithm::RS512,
            JwtAlgorithm::ES256 => Algorithm::ES256,
            JwtAlgorithm::ES384 => Algorithm::ES384,
        }
    }

    /// Validate JWT token from Authorization header and optionally create session
    #[allow(clippy::too_many_arguments)]
    async fn validate_jwt_token_and_create_session(
        auth_header: Option<&str>,
        jwt_config: &JwtConfig,
        require_auth: bool,
        audit_logger: &Option<Arc<AuditLogger>>,
        session_manager: &Option<Arc<SessionManager>>,
        rate_limiter: &Option<Arc<RateLimiter>>,
        peer_addr: &str,
        peer_ip: &std::net::IpAddr,
    ) -> Result<Option<(HashMap<String, Value>, Option<String>)>, TransportError> {
        // Validate JWT token
        let claims_opt = match Self::validate_jwt_token(
            auth_header,
            jwt_config,
            require_auth,
            audit_logger,
            peer_addr,
        ) {
            Ok(opt) => opt,
            Err(e) => {
                // Record authentication failure on validation error
                if let Some(limiter) = rate_limiter {
                    limiter
                        .record_auth_failure_ip(
                            *peer_ip,
                            5,    // max_auth_failures
                            Duration::from_secs(300),  // 5 minutes
                        )
                        .await;
                }
                return Err(e);
            }
        };

        if let Some(claims) = claims_opt {
            // Reset authentication failures on successful validation
            if let Some(limiter) = rate_limiter {
                limiter.reset_auth_failures_ip(*peer_ip).await;
            }

            // Extract user information from claims
            let user_id = claims
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let roles = claims
                .get("roles")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            // Create session if session manager is available
            let session_id = if let Some(manager) = session_manager {
                match manager.create_session(user_id.clone()).await {
                    Ok(mut session) => {
                        // Store roles in metadata
                        if !roles.is_empty() {
                            session
                                .metadata
                                .insert("roles".to_string(), roles.join(","));
                        }

                        // Activate session
                        if let Ok(Some(activated)) = manager.activate_session(&session.id).await {
                            // Log session creation
                            if let Some(logger) = audit_logger {
                                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                                let entry = AuditLogEntry::new(
                                    AuditLevel::Info,
                                    AuditCategory::NetworkActivity,
                                    format!(
                                        "Session created for user '{}' (session_id: {})",
                                        user_id, activated.id
                                    ),
                                )
                                .with_request_info(peer_addr.to_string(), String::new())
                                .add_metadata("user_id".to_string(), user_id.clone())
                                .add_metadata("session_id".to_string(), activated.id.to_string());

                                let logger_clone = Arc::clone(logger);
                                tokio::spawn(async move {
                                    let _ = logger_clone.log(entry).await;
                                });
                            }

                            Some(activated.id.to_string())
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Failed to create session for user {}: {}", user_id, e);
                        None
                    }
                }
            } else {
                None
            };

            Ok(Some((claims, session_id)))
        } else {
            Ok(None)
        }
    }

    /// Validate JWT token from Authorization header
    fn validate_jwt_token(
        auth_header: Option<&str>,
        jwt_config: &JwtConfig,
        require_auth: bool,
        audit_logger: &Option<Arc<AuditLogger>>,
        peer_addr: &str,
    ) -> Result<Option<HashMap<String, Value>>, TransportError> {
        // Check if authentication is required
        if require_auth && auth_header.is_none() {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Warning,
                    AuditCategory::SecurityAttack,
                    "WebSocket connection rejected: Missing required Authorization header"
                        .to_string(),
                )
                .with_request_info(peer_addr.to_string(), String::new());

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            return Err(TransportError::Unauthorized(
                "Authorization header is required".to_string(),
            ));
        }

        // If authentication is not required and no header, return None
        if auth_header.is_none() {
            return Ok(None);
        }

        let auth_value = auth_header.unwrap();

        // Parse "Bearer {token}" format
        let token = if auth_value.starts_with("Bearer ") {
            auth_value.strip_prefix("Bearer ").unwrap()
        } else {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Warning,
                    AuditCategory::SecurityAttack,
                    "WebSocket connection rejected: Invalid Authorization header format"
                        .to_string(),
                )
                .with_request_info(peer_addr.to_string(), String::new());

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            return Err(TransportError::Unauthorized(
                "Authorization header must use Bearer scheme".to_string(),
            ));
        };

        // Set up JWT validation
        let algorithm = Self::jwt_algorithm_to_jsonwebtoken(&jwt_config.algorithm);
        let mut validation = Validation::new(algorithm);
        validation.validate_exp = jwt_config.validate_exp;
        validation.validate_nbf = jwt_config.validate_nbf;
        validation.leeway = jwt_config.leeway_seconds;

        // Decode and validate token
        let decoding_key = match jwt_config.algorithm {
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512 => {
                DecodingKey::from_secret(jwt_config.secret.as_bytes())
            }
            JwtAlgorithm::RS256
            | JwtAlgorithm::RS384
            | JwtAlgorithm::RS512
            | JwtAlgorithm::ES256
            | JwtAlgorithm::ES384 => {
                // For RSA/ECDSA, the secret should be base64-encoded DER
                DecodingKey::from_base64_secret(&jwt_config.secret).map_err(|e| {
                    TransportError::Unauthorized(format!("Invalid RSA/ECDSA key: {}", e))
                })?
            }
        };

        let token_data = decode::<HashMap<String, Value>>(token, &decoding_key, &validation)
            .map_err(|e| {
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        format!(
                            "WebSocket connection rejected: JWT validation failed: {}",
                            e
                        ),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("error".to_string(), e.to_string());

                    let logger_clone = Arc::clone(logger);
                    tokio::spawn(async move {
                        let _ = logger_clone.log(entry).await;
                    });
                }
                TransportError::Unauthorized(format!("Invalid JWT token: {}", e))
            })?;

        let claims = token_data.claims;

        // Validate required claims
        for claim in &jwt_config.required_claims {
            if !claims.contains_key(claim) {
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        format!(
                            "WebSocket connection rejected: Missing required claim '{}'",
                            claim
                        ),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("claim".to_string(), claim.clone());

                    let logger_clone = Arc::clone(logger);
                    tokio::spawn(async move {
                        let _ = logger_clone.log(entry).await;
                    });
                }
                return Err(TransportError::Unauthorized(format!(
                    "Missing required claim: {}",
                    claim
                )));
            }
        }

        // Validate roles if configured
        if !jwt_config.allowed_roles.is_empty() {
            let user_roles = claims
                .get("roles")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            let has_allowed_role = user_roles
                .iter()
                .any(|role| jwt_config.allowed_roles.contains(role));

            if !has_allowed_role {
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        "WebSocket connection rejected: User does not have required role"
                            .to_string(),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("user_roles".to_string(), format!("{:?}", user_roles))
                    .add_metadata(
                        "allowed_roles".to_string(),
                        format!("{:?}", jwt_config.allowed_roles),
                    );

                    let logger_clone = Arc::clone(logger);
                    tokio::spawn(async move {
                        let _ = logger_clone.log(entry).await;
                    });
                }
                return Err(TransportError::Unauthorized(
                    "User does not have required role".to_string(),
                ));
            }
        }

        // Log successful authentication
        if let Some(logger) = audit_logger {
            use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
            let sub = claims
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let entry = AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::NetworkActivity,
                format!(
                    "WebSocket connection authenticated: User '{}' validated successfully",
                    sub
                ),
            )
            .with_request_info(peer_addr.to_string(), String::new())
            .add_metadata("subject".to_string(), sub.to_string());

            let logger_clone = Arc::clone(logger);
            tokio::spawn(async move {
                let _ = logger_clone.log(entry).await;
            });
        }

        Ok(Some(claims))
    }

    /// Start WebSocket server
    async fn start_server(&self) -> Result<(), TransportError> {
        let addr = self
            .config
            .url
            .strip_prefix("ws://")
            .or_else(|| self.config.url.strip_prefix("wss://"))
            .ok_or_else(|| {
                TransportError::Configuration(format!("Invalid WebSocket URL: {}", self.config.url))
            })?;

        let listener = TcpListener::bind(addr).await.map_err(|e| {
            TransportError::Internal(format!("Failed to bind WebSocket server: {}", e))
        })?;

        info!(
            "WebSocket server listening on: {} (TLS: {})",
            addr, self.config.use_tls
        );

        let server_connection = Arc::clone(&self.server_connection);
        let _incoming_tx = self.incoming_tx.clone();
        let state = Arc::clone(&self.state);
        let config = self.config.clone();
        let audit_logger = self.audit_logger.clone();
        let session_manager = self.session_manager.clone();
        let session_connections = Arc::clone(&self.session_connections);
        let rate_limiter = self.rate_limiter.clone();

        // Accept first connection (simplified implementation)
        tokio::spawn(async move {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("WebSocket client connected from: {}", peer_addr);
                    *state.write().await = ConnectionState::Connecting;

                    // Handle TLS if enabled
                    if config.use_tls {
                        // TLS server implementation with Origin validation and session management
                        match Self::accept_tls_connection_with_origin_validation(
                            stream,
                            peer_addr,
                            &config,
                            &audit_logger,
                            &session_manager,
                            &session_connections,
                            &rate_limiter,
                        )
                        .await
                        {
                            Ok(ws_stream) => {
                                info!("WebSocket TLS handshake completed with Origin validation and JWT authentication");
                                *server_connection.write().await = Some(ws_stream);
                                *state.write().await = ConnectionState::Connected;
                            }
                            Err(e) => {
                                error!("WebSocket TLS handshake failed: {}", e);
                                *state.write().await = ConnectionState::Disconnected;
                            }
                        }
                    } else {
                        // Plain WebSocket with Origin validation and JWT authentication
                        let origin_policy = config.origin_validation.clone();
                        let require_origin = config.require_origin_header;
                        let jwt_config_for_callback = config.jwt_config.clone();
                        let jwt_config_for_session = config.jwt_config.clone();
                        let require_auth = config.require_authentication;
                        let audit_logger_for_callback = audit_logger.clone();
                        let audit_logger_for_session = audit_logger.clone();
                        let session_manager_clone = session_manager.clone();
                        let rate_limiter_for_session = rate_limiter.clone();
                        let peer_addr_str = peer_addr.to_string();
                        let peer_addr_str_for_callback = peer_addr_str.clone();
                        let peer_ip = Self::extract_ip_from_socket_addr(&peer_addr);
                        let peer_ip_for_session = peer_ip;
                        let _max_auth_failures = config.max_auth_failures;
                        let _auth_failure_block_duration_secs = config.auth_failure_block_duration_secs;

                        // Store auth header for session creation after handshake
                        let auth_header_captured = Arc::new(RwLock::new(None::<String>));
                        let auth_header_for_callback = auth_header_captured.clone();

                        let callback = move |req: &Request, response: Response| {
                            // Extract Origin header
                            let origin = req.headers().get("Origin").and_then(|h| h.to_str().ok());

                            // Validate Origin
                            if let Err(e) = Self::validate_origin(
                                origin,
                                &origin_policy,
                                require_origin,
                                &audit_logger_for_callback,
                                &peer_addr_str_for_callback,
                            ) {
                                warn!("Origin validation failed: {}", e);
                                return Err(http::Response::builder()
                                    .status(403)
                                    .body(Some(format!("Forbidden: {}", e)))
                                    .unwrap());
                            }

                            // Validate JWT if configured
                            if let Some(ref jwt_cfg) = jwt_config_for_callback {
                                let auth_header = req
                                    .headers()
                                    .get("Authorization")
                                    .and_then(|h| h.to_str().ok());

                                // Store auth header for session creation
                                if let Some(header_val) = auth_header {
                                    // Use blocking write since we're in sync context
                                    if let Ok(mut captured) = auth_header_for_callback.try_write() {
                                        *captured = Some(header_val.to_string());
                                    }
                                }

                                if let Err(e) = Self::validate_jwt_token(
                                    auth_header,
                                    jwt_cfg,
                                    require_auth,
                                    &audit_logger_for_callback,
                                    &peer_addr_str_for_callback,
                                ) {
                                    warn!("JWT authentication failed: {}", e);
                                    return Err(http::Response::builder()
                                        .status(401)
                                        .body(Some(format!("Unauthorized: {}", e)))
                                        .unwrap());
                                }
                            }

                            Ok(response)
                        };

                        match accept_hdr_async(MaybeTlsStream::Plain(stream), callback).await {
                            Ok(ws_stream) => {
                                // Create session after successful handshake
                                if let (Some(ref jwt_cfg), Some(ref sess_mgr)) =
                                    (&jwt_config_for_session, &session_manager_clone)
                                {
                                    let auth_header_value =
                                        auth_header_captured.read().await.clone();
                                    if let Some(auth_header) = auth_header_value.as_deref() {
                                        match Self::validate_jwt_token_and_create_session(
                                            Some(auth_header),
                                            jwt_cfg,
                                            require_auth,
                                            &audit_logger_for_session,
                                            &Some(Arc::clone(sess_mgr)),
                                            &rate_limiter_for_session,
                                            &peer_addr_str,
                                            &peer_ip_for_session,
                                        )
                                        .await
                                        {
                                            Ok(Some((_claims, Some(session_id)))) => {
                                                info!(
                                                    "Session created for WebSocket connection: {}",
                                                    session_id
                                                );
                                                // Store session_id -> peer_addr mapping
                                                session_connections
                                                    .write()
                                                    .await
                                                    .insert(SessionId::from(session_id.clone()), peer_addr);
                                                debug!(
                                                    "Stored session mapping: {} -> {}",
                                                    session_id, peer_addr
                                                );
                                            }
                                            Ok(_) => {
                                                debug!("No session created (auth not required or session management disabled)");
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "Failed to create session after handshake: {}",
                                                    e
                                                );
                                                // Connection is already established, just log the error
                                            }
                                        }
                                    }
                                }

                                info!("WebSocket handshake completed with Origin validation and JWT authentication");
                                *server_connection.write().await = Some(ws_stream);
                                *state.write().await = ConnectionState::Connected;
                            }
                            Err(e) => {
                                error!("WebSocket handshake failed: {}", e);
                                *state.write().await = ConnectionState::Disconnected;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Accept TLS connection (server mode)
    async fn accept_tls_connection(
        stream: TcpStream,
        peer_addr: SocketAddr,
        config: &WebSocketConfig,
        audit_logger: &Option<Arc<AuditLogger>>,
    ) -> Result<WsStream, TransportError> {
        use native_tls::Identity;
        use std::fs;
        use tokio_native_tls::TlsAcceptor;

        let tls_config = config.tls_config.as_ref().ok_or_else(|| {
            TransportError::Configuration(
                "TLS enabled but no TLS configuration provided".to_string(),
            )
        })?;

        // Load certificate and private key
        let cert_path = tls_config.cert_path.as_ref().ok_or_else(|| {
            TransportError::Configuration("TLS certificate path not provided".to_string())
        })?;

        let key_path = tls_config.key_path.as_ref().ok_or_else(|| {
            TransportError::Configuration("TLS private key path not provided".to_string())
        })?;

        // Log certificate loading attempt
        if let Some(logger) = audit_logger {
            use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};

            let entry = AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::NetworkActivity,
                "Loading TLS certificate for WebSocket server".to_string(),
            )
            .add_metadata("cert_path".to_string(), cert_path.display().to_string())
            .add_metadata("peer_addr".to_string(), peer_addr.to_string());

            let _ = logger.log(entry).await;
        }

        // Read certificate and key files
        let cert = fs::read(cert_path).map_err(|e| {
            TransportError::Configuration(format!("Failed to read certificate: {}", e))
        })?;

        let key = fs::read(key_path).map_err(|e| {
            TransportError::Configuration(format!("Failed to read private key: {}", e))
        })?;

        // Create identity from certificate and key
        let identity = Identity::from_pkcs8(&cert, &key).map_err(|e| {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Error,
                    AuditCategory::Error,
                    format!("Failed to create TLS identity: {}", e),
                )
                .with_request_info(peer_addr.to_string(), String::new());

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            TransportError::Configuration(format!("Failed to create identity: {}", e))
        })?;

        // Create TLS acceptor
        let acceptor = native_tls::TlsAcceptor::builder(identity)
            .build()
            .map_err(|e| {
                TransportError::Configuration(format!("Failed to build TLS acceptor: {}", e))
            })?;

        let acceptor = TlsAcceptor::from(acceptor);

        // Accept TLS connection
        let tls_stream = match acceptor.accept(stream).await {
            Ok(stream) => {
                // Log successful TLS handshake
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::NetworkActivity,
                        "TLS handshake successful for WebSocket connection".to_string(),
                    )
                    .with_request_info(peer_addr.to_string(), String::new());

                    let _ = logger.log(entry).await;
                }
                stream
            }
            Err(e) => {
                // Log failed TLS handshake as security attack
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        "TLS handshake failed - possible attack or misconfiguration".to_string(),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("error".to_string(), e.to_string());

                    let _ = logger.log(entry).await;
                }
                return Err(TransportError::Internal(format!(
                    "TLS accept failed: {}",
                    e
                )));
            }
        };

        // Perform WebSocket handshake
        let ws_stream = accept_async(MaybeTlsStream::NativeTls(tls_stream))
            .await
            .map_err(|e| TransportError::Internal(format!("WebSocket handshake failed: {}", e)))?;

        Ok(ws_stream)
    }

    /// Accept TLS connection with Origin validation (server mode)
    async fn accept_tls_connection_with_origin_validation(
        stream: TcpStream,
        peer_addr: SocketAddr,
        config: &WebSocketConfig,
        audit_logger: &Option<Arc<AuditLogger>>,
        session_manager: &Option<Arc<SessionManager>>,
        session_connections: &Arc<RwLock<HashMap<SessionId, SocketAddr>>>,
        rate_limiter: &Option<Arc<RateLimiter>>,
    ) -> Result<WsStream, TransportError> {
        use native_tls::Identity;
        use std::fs;
        use tokio_native_tls::TlsAcceptor;

        let tls_config = config.tls_config.as_ref().ok_or_else(|| {
            TransportError::Configuration(
                "TLS enabled but no TLS configuration provided".to_string(),
            )
        })?;

        // Load certificate and private key
        let cert_path = tls_config.cert_path.as_ref().ok_or_else(|| {
            TransportError::Configuration("TLS certificate path not provided".to_string())
        })?;

        let key_path = tls_config.key_path.as_ref().ok_or_else(|| {
            TransportError::Configuration("TLS private key path not provided".to_string())
        })?;

        // Log certificate loading attempt
        if let Some(logger) = audit_logger {
            use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
            let entry = AuditLogEntry::new(
                AuditLevel::Info,
                AuditCategory::NetworkActivity,
                "Loading TLS certificate for WebSocket server".to_string(),
            )
            .add_metadata("cert_path".to_string(), cert_path.display().to_string())
            .add_metadata("peer_addr".to_string(), peer_addr.to_string());

            let _ = logger.log(entry).await;
        }

        // Read certificate and key files
        let cert = fs::read(cert_path).map_err(|e| {
            TransportError::Configuration(format!("Failed to read certificate: {}", e))
        })?;

        let key = fs::read(key_path).map_err(|e| {
            TransportError::Configuration(format!("Failed to read private key: {}", e))
        })?;

        // Create identity from certificate and key
        let identity = Identity::from_pkcs8(&cert, &key).map_err(|e| {
            if let Some(logger) = audit_logger {
                use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                let entry = AuditLogEntry::new(
                    AuditLevel::Error,
                    AuditCategory::Error,
                    format!("Failed to create TLS identity: {}", e),
                )
                .with_request_info(peer_addr.to_string(), String::new());

                let logger_clone = Arc::clone(logger);
                tokio::spawn(async move {
                    let _ = logger_clone.log(entry).await;
                });
            }
            TransportError::Configuration(format!("Failed to create identity: {}", e))
        })?;

        // Create TLS acceptor
        let acceptor = native_tls::TlsAcceptor::builder(identity)
            .build()
            .map_err(|e| {
                TransportError::Configuration(format!("Failed to build TLS acceptor: {}", e))
            })?;

        let acceptor = TlsAcceptor::from(acceptor);

        // Accept TLS connection
        let tls_stream = match acceptor.accept(stream).await {
            Ok(stream) => {
                // Log successful TLS handshake
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::NetworkActivity,
                        "TLS handshake successful for WebSocket connection".to_string(),
                    )
                    .with_request_info(peer_addr.to_string(), String::new());

                    let _ = logger.log(entry).await;
                }
                stream
            }
            Err(e) => {
                // Log failed TLS handshake as security attack
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        "TLS handshake failed - possible attack or misconfiguration".to_string(),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("error".to_string(), e.to_string());

                    let _ = logger.log(entry).await;
                }
                return Err(TransportError::Internal(format!(
                    "TLS accept failed: {}",
                    e
                )));
            }
        };

        // Perform WebSocket handshake with Origin validation and JWT authentication
        let origin_policy = config.origin_validation.clone();
        let require_origin = config.require_origin_header;
        let jwt_config_for_callback = config.jwt_config.clone();
        let jwt_config_for_session = config.jwt_config.clone();
        let require_auth = config.require_authentication;
        let audit_logger_for_callback = audit_logger.clone();
        let audit_logger_for_session = audit_logger.clone();
        let session_manager_clone = session_manager.clone();
        let rate_limiter_for_session = rate_limiter.clone();
        let peer_addr_str = peer_addr.to_string();
        let peer_addr_str_for_callback = peer_addr_str.clone();
        let peer_ip = Self::extract_ip_from_socket_addr(&peer_addr);
        let peer_ip_for_session = peer_ip;
        let _max_auth_failures = config.max_auth_failures;
        let _auth_failure_block_duration_secs = config.auth_failure_block_duration_secs;

        // Store auth header for session creation after handshake
        let auth_header_captured = Arc::new(RwLock::new(None::<String>));
        let auth_header_for_callback = auth_header_captured.clone();

        let callback = move |req: &Request, response: Response| {
            // Extract Origin header
            let origin = req.headers().get("Origin").and_then(|h| h.to_str().ok());

            // Validate Origin
            if let Err(e) = Self::validate_origin(
                origin,
                &origin_policy,
                require_origin,
                &audit_logger_for_callback,
                &peer_addr_str_for_callback,
            ) {
                warn!("TLS WebSocket Origin validation failed: {}", e);
                return Err(http::Response::builder()
                    .status(403)
                    .body(Some(format!("Forbidden: {}", e)))
                    .unwrap());
            }

            // Validate JWT if configured
            if let Some(ref jwt_cfg) = jwt_config_for_callback {
                let auth_header = req
                    .headers()
                    .get("Authorization")
                    .and_then(|h| h.to_str().ok());

                // Store auth header for session creation
                if let Some(header_val) = auth_header {
                    if let Ok(mut captured) = auth_header_for_callback.try_write() {
                        *captured = Some(header_val.to_string());
                    }
                }

                if let Err(e) = Self::validate_jwt_token(
                    auth_header,
                    jwt_cfg,
                    require_auth,
                    &audit_logger_for_callback,
                    &peer_addr_str_for_callback,
                ) {
                    warn!("TLS WebSocket JWT authentication failed: {}", e);
                    return Err(http::Response::builder()
                        .status(401)
                        .body(Some(format!("Unauthorized: {}", e)))
                        .unwrap());
                }
            }

            Ok(response)
        };

        let ws_stream = accept_hdr_async(MaybeTlsStream::NativeTls(tls_stream), callback)
            .await
            .map_err(|e| TransportError::Internal(format!("WebSocket handshake failed: {}", e)))?;

        // Create session after successful handshake
        if let (Some(ref jwt_cfg), Some(ref sess_mgr)) =
            (&jwt_config_for_session, &session_manager_clone)
        {
            let auth_header_value = auth_header_captured.read().await.clone();
            if let Some(auth_header) = auth_header_value.as_deref() {
                match Self::validate_jwt_token_and_create_session(
                    Some(auth_header),
                    jwt_cfg,
                    require_auth,
                    &audit_logger_for_session,
                    &Some(Arc::clone(sess_mgr)),
                    &rate_limiter_for_session,
                    &peer_addr_str,
                    &peer_ip_for_session,
                )
                .await
                {
                    Ok(Some((_claims, Some(session_id)))) => {
                        info!(
                            "Session created for TLS WebSocket connection: {}",
                            session_id
                        );
                        // Store session_id -> peer_addr mapping
                        session_connections
                            .write()
                            .await
                            .insert(SessionId::from(session_id.clone()), peer_addr);
                        debug!(
                            "Stored TLS session mapping: {} -> {}",
                            session_id, peer_addr
                        );
                    }
                    Ok(_) => {
                        debug!("No session created for TLS connection (auth not required or session management disabled)");
                    }
                    Err(e) => {
                        warn!("Failed to create session after TLS handshake: {}", e);
                        // Connection is already established, just log the error
                    }
                }
            }
        }

        Ok(ws_stream)
    }

    /// Start WebSocket client
    async fn start_client(&self) -> Result<(), TransportError> {
        *self.state.write().await = ConnectionState::Connecting;

        let url = &self.config.url;
        debug!(
            "Connecting to WebSocket server: {} (TLS: {})",
            url, self.config.use_tls
        );

        let timeout_duration = Duration::from_secs(self.config.timeout_seconds.unwrap_or(30));

        let ws_stream = if self.config.use_tls {
            // TLS client connection
            timeout(
                timeout_duration,
                Self::connect_tls(url, &self.config, &self.audit_logger),
            )
            .await
            .map_err(|_| {
                TransportError::Timeout(format!(
                    "WebSocket TLS connection timeout after {:?}",
                    timeout_duration
                ))
            })?
            .map_err(|e| TransportError::Internal(format!("WebSocket TLS connect error: {}", e)))?
        } else {
            // Plain WebSocket connection
            let connect_future = connect_async(url);
            timeout(timeout_duration, connect_future)
                .await
                .map_err(|_| {
                    TransportError::Timeout(format!(
                        "WebSocket connection timeout after {:?}",
                        timeout_duration
                    ))
                })?
                .map_err(|e| TransportError::Internal(format!("WebSocket connect error: {}", e)))?
                .0
        };

        info!("WebSocket client connected to: {}", url);
        *self.client_connection.write().await = Some(ws_stream);
        *self.state.write().await = ConnectionState::Connected;

        Ok(())
    }

    /// Connect to TLS WebSocket server (client mode)
    async fn connect_tls(
        url: &str,
        config: &WebSocketConfig,
        audit_logger: &Option<Arc<AuditLogger>>,
    ) -> Result<WsStream, TransportError> {
        use native_tls::TlsConnector;
        use std::fs;
        use tokio_native_tls::TlsConnector as TokioTlsConnector;

        let tls_config = config.tls_config.as_ref();

        // Build TLS connector
        let mut builder = TlsConnector::builder();

        // Configure certificate verification
        if let Some(tls_cfg) = tls_config {
            if tls_cfg.accept_invalid_certs {
                warn!("TLS certificate verification disabled - for testing only!");

                // Log security warning for accepting invalid certificates
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        "WebSocket client configured to accept invalid TLS certificates - SECURITY RISK".to_string(),
                    )
                    .add_metadata("url".to_string(), url.to_string())
                    .add_metadata("security_risk".to_string(), "high".to_string());

                    let _ = logger.log(entry).await;
                }

                builder.danger_accept_invalid_certs(true);
            }

            if !tls_cfg.verify_server {
                warn!("TLS server verification disabled");

                // Log security warning for disabled server verification
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Warning,
                        AuditCategory::SecurityAttack,
                        "WebSocket client configured to skip hostname verification - SECURITY RISK"
                            .to_string(),
                    )
                    .add_metadata("url".to_string(), url.to_string());

                    let _ = logger.log(entry).await;
                }

                builder.danger_accept_invalid_hostnames(true);
            }

            // Add CA certificate if provided
            if let Some(ca_path) = &tls_cfg.ca_cert_path {
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::ConfigChange,
                        "Loading custom CA certificate for WebSocket TLS".to_string(),
                    )
                    .add_metadata("ca_path".to_string(), ca_path.display().to_string());

                    let _ = logger.log(entry).await;
                }

                let ca_cert = fs::read(ca_path).map_err(|e| {
                    TransportError::Configuration(format!("Failed to read CA certificate: {}", e))
                })?;

                let ca_cert = native_tls::Certificate::from_pem(&ca_cert).map_err(|e| {
                    TransportError::Configuration(format!("Failed to parse CA certificate: {}", e))
                })?;

                builder.add_root_certificate(ca_cert);
            }
        }

        let connector = builder.build().map_err(|e| {
            TransportError::Configuration(format!("Failed to build TLS connector: {}", e))
        })?;

        let connector = TokioTlsConnector::from(connector);

        // Parse URL to extract host
        let url_parsed = url::Url::parse(url)
            .map_err(|e| TransportError::Configuration(format!("Invalid WebSocket URL: {}", e)))?;

        let host = url_parsed
            .host_str()
            .ok_or_else(|| TransportError::Configuration("No host in WebSocket URL".to_string()))?;

        // Connect via TLS
        let stream = TcpStream::connect(format!("{}:{}", host, url_parsed.port().unwrap_or(443)))
            .await
            .map_err(|e| TransportError::Internal(format!("TCP connect failed: {}", e)))?;

        let tls_stream = match connector.connect(host, stream).await {
            Ok(stream) => {
                // Log successful TLS connection
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::NetworkActivity,
                        "WebSocket TLS client connection established".to_string(),
                    )
                    .add_metadata("url".to_string(), url.to_string())
                    .add_metadata("host".to_string(), host.to_string());

                    let _ = logger.log(entry).await;
                }
                stream
            }
            Err(e) => {
                // Log failed TLS connection
                if let Some(logger) = audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Error,
                        AuditCategory::Error,
                        "WebSocket TLS client connection failed".to_string(),
                    )
                    .add_metadata("url".to_string(), url.to_string())
                    .add_metadata("error".to_string(), e.to_string());

                    let _ = logger.log(entry).await;
                }
                return Err(TransportError::Internal(format!(
                    "TLS connect failed: {}",
                    e
                )));
            }
        };

        // Perform WebSocket handshake
        let (ws_stream, _response) =
            tokio_tungstenite::client_async(url, MaybeTlsStream::NativeTls(tls_stream))
                .await
                .map_err(|e| {
                    TransportError::Internal(format!("WebSocket handshake failed: {}", e))
                })?;

        Ok(ws_stream)
    }

    /// Start message processing loop
    async fn start_message_loop(&self) -> Result<(), TransportError> {
        let connection = if self.config.server_mode {
            Arc::clone(&self.server_connection)
        } else {
            Arc::clone(&self.client_connection)
        };

        let outgoing_rx = Arc::clone(&self.outgoing_rx);
        let incoming_tx = self.incoming_tx.clone();
        let state = Arc::clone(&self.state);
        let messages_sent = Arc::clone(&self.messages_sent);
        let messages_received = Arc::clone(&self.messages_received);
        let bytes_sent = Arc::clone(&self.bytes_sent);
        let bytes_received = Arc::clone(&self.bytes_received);
        let config = self.config.clone();
        let _rate_limiter = self.rate_limiter.clone();
        let _audit_logger = self.audit_logger.clone();

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        tokio::spawn(async move {
            let mut heartbeat_interval_timer =
                interval(Duration::from_secs(config.heartbeat_interval));

            loop {
                let mut conn_guard = connection.write().await;
                let ws_stream = match conn_guard.as_mut() {
                    Some(stream) => stream,
                    None => {
                        debug!("No active WebSocket connection");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };

                tokio::select! {
                    // Handle incoming WebSocket messages
                    msg = ws_stream.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                debug!("Received text message: {} bytes", text.len());
                                *bytes_received.write().await += text.len() as u64;

                                match serde_json::from_str::<JsonRpcRequest>(&text) {
                                    Ok(request) => {
                                        *messages_received.write().await += 1;
                                        if let Err(e) = incoming_tx.send(request) {
                                            error!("Failed to queue incoming message: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse JSON-RPC request: {}", e);
                                    }
                                }
                            }
                            Some(Ok(Message::Binary(data))) => {
                                debug!("Received binary message: {} bytes", data.len());
                                *bytes_received.write().await += data.len() as u64;
                            }
                            Some(Ok(Message::Ping(data))) => {
                                debug!("Received ping, sending pong");
                                if let Err(e) = ws_stream.send(Message::Pong(data)).await {
                                    error!("Failed to send pong: {}", e);
                                }
                            }
                            Some(Ok(Message::Pong(_))) => {
                                debug!("Received pong");
                            }
                            Some(Ok(Message::Close(frame))) => {
                                info!("WebSocket close frame received: {:?}", frame);
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            Some(Err(e)) => {
                                error!("WebSocket error: {}", e);
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            None => {
                                info!("WebSocket connection closed");
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            _ => {}
                        }
                    }

                    // Handle outgoing messages
                    msg = async {
                        let mut rx = outgoing_rx.write().await;
                        rx.recv().await
                    } => {
                        if let Some(response) = msg {
                            match serde_json::to_string(&response) {
                                Ok(json) => {
                                    let msg_size = json.len();
                                    if let Err(e) = ws_stream.send(Message::Text(json.into())).await {
                                        error!("Failed to send message: {}", e);
                                        *state.write().await = ConnectionState::Disconnected;
                                        break;
                                    } else {
                                        *messages_sent.write().await += 1;
                                        *bytes_sent.write().await += msg_size as u64;
                                        debug!("Sent message: {} bytes", msg_size);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to serialize response: {}", e);
                                }
                            }
                        }
                    }

                    // Heartbeat
                    _ = heartbeat_interval_timer.tick(), if config.heartbeat_interval > 0 => {
                        debug!("Sending heartbeat ping");
                        if let Err(e) = ws_stream.send(Message::Ping(vec![].into())).await {
                            error!("Failed to send ping: {}", e);
                            *state.write().await = ConnectionState::Disconnected;
                            break;
                        }
                    }

                    // Shutdown signal
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown signal received");
                        *state.write().await = ConnectionState::ShuttingDown;
                        let _ = ws_stream.send(Message::Close(None)).await;
                        break;
                    }
                }
            }

            info!("WebSocket message loop terminated");
        });

        Ok(())
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    type Error = TransportError;

    async fn start(&mut self) -> std::result::Result<(), Self::Error> {
        info!(
            "Starting WebSocket transport in {} mode",
            if self.config.server_mode {
                "server"
            } else {
                "client"
            }
        );

        if self.config.server_mode {
            self.start_server().await?;
            // Wait for first connection
            for _ in 0..50 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if *self.state.read().await == ConnectionState::Connected {
                    break;
                }
            }
        } else {
            self.start_client().await?;
        }

        // Start message processing loop
        self.start_message_loop().await?;

        info!("WebSocket transport started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        info!("Stopping WebSocket transport");

        *self.state.write().await = ConnectionState::ShuttingDown;

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(()).await;
        }

        // Wait for graceful shutdown
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Clean up session mappings and invalidate sessions
        if let Some(ref session_mgr) = self.session_manager {
            let mut sessions = self.session_connections.write().await;
            for (session_id, peer_addr) in sessions.iter() {
                // Delete session from SessionManager
                if let Err(e) = session_mgr.delete_session(session_id).await {
                    warn!(
                        "Failed to delete session {} for {}: {}",
                        session_id, peer_addr, e
                    );
                } else {
                    info!("Session {} invalidated on connection close", session_id);
                }

                // Log session cleanup to audit log
                if let Some(ref logger) = self.audit_logger {
                    use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
                    let entry = AuditLogEntry::new(
                        AuditLevel::Info,
                        AuditCategory::NetworkActivity,
                        format!("Session invalidated on WebSocket disconnect: {}", session_id),
                    )
                    .with_request_info(peer_addr.to_string(), String::new())
                    .add_metadata("session_id".to_string(), session_id.to_string());

                    let logger_clone = Arc::clone(logger);
                    let entry_clone = entry;
                    tokio::spawn(async move {
                        let _ = logger_clone.log(entry_clone).await;
                    });
                }
            }
            sessions.clear();
            debug!("All session mappings cleared");
        }

        // Close connections
        *self.client_connection.write().await = None;
        *self.server_connection.write().await = None;

        *self.state.write().await = ConnectionState::Disconnected;

        info!("WebSocket transport stopped");
        Ok(())
    }

    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error> {
        if *self.state.read().await != ConnectionState::Connected {
            return Err(TransportError::Internal(
                "WebSocket not connected".to_string(),
            ));
        }

        self.outgoing_tx.send(message).map_err(|e| {
            TransportError::Internal(format!("Failed to queue outgoing message: {}", e))
        })?;

        Ok(())
    }

    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        if *self.state.read().await == ConnectionState::Disconnected {
            return Err(TransportError::Internal(
                "WebSocket not connected".to_string(),
            ));
        }

        // Non-blocking receive with timeout
        match timeout(
            Duration::from_millis(100),
            self.incoming_rx.write().await.recv(),
        )
        .await
        {
            Ok(Some(request)) => Ok(Some(request)),
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // Timeout - no message available
        }
    }

    fn is_connected(&self) -> bool {
        // This is sync, so we can't await. Use try_read for non-blocking access
        match self.state.try_read() {
            Ok(state) => *state == ConnectionState::Connected,
            Err(_) => false,
        }
    }

    fn transport_info(&self) -> crate::transport::TransportInfo {
        use crate::transport::{
            FramingMethod, TransportCapabilities, TransportInfo, TransportType,
        };

        TransportInfo {
            transport_type: TransportType::WebSocket {
                url: self.config.url.clone(),
            },
            description: format!(
                "WebSocket transport ({} mode, TLS: {})",
                if self.config.server_mode {
                    "server"
                } else {
                    "client"
                },
                self.config.use_tls
            ),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: false,
                max_message_size: Some(self.config.max_message_size),
                framing_methods: vec![FramingMethod::WebSocketFrame],
            },
        }
    }

    fn connection_stats(&self) -> crate::transport::ConnectionStats {
        use crate::transport::ConnectionStats;

        // Try to read stats without blocking
        let messages_sent = self.messages_sent.try_read().map(|v| *v).unwrap_or(0);
        let messages_received = self.messages_received.try_read().map(|v| *v).unwrap_or(0);
        let bytes_sent = self.bytes_sent.try_read().map(|v| *v).unwrap_or(0);
        let bytes_received = self.bytes_received.try_read().map(|v| *v).unwrap_or(0);

        ConnectionStats {
            messages_sent,
            messages_received,
            bytes_sent,
            bytes_received,
            uptime: Duration::from_secs(0),
            last_activity: None,
        }
    }
}
