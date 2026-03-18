//! Network Policy Module
//!
//! このモジュールは、MCPサーバーへのネットワークアクセスを制御します。
//! デフォルトでは、localhostからの接続のみを許可し、外部からのアクセスを拒否します。

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use thiserror::Error;
use tracing::warn;

/// ネットワークポリシー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// 外部接続を拒否するかどうか（デフォルト: true）
    #[serde(default = "default_reject_external")]
    pub reject_external_connections: bool,

    /// 外部IPでのバインド時に警告を出すかどうか（デフォルト: true）
    #[serde(default = "default_warn_external_bind")]
    pub warn_on_external_bind: bool,

    /// 許可するIPアドレスのホワイトリスト
    #[serde(default)]
    pub ip_whitelist: Vec<String>,
}

fn default_reject_external() -> bool {
    true
}

fn default_warn_external_bind() -> bool {
    true
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            reject_external_connections: true,
            warn_on_external_bind: true,
            ip_whitelist: vec![],
        }
    }
}

/// ネットワークポリシーエラー
#[derive(Error, Debug)]
pub enum NetworkPolicyError {
    #[error("External connections are not allowed: {0}")]
    ExternalAccessDenied(String),

    #[error("IP address is not whitelisted: {0}")]
    IpNotWhitelisted(String),
}

impl NetworkPolicy {
    /// 新しいネットワークポリシーを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 接続を検証する
    pub fn validate_connection(&self, remote_addr: &SocketAddr) -> Result<(), NetworkPolicyError> {
        let ip = remote_addr.ip();

        // reject_external_connectionsが有効な場合、loopback以外を拒否
        if self.reject_external_connections && !ip.is_loopback() {
            return Err(NetworkPolicyError::ExternalAccessDenied(format!(
                "Connection from {} is not allowed (only localhost connections are permitted)",
                remote_addr
            )));
        }

        // ホワイトリストが設定されている場合、リストにあるIPのみ許可
        if !self.ip_whitelist.is_empty() && !self.is_ip_whitelisted(&ip) {
            return Err(NetworkPolicyError::IpNotWhitelisted(format!(
                "IP address {} is not in the whitelist",
                ip
            )));
        }

        Ok(())
    }

    /// バインドアドレスを検証する（警告のみ）
    pub fn validate_bind_address(&self, bind_addr: &SocketAddr) -> Result<(), NetworkPolicyError> {
        if self.warn_on_external_bind {
            let ip = bind_addr.ip();
            if !ip.is_loopback() && !(ip.is_unspecified())
            // 0.0.0.0 や ::
            {
                warn!(
                    "⚠️  Server is binding to external address: {}. This may expose the server to network access.",
                    bind_addr
                );
            } else if ip.is_unspecified() {
                warn!(
                    "⚠️  Server is binding to all interfaces ({}). This will accept connections from any network.",
                    bind_addr
                );
            }
        }
        Ok(())
    }

    /// IPアドレスがホワイトリストに含まれるかチェック
    fn is_ip_whitelisted(&self, ip: &IpAddr) -> bool {
        // localhostは常に許可
        if ip.is_loopback() {
            return true;
        }

        // ホワイトリストをチェック
        for allowed in &self.ip_whitelist {
            // 簡易的な実装: 完全一致のみ
            // TODO: CIDR表記のサポート
            if let Ok(allowed_ip) = allowed.parse::<IpAddr>() {
                if ip == &allowed_ip {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_allows_localhost() {
        let policy = NetworkPolicy::default();
        let localhost_v4: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let localhost_v6: SocketAddr = "[::1]:8080".parse().unwrap();

        assert!(policy.validate_connection(&localhost_v4).is_ok());
        assert!(policy.validate_connection(&localhost_v6).is_ok());
    }

    #[test]
    fn test_default_policy_rejects_external() {
        let policy = NetworkPolicy::default();
        let external: SocketAddr = "192.168.1.100:8080".parse().unwrap();

        assert!(matches!(
            policy.validate_connection(&external),
            Err(NetworkPolicyError::ExternalAccessDenied(_))
        ));
    }

    #[test]
    fn test_whitelist_specific_ip() {
        let policy = NetworkPolicy {
            reject_external_connections: false,
            ip_whitelist: vec!["192.168.1.100".to_string()],
            ..NetworkPolicy::default()
        };

        let allowed: SocketAddr = "192.168.1.100:8080".parse().unwrap();
        let denied: SocketAddr = "192.168.1.101:8080".parse().unwrap();

        assert!(policy.validate_connection(&allowed).is_ok());
        assert!(matches!(
            policy.validate_connection(&denied),
            Err(NetworkPolicyError::IpNotWhitelisted(_))
        ));
    }
}
