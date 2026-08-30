//! Kubernetes Resource Creation
//!
//! Deployment and Service creation logic

use super::crd::{MCPServer, MCPServerSpec, SecurityPolicy};
use super::types::{OperatorError, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{
    Container, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::api::networking::v1::{
    IPBlock, NetworkPolicy as K8sNetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyPeer,
    NetworkPolicyPort, NetworkPolicySpec as K8sNetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
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

/// `SecurityPolicy.spec.network_policy` から実際の
/// `networking.k8s.io/v1 NetworkPolicy` リソースを構築する。
/// `network_policy` が未設定の場合は作成対象がないため `None` を返す。
///
/// マッピング方針:
/// - `podSelector` は空にする（`SecurityPolicySpec` に対象Pod選択用の
///   フィールドが無いため、名前空間内の全Podに適用される）
/// - `policyTypes` は `Egress` のみ（`allowed_ports`/`blocked_ips`/
///   `allowed_cidrs` はいずれも送信方向の制御に対応するフィールドのため）
/// - `allowed_cidrs` が指定されていればそのCIDR群への通信のみ許可する。
///   未指定の場合は `0.0.0.0/0`（全て）を許可した上で `blocked_ips` を
///   `except` として除外する（「特定IPだけ拒否」という設定意図に合わせる）
/// - `allowed_ports` が指定されていればそのポートのみ許可、未指定なら
///   ポート制限なし（全ポート許可）
pub fn create_security_policy_network_policy(
    policy: &SecurityPolicy,
) -> Result<Option<K8sNetworkPolicy>> {
    let Some(config) = &policy.spec.network_policy else {
        return Ok(None);
    };

    let name = policy.name_any();

    let base_cidrs = config
        .allowed_cidrs
        .clone()
        .unwrap_or_else(|| vec!["0.0.0.0/0".to_string()]);

    let except: Option<Vec<String>> = config
        .blocked_ips
        .as_ref()
        .map(|ips| ips.iter().map(|ip| to_cidr(ip)).collect());

    let to: Vec<NetworkPolicyPeer> = base_cidrs
        .into_iter()
        .map(|cidr| NetworkPolicyPeer {
            ip_block: Some(IPBlock {
                cidr,
                except: except.clone(),
            }),
            ..Default::default()
        })
        .collect();

    let ports = config.allowed_ports.as_ref().map(|ports| {
        ports
            .iter()
            .map(|p| NetworkPolicyPort {
                port: Some(IntOrString::Int(*p)),
                protocol: Some("TCP".to_string()),
                ..Default::default()
            })
            .collect()
    });

    let network_policy = K8sNetworkPolicy {
        metadata: ObjectMeta {
            name: Some(name),
            ..Default::default()
        },
        spec: Some(K8sNetworkPolicySpec {
            pod_selector: LabelSelector::default(),
            policy_types: Some(vec!["Egress".to_string()]),
            egress: Some(vec![NetworkPolicyEgressRule {
                to: Some(to),
                ports,
            }]),
            ingress: None,
        }),
    };

    Ok(Some(network_policy))
}

/// IPアドレス文字列をCIDR表記に変換する（既にCIDRならそのまま）。
fn to_cidr(ip_or_cidr: &str) -> String {
    if ip_or_cidr.contains('/') {
        ip_or_cidr.to_string()
    } else if ip_or_cidr.parse::<std::net::Ipv4Addr>().is_ok() {
        format!("{}/32", ip_or_cidr)
    } else {
        format!("{}/128", ip_or_cidr)
    }
}
