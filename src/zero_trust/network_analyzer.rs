//! ネットワーク行動分析モジュール

use crate::zero_trust::{TrustScore, VerificationResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// ネットワーク情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// 送信元IPアドレス
    pub source_ip: IpAddr,
    /// 国コード
    pub country_code: Option<String>,
    /// 都市
    pub city: Option<String>,
    /// ISP情報
    pub isp: Option<String>,
    /// VPN/Proxy検出
    pub is_vpn: bool,
    /// Tor検出
    pub is_tor: bool,
    /// ホスト名
    pub hostname: Option<String>,
}

impl NetworkInfo {
    /// 新しいネットワーク情報を作成
    pub fn new(source_ip: IpAddr) -> Self {
        Self {
            source_ip,
            country_code: None,
            city: None,
            isp: None,
            is_vpn: false,
            is_tor: false,
            hostname: None,
        }
    }

    /// プライベートIPか判定
    pub fn is_private_ip(&self) -> bool {
        match self.source_ip {
            IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
            IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
        }
    }
}

/// 接続パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPattern {
    /// ユーザーID
    pub user_id: String,
    /// 接続元IP履歴
    pub ip_history: Vec<IpAddr>,
    /// 国履歴
    pub country_history: Vec<String>,
    /// 接続回数
    pub connection_count: u32,
    /// 最終接続時刻
    pub last_seen: u64,
}

impl ConnectionPattern {
    /// 新しいパターンを作成
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            ip_history: Vec::new(),
            country_history: Vec::new(),
            connection_count: 0,
            last_seen: 0,
        }
    }

    /// 接続を記録
    pub fn record_connection(&mut self, ip: IpAddr, country: Option<String>, timestamp: u64) {
        self.ip_history.push(ip);
        if let Some(c) = country {
            self.country_history.push(c);
        }
        self.connection_count += 1;
        self.last_seen = timestamp;

        // 履歴を最大100件に制限
        if self.ip_history.len() > 100 {
            self.ip_history.drain(0..self.ip_history.len() - 100);
        }
        if self.country_history.len() > 100 {
            self.country_history
                .drain(0..self.country_history.len() - 100);
        }
    }

    /// 地理的異常を検出
    pub fn detect_geo_anomaly(&self, current_country: &str) -> bool {
        if self.country_history.is_empty() {
            return false;
        }

        // 直近5回の接続で異なる国からのアクセスが多い場合
        let recent_countries: Vec<_> = self.country_history.iter().rev().take(5).collect();

        let unique_countries: std::collections::HashSet<_> =
            recent_countries.iter().cloned().collect();

        // 5回中4カ国以上異なる場合は異常
        unique_countries.len() >= 4 && !recent_countries.contains(&&current_country.to_string())
    }

    /// IP異常を検出
    pub fn detect_ip_anomaly(&self, current_ip: &IpAddr) -> bool {
        if self.ip_history.is_empty() {
            return false;
        }

        // 直近10回の接続でIPが頻繁に変わる場合
        let recent_ips: Vec<_> = self.ip_history.iter().rev().take(10).collect();

        let unique_ips: std::collections::HashSet<_> = recent_ips.iter().cloned().collect();

        // 10回中8個以上異なるIPの場合は異常
        unique_ips.len() >= 8 && !recent_ips.contains(&current_ip)
    }
}

/// ネットワーク分析器
pub struct NetworkAnalyzer {
    /// ユーザーの接続パターン
    patterns: HashMap<String, ConnectionPattern>,
    /// 信頼できる国リスト
    trusted_countries: Vec<String>,
    /// ブロックリストIP
    blocked_ips: Vec<IpAddr>,
}

impl NetworkAnalyzer {
    /// 新しい分析器を作成
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
            trusted_countries: vec!["JP".to_string(), "US".to_string()],
            blocked_ips: Vec::new(),
        }
    }

    /// ネットワーク情報を分析
    pub fn analyze(&mut self, user_id: &str, network_info: &NetworkInfo) -> VerificationResult {
        let mut trust_score: TrustScore = 50;
        let mut reasons = Vec::new();

        // ブロックリストチェック
        if self.blocked_ips.contains(&network_info.source_ip) {
            return VerificationResult::failure("IP is blocked");
        }

        // プライベートIPチェック
        if network_info.is_private_ip() {
            trust_score += 10;
            reasons.push("Private IP");
        }

        // VPN/Tor検出
        if network_info.is_vpn {
            trust_score = trust_score.saturating_sub(15);
            reasons.push("VPN detected");
        }
        if network_info.is_tor {
            trust_score = trust_score.saturating_sub(30);
            reasons.push("Tor detected");
        }

        // 国チェック
        if let Some(country) = &network_info.country_code {
            if self.trusted_countries.contains(country) {
                trust_score += 15;
                reasons.push("Trusted country");
            } else {
                trust_score = trust_score.saturating_sub(10);
                reasons.push("Untrusted country");
            }
        }

        // パターン分析
        let pattern = self
            .patterns
            .entry(user_id.to_string())
            .or_insert_with(|| ConnectionPattern::new(user_id));

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // 地理的異常検出
        if let Some(country) = &network_info.country_code {
            if pattern.detect_geo_anomaly(country) {
                trust_score = trust_score.saturating_sub(20);
                reasons.push("Geographic anomaly detected");
            }
        }

        // IP異常検出
        if pattern.detect_ip_anomaly(&network_info.source_ip) {
            trust_score = trust_score.saturating_sub(15);
            reasons.push("IP anomaly detected");
        }

        // パターン更新
        pattern.record_connection(
            network_info.source_ip,
            network_info.country_code.clone(),
            timestamp,
        );

        let success = trust_score >= 40;

        VerificationResult {
            success,
            trust_score: trust_score.min(100),
            reason: reasons.join(", "),
            details: HashMap::new(),
        }
    }

    /// IPをブロックリストに追加
    pub fn block_ip(&mut self, ip: IpAddr) {
        if !self.blocked_ips.contains(&ip) {
            self.blocked_ips.push(ip);
        }
    }

    /// 信頼できる国を追加
    pub fn add_trusted_country(&mut self, country_code: impl Into<String>) {
        self.trusted_countries.push(country_code.into());
    }
}

impl Default for NetworkAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_network_info_private_ip() {
        let info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(info.is_private_ip());

        let info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!info.is_private_ip());
    }

    #[test]
    fn test_connection_pattern() {
        let mut pattern = ConnectionPattern::new("user123");

        pattern.record_connection(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            Some("JP".to_string()),
            1000,
        );

        assert_eq!(pattern.connection_count, 1);
        assert_eq!(pattern.ip_history.len(), 1);
    }

    #[test]
    fn test_network_analyzer() {
        let mut analyzer = NetworkAnalyzer::new();

        let mut info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        info.country_code = Some("JP".to_string());

        let result = analyzer.analyze("user123", &info);

        assert!(result.success);
        assert!(result.trust_score >= 40);
    }

    #[test]
    fn test_vpn_detection() {
        let mut analyzer = NetworkAnalyzer::new();

        let mut info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
        info.is_vpn = true;

        let result = analyzer.analyze("user123", &info);

        assert!(result.trust_score < 50);
        assert!(result.reason.contains("VPN"));
    }

    #[test]
    fn test_blocked_ip() {
        let mut analyzer = NetworkAnalyzer::new();
        let blocked_ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));

        analyzer.block_ip(blocked_ip);

        let info = NetworkInfo::new(blocked_ip);
        let result = analyzer.analyze("user123", &info);

        assert!(!result.success);
        assert!(result.reason.contains("blocked"));
    }

    #[test]
    fn test_geo_anomaly() {
        let mut pattern = ConnectionPattern::new("user123");

        // 異なる国から複数回アクセス
        pattern.record_connection(
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            Some("US".to_string()),
            1000,
        );
        pattern.record_connection(
            IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2)),
            Some("UK".to_string()),
            1001,
        );
        pattern.record_connection(
            IpAddr::V4(Ipv4Addr::new(3, 3, 3, 3)),
            Some("FR".to_string()),
            1002,
        );
        pattern.record_connection(
            IpAddr::V4(Ipv4Addr::new(4, 4, 4, 4)),
            Some("DE".to_string()),
            1003,
        );

        assert!(pattern.detect_geo_anomaly("CN"));
    }
}
