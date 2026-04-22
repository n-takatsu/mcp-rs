//! HTTP Transport implementation for MCP-RS
//!
//! This module provides HTTP-based JSON-RPC transport for MCP communication.

use crate::{
    error::{Error, Result},
    security::NetworkPolicy,
    transport::{
        ConnectionStats, Transport, TransportCapabilities, TransportError, TransportInfo,
        TransportType,
    },
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
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
    net::SocketAddr,
    sync::Arc,
    time::Instant,
};
use tokio::{net::TcpListener, sync::RwLock};
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};

/// HTTP Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Bind address for HTTP server
    pub bind_addr: SocketAddr,
    /// Enable CORS
    pub cors_enabled: bool,
    /// Maximum request size in bytes
    pub max_request_size: usize,
    /// Request timeout in milliseconds
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

/// Shared state for HTTP transport
#[derive(Clone)]
struct HttpTransportState {
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
}

/// HTTP Transport implementation
#[derive(Debug)]
pub struct HttpTransport {
    config: HttpConfig,
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<String>>>,
    sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
    bound_addr: Arc<RwLock<Option<SocketAddr>>>,
}

impl HttpTransport {
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
            bound_addr: Arc::new(RwLock::new(None)),
        })
    }

    /// Start the HTTP server
    pub async fn start_server(&self) -> Result<()> {
        // Validate bind address
        self.config
            .network_policy
            .validate_bind_address(&self.config.bind_addr)
            .map_err(|e| Error::TransportError(TransportError::Configuration(e.to_string())))?;

        validate_tls_security_settings(&self.config)
            .map_err(|e| Error::TransportError(TransportError::Configuration(e)))?;

        let pinned_certificates_sha256: HashSet<String> = self
            .config
            .pinned_certificates_sha256
            .iter()
            .filter_map(|fingerprint| normalize_fingerprint(fingerprint))
            .collect();

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
        };

        let app = Router::new()
            .route("/", post(handle_jsonrpc_request))
            .route("/mcp", post(handle_jsonrpc_request))
            .layer(if self.config.cors_enabled {
                CorsLayer::permissive()
            } else {
                CorsLayer::new()
            })
            .with_state(state);

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
            .map_err(|e| Error::Internal(format!("Failed to bind HTTP server: {}", e)))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| Error::Internal(format!("Failed to get bound HTTP address: {}", e)))?;
        *self.bound_addr.write().await = Some(local_addr);

        if self.config.tls_enabled {
            let tls_acceptor = build_tls_acceptor(&self.config, &pinned_certificates_sha256)?;
            tokio::spawn(async move {
                serve_tls(listener, app, tls_acceptor).await;
            });
        } else {
            tokio::spawn(async move {
                if let Err(e) = axum::serve(listener, app).await {
                    error!("HTTP server error: {}", e);
                }
            });
        }

        Ok(())
    }

    pub async fn bound_addr(&self) -> Option<SocketAddr> {
        *self.bound_addr.read().await
    }
}

async fn serve_tls(listener: TcpListener, app: Router, tls_acceptor: TlsAcceptor) {
    let mut make_service = app.into_make_service_with_connect_info::<SocketAddr>();

    loop {
        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(connection) => connection,
            Err(e) => {
                error!("HTTPS accept error: {}", e);
                continue;
            }
        };

        let tls_acceptor = tls_acceptor.clone();
        let tower_service = match make_service.call(remote_addr).await {
            Ok(service) => service,
            Err(e) => {
                error!("Failed to create HTTPS service for {}: {}", remote_addr, e);
                continue;
            }
        };

        tokio::spawn(async move {
            let tls_stream = match tls_acceptor.accept(tcp_stream).await {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("TLS handshake failed for {}: {}", remote_addr, e);
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);
            let builder = HyperConnectionBuilder::new(TokioExecutor::new());
            let hyper_service = TowerToHyperService::new(tower_service);

            if let Err(e) = builder
                .serve_connection_with_upgrades(io, hyper_service)
                .await
            {
                error!("HTTPS connection error for {}: {}", remote_addr, e);
            }
        });
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
                Error::Internal(format!("Invalid mTLS client verifier configuration: {}", e))
            })?;

        RustlsServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certificates, private_key)
            .map_err(|e| Error::Internal(format!("Invalid TLS certificate configuration: {}", e)))?
    } else {
        RustlsServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)
            .map_err(|e| Error::Internal(format!("Invalid TLS certificate configuration: {}", e)))?
    };
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

fn load_tls_certificates(config: &HttpConfig) -> Result<Vec<CertificateDer<'static>>> {
    let cert_path = config.tls_cert_path.as_deref().ok_or_else(|| {
        Error::Internal("tls_cert_path is required when tls_enabled=true".to_string())
    })?;

    let cert_file = File::open(cert_path).map_err(|e| {
        Error::Internal(format!(
            "Failed to open TLS certificate {}: {}",
            cert_path, e
        ))
    })?;
    let mut cert_reader = BufReader::new(cert_file);
    let certificates = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Internal(format!("Failed to parse TLS certificates: {}", e)))?;

    if certificates.is_empty() {
        return Err(Error::Internal(
            "TLS certificate chain is empty".to_string(),
        ));
    }

    Ok(certificates)
}

fn load_tls_private_key(config: &HttpConfig) -> Result<PrivateKeyDer<'static>> {
    let key_path = config.tls_key_path.as_deref().ok_or_else(|| {
        Error::Internal("tls_key_path is required when tls_enabled=true".to_string())
    })?;

    let key_file = File::open(key_path).map_err(|e| {
        Error::Internal(format!(
            "Failed to open TLS private key {}: {}",
            key_path, e
        ))
    })?;
    let mut key_reader = BufReader::new(key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| Error::Internal(format!("Failed to parse TLS private key: {}", e)))?
        .ok_or_else(|| Error::Internal("TLS private key not found in key file".to_string()))?;

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
        TransportInfo {
            transport_type: TransportType::Http {
                addr: self.config.bind_addr,
            },
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
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> std::result::Result<impl IntoResponse, StatusCode> {
    // Validate connection
    if let Err(e) = state.network_policy.validate_connection(&remote_addr) {
        error!("Connection rejected from {}: {}", remote_addr, e);
        return Err(StatusCode::FORBIDDEN);
    }

    if state.enforce_https && !state.tls_terminated_locally && !is_forwarded_https(&headers) {
        warn!(
            "Rejected non-HTTPS request from {} due to enforce_https policy",
            remote_addr
        );
        return Err(StatusCode::UPGRADE_REQUIRED);
    }

    if let Some(min_tls_version) = state.min_tls_version.as_deref() {
        if let Err(reason) = validate_forwarded_tls_version(
            &headers,
            min_tls_version,
            state.enforce_https && !state.tls_terminated_locally,
        ) {
            warn!(
                "Rejected request from {} due to TLS metadata validation failure: {}",
                remote_addr, reason
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if state.certificate_pinning_enabled {
        let Some(actual_fingerprint_raw) = header_value(&headers, &state.certificate_pin_header)
        else {
            warn!(
                "Rejected request from {} due to missing certificate pin header {}",
                remote_addr, state.certificate_pin_header
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

        if !state
            .pinned_certificates_sha256
            .contains(actual_fingerprint.as_str())
        {
            warn!(
                "Rejected request from {} due to certificate pin mismatch",
                remote_addr
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

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
    let request_str = serde_json::to_string(&request).map_err(|_| {
        // Update error stats
        let mut stats = state.stats.blocking_write();
        stats.total_errors += 1;
        StatusCode::BAD_REQUEST
    })?;

    state.request_sender.send(request_str).await.map_err(|_| {
        // Update error stats
        let mut stats = state.stats.blocking_write();
        stats.total_errors += 1;
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // If request has ID, wait for actual response
    if let Some(id) = request_id {
        // Create oneshot channel for response
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // Register pending response
        {
            let mut pending = state.pending_responses.write().await;
            pending.insert(id.clone(), response_tx);
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

                    Ok((response_headers(&state), Json(response_value)))
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
        // Notification (no ID) - return immediate acknowledgment
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "status": "accepted",
                "message": "Notification received"
            }
        });

        let response_size = serde_json::to_string(&response).unwrap_or_default().len() as u64;
        {
            let mut stats = state.stats.write().await;
            stats.total_responses += 1;
            stats.total_bytes_sent += response_size;
        }

        Ok((response_headers(&state), Json(response)))
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
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use tokio::net::TcpStream;
    use tokio::time::{sleep, Duration, Instant};

    async fn wait_for_server_ready(bind_addr: SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            match TcpStream::connect(bind_addr).await {
                Ok(stream) => {
                    drop(stream);
                    return;
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::AddrNotAvailable
                            | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(err) => {
                    panic!("failed to probe listener readiness for {bind_addr}: {err}");
                }
            }

            if Instant::now() >= deadline {
                panic!("server did not start listening on {bind_addr} within timeout");
            }

            sleep(Duration::from_millis(50)).await;
        }
    }

    fn reqwest_error_has_io_kind(error: &reqwest::Error, kind: std::io::ErrorKind) -> bool {
        let mut source = std::error::Error::source(error);
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
        wait_for_server_ready(server_addr).await;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
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
        wait_for_server_ready(server_addr).await;

        let root_ca = reqwest::Certificate::from_pem(ca_cert.pem().as_bytes()).unwrap();
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
        wait_for_server_ready(server_addr).await;

        let root_ca = reqwest::Certificate::from_pem(ca_cert.pem().as_bytes()).unwrap();
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
        wait_for_server_ready(server_addr).await;

        let root_ca = reqwest::Certificate::from_pem(trusted_ca_cert.pem().as_bytes()).unwrap();
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
}
