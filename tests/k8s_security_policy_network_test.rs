//! Integration test proving the operator actually reconciles
//! `SecurityPolicy.spec.networkPolicy` into a real
//! `networking.k8s.io/v1 NetworkPolicy` object, and removes it again when
//! the `SecurityPolicy` is deleted (Issue #299).
//!
//! Requires a real Kubernetes cluster reachable via the default kubeconfig
//! (e.g. `kind create cluster`), with `k8s/crds/securitypolicy-crd.yaml`
//! applied. Soft-skips (prints a message and returns) when no cluster is
//! reachable, since CI has no cluster available.

#![cfg(feature = "kubernetes-operator")]

use k8s_openapi::api::networking::v1::NetworkPolicy as K8sNetworkPolicy;
use kube::api::{DeleteParams, ListParams, PostParams};
use kube::{Api, Client, Resource, ResourceExt};
use mcp_rs::operator::{
    reconcile_security_policy, Context, NetworkPolicyConfig, SecurityPolicy, SecurityPolicySpec,
};
use std::sync::Arc;

async fn try_connect() -> Option<Client> {
    // kube's Client uses rustls, which requires an explicit process-level
    // CryptoProvider when more than one backend (aws-lc-rs/ring) is reachable
    // in the dependency graph. Matches the existing pattern in
    // src/transport/http.rs. Safe to call more than once across tests.
    let _ = rustls::crypto::ring::default_provider().install_default();

    match Client::try_default().await {
        Ok(client) => {
            // Confirm the SecurityPolicy CRD is actually installed; if not,
            // this cluster isn't set up for this test either.
            let api: Api<SecurityPolicy> = Api::namespaced(client.clone(), "default");
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

fn test_spec() -> SecurityPolicySpec {
    SecurityPolicySpec {
        enabled: true,
        authentication: None,
        authorization: None,
        rate_limiting: None,
        network_policy: Some(NetworkPolicyConfig {
            allowed_ports: Some(vec![443]),
            blocked_ips: Some(vec!["10.0.0.5".to_string()]),
            allowed_cidrs: Some(vec!["10.0.0.0/8".to_string()]),
        }),
        encryption: None,
        audit: None,
        threat_intelligence: None,
    }
}

#[tokio::test]
async fn security_policy_reconciles_and_cleans_up_a_real_network_policy() {
    let Some(client) = try_connect().await else {
        return;
    };

    let ns = "default";
    let name = "mcp-rs-issue-299-test-policy";
    let ctx = Arc::new(Context::new(client.clone()));

    let policies: Api<SecurityPolicy> = Api::namespaced(client.clone(), ns);
    let network_policies: Api<K8sNetworkPolicy> = Api::namespaced(client.clone(), ns);

    // Clean up any leftovers from a previous failed run before starting.
    let _ = policies.delete(name, &DeleteParams::default()).await;
    let _ = network_policies
        .delete(name, &DeleteParams::default())
        .await;

    // 1. Create the SecurityPolicy CR for real.
    let mut policy = SecurityPolicy::new(name, test_spec());
    policy.metadata.namespace = Some(ns.to_string());
    let created = policies
        .create(&PostParams::default(), &policy)
        .await
        .expect("failed to create SecurityPolicy - is the CRD applied?");

    // 2. Reconcile (Apply path). `kube::runtime::finalizer` only *adds* the
    //    finalizer on the first call for an object that doesn't have it yet
    //    (it does not invoke the Apply callback in that same call — a real
    //    watch-based Controller would notice the resulting update and
    //    reconcile again). Simulate that second pass explicitly here.
    reconcile_security_policy(Arc::new(created), ctx.clone())
        .await
        .expect("reconcile (add finalizer) failed");
    let with_finalizer = policies
        .get(name)
        .await
        .expect("object should still exist after the finalizer was added");
    assert!(
        !with_finalizer.finalizers().is_empty(),
        "expected the finalizer to have been added by the first reconcile"
    );

    reconcile_security_policy(Arc::new(with_finalizer), ctx.clone())
        .await
        .expect("reconcile (apply) failed");

    let np = network_policies
        .get(name)
        .await
        .expect("expected a networking.k8s.io/v1 NetworkPolicy to have been created");
    let spec = np.spec.expect("NetworkPolicy should have a spec");
    assert_eq!(
        spec.policy_types.as_deref(),
        Some(&["Egress".to_string()][..])
    );
    let egress = spec.egress.expect("expected an egress rule");
    assert_eq!(egress.len(), 1);
    let ports = egress[0].ports.as_ref().expect("expected ports");
    assert_eq!(ports.len(), 1);
    let to = egress[0].to.as_ref().expect("expected peers");
    let ip_block = to[0].ip_block.as_ref().expect("expected an ipBlock peer");
    assert_eq!(ip_block.cidr, "10.0.0.0/8");
    assert_eq!(
        ip_block.except.as_deref(),
        Some(&["10.0.0.5/32".to_string()][..])
    );

    // 3. Delete the SecurityPolicy CR, then reconcile again to drive the
    //    finalizer's Cleanup branch (the object still exists at this point,
    //    with a deletionTimestamp set, until the finalizer is removed).
    policies
        .delete(name, &DeleteParams::default())
        .await
        .expect("failed to delete SecurityPolicy");
    let pending_delete = policies
        .get(name)
        .await
        .expect("object should still exist pending finalizer cleanup");
    assert!(
        pending_delete.meta().deletion_timestamp.is_some(),
        "expected a deletionTimestamp while the finalizer is still present"
    );

    reconcile_security_policy(Arc::new(pending_delete), ctx.clone())
        .await
        .expect("reconcile (cleanup) failed");

    // 4. Both the NetworkPolicy and the SecurityPolicy CR itself should now
    //    be gone (the finalizer's removal lets Kubernetes finish deleting it).
    assert!(
        network_policies.get(name).await.is_err(),
        "NetworkPolicy should have been deleted on SecurityPolicy cleanup"
    );
    assert!(
        policies.get(name).await.is_err(),
        "SecurityPolicy should be fully deleted once its finalizer is removed"
    );
}

#[tokio::test]
async fn security_policy_without_network_policy_creates_no_network_policy() {
    let Some(client) = try_connect().await else {
        return;
    };

    let ns = "default";
    let name = "mcp-rs-issue-299-test-policy-no-netpol";
    let ctx = Arc::new(Context::new(client.clone()));

    let policies: Api<SecurityPolicy> = Api::namespaced(client.clone(), ns);
    let network_policies: Api<K8sNetworkPolicy> = Api::namespaced(client.clone(), ns);

    let _ = policies.delete(name, &DeleteParams::default()).await;
    let _ = network_policies
        .delete(name, &DeleteParams::default())
        .await;

    let spec = SecurityPolicySpec {
        enabled: true,
        authentication: None,
        authorization: None,
        rate_limiting: None,
        network_policy: None,
        encryption: None,
        audit: None,
        threat_intelligence: None,
    };
    let mut policy = SecurityPolicy::new(name, spec);
    policy.metadata.namespace = Some(ns.to_string());
    let created = policies
        .create(&PostParams::default(), &policy)
        .await
        .expect("failed to create SecurityPolicy");

    // First reconcile only adds the finalizer (see comment in the other
    // test); the second actually runs the Apply path.
    reconcile_security_policy(Arc::new(created), ctx.clone())
        .await
        .expect("reconcile (add finalizer) failed");
    let with_finalizer = policies.get(name).await.expect("should still exist");

    reconcile_security_policy(Arc::new(with_finalizer), ctx.clone())
        .await
        .expect("reconcile (apply) failed");

    assert!(
        network_policies.get(name).await.is_err(),
        "no NetworkPolicy should be created when spec.network_policy is None"
    );

    // Cleanup
    policies
        .delete(name, &DeleteParams::default())
        .await
        .expect("failed to delete SecurityPolicy");
    let pending_delete = policies.get(name).await.expect("should still exist");
    reconcile_security_policy(Arc::new(pending_delete), ctx.clone())
        .await
        .expect("reconcile (cleanup) failed");
}
