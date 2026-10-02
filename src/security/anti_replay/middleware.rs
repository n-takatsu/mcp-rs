//! Anti-Replay Attack Middleware
//!
//! Validates requests for replay attack prevention using nonce,
//! timestamp, and device fingerprint verification.

use super::device_fingerprint::{DeviceFingerprint, DeviceFingerprintManager};
use super::nonce::NonceManager;
use super::timestamp::TimestampValidator;
use super::types::{AntiReplayConfig, ReplayError, SecurityHeaders};
use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::IpAddr;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Anti-Replay Middleware
#[derive(Clone)]
pub struct AntiReplayMiddleware {
    /// Nonce manager
    nonce_manager: Arc<NonceManager>,

    /// Timestamp validator
    timestamp_validator: Arc<TimestampValidator>,

    /// Device fingerprint manager
    device_manager: Arc<DeviceFingerprintManager>,

    /// Configuration
    config: AntiReplayConfig,
}

impl AntiReplayMiddleware {
    /// Create a new AntiReplayMiddleware
    pub fn new(config: AntiReplayConfig) -> Self {
        let nonce_manager = Arc::new(NonceManager::new(
            config.nonce_ttl_secs,
            config.max_nonce_cache,
        ));

        let timestamp_validator = Arc::new(TimestampValidator::new(config.time_window_secs));

        let device_manager = Arc::new(DeviceFingerprintManager::new(config.max_devices_per_user));

        Self {
            nonce_manager,
            timestamp_validator,
            device_manager,
            config,
        }
    }

    /// Extract security headers from request
    fn extract_security_headers(request: &Request) -> SecurityHeaders {
        let headers = request.headers();

        let nonce = headers
            .get("X-Nonce")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let timestamp = headers
            .get("X-Timestamp")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let device_id = headers
            .get("X-Device-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Extract IP address from request
        let ip_address = request
            .extensions()
            .get::<std::net::SocketAddr>()
            .map(|addr| addr.ip());

        SecurityHeaders {
            nonce,
            timestamp,
            device_id,
            user_agent,
            ip_address,
        }
    }

    /// Validate request against replay attacks
    pub async fn validate_request(
        &self,
        request: &Request,
        user_id: Option<&str>,
    ) -> Result<DeviceFingerprint, ReplayError> {
        let headers = Self::extract_security_headers(request);
        self.validate_headers(&headers, user_id).await
    }

    /// Validate already-extracted security headers against replay attacks.
    ///
    /// This is the transport-agnostic core used by `validate_request` (Axum)
    /// as well as by non-HTTP call sites (WebSocket per-message checks,
    /// plugin broker messages) that have no `axum::extract::Request` to
    /// extract headers from.
    pub async fn validate_headers(
        &self,
        headers: &SecurityHeaders,
        user_id: Option<&str>,
    ) -> Result<DeviceFingerprint, ReplayError> {
        // 1. Validate Nonce (if enabled)
        if self.config.enable_nonce {
            let nonce = headers.nonce.as_ref().ok_or(ReplayError::MissingNonce)?;

            self.nonce_manager
                .validate_and_consume(nonce, user_id.map(|s| s.to_string()))
                .await?;

            debug!("Nonce validated: {}", nonce);
        }

        // 2. Validate Timestamp (if enabled)
        if self.config.enable_timestamp {
            let timestamp_str = headers
                .timestamp
                .as_ref()
                .ok_or(ReplayError::MissingTimestamp)?;

            self.timestamp_validator.validate_timestamp(timestamp_str)?;

            debug!("Timestamp validated: {}", timestamp_str);
        }

        // 3. Create/Verify Device Fingerprint (if enabled)
        let fingerprint = if self.config.enable_device_fingerprint {
            // Get IP address
            let ip_address = headers.ip_address.ok_or_else(|| {
                ReplayError::DeviceVerificationFailed("Missing IP address".to_string())
            })?;

            let fingerprint = DeviceFingerprint::new(
                headers.user_agent.as_deref(),
                ip_address,
                None, // TLS fingerprint can be added later
            );

            // Verify device if user is authenticated
            if let Some(uid) = user_id {
                let is_verified = self
                    .device_manager
                    .verify_device(uid, &fingerprint, self.config.device_match_threshold)
                    .await?;

                if !is_verified {
                    if self.config.allow_new_devices {
                        // Register new device
                        self.device_manager
                            .register_device(uid, fingerprint.clone())
                            .await?;
                        info!(
                            "New device registered for user {}: {}",
                            uid, fingerprint.device_id
                        );
                    } else {
                        warn!(
                            "Unknown device rejected for user {}: {}",
                            uid, fingerprint.device_id
                        );
                        return Err(ReplayError::UnknownDevice);
                    }
                } else {
                    debug!(
                        "Device verified for user {}: {}",
                        uid, fingerprint.device_id
                    );
                }
            }

            fingerprint
        } else {
            // Create a basic fingerprint even if not enforcing
            DeviceFingerprint::new(
                headers.user_agent.as_deref(),
                headers
                    .ip_address
                    .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
                None,
            )
        };

        Ok(fingerprint)
    }

    /// Axum middleware handler
    pub async fn middleware(
        self: Arc<Self>,
        request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        // Extract user ID if available (from extensions set by auth middleware)
        let user_id = request.extensions().get::<String>().map(|s| s.as_str());

        // Validate request
        match self.validate_request(&request, user_id).await {
            Ok(fingerprint) => {
                // Store fingerprint in request extensions for later use
                let mut request = request;
                request.extensions_mut().insert(fingerprint);

                Ok(next.run(request).await)
            }
            Err(e) => {
                warn!("Anti-replay validation failed: {}", e);

                // Return appropriate error response
                let status = match e {
                    ReplayError::MissingNonce | ReplayError::MissingTimestamp => {
                        StatusCode::BAD_REQUEST
                    }
                    ReplayError::NonceAlreadyUsed => StatusCode::CONFLICT,
                    ReplayError::TimestampOutOfWindow
                    | ReplayError::FutureTimestamp
                    | ReplayError::ExpiredTimestamp => StatusCode::REQUEST_TIMEOUT,
                    ReplayError::UnknownDevice | ReplayError::DeviceMismatch(_) => {
                        StatusCode::FORBIDDEN
                    }
                    _ => StatusCode::BAD_REQUEST,
                };

                Err(status)
            }
        }
    }

    /// Get nonce manager reference
    pub fn nonce_manager(&self) -> &Arc<NonceManager> {
        &self.nonce_manager
    }

    /// Get timestamp validator reference
    pub fn timestamp_validator(&self) -> &Arc<TimestampValidator> {
        &self.timestamp_validator
    }

    /// Get device manager reference
    pub fn device_manager(&self) -> &Arc<DeviceFingerprintManager> {
        &self.device_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use chrono::Utc;

    fn create_test_middleware() -> AntiReplayMiddleware {
        let config = AntiReplayConfig::default();
        AntiReplayMiddleware::new(config)
    }

    #[test]
    fn test_extract_security_headers() {
        let request = HttpRequest::builder()
            .header("X-Nonce", "test-nonce-123")
            .header("X-Timestamp", "2026-02-20T10:00:00Z")
            .header("User-Agent", "Mozilla/5.0")
            .body(Body::empty())
            .unwrap();

        let headers = AntiReplayMiddleware::extract_security_headers(&request);

        assert_eq!(headers.nonce, Some("test-nonce-123".to_string()));
        assert_eq!(headers.timestamp, Some("2026-02-20T10:00:00Z".to_string()));
        assert_eq!(headers.user_agent, Some("Mozilla/5.0".to_string()));
    }

    #[tokio::test]
    async fn test_validate_request_with_valid_headers() {
        let middleware = create_test_middleware();
        let nonce = NonceManager::generate_nonce();
        let timestamp = Utc::now().to_rfc3339();

        let request = HttpRequest::builder()
            .header("X-Nonce", &nonce)
            .header("X-Timestamp", &timestamp)
            .header("User-Agent", "Mozilla/5.0")
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_request_missing_nonce() {
        let middleware = create_test_middleware();
        let timestamp = Utc::now().to_rfc3339();

        let request = HttpRequest::builder()
            .header("X-Timestamp", &timestamp)
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request, None).await;
        assert!(matches!(result, Err(ReplayError::MissingNonce)));
    }

    #[tokio::test]
    async fn test_validate_request_missing_timestamp() {
        let middleware = create_test_middleware();
        let nonce = NonceManager::generate_nonce();

        let request = HttpRequest::builder()
            .header("X-Nonce", &nonce)
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request, None).await;
        assert!(matches!(result, Err(ReplayError::MissingTimestamp)));
    }

    #[tokio::test]
    async fn test_validate_request_replay_attack() {
        let middleware = create_test_middleware();
        let nonce = NonceManager::generate_nonce();
        let timestamp = Utc::now().to_rfc3339();

        // First request should succeed
        let request1 = HttpRequest::builder()
            .header("X-Nonce", &nonce)
            .header("X-Timestamp", &timestamp)
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request1, None).await;
        assert!(result.is_ok());

        // Second request with same nonce should fail
        let request2 = HttpRequest::builder()
            .header("X-Nonce", &nonce)
            .header("X-Timestamp", &timestamp)
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request2, None).await;
        assert!(matches!(result, Err(ReplayError::NonceAlreadyUsed)));
    }

    #[tokio::test]
    async fn test_validate_request_expired_timestamp() {
        let middleware = create_test_middleware();
        let nonce = NonceManager::generate_nonce();
        let old_timestamp = (Utc::now() - chrono::Duration::seconds(100)).to_rfc3339();

        let request = HttpRequest::builder()
            .header("X-Nonce", &nonce)
            .header("X-Timestamp", &old_timestamp)
            .body(Body::empty())
            .unwrap();

        let result = middleware.validate_request(&request, None).await;
        assert!(matches!(result, Err(ReplayError::ExpiredTimestamp)));
    }
}
