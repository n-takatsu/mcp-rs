pub mod anti_replay;
pub mod audit_log;
pub mod auth;
pub mod credential_tool;
pub mod encryption;
#[cfg(feature = "hsm")]
pub mod hsm;
pub mod ids;
pub mod keyring_store;
pub mod keyring_tool;
pub mod network_policy;
pub mod rate_limiter;

pub use anti_replay::{
    AntiReplayConfig, AntiReplayMiddleware, DeviceFingerprint, DeviceFingerprintManager,
    NonceManager, ReplayError, SecurityHeaders, TimestampValidator,
};
pub use audit_log::{
    AuditCategory, AuditConfig, AuditFilter, AuditLevel, AuditLogEntry, AuditLogger,
    AuditStatistics,
};
pub use auth::{
    ApiKey, ApiKeyConfig, ApiKeyManager, ApiKeyPermission, AuthError, AuthMethod, AuthProvider,
    AuthResult, AuthUser, Credentials, PasswordHasher, Permission, Role,
};
pub use encryption::{EncryptedCredentials, EncryptionError, SecureCredentials};
pub use ids::{
    alerts::{AggregatedAlert, Alert, AlertConfig, AlertLevel, AlertManager, NotificationChannel},
    behavioral::BehavioralDetector,
    network::NetworkMonitor,
    signature::SignatureDetector,
    DetectionResult, DetectionType, IDSConfig, IntrusionDetectionSystem, RecommendedAction,
    RequestData, Severity,
};
pub use network_policy::{NetworkPolicy, NetworkPolicyError};
pub use rate_limiter::RateLimiter;
