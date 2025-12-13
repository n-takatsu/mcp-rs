use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ==================== MCPServer CRD ====================

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "mcp.n-takatsu.dev",
    version = "v1alpha1",
    kind = "MCPServer",
    plural = "mcpservers",
    shortname = "mcp",
    status = "MCPServerStatus",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct MCPServerSpec {
    /// Number of desired pods
    pub replicas: i32,

    /// Container image name
    #[serde(default = "default_image")]
    pub image: String,

    /// MCP transport protocol
    pub transport: String,

    /// Server port (http/websocket only)
    #[serde(default = "default_port")]
    pub port: i32,

    /// Resource requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,

    /// Reference to SecurityPolicy CR
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_policy: Option<String>,

    /// List of plugins to enable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<String>>,

    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<EnvVar>>,
}

fn default_image() -> String {
    "mcp-rs:latest".to_string()
}

fn default_port() -> i32 {
    3000
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<BTreeMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MCPServerStatus {
    /// Current phase of the MCPServer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// Number of ready pods
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<i32>,

    /// Current service state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,

    /// Last observed generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ==================== Plugin CRD ====================

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "mcp.n-takatsu.dev",
    version = "v1alpha1",
    kind = "Plugin",
    plural = "plugins",
    status = "PluginStatus",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct PluginSpec {
    /// Plugin name
    pub name: String,

    /// Container image for plugin
    pub image: String,

    /// Plugin version
    #[serde(default = "default_version")]
    pub version: String,

    /// Isolation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation: Option<IsolationConfig>,

    /// Resource limits for plugin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,

    /// Plugin-specific configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,

    /// Required plugins
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
}

fn default_version() -> String {
    "latest".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsolationConfig {
    #[serde(default = "default_isolation_level")]
    pub level: String,

    #[serde(default = "default_true")]
    pub network_isolation: bool,

    #[serde(default = "default_true")]
    pub filesystem_isolation: bool,

    #[serde(default = "default_true")]
    pub process_isolation: bool,
}

fn default_isolation_level() -> String {
    "Container".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    /// Current phase of the Plugin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,

    /// Current resource usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_usage: Option<ResourceUsage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_io: Option<String>,
}

// ==================== SecurityPolicy CRD ====================

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "mcp.n-takatsu.dev",
    version = "v1alpha1",
    kind = "SecurityPolicy",
    plural = "securitypolicies",
    shortname = "secpol",
    status = "SecurityPolicyStatus",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicySpec {
    /// Enable security policy enforcement
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Authentication settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationConfig>,

    /// Authorization settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<AuthorizationConfig>,

    /// Rate limiting configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiting: Option<RateLimitingConfig>,

    /// Network security policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<NetworkPolicyConfig>,

    /// Encryption settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<EncryptionConfig>,

    /// Audit logging settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditConfig>,

    /// Threat intelligence integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threat_intelligence: Option<ThreatIntelligenceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationConfig {
    #[serde(default = "default_true")]
    pub required: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationConfig {
    #[serde(default = "default_rbac")]
    pub mode: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_roles: Option<Vec<String>>,
}

fn default_rbac() -> String {
    "rbac".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_rate_limit")]
    pub requests_per_minute: i32,

    #[serde(default = "default_burst_size")]
    pub burst_size: i32,
}

fn default_rate_limit() -> i32 {
    100
}

fn default_burst_size() -> i32 {
    200
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_ports: Option<Vec<i32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_ips: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_cidrs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionConfig {
    #[serde(default = "default_true")]
    pub tls_enabled: bool,

    #[serde(default = "default_tls_version")]
    pub tls_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_secret_name: Option<String>,
}

fn default_tls_version() -> String {
    "1.3".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_retention_days")]
    pub retention_days: i32,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_retention_days() -> i32 {
    30
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreatIntelligenceConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub abuse_ipdb_enabled: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub abuse_ipdb_api_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicyStatus {
    /// Current phase of the SecurityPolicy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// Last time policy was applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,

    /// Number of policy violations detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violations: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
}
