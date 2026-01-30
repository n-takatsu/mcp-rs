//! Kubernetes Operator for MCP Plugin Management
//!
//! This module implements the Kubernetes operator that manages the lifecycle
//! of MCP plugins running in a Kubernetes cluster.

use crate::error::McpError;
use crate::k8s::{PluginDeployment, PluginPolicy};
use crate::plugin_isolation::IsolatedPluginManager;
use kube::runtime::Controller;
use kube::{Api, Client, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Configuration for the Kubernetes operator
#[derive(Debug, Clone)]
pub struct OperatorConfig {
    /// Namespace to watch for plugin deployments
    pub namespace: String,
    
    /// Reconciliation interval in seconds
    pub reconcile_interval_secs: u64,
    
    /// Error policy: retry backoff in seconds
    pub error_backoff_secs: u64,
    
    /// Maximum number of concurrent reconciliations
    pub max_concurrent_reconciles: usize,
    
    /// Default container registry for plugin images
    pub default_registry: String,
}

impl Default for OperatorConfig {
    fn default() -> Self {
        Self {
            namespace: "default".to_string(),
            reconcile_interval_secs: 300, // 5 minutes
            error_backoff_secs: 60,        // 1 minute
            max_concurrent_reconciles: 10,
            default_registry: "ghcr.io/n-takatsu/mcp-rs".to_string(),
        }
    }
}

/// PluginOperator manages MCP plugins in a Kubernetes cluster
pub struct PluginOperator {
    client: Client,
    config: OperatorConfig,
    plugin_manager: Arc<RwLock<IsolatedPluginManager>>,
}

impl PluginOperator {
    /// Create a new PluginOperator instance
    pub async fn new(
        config: OperatorConfig,
        plugin_manager: Arc<RwLock<IsolatedPluginManager>>,
    ) -> Result<Self, McpError> {
        let client = Client::try_default()
            .await
            .map_err(|e| McpError::KubernetesError(format!("Failed to create Kubernetes client: {}", e)))?;

        Ok(Self {
            client,
            config,
            plugin_manager,
        })
    }

    /// Start the operator controller
    pub async fn run(self: Arc<Self>) -> Result<(), McpError> {
        info!("Starting MCP Plugin Operator in namespace: {}", self.config.namespace);

        // Create API clients for our CRDs
        let plugin_deployments: Api<PluginDeployment> = if self.config.namespace == "all" {
            Api::all(self.client.clone())
        } else {
            Api::namespaced(self.client.clone(), &self.config.namespace)
        };

        let plugin_policies: Api<PluginPolicy> = if self.config.namespace == "all" {
            Api::all(self.client.clone())
        } else {
            Api::namespaced(self.client.clone(), &self.config.namespace)
        };

        // Create controllers for each CRD
        let deployment_controller = Controller::new(
            plugin_deployments.clone(),
            Default::default(),
        )
        .run(
            move |deployment, ctx| {
                let operator = ctx.clone();
                async move {
                    operator.reconcile_deployment(deployment).await
                }
            },
            |deployment, error, _ctx| {
                error!(
                    plugin = %deployment.name_any(),
                    error = %error,
                    "Reconciliation failed for PluginDeployment"
                );
                std::result::Result::Ok(kube::runtime::controller::Action::requeue(
                    Duration::from_secs(60),
                ))
            },
            self.clone(),
        );

        let policy_controller = Controller::new(
            plugin_policies.clone(),
            Default::default(),
        )
        .run(
            move |policy, ctx| {
                let operator = ctx.clone();
                async move {
                    operator.reconcile_policy(policy).await
                }
            },
            |policy, error, _ctx| {
                error!(
                    policy = %policy.name_any(),
                    error = %error,
                    "Reconciliation failed for PluginPolicy"
                );
                std::result::Result::Ok(kube::runtime::controller::Action::requeue(
                    Duration::from_secs(60),
                ))
            },
            self.clone(),
        );

        // Run both controllers concurrently
        tokio::select! {
            result = deployment_controller => {
                error!("PluginDeployment controller stopped: {:?}", result);
            }
            result = policy_controller => {
                error!("PluginPolicy controller stopped: {:?}", result);
            }
        }

        Ok(())
    }

    /// Reconcile a PluginDeployment resource
    async fn reconcile_deployment(
        &self,
        deployment: PluginDeployment,
    ) -> Result<kube::runtime::controller::Action, McpError> {
        let name = deployment.name_any();
        let namespace = deployment.namespace().unwrap_or_else(|| "default".to_string());
        
        info!(
            plugin = %name,
            namespace = %namespace,
            "Reconciling PluginDeployment"
        );

        let spec = &deployment.spec;
        
        // Validate the deployment specification
        self.validate_deployment_spec(spec)?;

        // Get or create the plugin deployment
        let mut pm = self.plugin_manager.write().await;
        
        match pm.get_plugin(&spec.plugin_id).await {
            Ok(_plugin) => {
                // Plugin exists, check if update is needed
                info!(plugin_id = %spec.plugin_id, "Plugin already exists, checking for updates");
                // TODO: Implement update logic
            }
            Err(_) => {
                // Plugin doesn't exist, create it
                info!(plugin_id = %spec.plugin_id, "Creating new plugin deployment");
                
                // Create plugin configuration
                let config = serde_json::to_value(&spec.config).unwrap_or_default();
                
                // Create the plugin
                pm.create_plugin(
                    &spec.plugin_id,
                    &spec.image,
                    config,
                    Some(crate::plugin_isolation::ResourceLimits {
                        max_cpu_percent: spec.resource_limits.max_cpu_percent,
                        max_memory_mb: spec.resource_limits.max_memory_mb,
                        max_disk_io_bps: spec.resource_limits.max_disk_iops.unwrap_or(10000),
                    }),
                )
                .await?;
            }
        }

        // Update the status
        self.update_deployment_status(&deployment, &namespace).await?;

        // Requeue after the reconciliation interval
        Ok(kube::runtime::controller::Action::requeue(
            Duration::from_secs(self.config.reconcile_interval_secs),
        ))
    }

    /// Reconcile a PluginPolicy resource
    async fn reconcile_policy(
        &self,
        policy: PluginPolicy,
    ) -> Result<kube::runtime::controller::Action, McpError> {
        let name = policy.name_any();
        let namespace = policy.namespace().unwrap_or_else(|| "default".to_string());
        
        info!(
            policy = %name,
            namespace = %namespace,
            "Reconciling PluginPolicy"
        );

        let spec = &policy.spec;
        
        // Apply the policy to matching plugins
        let mut affected_count = 0;
        let pm = self.plugin_manager.read().await;
        
        // Find plugins that match the selector
        if let Some(plugin_id) = &spec.plugin_selector.plugin_id {
            if let Ok(_plugin) = pm.get_plugin(plugin_id).await {
                info!(plugin_id = %plugin_id, "Applying policy to plugin");
                // TODO: Implement policy application logic
                affected_count += 1;
            }
        }

        // Update the policy status
        self.update_policy_status(&policy, &namespace, affected_count).await?;

        // Requeue after the reconciliation interval
        Ok(kube::runtime::controller::Action::requeue(
            Duration::from_secs(self.config.reconcile_interval_secs),
        ))
    }

    /// Validate a PluginDeployment specification
    fn validate_deployment_spec(
        &self,
        spec: &crate::k8s::crd::PluginDeploymentSpec,
    ) -> Result<(), McpError> {
        // Validate plugin ID
        if spec.plugin_id.is_empty() {
            return Err(McpError::ValidationError("plugin_id cannot be empty".to_string()));
        }

        // Validate replicas
        if spec.replicas < 0 {
            return Err(McpError::ValidationError(
                "replicas must be non-negative".to_string(),
            ));
        }

        // Validate resource limits
        if spec.resource_limits.max_cpu_percent <= 0.0 || spec.resource_limits.max_cpu_percent > 100.0 {
            return Err(McpError::ValidationError(
                "max_cpu_percent must be between 0 and 100".to_string(),
            ));
        }

        if spec.resource_limits.max_memory_mb == 0 {
            return Err(McpError::ValidationError(
                "max_memory_mb must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Update the status of a PluginDeployment
    async fn update_deployment_status(
        &self,
        deployment: &PluginDeployment,
        namespace: &str,
    ) -> Result<(), McpError> {
        let name = deployment.name_any();
        
        info!(
            plugin = %name,
            namespace = %namespace,
            "Updating PluginDeployment status"
        );

        // TODO: Implement status update using Kubernetes API
        // This would typically involve:
        // 1. Getting the current status from the plugin manager
        // 2. Creating a status object
        // 3. Updating the CRD status via the Kubernetes API

        Ok(())
    }

    /// Update the status of a PluginPolicy
    async fn update_policy_status(
        &self,
        policy: &PluginPolicy,
        namespace: &str,
        affected_count: i32,
    ) -> Result<(), McpError> {
        let name = policy.name_any();
        
        info!(
            policy = %name,
            namespace = %namespace,
            affected_plugins = affected_count,
            "Updating PluginPolicy status"
        );

        // TODO: Implement status update using Kubernetes API

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_operator_config_default() {
        let config = OperatorConfig::default();
        assert_eq!(config.namespace, "default");
        assert_eq!(config.reconcile_interval_secs, 300);
    }

    #[test]
    fn test_validate_deployment_spec() {
        // This test would require mocking the operator
        // For now, it's a placeholder
    }
}
