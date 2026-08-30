//! Security Policy Controller
//!
//! Kubernetes controller for SecurityPolicy resources

use super::crd::{SecurityPolicy, SecurityPolicyStatus};
use super::resources::create_security_policy_network_policy;
use super::types::{Context, OperatorError, Result, FINALIZER_NAME};
use k8s_openapi::api::networking::v1::NetworkPolicy as K8sNetworkPolicy;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::Action;
use kube::runtime::finalizer::{finalizer, Event as Finalizer};
use kube::{Api, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

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

    let policies: Api<SecurityPolicy> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&policies, FINALIZER_NAME, policy, |event| async {
        match event {
            Finalizer::Apply(policy) => reconcile_security_policy_apply(policy, ctx.clone()).await,
            Finalizer::Cleanup(policy) => {
                reconcile_security_policy_cleanup(policy, ctx.clone()).await
            }
        }
    })
    .await
    .map_err(|e| OperatorError::ReconcileError(e.to_string()))
}

async fn reconcile_security_policy_apply(
    policy: Arc<SecurityPolicy>,
    ctx: Arc<Context>,
) -> Result<Action> {
    let ns = policy.namespace().unwrap();
    let name = policy.name_any();

    // spec.network_policy が設定されている場合のみ、対応する
    // networking.k8s.io/v1 NetworkPolicy を作成・更新する
    if let Some(network_policy) = create_security_policy_network_policy(&policy)? {
        let network_policies: Api<K8sNetworkPolicy> = Api::namespaced(ctx.client.clone(), &ns);

        match network_policies.get(&name).await {
            Ok(_) => {
                debug!("Updating existing NetworkPolicy {}/{}", ns, name);
                network_policies
                    .patch(
                        &name,
                        &PatchParams::apply("mcp-operator"),
                        &Patch::Apply(&network_policy),
                    )
                    .await?;
            }
            Err(_) => {
                debug!("Creating new NetworkPolicy {}/{}", ns, name);
                network_policies
                    .create(&PostParams::default(), &network_policy)
                    .await?;
            }
        }
    }

    update_security_policy_status(&policy, &ctx, "Active").await?;

    info!("Successfully reconciled SecurityPolicy {}/{}", ns, name);
    Ok(Action::requeue(Duration::from_secs(300)))
}

async fn reconcile_security_policy_cleanup(
    policy: Arc<SecurityPolicy>,
    ctx: Arc<Context>,
) -> Result<Action> {
    let ns = policy.namespace().unwrap();
    let name = policy.name_any();

    info!("Cleaning up SecurityPolicy {}/{}", ns, name);

    // network_policy が設定されていなかった場合、対応するNetworkPolicyは
    // 元々作成されていないため、delete は単に no-op になる
    let network_policies: Api<K8sNetworkPolicy> = Api::namespaced(ctx.client.clone(), &ns);
    let _ = network_policies
        .delete(&name, &DeleteParams::default())
        .await;

    Ok(Action::await_change())
}

async fn update_security_policy_status(
    policy: &SecurityPolicy,
    ctx: &Context,
    phase: &str,
) -> Result<()> {
    let ns = policy.namespace().unwrap();
    let name = policy.name_any();
    let policies: Api<SecurityPolicy> = Api::namespaced(ctx.client.clone(), &ns);

    let status = SecurityPolicyStatus {
        phase: Some(phase.to_string()),
        ..Default::default()
    };

    let patch = serde_json::json!({
        "status": status
    });

    policies
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;

    Ok(())
}
