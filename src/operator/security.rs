//! Security Policy Controller
//!
//! Kubernetes controller for SecurityPolicy resources

use super::crd::{SecurityPolicy, SecurityPolicyStatus};
use super::types::{Context, OperatorError, Result};
use kube::api::{Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::{Api, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Reconciliation logic for SecurityPolicy resources
pub async fn reconcile_security_policy(
    policy: Arc<SecurityPolicy>,
    ctx: Arc<Context>,
) -> Result<Action> {
    let ns = policy
        .namespace()
        .ok_or_else(|| OperatorError::InvalidSpec("namespace required".to_string()))?;
    let name = policy.name_any();

    info!("Reconciling SecurityPolicy {}/{}", ns, name);

    // Security policy enforcement logic would go here
    let policies: Api<SecurityPolicy> = Api::namespaced(ctx.client.clone(), &ns);

    let status = SecurityPolicyStatus {
        phase: Some("Active".to_string()),
        ..Default::default()
    };

    let patch = serde_json::json!({
        "status": status
    });

    policies
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}
