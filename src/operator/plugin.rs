//! Plugin Controller
//!
//! Kubernetes controller for Plugin resources

use super::crd::{Plugin, PluginStatus};
use super::types::{Context, OperatorError, Result};
use kube::api::{Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::{Api, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Reconciliation logic for Plugin resources
pub async fn reconcile_plugin(plugin: Arc<Plugin>, ctx: Arc<Context>) -> Result<Action> {
    let ns = plugin
        .namespace()
        .ok_or_else(|| OperatorError::InvalidSpec("namespace required".to_string()))?;
    let name = plugin.name_any();

    info!("Reconciling Plugin {}/{}", ns, name);

    // Plugin isolation logic would go here
    // For now, just update status
    let plugins: Api<Plugin> = Api::namespaced(ctx.client.clone(), &ns);

    let status = PluginStatus {
        phase: Some("Running".to_string()),
        ..Default::default()
    };

    let patch = serde_json::json!({
        "status": status
    });

    plugins
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}
