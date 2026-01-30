//! Custom Resource Definitions for MCP Kubernetes Operator
//!
//! This module defines the CRDs used by the MCP Kubernetes operator:
//! - PluginDeployment: Manages plugin lifecycle and deployment
//! - PluginPolicy: Defines security policies for plugins

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ResourceLimits defines the resource constraints for a plugin
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "mcp-rs.io",
    version = "v1",
    kind = "PluginDeployment",
    namespaced
)]
#[kube(status = "PluginDeploymentStatus")]
#[serde(rename_all = "camelCase")]
pub struct PluginDeploymentSpec {
    /// Unique identifier for the plugin
    pub plugin_id: String,
    
    /// Container image to use for the plugin
    pub image: String,
    
    /// Number of replicas to deploy
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    
    /// Resource limits for the plugin
    pub resource_limits: ResourceLimits,
    
    /// Environment variables to set in the plugin container
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// Plugin configuration as JSON
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    
    /// Auto-scaling configuration
    #[serde(default)]
    pub auto_scaling: Option<AutoScaling>,
    
    /// Health check configuration
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
}

fn default_replicas() -> i32 {
    1
}

/// ResourceLimits specifies CPU, memory, and disk constraints
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    /// Maximum CPU percentage (0-100)
    pub max_cpu_percent: f32,
    
    /// Maximum memory in megabytes
    pub max_memory_mb: u64,
    
    /// Maximum disk I/O operations per second
    #[serde(default)]
    pub max_disk_iops: Option<u64>,
    
    /// Network bandwidth limit in Mbps
    #[serde(default)]
    pub max_network_mbps: Option<u64>,
}

/// AutoScaling configuration for horizontal pod autoscaling
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoScaling {
    /// Minimum number of replicas
    pub min_replicas: i32,
    
    /// Maximum number of replicas
    pub max_replicas: i32,
    
    /// Target CPU utilization percentage
    pub target_cpu_utilization_percentage: i32,
    
    /// Target memory utilization percentage
    #[serde(default)]
    pub target_memory_utilization_percentage: Option<i32>,
}

/// HealthCheck configuration for plugin health monitoring
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    /// HTTP path for health checks
    pub path: String,
    
    /// Port for health checks
    pub port: u16,
    
    /// Initial delay in seconds before starting health checks
    #[serde(default = "default_initial_delay")]
    pub initial_delay_seconds: u32,
    
    /// Period in seconds between health checks
    #[serde(default = "default_period")]
    pub period_seconds: u32,
    
    /// Timeout in seconds for each health check
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    
    /// Number of consecutive failures before marking unhealthy
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
}

fn default_initial_delay() -> u32 {
    30
}

fn default_period() -> u32 {
    10
}

fn default_timeout() -> u32 {
    5
}

fn default_failure_threshold() -> u32 {
    3
}

/// PluginDeploymentStatus tracks the current state of the plugin deployment
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDeploymentStatus {
    /// Current phase of the deployment
    pub phase: DeploymentPhase,
    
    /// Number of ready replicas
    pub ready_replicas: i32,
    
    /// Total number of replicas
    pub total_replicas: i32,
    
    /// Last update timestamp
    pub last_update_time: String,
    
    /// Conditions describing the deployment state
    #[serde(default)]
    pub conditions: Vec<DeploymentCondition>,
    
    /// Status message
    #[serde(default)]
    pub message: Option<String>,
}

/// DeploymentPhase represents the current state of a plugin deployment
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
pub enum DeploymentPhase {
    /// Deployment is being created
    Pending,
    
    /// Deployment is progressing
    Progressing,
    
    /// Deployment is running successfully
    Running,
    
    /// Deployment has failed
    Failed,
    
    /// Deployment is being terminated
    Terminating,
}

/// DeploymentCondition describes a condition of the deployment
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentCondition {
    /// Type of condition
    #[serde(rename = "type")]
    pub condition_type: String,
    
    /// Status of the condition (True, False, Unknown)
    pub status: String,
    
    /// Last time the condition transitioned
    pub last_transition_time: String,
    
    /// Reason for the condition's last transition
    pub reason: String,
    
    /// Human-readable message
    pub message: String,
}

/// PluginPolicy defines security and runtime policies for plugins
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "mcp-rs.io",
    version = "v1",
    kind = "PluginPolicy",
    namespaced
)]
#[kube(status = "PluginPolicyStatus")]
#[serde(rename_all = "camelCase")]
pub struct PluginPolicySpec {
    /// Plugin ID or pattern this policy applies to
    pub plugin_selector: PluginSelector,
    
    /// Network access rules
    pub network_policy: NetworkPolicy,
    
    /// Security context settings
    pub security_context: SecurityContext,
    
    /// Allowed API endpoints
    #[serde(default)]
    pub allowed_apis: Vec<String>,
    
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
}

/// PluginSelector defines which plugins a policy applies to
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginSelector {
    /// Match by plugin ID (exact match)
    #[serde(default)]
    pub plugin_id: Option<String>,
    
    /// Match by label selectors
    #[serde(default)]
    pub match_labels: HashMap<String, String>,
}

/// NetworkPolicy defines network access rules for plugins
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicy {
    /// List of allowed egress destinations
    #[serde(default)]
    pub allowed_egress: Vec<String>,
    
    /// List of blocked domains
    #[serde(default)]
    pub blocked_domains: Vec<String>,
    
    /// Whether to allow all egress traffic
    #[serde(default)]
    pub allow_all_egress: bool,
}

/// SecurityContext defines security settings for plugin containers
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityContext {
    /// Run as non-root user
    #[serde(default = "default_run_as_non_root")]
    pub run_as_non_root: bool,
    
    /// Read-only root filesystem
    #[serde(default = "default_read_only_root_filesystem")]
    pub read_only_root_filesystem: bool,
    
    /// Drop all Linux capabilities
    #[serde(default = "default_drop_all_capabilities")]
    pub drop_all_capabilities: bool,
    
    /// Additional capabilities to add
    #[serde(default)]
    pub add_capabilities: Vec<String>,
    
    /// SELinux context
    #[serde(default)]
    pub selinux_options: Option<SELinuxOptions>,
    
    /// Seccomp profile
    #[serde(default)]
    pub seccomp_profile: Option<SeccompProfile>,
}

fn default_run_as_non_root() -> bool {
    true
}

fn default_read_only_root_filesystem() -> bool {
    true
}

fn default_drop_all_capabilities() -> bool {
    true
}

/// SELinux options for the security context
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SELinuxOptions {
    /// SELinux level
    pub level: Option<String>,
    
    /// SELinux role
    pub role: Option<String>,
    
    /// SELinux type
    #[serde(rename = "type")]
    pub selinux_type: Option<String>,
    
    /// SELinux user
    pub user: Option<String>,
}

/// Seccomp profile configuration
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeccompProfile {
    /// Type of seccomp profile (RuntimeDefault, Localhost, Unconfined)
    #[serde(rename = "type")]
    pub profile_type: String,
    
    /// Path to the seccomp profile file (for Localhost type)
    pub localhost_profile: Option<String>,
}

/// RateLimit configuration for API calls
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// Maximum requests per second
    pub requests_per_second: u32,
    
    /// Burst size
    pub burst: u32,
}

/// PluginPolicyStatus tracks the status of the policy
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginPolicyStatus {
    /// Whether the policy is currently active
    pub active: bool,
    
    /// Number of plugins affected by this policy
    pub affected_plugins: i32,
    
    /// Last update timestamp
    pub last_update_time: String,
    
    /// Status message
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_deployment_spec_deserialization() {
        let yaml = r#"
pluginId: "test-plugin"
image: "mcp-rs/test-plugin:latest"
replicas: 3
resourceLimits:
  maxCpuPercent: 50.0
  maxMemoryMb: 512
env:
  LOG_LEVEL: "info"
"#;
        
        let spec: PluginDeploymentSpec = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(spec.plugin_id, "test-plugin");
        assert_eq!(spec.replicas, 3);
        assert_eq!(spec.resource_limits.max_cpu_percent, 50.0);
    }

    #[test]
    fn test_plugin_policy_spec_deserialization() {
        let yaml = r#"
pluginSelector:
  pluginId: "wordpress-plugin"
networkPolicy:
  allowedEgress:
    - "api.wordpress.org"
  blockedDomains:
    - "malicious.com"
  allowAllEgress: false
securityContext:
  runAsNonRoot: true
  readOnlyRootFilesystem: true
  dropAllCapabilities: true
"#;
        
        let spec: PluginPolicySpec = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(spec.plugin_selector.plugin_id, Some("wordpress-plugin".to_string()));
        assert_eq!(spec.network_policy.allowed_egress.len(), 1);
    }
}
