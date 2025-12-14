//! MCPServer Controller
//!
//! Kubernetes controller for MCPServer resources

use super::crd::MCPServer;
use super::resources::{create_mcpserver_deployment, create_mcpserver_service};
use super::types::{Context, OperatorError, Result, FINALIZER_NAME};
use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Service;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{finalizer, Event as Finalizer};
use kube::runtime::watcher::Config as WatcherConfig;
use kube::{Api, Client, ResourceExt};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Main reconciliation logic for MCPServer resources
pub async fn reconcile_mcpserver(mcp: Arc<MCPServer>, ctx: Arc<Context>) -> Result<Action> {
    let ns = mcp
        .namespace()
        .ok_or_else(|| OperatorError::InvalidSpec("namespace required".to_string()))?;
    let name = mcp.name_any();

    info!("Reconciling MCPServer {}/{}", ns, name);

    let mcps: Api<MCPServer> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(&mcps, FINALIZER_NAME, mcp, |event| async {
        match event {
            Finalizer::Apply(mcp) => reconcile_mcpserver_apply(mcp, ctx.clone()).await,
            Finalizer::Cleanup(mcp) => reconcile_mcpserver_cleanup(mcp, ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| OperatorError::ReconcileError(e.to_string()))
}

async fn reconcile_mcpserver_apply(mcp: Arc<MCPServer>, ctx: Arc<Context>) -> Result<Action> {
    let ns = mcp.namespace().unwrap();
    let name = mcp.name_any();

    // Create or update Deployment
    let deployment = create_mcpserver_deployment(&mcp)?;
    let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), &ns);

    match deployments.get(&name).await {
        Ok(_) => {
            debug!("Updating existing Deployment {}/{}", ns, name);
            deployments
                .patch(
                    &name,
                    &PatchParams::apply("mcp-operator"),
                    &Patch::Apply(&deployment),
                )
                .await?;
        }
        Err(_) => {
            debug!("Creating new Deployment {}/{}", ns, name);
            deployments
                .create(&PostParams::default(), &deployment)
                .await?;
        }
    }

    // Create or update Service
    let service = create_mcpserver_service(&mcp)?;
    let services: Api<Service> = Api::namespaced(ctx.client.clone(), &ns);

    match services.get(&name).await {
        Ok(_) => {
            debug!("Updating existing Service {}/{}", ns, name);
            services
                .patch(
                    &name,
                    &PatchParams::apply("mcp-operator"),
                    &Patch::Apply(&service),
                )
                .await?;
        }
        Err(_) => {
            debug!("Creating new Service {}/{}", ns, name);
            services.create(&PostParams::default(), &service).await?;
        }
    }

    // Update status
    update_mcpserver_status(&mcp, &ctx, "Running").await?;

    info!("Successfully reconciled MCPServer {}/{}", ns, name);
    Ok(Action::requeue(Duration::from_secs(300)))
}

async fn reconcile_mcpserver_cleanup(mcp: Arc<MCPServer>, ctx: Arc<Context>) -> Result<Action> {
    let ns = mcp.namespace().unwrap();
    let name = mcp.name_any();

    info!("Cleaning up MCPServer {}/{}", ns, name);

    // Delete Deployment
    let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), &ns);
    let _ = deployments.delete(&name, &DeleteParams::default()).await;

    // Delete Service
    let services: Api<Service> = Api::namespaced(ctx.client.clone(), &ns);
    let _ = services.delete(&name, &DeleteParams::default()).await;

    Ok(Action::await_change())
}

async fn update_mcpserver_status(mcp: &MCPServer, ctx: &Context, phase: &str) -> Result<()> {
    let ns = mcp.namespace().unwrap();
    let name = mcp.name_any();
    let mcps: Api<MCPServer> = Api::namespaced(ctx.client.clone(), &ns);

    let mut status = mcp.status.clone().unwrap_or_default();
    status.phase = Some(phase.to_string());

    let patch = serde_json::json!({
        "status": status
    });

    mcps.patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;

    Ok(())
}

/// Error handler for the controller
fn error_policy(_mcp: Arc<MCPServer>, error: &OperatorError, _ctx: Arc<Context>) -> Action {
    error!("Reconciliation error: {:?}", error);
    Action::requeue(Duration::from_secs(60))
}

/// Start the MCPServer controller
pub async fn run_mcpserver_controller(client: Client) -> Result<()> {
    let mcps: Api<MCPServer> = Api::all(client.clone());
    let ctx = Arc::new(Context::new(client.clone()));

    info!("Starting MCPServer controller");

    Controller::new(mcps, WatcherConfig::default())
        .run(reconcile_mcpserver, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok((obj, _action)) => {
                    debug!("Reconciled {:?}", obj);
                }
                Err(e) => {
                    warn!("Reconciliation failed: {:?}", e);
                }
            }
        })
        .await;

    Ok(())
}
