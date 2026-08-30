//! Proves that `[transport.http]` network-policy keys in a TOML config file
//! actually flow through `McpConfig::to_transport_config()` into the real
//! `HttpConfig.network_policy` used at runtime (Issue #256) — closing the
//! "configured values match observed behavior" gap, since previously
//! `to_transport_config()` hardcoded `NetworkPolicy::default()` regardless
//! of what was in any TOML file.

use mcp_rs::config::McpConfig;

#[test]
fn toml_network_policy_keys_flow_into_http_config() {
    let toml_str = r#"
        [server]
        stdio = true

        [transport]
        transport_type = "http"

        [transport.http]
        addr = "127.0.0.1"
        port = 8081
        network_policy_reject_external_connections = false
        network_policy_warn_on_external_bind = false
        network_policy_ip_whitelist = ["10.0.0.0/8"]

        [handlers]
    "#;

    let mcp_config: McpConfig = toml::from_str(toml_str).expect("valid TOML");
    let transport_config = mcp_config.to_transport_config();
    let policy = &transport_config.http.network_policy;

    assert!(!policy.reject_external_connections);
    assert!(!policy.warn_on_external_bind);
    assert_eq!(policy.ip_whitelist, vec!["10.0.0.0/8".to_string()]);
}

#[test]
fn omitted_network_policy_keys_keep_secure_defaults() {
    let toml_str = r#"
        [server]
        stdio = true

        [transport]
        transport_type = "http"

        [transport.http]
        addr = "127.0.0.1"
        port = 8081

        [handlers]
    "#;

    let mcp_config: McpConfig = toml::from_str(toml_str).expect("valid TOML");
    let transport_config = mcp_config.to_transport_config();
    let policy = &transport_config.http.network_policy;

    assert!(policy.reject_external_connections);
    assert!(policy.warn_on_external_bind);
    assert!(policy.ip_whitelist.is_empty());
}
