//! Network Monitoring
//!
//! リクエストパターン分析とDDoS攻撃検知を実行します。

use super::RequestData;
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
    /// パスごとに、最後に見た時刻（PR #728のレビュー）。ポートスキャンは、
    /// `port_scan_window_seconds`の間に見たパスの数で判定する
    #[serde(default)]
    pub path_last_seen: HashMap<String, DateTime<Utc>>,
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
    /// 記録するIPアドレスの数の上限（#543）。超えると、最後に見た時刻の
    /// 古いものから消す。多くのアドレス（IPv6等）から送られても、記録が
    /// 際限なく増えないようにする
    #[serde(default = "default_max_tracked_ips")]
    pub max_tracked_ips: usize,
}

fn default_max_tracked_ips() -> usize {
    10_000
}

/// 1つのIPアドレスについて覚えておく、リクエストの時刻の数の上限の
/// 最小（#543）。頻度の判定（1秒に`ddos_threshold_rps`、1分に60）には、
/// 既定のしきい値ならこれで足りる
const MAX_TIMESTAMPS_PER_IP: usize = 1_000;

/// 1つのIPアドレスについて覚えておく、パスの数の上限の最小（#543）
const MAX_PATHS_PER_IP: usize = 100;

impl NetworkMonitorConfig {
    /// 1つのIPアドレスについて覚えておく、リクエストの時刻の数の上限。
    /// しきい値の2倍以上にする（#724。しきい値を上げると、固定の上限では
    /// スコアが判定の値に届かなかった）
    fn max_timestamps_per_ip(&self) -> usize {
        let threshold = usize::try_from(self.ddos_threshold_rps).unwrap_or(usize::MAX);
        MAX_TIMESTAMPS_PER_IP.max(threshold.saturating_mul(2))
    }

    /// 1つのIPアドレスについて覚えておく、パスの数の上限。しきい値の2倍以上
    /// にする（#724）
    fn max_paths_per_ip(&self) -> usize {
        MAX_PATHS_PER_IP.max(self.port_scan_threshold.saturating_mul(2))
    }
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
            max_tracked_ips: default_max_tracked_ips(),
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
            if ddos_score > DDOS_DETECTION_SCORE {
                detected_patterns.push(NetworkAttackPattern::DdosAttack);
                risk_scores.push(ddos_score);
            }
        }

        // 2. ポートスキャン検知
        if let Some(scan_score) = self.check_port_scan(ip).await {
            if scan_score > PORT_SCAN_DETECTION_SCORE {
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
        let cutoff = Utc::now() - Duration::seconds(self.config.counter_retention_seconds);
        if !counters.contains_key(&ip) && counters.len() >= self.config.max_tracked_ips {
            // 上限（#543）: 期限の過ぎたものを消し、まだ多ければ、最後の
            // リクエストの古いものから消す
            counters.retain(|_, counter| counter.back().is_some_and(|t| *t >= cutoff));
            evict_oldest(&mut counters, self.config.max_tracked_ips, |counter| {
                counter.back().copied()
            });
        }
        let counter = counters.entry(ip).or_insert_with(VecDeque::new);

        counter.push_back(Utc::now());

        // 古いエントリを削除
        while let Some(timestamp) = counter.front() {
            if *timestamp < cutoff || counter.len() > self.config.max_timestamps_per_ip() {
                counter.pop_front();
            } else {
                break;
            }
        }
    }

    /// トラフィックパターンを更新
    async fn update_traffic_pattern(&self, ip: IpAddr, request: &RequestData) {
        let mut patterns = self.traffic_patterns.write().await;
        if !patterns.contains_key(&ip) && patterns.len() >= self.config.max_tracked_ips {
            // 上限（#543）: 最後に見た時刻の古いものから消す
            evict_oldest(&mut patterns, self.config.max_tracked_ips, |pattern| {
                Some(pattern.last_seen)
            });
        }
        let pattern = patterns.entry(ip).or_insert_with(|| TrafficPattern {
            ip_address: ip,
            total_requests: 0,
            failed_requests: 0,
            avg_interval_ms: 0.0,
            path_distribution: HashMap::new(),
            path_last_seen: HashMap::new(),
            user_agents: Vec::new(),
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            risk_score: 0.0,
        });

        let now = Utc::now();
        pattern.total_requests += 1;
        pattern.last_seen = now;

        // パス分布を更新（新しいパスは、上限まで。#543）
        if let Some(count) = pattern.path_distribution.get_mut(&request.path) {
            *count += 1;
        } else if pattern.path_distribution.len() < self.config.max_paths_per_ip() {
            pattern.path_distribution.insert(request.path.clone(), 1);
        }
        // パスを最後に見た時刻（PR #728のレビュー）。上限に達したら、最後に
        // 見たのがいちばん古いパスを忘れて、新しいパスを必ず覚える（新しい
        // パスを捨てると、窓の中のパスを少なく数えた）。覚えているパスは、
        // 時刻を更新するだけ
        if !pattern.path_last_seen.contains_key(&request.path)
            && pattern.path_last_seen.len() >= self.config.max_paths_per_ip()
        {
            let oldest = pattern
                .path_last_seen
                .iter()
                .min_by_key(|(_, seen)| **seen)
                .map(|(path, _)| path.clone());
            if let Some(oldest) = oldest {
                pattern.path_last_seen.remove(&oldest);
            }
        }
        pattern.path_last_seen.insert(request.path.clone(), now);

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

        Some(ddos_score(recent_count, self.config.ddos_threshold_rps))
    }

    /// ポートスキャンをチェック
    async fn check_port_scan(&self, ip: IpAddr) -> Option<f64> {
        let patterns = self.traffic_patterns.read().await;
        let pattern = patterns.get(&ip)?;

        let window_start = Utc::now() - Duration::seconds(self.config.port_scan_window_seconds);

        // ウィンドウ内のユニークパス数をチェック（PR #728のレビュー。以前は
        // 累積の数で、ゆっくり多くのパスを使う相手も、ポートスキャンとした）
        let unique_paths = pattern
            .path_last_seen
            .values()
            .filter(|seen| **seen > window_start)
            .count();

        Some(port_scan_score(
            unique_paths,
            self.config.port_scan_threshold,
        ))
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

    /// 定期的なクリーンアップ。24時間見ていないIPアドレスのパターンと、
    /// 保持期間の過ぎたカウンターを消す（カウンターは#543から）
    pub async fn cleanup_old_data(&self) {
        let cutoff = Utc::now() - Duration::hours(24);

        let mut patterns = self.traffic_patterns.write().await;
        patterns.retain(|_, pattern| pattern.last_seen > cutoff);
        let remaining = patterns.len();
        drop(patterns);

        let counter_cutoff = Utc::now() - Duration::seconds(self.config.counter_retention_seconds);
        self.request_counters
            .write()
            .await
            .retain(|_, counter| counter.back().is_some_and(|t| *t >= counter_cutoff));

        debug!(
            "Cleaned up old traffic patterns, {} patterns remaining",
            remaining
        );
    }
}

/// しきい値を超えたときの最小のDDoSのスコア（#759）。
/// [`DDOS_DETECTION_SCORE`]より大きくする
const DDOS_MIN_SCORE: f64 = 0.75;

/// `check_traffic`がDDoSとするスコアの下限（これより大きいもの）
const DDOS_DETECTION_SCORE: f64 = 0.7;

/// 1秒の要求の数から、DDoSのスコアを出す（#759）。しきい値
/// （`ddos_threshold_rps`）を超えたら、必ず判定の値（0.7）より大きく
/// （0.75から）、しきい値の2倍で1.0にする。超えなければ0.0。以前は超過の
/// 割合だけをスコアにしたので、既定（50）では51件で0.02になり、86件まで
/// DDoSとしなかった
fn ddos_score(recent_count: u64, threshold_rps: u64) -> f64 {
    if recent_count <= threshold_rps {
        return 0.0;
    }
    let excess_ratio = recent_count as f64 / threshold_rps.max(1) as f64 - 1.0;
    DDOS_MIN_SCORE + (1.0 - DDOS_MIN_SCORE) * excess_ratio.min(1.0)
}

/// しきい値を超えたときの最小のポートスキャンのスコア（#760）。
/// [`PORT_SCAN_DETECTION_SCORE`]より大きくする。DDoSと同じく、IDSの既定の
/// 最小の信頼度（0.7）よりも大きくする
const PORT_SCAN_MIN_SCORE: f64 = 0.75;

/// `check_traffic`がポートスキャンとするスコアの下限（これより大きいもの）
const PORT_SCAN_DETECTION_SCORE: f64 = 0.6;

/// 窓の中で見たパスの数から、ポートスキャンのスコアを出す（#760）。しきい値
/// （`port_scan_threshold`）を超えたら、必ず判定の値（0.6）より大きく
/// （0.75から）、しきい値の2倍で1.0にする。超えなければ0.0。以前は超過の
/// 割合だけをスコアにしたので、既定（20）では21個で0.05になり、33個まで
/// ポートスキャンとしなかった
fn port_scan_score(unique_paths: usize, threshold: usize) -> f64 {
    if unique_paths <= threshold {
        return 0.0;
    }
    let excess_ratio = unique_paths as f64 / threshold.max(1) as f64 - 1.0;
    PORT_SCAN_MIN_SCORE + (1.0 - PORT_SCAN_MIN_SCORE) * excess_ratio.min(1.0)
}

/// `map`が`max`以上なら、`last_seen`の古いものから消して、`max`の9割に
/// する（#543）。1件ずつ消すと、新しいアドレスが来るたびに並べ替えることに
/// なるので、まとめて消す
pub(super) fn evict_oldest<K: Clone + Eq + std::hash::Hash + Ord, V>(
    map: &mut HashMap<K, V>,
    max: usize,
    last_seen: impl Fn(&V) -> Option<DateTime<Utc>>,
) {
    if map.len() < max {
        return;
    }
    let keep = max.saturating_sub(max / 10).saturating_sub(1);
    let mut by_age: Vec<(Option<DateTime<Utc>>, K)> = map
        .iter()
        .map(|(key, v)| (last_seen(v), key.clone()))
        .collect();
    by_age.sort_unstable();
    let excess = map.len().saturating_sub(keep);
    for (_, key) in by_age.into_iter().take(excess) {
        map.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// IPごとの記録の上限は、しきい値の2倍以上（#724）
    #[test]
    fn test_per_ip_limits_follow_the_thresholds() {
        let default = NetworkMonitorConfig::default();
        assert_eq!(default.max_timestamps_per_ip(), MAX_TIMESTAMPS_PER_IP);
        assert_eq!(default.max_paths_per_ip(), MAX_PATHS_PER_IP);
        let high = NetworkMonitorConfig {
            ddos_threshold_rps: 600,
            port_scan_threshold: 80,
            ..NetworkMonitorConfig::default()
        };
        assert_eq!(high.max_timestamps_per_ip(), 1200);
        assert_eq!(high.max_paths_per_ip(), 160);
    }

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
            caller_id: None,
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
            caller_id: None,
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
                caller_id: None,
                timestamp: Utc::now(),
            };

            let _ = monitor.check_traffic(&request).await;
        }

        let pattern = monitor.get_traffic_pattern(ip).await;
        assert!(pattern.is_some());

        let pattern = pattern.unwrap();
        assert_eq!(pattern.total_requests, 10);
    }

    /// ポートスキャンは、`port_scan_window_seconds`の間に見たパスの数で判定
    /// する（PR #728のレビュー。累積の数では、ゆっくり多くのパスを使う相手も
    /// ポートスキャンとした）
    #[tokio::test]
    async fn test_port_scan_counts_paths_within_the_window() {
        let monitor = NetworkMonitor::with_config(NetworkMonitorConfig {
            port_scan_threshold: 80,
            port_scan_window_seconds: 60,
            ..NetworkMonitorConfig::default()
        })
        .await
        .unwrap();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        for i in 0..129 {
            let request = RequestData {
                request_id: format!("r-{i}"),
                method: "GET".to_string(),
                path: format!("/p/{i}"),
                query_params: HashMap::new(),
                headers: HashMap::new(),
                body: None,
                source_ip: Some(ip),
                caller_id: None,
                timestamp: Utc::now(),
            };
            monitor.update_traffic_pattern(ip, &request).await;
        }
        // 窓の中に129個: 検知する
        let score = monitor.check_port_scan(ip).await.unwrap();
        assert!(score > 0.6, "{score}");
        // どれも窓より前に見たことにすると、検知しない
        {
            let mut patterns = monitor.traffic_patterns.write().await;
            let pattern = patterns.get_mut(&ip).unwrap();
            for seen in pattern.path_last_seen.values_mut() {
                *seen = Utc::now() - Duration::seconds(61);
            }
        }
        assert_eq!(monitor.check_port_scan(ip).await, Some(0.0));
    }

    /// パスの記録が上限に達している間に来た新しいパスも、最後に見たのが
    /// いちばん古いパスと入れ替えて覚える（PR #728のレビュー。以前は捨てた
    /// ので、窓の中のパスを少なく数えた）
    #[tokio::test]
    async fn test_new_paths_are_kept_while_the_record_is_full() {
        let monitor = NetworkMonitor::new().await.unwrap();
        let ip: IpAddr = "192.0.2.8".parse().unwrap();
        let request = |path: String| RequestData {
            request_id: "r".to_string(),
            method: "GET".to_string(),
            path,
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some(ip),
            caller_id: None,
            timestamp: Utc::now(),
        };
        let max = monitor.config.max_paths_per_ip();
        for i in 0..max {
            monitor
                .update_traffic_pattern(ip, &request(format!("/old/{i}")))
                .await;
        }
        // 古いパスは、59秒前（まだ窓の中）に見たことにする
        {
            let mut patterns = monitor.traffic_patterns.write().await;
            for seen in patterns.get_mut(&ip).unwrap().path_last_seen.values_mut() {
                *seen = Utc::now() - Duration::seconds(59);
            }
        }
        for i in 0..33 {
            monitor
                .update_traffic_pattern(ip, &request(format!("/new/{i}")))
                .await;
        }
        let patterns = monitor.traffic_patterns.read().await;
        let seen = &patterns[&ip].path_last_seen;
        assert_eq!(seen.len(), max);
        assert!((0..33).all(|i| seen.contains_key(&format!("/new/{i}"))));
    }

    /// しきい値を超えたら、判定の値（0.7）より大きいスコアにする（#759）
    #[test]
    fn test_ddos_score_follows_the_threshold() {
        assert_eq!(ddos_score(0, 50), 0.0);
        assert_eq!(ddos_score(26, 50), 0.0);
        assert_eq!(ddos_score(50, 50), 0.0);
        assert!(ddos_score(51, 50) > DDOS_DETECTION_SCORE);
        assert!(ddos_score(51, 50) < ddos_score(75, 50));
        assert_eq!(ddos_score(100, 50), 1.0);
        assert_eq!(ddos_score(1000, 50), 1.0);
        assert!(ddos_score(2, 1) > DDOS_DETECTION_SCORE);
        assert!(ddos_score(1, 0) > DDOS_DETECTION_SCORE);
    }

    /// しきい値を超えたら、判定の値（0.6）より大きいスコアにする（#760）
    #[test]
    fn test_port_scan_score_follows_the_threshold() {
        assert_eq!(port_scan_score(0, 20), 0.0);
        assert_eq!(port_scan_score(11, 20), 0.0);
        assert_eq!(port_scan_score(20, 20), 0.0);
        assert!(port_scan_score(21, 20) > PORT_SCAN_DETECTION_SCORE);
        assert!(port_scan_score(21, 20) < port_scan_score(30, 20));
        assert_eq!(port_scan_score(40, 20), 1.0);
        assert_eq!(port_scan_score(1000, 20), 1.0);
        assert!(port_scan_score(1, 0) > PORT_SCAN_DETECTION_SCORE);
    }

    /// 既定（20パス）で、窓の中に21個のパスがあればポートスキャンとし、
    /// 20個ならしない（#760。以前は33個までポートスキャンとしなかった）
    #[tokio::test]
    async fn test_port_scan_is_detected_just_above_the_threshold() {
        let monitor = NetworkMonitor::new().await.unwrap();
        let request = |ip: IpAddr, i: usize| RequestData {
            request_id: format!("r-{i}"),
            method: "GET".to_string(),
            path: format!("/p/{i}"),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some(ip),
            caller_id: None,
            timestamp: Utc::now(),
        };
        let below: IpAddr = "192.0.2.30".parse().unwrap();
        let mut last = None;
        for i in 0..20 {
            last = Some(monitor.check_traffic(&request(below, i)).await.unwrap());
        }
        assert!(!last
            .unwrap()
            .detected_patterns
            .contains(&NetworkAttackPattern::PortScan));

        let above: IpAddr = "192.0.2.31".parse().unwrap();
        for i in 0..20 {
            monitor.check_traffic(&request(above, i)).await.unwrap();
        }
        let result = monitor.check_traffic(&request(above, 20)).await.unwrap();
        assert!(
            result
                .detected_patterns
                .contains(&NetworkAttackPattern::PortScan),
            "{:?}",
            result.detected_patterns
        );
        assert!(result.risk_score > PORT_SCAN_DETECTION_SCORE);
    }

    /// 既定（50 rps）で、1秒に51件ならDDoSとし、50件ならしない（#759。
    /// 以前は86件までDDoSとしなかった）
    #[tokio::test]
    async fn test_ddos_is_detected_just_above_the_threshold() {
        let monitor = NetworkMonitor::new().await.unwrap();
        let request = |ip: IpAddr| RequestData {
            request_id: "r".to_string(),
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some(ip),
            caller_id: None,
            timestamp: Utc::now(),
        };
        let below: IpAddr = "192.0.2.20".parse().unwrap();
        let mut last = None;
        for _ in 0..50 {
            last = Some(monitor.check_traffic(&request(below)).await.unwrap());
        }
        assert!(!last
            .unwrap()
            .detected_patterns
            .contains(&NetworkAttackPattern::DdosAttack));

        let above: IpAddr = "192.0.2.21".parse().unwrap();
        for _ in 0..50 {
            monitor.check_traffic(&request(above)).await.unwrap();
        }
        let result = monitor.check_traffic(&request(above)).await.unwrap();
        assert!(
            result
                .detected_patterns
                .contains(&NetworkAttackPattern::DdosAttack),
            "{:?}",
            result.detected_patterns
        );
        assert!(result.risk_score > DDOS_DETECTION_SCORE);
    }
}
