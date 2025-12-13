use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{
    Container, Pod, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{DeleteParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{finalizer, Event as Finalizer};
use kube::runtime::watcher::Config as WatcherConfig;
use kube::{Api, Client, Resource, ResourceExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

mod crd;
pub use crd::{
    MCPServer, MCPServerSpec, MCPServerStatus, Plugin, PluginSpec, PluginStatus, SecurityPolicy,
    SecurityPolicySpec, SecurityPolicyStatus,
};

const FINALIZER_NAME: &str = "mcp.n-takatsu.dev/finalizer";

#[derive(Error, Debug)]
pub enum OperatorError {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid resource spec: {0}")]
    InvalidSpec(String),

    #[error("Reconciliation failed: {0}")]
    ReconcileError(String),
}

type Result<T, E = OperatorError> = std::result::Result<T, E>;

/// Context data for the controller
pub struct Context {
    pub client: Client,
}

impl Context {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

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

fn create_mcpserver_deployment(mcp: &MCPServer) -> Result<Deployment> {
    let name = mcp.name_any();
    let spec = &mcp.spec;

    let mut labels = std::collections::BTreeMap::new();
    labels.insert("app".to_string(), name.clone());
    labels.insert("managed-by".to_string(), "mcp-operator".to_string());

    // Create container
    let mut container = Container {
        name: "mcp-server".to_string(),
        image: Some(spec.image.clone()),
        ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
            container_port: spec.port,
            protocol: Some("TCP".to_string()),
            ..Default::default()
        }]),
        env: spec.env.as_ref().map(|envs| {
            envs.iter()
                .map(|e| k8s_openapi::api::core::v1::EnvVar {
                    name: e.name.clone(),
                    value: Some(e.value.clone()),
                    ..Default::default()
                })
                .collect()
        }),
        resources: spec.resources.as_ref().map(|r| {
            k8s_openapi::api::core::v1::ResourceRequirements {
                limits: r.limits.as_ref().map(|l| {
                    l.iter()
                        .map(|(k, v)| (k.clone(), Quantity(v.clone())))
                        .collect()
                }),
                requests: r.requests.as_ref().map(|req| {
                    req.iter()
                        .map(|(k, v)| (k.clone(), Quantity(v.clone())))
                        .collect()
                }),
                ..Default::default()
            }
        }),
        ..Default::default()
    };

    // Add transport-specific args
    container.args = Some(vec!["--transport".to_string(), spec.transport.clone()]);

    if spec.transport == "http" || spec.transport == "websocket" {
        if let Some(args) = container.args.as_mut() {
            args.push("--port".to_string());
            args.push(spec.port.to_string());
        }
    }

    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::apps::v1::DeploymentSpec {
            replicas: Some(spec.replicas),
            selector: k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                match_labels: Some(labels.clone()),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![container],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    Ok(deployment)
}

fn create_mcpserver_service(mcp: &MCPServer) -> Result<Service> {
    let name = mcp.name_any();
    let spec = &mcp.spec;

    let mut labels = std::collections::BTreeMap::new();
    labels.insert("app".to_string(), name.clone());
    labels.insert("managed-by".to_string(), "mcp-operator".to_string());

    let service = Service {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(labels),
            ports: Some(vec![ServicePort {
                port: spec.port,
                target_port: Some(
                    k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(spec.port),
                ),
                protocol: Some("TCP".to_string()),
                ..Default::default()
            }]),
            type_: Some("ClusterIP".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    Ok(service)
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

// Plugin controller (simplified version)
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

// SecurityPolicy controller (simplified version)
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
