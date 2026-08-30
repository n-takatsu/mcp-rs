//! Integration tests proving `IsolationEngine` actually cuts off external
//! network access for a plugin container when `use_network_namespace` is
//! enabled, rather than the previous behavior where the underlying Docker
//! command was silently broken and never worked at all (Issue #298).
//!
//! These tests drive a real Docker daemon. Since `windows-latest` GitHub
//! Actions runners default to a Windows-containers daemon (no Linux
//! `alpine` image support), and Docker may simply be unavailable in some
//! environments, each test soft-skips (prints a message and returns) when
//! a Linux-capable Docker daemon isn't reachable, rather than failing.

use mcp_rs::plugin_isolation::isolation_engine::IsolationEngine;
use mcp_rs::plugin_isolation::IsolationConfig;
use std::process::Command;
use uuid::Uuid;

/// Returns true if `docker info` reports a Linux daemon (required for the
/// `alpine:latest` image used by `IsolationEngine::create_container`).
fn linux_docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.OSType}}"])
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "linux")
        .unwrap_or(false)
}

fn test_config(use_network_namespace: bool) -> IsolationConfig {
    IsolationConfig {
        container_runtime: "docker".to_string(),
        use_network_namespace,
        // Filesystem isolation bind-mounts Linux host paths (/usr, /lib)
        // unrelated to this test; keep it off to isolate what we're testing.
        filesystem_isolation: false,
        process_isolation: false,
    }
}

/// Runs `wget` inside the container against a real external host, returning
/// whether it succeeded.
fn container_can_reach_external_host(container_id: &str) -> bool {
    Command::new("docker")
        .args([
            "exec",
            container_id,
            "wget",
            "-q",
            "-T",
            "5",
            "-O",
            "/dev/null",
            "http://example.com",
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn plugin_network_isolation_blocks_external_access() {
    if !linux_docker_available() {
        println!("skipping: no Linux-capable Docker daemon reachable");
        return;
    }

    let engine = IsolationEngine::new(test_config(true)).await.unwrap();
    let plugin_id = Uuid::new_v4();

    let container_id = engine.start_plugin(plugin_id).await.unwrap();
    let reachable = container_can_reach_external_host(&container_id);
    engine
        .stop_plugin(plugin_id, &container_id)
        .await
        .expect("cleanup should succeed even if the assertion below fails");

    assert!(
        !reachable,
        "a plugin container with use_network_namespace=true should not be able to reach external hosts"
    );
}

#[tokio::test]
async fn plugin_network_isolation_allows_external_access_when_disabled() {
    if !linux_docker_available() {
        println!("skipping: no Linux-capable Docker daemon reachable");
        return;
    }

    let engine = IsolationEngine::new(test_config(false)).await.unwrap();
    let plugin_id = Uuid::new_v4();

    let container_id = engine.start_plugin(plugin_id).await.unwrap();
    let reachable = container_can_reach_external_host(&container_id);
    engine
        .stop_plugin(plugin_id, &container_id)
        .await
        .expect("cleanup should succeed even if the assertion below fails");

    assert!(
        reachable,
        "a plugin container with use_network_namespace=false should reach external hosts normally"
    );
}
