//! Network Monitoring
//!
//! リクエストパターン分析とDDoS攻撃検知を実行します。

use super::{RequestData, Severity};
use crate::error::McpError;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// ネットワーク監視器
pub struct NetworkMonitor {
    /// トラフィックパターン履歴
    traffic_patterns: Arc<RwLock<HashMap<IpAddr, TrafficPattern>>>,
    /// リクエストカウンター
    request_counters: Arc<RwLock<HashMap<IpAddr, VecDeque<DateTime<Utc>>>>>,
    /// 設定
    config: NetworkMonitorConfig,
}

/// トラフィックパターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficPattern {
    /// IPアドレス
    pub ip_address: IpAddr,
    /// 総リクエスト数
    pub total_requests: u64,
    /// 失敗リクエスト数
    pub failed_requests: u64,
    /// 平均リクエスト間隔（ミリ秒）
    pub avg_interval_ms: f64,
    /// アクセスパス分布
    pub path_distribution: HashMap<String, u64>,
    /// ユーザーエージェント
    pub user_agents: Vec<String>,
    /// 最初の観測時刻
    pub first_seen: DateTime<Utc>,
    /// 最後の観測時刻
    pub last_seen: DateTime<Utc>,
    /// リスクスコア
    pub risk_score: f64,
}

/// ネットワーク監視設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMonitorConfig {
    /// DDoSしきい値（秒あたりリクエスト数）
    pub ddos_threshold_rps: u64,
    /// ポートスキャン検出ウィンドウ（秒）
    pub port_scan_window_seconds: i64,
    /// ポートスキャンしきい値（ユニークパス数）
    pub port_scan_threshold: usize,
    /// レート制限ウィンドウ（秒）
    pub rate_limit_window_seconds: i64,
    /// カウンター履歴保持期間（秒）
    pub counter_retention_seconds: i64,
    /// 疑わしいUser-Agentパターン
    pub suspicious_user_agents: Vec<String>,
}

impl Default for NetworkMonitorConfig {
    fn default() -> Self {
        Self {
            ddos_threshold_rps: 50,
            port_scan_window_seconds: 60,
            port_scan_threshold: 20,
            rate_limit_window_seconds: 60,
            counter_retention_seconds: 300,
            suspicious_user_agents: vec![
                "bot".to_string(),
                "crawler".to_string(),
                "scanner".to_string(),
                "sqlmap".to_string(),
                "nikto".to_string(),
            ],
        }
    }
}

/// ネットワーク分析結果
#[derive(Debug, Clone)]
pub struct NetworkAnalysisResult {
    /// 疑わしいフラグ
    pub is_suspicious: bool,
    /// リスクスコア（0.0-1.0）
    pub risk_score: f64,
    /// 検知されたパターン
    pub detected_patterns: Vec<NetworkAttackPattern>,
    /// 詳細説明
    pub details: String,
}

/// ネットワーク攻撃パターン
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkAttackPattern {
    /// DDoS攻撃
    DdosAttack,
    /// ポートスキャン
    PortScan,
    /// レート制限違反
    RateLimitViolation,
    /// 疑わしいUser-Agent
    SuspiciousUserAgent,
    /// 異常なトラフィックパターン
    AbnormalTrafficPattern,
    /// 分散攻撃（複数IP）
    DistributedAttack,
}

impl NetworkMonitor {
    /// 新しいネットワーク監視器を作成
    pub async fn new() -> Result<Self, McpError> {
        Self::with_config(NetworkMonitorConfig::default()).await
    }

    /// 設定付きでネットワーク監視器を作成
    pub async fn with_config(config: NetworkMonitorConfig) -> Result<Self, McpError> {
        info!("Initializing network monitor");

        Ok(Self {
            traffic_patterns: Arc::new(RwLock::new(HashMap::new())),
            request_counters: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    /// トラフィックをチェック
    pub async fn check_traffic(
        &self,
        request: &RequestData,
    ) -> Result<NetworkAnalysisResult, McpError> {
        debug!(
            "Checking network traffic for request: {}",
            request.request_id
        );

        let ip = match request.source_ip {
            Some(ip) => ip,
            None => {
                return Ok(NetworkAnalysisResult {
                    is_suspicious: false,
                    risk_score: 0.0,
                    detected_patterns: Vec::new(),
                    details: "No IP address available".to_string(),
                });
            }
        };

        // リクエストカウンターを更新
        self.update_request_counter(ip).await;

        // トラフィックパターンを更新
        self.update_traffic_pattern(ip, request).await;

        let mut detected_patterns = Vec::new();
        let mut risk_scores = Vec::new();

        // 1. DDoS攻撃検知
        if let Some(ddos_score) = self.check_ddos_attack(ip).await {
            if ddos_score > 0.7 {
                detected_patterns.push(NetworkAttackPattern::DdosAttack);
                risk_scores.push(ddos_score);
            }
        }

        // 2. ポートスキャン検知
        if let Some(scan_score) = self.check_port_scan(ip).await {
            if scan_score > 0.6 {
                detected_patterns.push(NetworkAttackPattern::PortScan);
                risk_scores.push(scan_score);
            }
        }

        // 3. レート制限違反検知
        if let Some(rate_score) = self.check_rate_limit(ip).await {
            if rate_score > 0.5 {
                detected_patterns.push(NetworkAttackPattern::RateLimitViolation);
                risk_scores.push(rate_score);
            }
        }

        // 4. User-Agentチェック
        if let Some(ua_score) = self.check_user_agent(request) {
            if ua_score > 0.6 {
                detected_patterns.push(NetworkAttackPattern::SuspiciousUserAgent);
                risk_scores.push(ua_score);
            }
        }

        // 5. 異常なトラフィックパターン
        if let Some(traffic_score) = self.check_traffic_pattern(ip).await {
            if traffic_score > 0.6 {
                detected_patterns.push(NetworkAttackPattern::AbnormalTrafficPattern);
                risk_scores.push(traffic_score);
            }
        }

        // 総合リスクスコア
        let max_score = risk_scores.iter().cloned().fold(0.0, f64::max);
        let is_suspicious = max_score > 0.5;

        let pattern_count = detected_patterns.len();

        if is_suspicious {
            warn!(
                "Suspicious network activity from {}: score={:.2}, patterns={:?}",
                ip, max_score, detected_patterns
            );
        }

        Ok(NetworkAnalysisResult {
            is_suspicious,
            risk_score: max_score,
            detected_patterns,
            details: format!(
                "Network analysis: {} suspicious patterns detected with max score {:.2}",
                pattern_count, max_score
            ),
        })
    }

    /// トラフィックパターンを取得
    pub async fn get_traffic_pattern(&self, ip: IpAddr) -> Option<TrafficPattern> {
        self.traffic_patterns.read().await.get(&ip).cloned()
    }

    /// 全トラフィックパターンを取得
    pub async fn get_all_patterns(&self) -> Vec<TrafficPattern> {
        self.traffic_patterns
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// リスクスコア上位のIPを取得
    pub async fn get_top_risk_ips(&self, limit: usize) -> Vec<(IpAddr, f64)> {
        let patterns = self.traffic_patterns.read().await;
        let mut risk_ips: Vec<(IpAddr, f64)> = patterns
            .iter()
            .map(|(ip, pattern)| (*ip, pattern.risk_score))
            .collect();

        risk_ips.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        risk_ips.truncate(limit);
        risk_ips
    }

    /// リクエストカウンターを更新
    async fn update_request_counter(&self, ip: IpAddr) {
        let mut counters = self.request_counters.write().await;
        let counter = counters.entry(ip).or_insert_with(VecDeque::new);

        counter.push_back(Utc::now());

        // 古いエントリを削除
        let cutoff = Utc::now() - Duration::seconds(self.config.counter_retention_seconds);
        while let Some(timestamp) = counter.front() {
            if *timestamp < cutoff {
                counter.pop_front();
            } else {
                break;
            }
        }
    }

    /// トラフィックパターンを更新
    async fn update_traffic_pattern(&self, ip: IpAddr, request: &RequestData) {
        let mut patterns = self.traffic_patterns.write().await;
        let pattern = patterns.entry(ip).or_insert_with(|| TrafficPattern {
            ip_address: ip,
            total_requests: 0,
            failed_requests: 0,
            avg_interval_ms: 0.0,
            path_distribution: HashMap::new(),
            user_agents: Vec::new(),
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            risk_score: 0.0,
        });

        pattern.total_requests += 1;
        pattern.last_seen = Utc::now();

        // パス分布を更新
        *pattern
            .path_distribution
            .entry(request.path.clone())
            .or_insert(0) += 1;

        // User-Agentを記録
        if let Some(ua) = request.headers.get("User-Agent") {
            if !pattern.user_agents.contains(ua) && pattern.user_agents.len() < 10 {
                pattern.user_agents.push(ua.clone());
            }
        }
    }

    /// DDoS攻撃をチェック
    async fn check_ddos_attack(&self, ip: IpAddr) -> Option<f64> {
        let counters = self.request_counters.read().await;
        let counter = counters.get(&ip)?;

        // 過去1秒間のリクエスト数
        let one_second_ago = Utc::now() - Duration::seconds(1);
        let recent_count = counter.iter().filter(|t| **t > one_second_ago).count() as u64;

        if recent_count > self.config.ddos_threshold_rps {
            let excess_ratio = recent_count as f64 / self.config.ddos_threshold_rps as f64;
            Some((excess_ratio - 1.0).min(1.0))
        } else if recent_count > self.config.ddos_threshold_rps / 2 {
            Some(0.6)
        } else {
            Some(0.0)
        }
    }

    /// ポートスキャンをチェック
    async fn check_port_scan(&self, ip: IpAddr) -> Option<f64> {
        let patterns = self.traffic_patterns.read().await;
        let pattern = patterns.get(&ip)?;

        let _window_start = Utc::now() - Duration::seconds(self.config.port_scan_window_seconds);

        // ウィンドウ内のユニークパス数をチェック
        let unique_paths = pattern.path_distribution.len();

        if unique_paths > self.config.port_scan_threshold {
            let excess_ratio = unique_paths as f64 / self.config.port_scan_threshold as f64;
            Some((excess_ratio - 1.0).min(1.0))
        } else if unique_paths > self.config.port_scan_threshold / 2 {
            Some(0.4)
        } else {
            Some(0.0)
        }
    }

    /// レート制限をチェック
    async fn check_rate_limit(&self, ip: IpAddr) -> Option<f64> {
        let counters = self.request_counters.read().await;
        let counter = counters.get(&ip)?;

        let window_start = Utc::now() - Duration::seconds(self.config.rate_limit_window_seconds);
        let requests_in_window = counter.iter().filter(|t| **t > window_start).count();

        // 1分あたり60リクエスト以上で警告
        let threshold = 60;
        if requests_in_window > threshold {
            let excess_ratio = requests_in_window as f64 / threshold as f64;
            Some((excess_ratio - 1.0).min(1.0))
        } else {
            Some(0.0)
        }
    }

    /// User-Agentをチェック
    fn check_user_agent(&self, request: &RequestData) -> Option<f64> {
        let user_agent = request.headers.get("User-Agent")?;
        let ua_lower = user_agent.to_lowercase();

        // 疑わしいパターンをチェック
        for suspicious in &self.config.suspicious_user_agents {
            if ua_lower.contains(suspicious) {
                return Some(0.8);
            }
        }

        // User-Agentが空または非常に短い
        if user_agent.len() < 10 {
            return Some(0.6);
        }

        Some(0.0)
    }

    /// トラフィックパターンをチェック
    async fn check_traffic_pattern(&self, ip: IpAddr) -> Option<f64> {
        let patterns = self.traffic_patterns.read().await;
        let pattern = patterns.get(&ip)?;

        let mut anomaly_score: f64 = 0.0;

        // 失敗率が高い
        if pattern.total_requests > 10 {
            let failure_rate = pattern.failed_requests as f64 / pattern.total_requests as f64;
            if failure_rate > 0.5 {
                anomaly_score = anomaly_score.max(0.7);
            } else if failure_rate > 0.3 {
                anomaly_score = anomaly_score.max(0.5);
            }
        }

        // パス分布が異常（1つのパスに集中）
        if !pattern.path_distribution.is_empty() {
            let max_path_count = pattern
                .path_distribution
                .values()
                .max()
                .copied()
                .unwrap_or(0);
            let concentration = max_path_count as f64 / pattern.total_requests as f64;

            if concentration > 0.9 && pattern.total_requests > 50 {
                anomaly_score = anomaly_score.max(0.6);
            }
        }

        Some(anomaly_score)
    }

    /// 定期的なクリーンアップ
    pub async fn cleanup_old_data(&self) {
        let cutoff = Utc::now() - Duration::hours(24);

        let mut patterns = self.traffic_patterns.write().await;
        patterns.retain(|_, pattern| pattern.last_seen > cutoff);

        info!(
            "Cleaned up old traffic patterns, {} patterns remaining",
            patterns.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_network_monitor_initialization() {
        let monitor = NetworkMonitor::new().await;
        assert!(monitor.is_ok());
    }

    #[tokio::test]
    async fn test_normal_traffic() {
        let monitor = NetworkMonitor::new().await.unwrap();

        let request = RequestData {
            request_id: "test-001".to_string(),
            method: "GET".to_string(),
            path: "/api/products".to_string(),
            query_params: HashMap::new(),
            headers: [("User-Agent".to_string(), "Mozilla/5.0".to_string())].into(),
            body: None,
            source_ip: Some("192.168.1.100".parse().unwrap()),
            timestamp: Utc::now(),
        };

        let result = monitor.check_traffic(&request).await.unwrap();
        assert!(!result.is_suspicious);
        assert!(result.risk_score < 0.5);
    }

    #[tokio::test]
    async fn test_suspicious_user_agent() {
        let monitor = NetworkMonitor::new().await.unwrap();

        let request = RequestData {
            request_id: "test-002".to_string(),
            method: "GET".to_string(),
            path: "/api/test".to_string(),
            query_params: HashMap::new(),
            headers: [("User-Agent".to_string(), "sqlmap/1.0".to_string())].into(),
            body: None,
            source_ip: Some("192.168.1.100".parse().unwrap()),
            timestamp: Utc::now(),
        };

        let result = monitor.check_traffic(&request).await.unwrap();
        assert!(result.is_suspicious);
        assert!(result
            .detected_patterns
            .contains(&NetworkAttackPattern::SuspiciousUserAgent));
    }

    #[tokio::test]
    async fn test_traffic_pattern_tracking() {
        let monitor = NetworkMonitor::new().await.unwrap();
        let ip: IpAddr = "192.168.1.100".parse().unwrap();

        // 複数のリクエストを送信
        for i in 0..10 {
            let request = RequestData {
                request_id: format!("test-{}", i),
                method: "GET".to_string(),
                path: "/api/products".to_string(),
                query_params: HashMap::new(),
                headers: HashMap::new(),
                body: None,
                source_ip: Some(ip),
                timestamp: Utc::now(),
            };

            let _ = monitor.check_traffic(&request).await;
        }

        let pattern = monitor.get_traffic_pattern(ip).await;
        assert!(pattern.is_some());

        let pattern = pattern.unwrap();
        assert_eq!(pattern.total_requests, 10);
    }
}
