//! Kubernetes Resource Creation
//!
//! Deployment and Service creation logic

use super::crd::{MCPServer, MCPServerSpec};
use super::types::{OperatorError, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{
    Container, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use std::collections::BTreeMap;

/// Create a Deployment for an MCPServer
pub fn create_mcpserver_deployment(mcp: &MCPServer) -> Result<Deployment> {
    let name = mcp.name_any();
    let spec = &mcp.spec;

    let mut labels = BTreeMap::new();
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

/// Create a Service for an MCPServer
pub fn create_mcpserver_service(mcp: &MCPServer) -> Result<Service> {
    let name = mcp.name_any();
    let spec = &mcp.spec;

    let mut labels = BTreeMap::new();
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
