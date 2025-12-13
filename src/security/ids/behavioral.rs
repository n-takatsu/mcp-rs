//! Behavioral Detection
//!
//! ベースライン学習と異常行動パターン検出を実行します。

use super::{DetectionType, RequestData};
use crate::error::McpError;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 振る舞い検知器
pub struct BehavioralDetector {
    /// ベースライン
    baselines: Arc<RwLock<HashMap<String, BehaviorBaseline>>>,
    /// リクエスト履歴（異常検知用）
    request_history: Arc<RwLock<VecDeque<RequestRecord>>>,
    /// 設定
    config: BehavioralConfig,
}

/// 振る舞いベースライン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorBaseline {
    /// ユーザーID または IPアドレス
    pub identifier: String,
    /// 平均リクエスト頻度（秒あたり）
    pub avg_request_rate: f64,
    /// リクエスト頻度の標準偏差
    pub request_rate_std_dev: f64,
    /// 典型的なアクセスパス
    pub typical_paths: HashMap<String, f64>,
    /// 典型的なリクエストサイズ（バイト）
    pub avg_request_size: f64,
    /// 典型的なアクセス時間帯
    pub typical_hours: Vec<u32>,
    /// セッション期間（秒）
    pub avg_session_duration: f64,
    /// 最終更新時刻
    pub last_updated: DateTime<Utc>,
    /// サンプル数
    pub sample_count: usize,
}

/// リクエスト記録
#[derive(Debug, Clone)]
struct RequestRecord {
    /// 識別子（ユーザーID or IP）
    identifier: String,
    /// パス
    path: String,
    /// リクエストサイズ
    size: usize,
    /// タイムスタンプ
    timestamp: DateTime<Utc>,
    /// IPアドレス
    ip_address: Option<IpAddr>,
}

/// 振る舞い検知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralConfig {
    /// ベースライン学習期間（時間）
    pub learning_period_hours: i64,
    /// 異常スコアしきい値
    pub anomaly_threshold: f64,
    /// リクエスト履歴保持数
    pub max_history_size: usize,
    /// 統計的異常検知の感度（標準偏差の倍数）
    pub statistical_sensitivity: f64,
    /// 最小サンプル数
    pub min_sample_size: usize,
}

impl Default for BehavioralConfig {
    fn default() -> Self {
        Self {
            learning_period_hours: 24,
            anomaly_threshold: 0.7,
            max_history_size: 10000,
            statistical_sensitivity: 2.0,
            min_sample_size: 100,
        }
    }
}

/// 振る舞い分析結果
#[derive(Debug, Clone)]
pub struct BehavioralAnalysisResult {
    /// 異常フラグ
    pub is_anomalous: bool,
    /// 異常スコア（0.0-1.0）
    pub anomaly_score: f64,
    /// 異常タイプ
    pub anomaly_types: Vec<AnomalyType>,
    /// 詳細説明
    pub details: String,
}

/// 異常タイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyType {
    /// リクエスト頻度の異常
    AbnormalRequestRate,
    /// 未知のパスへのアクセス
    UnusualPath,
    /// 異常なリクエストサイズ
    AbnormalRequestSize,
    /// 異常なアクセス時間帯
    UnusualAccessTime,
    /// 地理的異常
    GeographicalAnomaly,
    /// セッション期間の異常
    AbnormalSessionDuration,
}

impl BehavioralDetector {
    /// 新しい振る舞い検知器を作成
    pub async fn new() -> Result<Self, McpError> {
        Self::with_config(BehavioralConfig::default()).await
    }

    /// 設定付きで振る舞い検知器を作成
    pub async fn with_config(config: BehavioralConfig) -> Result<Self, McpError> {
        info!("Initializing behavioral detector");

        Ok(Self {
            baselines: Arc::new(RwLock::new(HashMap::new())),
            request_history: Arc::new(RwLock::new(VecDeque::new())),
            config,
        })
    }

    /// リクエストを分析
    pub async fn analyze(
        &self,
        request: &RequestData,
    ) -> Result<BehavioralAnalysisResult, McpError> {
        debug!(
            "Analyzing behavioral patterns for request: {}",
            request.request_id
        );

        // 識別子を取得（ユーザーID優先、なければIP）
        let identifier = self.extract_identifier(request);

        // リクエスト記録を追加
        self.record_request(request, &identifier).await;

        // ベースラインを取得または作成
        let mut baselines = self.baselines.write().await;
        let baseline = baselines
            .entry(identifier.clone())
            .or_insert_with(|| BehaviorBaseline::new(&identifier));

        // ベースライン更新（学習）
        self.update_baseline(baseline, request);

        // ベースラインが十分に確立されていない場合は学習モード
        if baseline.sample_count < self.config.min_sample_size {
            debug!(
                "Insufficient samples ({}) for baseline, continuing learning",
                baseline.sample_count
            );
            return Ok(BehavioralAnalysisResult {
                is_anomalous: false,
                anomaly_score: 0.0,
                anomaly_types: Vec::new(),
                details: "Learning mode: Insufficient baseline data".to_string(),
            });
        }

        // 異常検知
        let mut anomaly_types = Vec::new();
        let mut anomaly_scores = Vec::new();

        // 1. リクエスト頻度の異常検知
        if let Some(rate_score) = self.check_request_rate_anomaly(&identifier, baseline).await {
            if rate_score > self.config.anomaly_threshold {
                anomaly_types.push(AnomalyType::AbnormalRequestRate);
                anomaly_scores.push(rate_score);
            }
        }

        // 2. パスの異常検知
        if let Some(path_score) = self.check_path_anomaly(&request.path, baseline) {
            if path_score > self.config.anomaly_threshold {
                anomaly_types.push(AnomalyType::UnusualPath);
                anomaly_scores.push(path_score);
            }
        }

        // 3. リクエストサイズの異常検知
        let request_size = request.body.as_ref().map(|b| b.len()).unwrap_or(0);
        if let Some(size_score) = self.check_size_anomaly(request_size, baseline) {
            if size_score > self.config.anomaly_threshold {
                anomaly_types.push(AnomalyType::AbnormalRequestSize);
                anomaly_scores.push(size_score);
            }
        }

        // 4. アクセス時間の異常検知
        if let Some(time_score) = self.check_time_anomaly(&request.timestamp, baseline) {
            if time_score > self.config.anomaly_threshold {
                anomaly_types.push(AnomalyType::UnusualAccessTime);
                anomaly_scores.push(time_score);
            }
        }

        // 総合異常スコア（最大値）
        let max_score = anomaly_scores.iter().cloned().fold(0.0, f64::max);
        let is_anomalous = max_score > self.config.anomaly_threshold;
        let anomaly_count = anomaly_types.len();

        if is_anomalous {
            warn!(
                "Behavioral anomaly detected for {}: score={:.2}, types={:?}",
                identifier, max_score, anomaly_types
            );
        }

        Ok(BehavioralAnalysisResult {
            is_anomalous,
            anomaly_score: max_score,
            anomaly_types,
            details: format!(
                "Anomaly analysis: {} patterns detected with max score {:.2}",
                anomaly_count, max_score
            ),
        })
    }

    /// ベースラインを取得
    pub async fn get_baseline(&self, identifier: &str) -> Option<BehaviorBaseline> {
        self.baselines.read().await.get(identifier).cloned()
    }

    /// 全ベースラインを取得
    pub async fn get_all_baselines(&self) -> Vec<BehaviorBaseline> {
        self.baselines.read().await.values().cloned().collect()
    }

    /// ベースラインをリセット
    pub async fn reset_baseline(&self, identifier: &str) {
        self.baselines.write().await.remove(identifier);
        info!("Reset baseline for identifier: {}", identifier);
    }

    /// 識別子を抽出
    fn extract_identifier(&self, request: &RequestData) -> String {
        // ヘッダーからユーザーIDを取得
        if let Some(user_id) = request.headers.get("X-User-ID") {
            return format!("user:{}", user_id);
        }

        // なければIPアドレス
        if let Some(ip) = request.source_ip {
            return format!("ip:{}", ip);
        }

        // 最後の手段：リクエストIDのハッシュ
        let id_len = request.request_id.len().min(8);
        format!("unknown:{}", &request.request_id[..id_len])
    }

    /// リクエストを記録
    async fn record_request(&self, request: &RequestData, identifier: &str) {
        let record = RequestRecord {
            identifier: identifier.to_string(),
            path: request.path.clone(),
            size: request.body.as_ref().map(|b| b.len()).unwrap_or(0),
            timestamp: request.timestamp,
            ip_address: request.source_ip,
        };

        let mut history = self.request_history.write().await;
        history.push_back(record);

        // 履歴サイズ制限
        while history.len() > self.config.max_history_size {
            history.pop_front();
        }
    }

    /// ベースラインを更新
    fn update_baseline(&self, baseline: &mut BehaviorBaseline, request: &RequestData) {
        baseline.sample_count += 1;

        // パス頻度を更新
        *baseline
            .typical_paths
            .entry(request.path.clone())
            .or_insert(0.0) += 1.0;

        // アクセス時間を記録
        let hour = request.timestamp.hour();
        if !baseline.typical_hours.contains(&hour) {
            baseline.typical_hours.push(hour);
        }

        // リクエストサイズの移動平均
        let request_size = request.body.as_ref().map(|b| b.len()).unwrap_or(0) as f64;
        let alpha = 0.1;
        baseline.avg_request_size =
            alpha * request_size + (1.0 - alpha) * baseline.avg_request_size;

        // リクエスト頻度の統計を更新（簡易的な移動平均）
        // 1サンプルあたりの平均頻度を計算
        let sample_rate = 1.0 / 60.0; // 1分あたり1リクエストと仮定
        baseline.avg_request_rate = 0.1 * sample_rate + 0.9 * baseline.avg_request_rate;

        // 標準偏差の簡易更新（実際の統計手法より簡略化）
        if baseline.sample_count > 10 {
            let deviation = (sample_rate - baseline.avg_request_rate).abs();
            baseline.request_rate_std_dev = 0.1 * deviation + 0.9 * baseline.request_rate_std_dev;
        }

        baseline.last_updated = Utc::now();
    }

    /// リクエスト頻度の異常をチェック
    async fn check_request_rate_anomaly(
        &self,
        identifier: &str,
        baseline: &BehaviorBaseline,
    ) -> Option<f64> {
        let history = self.request_history.read().await;

        // 過去1分間のリクエスト数をカウント
        let one_minute_ago = Utc::now() - Duration::minutes(1);
        let recent_count = history
            .iter()
            .filter(|r| r.identifier == identifier && r.timestamp > one_minute_ago)
            .count();

        let current_rate = recent_count as f64 / 60.0;

        // Z-score計算（標準偏差が0の場合は0を返す）
        if baseline.request_rate_std_dev > 0.0 {
            let z_score =
                (current_rate - baseline.avg_request_rate).abs() / baseline.request_rate_std_dev;

            // Z-scoreを0-1のスコアに正規化
            let score = (z_score / (self.config.statistical_sensitivity * 2.0)).min(1.0);
            Some(score)
        } else {
            // 標準偏差がない場合は単純比較
            if current_rate > baseline.avg_request_rate * 3.0 {
                Some(0.8)
            } else {
                Some(0.0)
            }
        }
    }

    /// パスの異常をチェック
    fn check_path_anomaly(&self, path: &str, baseline: &BehaviorBaseline) -> Option<f64> {
        // パスが既知かチェック
        if baseline.typical_paths.is_empty() {
            return Some(0.0);
        }

        let total_accesses: f64 = baseline.typical_paths.values().sum();
        let path_frequency = baseline.typical_paths.get(path).copied().unwrap_or(0.0);
        let path_probability = path_frequency / total_accesses;

        // 未知のパスまたは非常に稀なパス
        if path_probability < 0.01 {
            Some(0.8)
        } else if path_probability < 0.05 {
            Some(0.6)
        } else {
            Some(0.0)
        }
    }

    /// リクエストサイズの異常をチェック
    fn check_size_anomaly(&self, size: usize, baseline: &BehaviorBaseline) -> Option<f64> {
        if baseline.avg_request_size == 0.0 {
            return Some(0.0);
        }

        let size_ratio = size as f64 / baseline.avg_request_size;

        // 通常の10倍以上または1/10以下
        if !(0.1..=10.0).contains(&size_ratio) {
            Some(0.9)
        } else if !(0.2..=5.0).contains(&size_ratio) {
            Some(0.7)
        } else if !(0.33..=3.0).contains(&size_ratio) {
            Some(0.5)
        } else {
            Some(0.0)
        }
    }

    /// アクセス時間の異常をチェック
    fn check_time_anomaly(
        &self,
        timestamp: &DateTime<Utc>,
        baseline: &BehaviorBaseline,
    ) -> Option<f64> {
        if baseline.typical_hours.is_empty() {
            return Some(0.0);
        }

        let current_hour = timestamp.hour();

        // 現在の時刻が典型的なアクセス時間帯かチェック
        if baseline.typical_hours.contains(&current_hour) {
            Some(0.0)
        } else {
            // 隣接する時間帯もチェック
            let adjacent_hours = [(current_hour + 23) % 24, (current_hour + 1) % 24];

            if adjacent_hours
                .iter()
                .any(|h| baseline.typical_hours.contains(h))
            {
                Some(0.4)
            } else {
                Some(0.7)
            }
        }
    }
}

impl BehaviorBaseline {
    /// 新しいベースラインを作成
    pub fn new(identifier: &str) -> Self {
        Self {
            identifier: identifier.to_string(),
            avg_request_rate: 0.0,
            request_rate_std_dev: 0.0,
            typical_paths: HashMap::new(),
            avg_request_size: 0.0,
            typical_hours: Vec::new(),
            avg_session_duration: 0.0,
            last_updated: Utc::now(),
            sample_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_behavioral_detector_initialization() {
        let detector = BehavioralDetector::new().await;
        assert!(detector.is_ok());
    }

    #[tokio::test]
    async fn test_baseline_creation() {
        let baseline = BehaviorBaseline::new("user:test");
        assert_eq!(baseline.identifier, "user:test");
        assert_eq!(baseline.sample_count, 0);
    }

    #[tokio::test]
    async fn test_learning_mode() {
        let detector = BehavioralDetector::new().await.unwrap();

        let request = RequestData {
            request_id: "test-001".to_string(),
            method: "GET".to_string(),
            path: "/api/test".to_string(),
            query_params: HashMap::new(),
            headers: [("X-User-ID".to_string(), "user123".to_string())].into(),
            body: None,
            source_ip: Some("192.168.1.1".parse().unwrap()),
            timestamp: Utc::now(),
        };

        let result = detector.analyze(&request).await.unwrap();

        // 学習モード中は異常を検知しない
        assert!(!result.is_anomalous);
        assert_eq!(result.anomaly_score, 0.0);
    }

    #[tokio::test]
    async fn test_baseline_update() {
        let detector = BehavioralDetector::new().await.unwrap();

        let mut request = RequestData {
            request_id: "test-001".to_string(),
            method: "GET".to_string(),
            path: "/api/products".to_string(),
            query_params: HashMap::new(),
            headers: [("X-User-ID".to_string(), "user123".to_string())].into(),
            body: None,
            source_ip: Some("192.168.1.1".parse().unwrap()),
            timestamp: Utc::now(),
        };

        // 複数回リクエストを記録
        for i in 0..10 {
            request.request_id = format!("test-{:03}", i);
            let _ = detector.analyze(&request).await;
        }

        let baseline = detector.get_baseline("user:user123").await;
        assert!(baseline.is_some());

        let baseline = baseline.unwrap();
        assert_eq!(baseline.sample_count, 10);
        assert!(baseline.typical_paths.contains_key("/api/products"));
    }
}
