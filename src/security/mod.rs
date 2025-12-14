pub mod audit;
pub mod audit_log;
pub mod auth;
pub mod encryption;
pub mod ids;
#[cfg(feature = "mfa")]
pub mod mfa;
pub mod rate_limiter;
pub mod secure_server;
pub mod sql_injection_protection;
pub mod tls_enforcement;
pub mod validation;
pub mod waf;
pub mod xss_protection;

pub use audit::{
    Alert as AuditAlert, AlertSeverity, AlertStatus, AnalysisResult, AnalysisStatistics,
    AuditAnalysisEngine, CorrelatedEvent, ExfiltrationEvent, PrivilegeEscalationEvent,
};
pub use audit_log::{
    AuditCategory, AuditConfig, AuditFilter, AuditLevel, AuditLogEntry, AuditLogger,
    AuditStatistics,
};
pub use auth::{
    ApiKey, ApiKeyConfig, ApiKeyManager, ApiKeyPermission, AuthConfig, AuthError, AuthMethod,
    AuthMiddleware, AuthProvider, AuthRequirement, AuthResult, AuthUser, AuthenticationProvider,
    Credentials, JwtAuth, JwtClaims, JwtConfig, JwtTokenPair, MultiAuthProvider, OAuth2Config,
    OAuth2Provider, OAuth2Token, PasswordHasher, Permission, Role, SessionAuth, SessionConfig,
    SessionToken,
};
pub use encryption::{EncryptedCredentials, EncryptionError, SecureCredentials};
pub use ids::{
    alerts::{AggregatedAlert, Alert, AlertConfig, AlertLevel, AlertManager, NotificationChannel},
    behavioral::BehavioralDetector,
    network::NetworkMonitor,
    signature::SignatureDetector,
    DetectionResult, DetectionType, IntrusionDetectionSystem, RecommendedAction, Severity,
};
#[cfg(feature = "mfa")]
pub use mfa::{MfaConfig, MfaError, TotpAlgorithm, TotpConfig, TotpSecret, TotpVerifier};
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
