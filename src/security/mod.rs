pub mod anti_replay;
pub mod audit;
pub mod audit_log;
pub mod auth;
pub mod credential_tool;
pub mod encryption;
#[cfg(feature = "hsm")]
pub mod hsm;
pub mod ids;
pub mod image_scanner;
pub mod mtls;
pub mod network_policy;
pub mod rate_limiter;
pub mod secure_server;
pub mod sql_injection_protection;
pub mod tls_enforcement;
pub mod validation;
pub mod waf;
pub mod xss_protection;

pub use anti_replay::{
    AntiReplayConfig, AntiReplayMiddleware, DeviceFingerprint, DeviceFingerprintManager,
    NonceManager, ReplayError, SecurityHeaders, TimestampValidator,
};
pub use audit::{
    Alert as AuditAlert, AlertSeverity, AlertStatus, AnalysisResult, AnalysisStatistics,
    AuditAnalysisEngine, CorrelatedEvent, ExfiltrationEvent, PrivilegeEscalationEvent,
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
pub use image_scanner::{ImageScanner, ScanReport, ScanSummary, ScannerType, Vulnerability};
pub use mtls::{
    CaConfig, Certificate, CertificateAuthority, CertificateManager, CertificateManagerConfig,
    CertificateRequest, CertificateStatistics, CertificateStatus, CertificateStore,
    ExtendedKeyUsage, IssuedCertificate, KeyAlgorithm, KeyUsage, OcspConfig, OcspResponder,
    OcspResponse, OcspStatus, RevocationReason, RotationConfig, RotationEvent, RotationScheduler,
    RotationStatus, StoreConfig, Subject, VerificationResult,
};
pub use network_policy::{NetworkPolicy, NetworkPolicyError};
pub use rate_limiter::RateLimiter;
pub use secure_server::{SecureMcpServer, SecurityConfig, SecurityMetrics};
pub use sql_injection_protection::{
    SqlInjectionDetection, SqlInjectionPattern, SqlInjectionProtector, SqlInjectionStats,
    SqlProtectionConfig, SqlQueryAnalysis, SqlQueryType, ThreatLevel,
};
pub use validation::{
    InputValidator, ValidationResult, ValidationRule, ValidationRuleType, ValidationStats,
};
pub use waf::{
    CorsConfig, CorsHandler, CspConfig, CspGenerator, CspViolation, FileUploadConfig, HstsConfig,
    RequestLimitsConfig, RequestValidator, SecurityHeaderManager, SecurityHeadersConfig, WafConfig,
    WafError, WebApplicationFirewall,
};
pub use xss_protection::{
    XssAttackType, XssDetectionResult, XssProtectionConfig, XssProtector, XssStatistics,
    XssThreatLevel,
};
