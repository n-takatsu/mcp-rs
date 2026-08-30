//! Verifies that the `networking.k8s.io/v1 NetworkPolicy` objects created by
//! `reconcile_security_policy` (see `tests/k8s_security_policy_network_test.rs`,
//! Issue #299) are actually enforced at the traffic level, not just accepted
//! by the Kubernetes API (Issue #304).
//!
//! `kind`'s default CNI (kindnet) accepts `NetworkPolicy` objects but does not
//! enforce them, so this test requires a cluster with an enforcing CNI, e.g.:
//!
//! ```sh
//! kind create cluster --name mcp-rs-calico --config kind-calico-config.yaml # disableDefaultCNI: true
//! kubectl apply -f https://raw.githubusercontent.com/projectcalico/calico/v3.28.0/manifests/calico.yaml
//! kubectl apply -f k8s/crds/securitypolicy-crd.yaml
//! ```
//!
//! Soft-skips (prints a message and returns) when no cluster is reachable,
//! since CI has no cluster available. Also requires `kubectl` on PATH and a
//! `default` kubeconfig context matching the one `Client::try_default()`
//! picks up, since pod connectivity is probed via `kubectl exec` (real traffic,
//! not just the Kubernetes API) rather than `kube`'s own exec support.

#![cfg(feature = "kubernetes-operator")]

use k8s_openapi::api::core::v1::{Container, Pod, PodSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{DeleteParams, ListParams, PostParams};
use kube::runtime::wait::{await_condition, conditions};
use kube::{Api, Client};
use mcp_rs::operator::{
    reconcile_security_policy, Context, NetworkPolicyConfig, SecurityPolicy, SecurityPolicySpec,
};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const NS: &str = "default";

async fn try_connect() -> Option<Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Client::try_default().await {
        Ok(client) => {
            let api: Api<SecurityPolicy> = Api::namespaced(client.clone(), NS);
            match api.list(&ListParams::default().limit(1)).await {
                Ok(_) => Some(client),
                Err(e) => {
                    println!(
                        "skipping: SecurityPolicy CRD not installed on reachable cluster ({e})"
                    );
                    None
                }
            }
        }
        Err(e) => {
            println!("skipping: no Kubernetes cluster reachable ({e})");
            None
        }
    }
}

fn nginx_pod(name: &str) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(NS.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "nginx".to_string(),
                image: Some("nginx:alpine".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn busybox_pod(name: &str) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(NS.to_string()),
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![Container {
                name: "busybox".to_string(),
                image: Some("busybox:1.36".to_string()),
                command: Some(vec!["sleep".to_string(), "3600".to_string()]),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

async fn wait_running(pods: &Api<Pod>, name: &str) {
    let _ = tokio::time::timeout(
        Duration::from_secs(120),
        await_condition(pods.clone(), name, conditions::is_pod_running()),
    )
    .await
    .unwrap_or_else(|_| panic!("pod {name} did not become Running in time"));
}

/// Runs `wget` for the given target IP from inside `client_pod` via a real
/// `kubectl exec` (actual traffic through the CNI, not the Kubernetes API),
/// returning whether the request succeeded.
fn client_can_reach(client_pod: &str, target_ip: &str) -> bool {
    let output = Command::new("kubectl")
        .args([
            "exec",
            client_pod,
            "-n",
            NS,
            "--",
            "wget",
            "-T",
            "3",
            "-O",
            "/dev/null",
            &format!("http://{target_ip}"),
        ])
        .output()
        .expect("failed to run kubectl exec - is kubectl on PATH?");
    output.status.success()
}

fn security_policy_spec(network_policy: NetworkPolicyConfig) -> SecurityPolicySpec {
    SecurityPolicySpec {
        enabled: true,
        authentication: None,
        authorization: None,
        rate_limiting: None,
        network_policy: Some(network_policy),
        encryption: None,
        audit: None,
        threat_intelligence: None,
    }
}

async fn cleanup(
    policies: &Api<SecurityPolicy>,
    pods: &Api<Pod>,
    policy_name: &str,
    pod_names: &[&str],
) {
    let _ = policies.delete(policy_name, &DeleteParams::default()).await;
    for name in pod_names {
        let _ = pods.delete(name, &DeleteParams::default()).await;
    }
}

#[tokio::test]
async fn network_policy_blocked_ips_actually_blocks_and_restores_traffic() {
    let Some(client) = try_connect().await else {
        return;
    };

    let policy_name = "mcp-rs-issue-304-blocked-ips-test";
    let server_name = "mcp-rs-issue-304-server-a";
    let client_name = "mcp-rs-issue-304-client-a";

    let ctx = Arc::new(Context::new(client.clone()));
    let policies: Api<SecurityPolicy> = Api::namespaced(client.clone(), NS);
    let pods: Api<Pod> = Api::namespaced(client.clone(), NS);

    cleanup(&policies, &pods, policy_name, &[server_name, client_name]).await;

    // 1. Stand up a real server and client pod.
    pods.create(&PostParams::default(), &nginx_pod(server_name))
        .await
        .expect("failed to create server pod");
    pods.create(&PostParams::default(), &busybox_pod(client_name))
        .await
        .expect("failed to create client pod");
    wait_running(&pods, server_name).await;
    wait_running(&pods, client_name).await;

    let server = pods
        .get(server_name)
        .await
        .expect("server pod should exist");
    let server_ip = server
        .status
        .as_ref()
        .and_then(|s| s.pod_ip.clone())
        .expect("server pod should have an IP by the time it's Running");

    // 2. Baseline: without any policy, the client can reach the server.
    assert!(
        client_can_reach(client_name, &server_ip),
        "expected baseline connectivity to work before any NetworkPolicy is applied"
    );

    // 3. Create a SecurityPolicy blocking the server's IP specifically
    //    (default-allow-all except this one IP) and reconcile it into a real
    //    NetworkPolicy (two calls: kube::runtime::finalizer only adds the
    //    finalizer on the first call, see Issue #299).
    let mut policy = SecurityPolicy::new(
        policy_name,
        security_policy_spec(NetworkPolicyConfig {
            allowed_ports: None,
            blocked_ips: Some(vec![server_ip.clone()]),
            allowed_cidrs: None,
        }),
    );
    policy.metadata.namespace = Some(NS.to_string());
    let created = policies
        .create(&PostParams::default(), &policy)
        .await
        .expect("failed to create SecurityPolicy");
    reconcile_security_policy(Arc::new(created), ctx.clone())
        .await
        .expect("reconcile (add finalizer) failed");
    let with_finalizer = policies.get(policy_name).await.expect("should still exist");
    reconcile_security_policy(Arc::new(with_finalizer), ctx.clone())
        .await
        .expect("reconcile (apply) failed");

    // Give the CNI a moment to program the policy.
    tokio::time::sleep(Duration::from_secs(3)).await;

    assert!(
        !client_can_reach(client_name, &server_ip),
        "expected the CNI to actually block traffic to the blocked_ips target, not just accept the NetworkPolicy object"
    );

    // 4. Remove the SecurityPolicy and reconcile cleanup; traffic should be
    //    restored, proving the policy (not something else) was the cause.
    policies
        .delete(policy_name, &DeleteParams::default())
        .await
        .expect("failed to delete SecurityPolicy");
    let pending_delete = policies
        .get(policy_name)
        .await
        .expect("should still exist pending cleanup");
    reconcile_security_policy(Arc::new(pending_delete), ctx.clone())
        .await
        .expect("reconcile (cleanup) failed");

    tokio::time::sleep(Duration::from_secs(3)).await;

    assert!(
        client_can_reach(client_name, &server_ip),
        "expected connectivity to be restored once the SecurityPolicy/NetworkPolicy was removed"
    );

    cleanup(&policies, &pods, policy_name, &[server_name, client_name]).await;
}

#[tokio::test]
async fn network_policy_allowed_cidrs_restricts_egress_to_only_those_ranges() {
    let Some(client) = try_connect().await else {
        return;
    };

    let policy_name = "mcp-rs-issue-304-allowed-cidrs-test";
    let server_name = "mcp-rs-issue-304-server-b";
    let client_name = "mcp-rs-issue-304-client-b";

    let ctx = Arc::new(Context::new(client.clone()));
    let policies: Api<SecurityPolicy> = Api::namespaced(client.clone(), NS);
    let pods: Api<Pod> = Api::namespaced(client.clone(), NS);

    cleanup(&policies, &pods, policy_name, &[server_name, client_name]).await;

    pods.create(&PostParams::default(), &nginx_pod(server_name))
        .await
        .expect("failed to create server pod");
    pods.create(&PostParams::default(), &busybox_pod(client_name))
        .await
        .expect("failed to create client pod");
    wait_running(&pods, server_name).await;
    wait_running(&pods, client_name).await;

    let server = pods
        .get(server_name)
        .await
        .expect("server pod should exist");
    let server_ip = server
        .status
        .as_ref()
        .and_then(|s| s.pod_ip.clone())
        .expect("server pod should have an IP by the time it's Running");

    assert!(
        client_can_reach(client_name, &server_ip),
        "expected baseline connectivity to work before any NetworkPolicy is applied"
    );

    // allowed_cidrs restricted to a range that deliberately does not contain
    // the server's IP: this should block egress to it even though blocked_ips
    // is untouched, proving the allow-list-only branch also enforces for real.
    let mut policy = SecurityPolicy::new(
        policy_name,
        security_policy_spec(NetworkPolicyConfig {
            allowed_ports: None,
            blocked_ips: None,
            allowed_cidrs: Some(vec!["10.99.0.0/16".to_string()]),
        }),
    );
    policy.metadata.namespace = Some(NS.to_string());
    let created = policies
        .create(&PostParams::default(), &policy)
        .await
        .expect("failed to create SecurityPolicy");
    reconcile_security_policy(Arc::new(created), ctx.clone())
        .await
        .expect("reconcile (add finalizer) failed");
    let with_finalizer = policies.get(policy_name).await.expect("should still exist");
    reconcile_security_policy(Arc::new(with_finalizer), ctx.clone())
        .await
        .expect("reconcile (apply) failed");

    tokio::time::sleep(Duration::from_secs(3)).await;

    assert!(
        !client_can_reach(client_name, &server_ip),
        "expected the CNI to block traffic outside the allowed_cidrs range"
    );

    policies
        .delete(policy_name, &DeleteParams::default())
        .await
        .expect("failed to delete SecurityPolicy");
    let pending_delete = policies
        .get(policy_name)
        .await
        .expect("should still exist pending cleanup");
    reconcile_security_policy(Arc::new(pending_delete), ctx.clone())
        .await
        .expect("reconcile (cleanup) failed");

    cleanup(&policies, &pods, policy_name, &[server_name, client_name]).await;
}
