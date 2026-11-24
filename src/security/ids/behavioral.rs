//! Behavioral Detection
//!
//! ベースライン学習と異常行動パターン検出を実行します。

use super::RequestData;
use crate::error::McpError;
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
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

/// リクエスト記録（頻度の分析に使う。パス・サイズは`update_baseline`で
/// ベースラインに反映するので、ここには持たない）
#[derive(Debug, Clone)]
struct RequestRecord {
    /// 識別子（ユーザーID or IP）
    identifier: String,
    /// タイムスタンプ
    timestamp: DateTime<Utc>,
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
    /// ベースラインを持つ呼び出し元の数の上限（#543）。超えると、最後に
    /// 更新した時刻の古いものから消す
    #[serde(default = "default_max_baselines")]
    pub max_baselines: usize,
}

fn default_max_baselines() -> usize {
    10_000
}

/// 1つのベースラインで覚えておく、パスの数の上限（#543）
const MAX_PATHS_PER_BASELINE: usize = 100;

/// この時間更新していないベースラインは、`cleanup_old_data`で消す（#543）
const BASELINE_RETENTION_HOURS: i64 = 24 * 7;

impl Default for BehavioralConfig {
    fn default() -> Self {
        Self {
            learning_period_hours: 24,
            anomaly_threshold: 0.7,
            max_history_size: 10000,
            statistical_sensitivity: 2.0,
            min_sample_size: 100,
            max_baselines: default_max_baselines(),
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

        // この呼び出し元の、今の頻度（#759。学習にも、判定にも使う）
        let current_rate = self.current_request_rate(&identifier).await;

        // ベースラインを取得または作成
        let mut baselines = self.baselines.write().await;
        if !baselines.contains_key(&identifier) {
            // 上限（#543）
            super::network::evict_oldest(&mut baselines, self.config.max_baselines, |b| {
                Some(b.last_updated)
            });
        }
        let baseline = baselines
            .entry(identifier.clone())
            .or_insert_with(|| BehaviorBaseline::new(&identifier));

        // パスは、このリクエストを覚える前の状態で判定する（PR #728の
        // レビュー。覚えた後では、未知のパスも1回は見たことになり、上限で
        // 入れ替えると、毎回違うパスを送られても検知しなかった）
        let path_score = self.check_path_anomaly(&request.path, baseline);
        // 頻度も、学習する前のベースラインと比べる（#759）
        let rate_score = self.check_request_rate_anomaly(current_rate, baseline);

        // ベースライン更新（学習）
        self.update_baseline(baseline, request, current_rate);

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

        // 1. リクエスト頻度の異常検知（学習する前のベースラインと比べたもの）
        if rate_score > self.config.anomaly_threshold {
            anomaly_types.push(AnomalyType::AbnormalRequestRate);
            anomaly_scores.push(rate_score);
        }

        // 2. パスの異常検知（覚える前の状態で判定したもの）
        if let Some(path_score) = path_score {
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

    /// 長く更新していないベースラインを消す（#543）
    pub async fn cleanup_old_data(&self) {
        let cutoff = Utc::now() - Duration::hours(BASELINE_RETENTION_HOURS);
        self.baselines
            .write()
            .await
            .retain(|_, baseline| baseline.last_updated > cutoff);
    }

    /// ベースラインをリセット
    pub async fn reset_baseline(&self, identifier: &str) {
        self.baselines.write().await.remove(identifier);
        info!("Reset baseline for identifier: {}", identifier);
    }

    /// 識別子を抽出
    fn extract_identifier(&self, request: &RequestData) -> String {
        // 認証した呼び出し元（#543）。以前は`X-User-ID`ヘッダーを使って
        // いたが、クライアントが何でも書けるので、他人の名前で記録させたり、
        // 毎回違う値で、記録を際限なく増やしたりできた
        if let Some(caller_id) = &request.caller_id {
            return format!("caller:{}", caller_id);
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
            timestamp: request.timestamp,
        };

        let mut history = self.request_history.write().await;
        history.push_back(record);

        // 履歴サイズ制限
        while history.len() > self.config.max_history_size {
            history.pop_front();
        }
    }

    /// ベースラインを更新。`current_rate`は、この呼び出し元の今の頻度
    /// （1秒あたり。[`Self::current_request_rate`]）
    fn update_baseline(
        &self,
        baseline: &mut BehaviorBaseline,
        request: &RequestData,
        current_rate: f64,
    ) {
        baseline.sample_count += 1;

        // パス頻度を更新（覚えるパスは、上限まで。#543）。上限に達したら、
        // いちばん少ないパスを忘れて、新しいパスを覚える（#724。以前は新しい
        // パスを覚えず、そのパスを何度使っても、未知のパスのままだった）
        if let Some(count) = baseline.typical_paths.get_mut(&request.path) {
            *count += 1.0;
        } else {
            if baseline.typical_paths.len() >= MAX_PATHS_PER_BASELINE {
                let rarest = baseline
                    .typical_paths
                    .iter()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(path, _)| path.clone());
                if let Some(rarest) = rarest {
                    baseline.typical_paths.remove(&rarest);
                }
            }
            baseline.typical_paths.insert(request.path.clone(), 1.0);
        }

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

        // リクエスト頻度の平均と標準偏差を、実測した頻度の指数移動平均で
        // 更新する（#759。以前は、頻度を常に毎分1件として学習したので、
        // 分散が0に近づき、普通の通信でも異常スコアが1.0になった）
        if baseline.sample_count == 1 {
            baseline.avg_request_rate = current_rate;
            baseline.request_rate_std_dev = 0.0;
        } else {
            let alpha = 0.1;
            let diff = current_rate - baseline.avg_request_rate;
            let variance = baseline.request_rate_std_dev.powi(2);
            baseline.avg_request_rate += alpha * diff;
            baseline.request_rate_std_dev =
                ((1.0 - alpha) * (variance + alpha * diff * diff)).sqrt();
        }

        baseline.last_updated = Utc::now();
    }

    /// この呼び出し元の今の頻度（1秒あたり。過去1分間の数から）
    async fn current_request_rate(&self, identifier: &str) -> f64 {
        let history = self.request_history.read().await;
        let one_minute_ago = Utc::now() - Duration::minutes(1);
        let recent_count = history
            .iter()
            .filter(|r| r.identifier == identifier && r.timestamp > one_minute_ago)
            .count();
        recent_count as f64 / 60.0
    }

    /// リクエスト頻度の異常をチェック（#759）。今の頻度を、ベースラインの
    /// 平均より多い側だけ、Zスコアで比べる（少ないのは攻撃ではない）。
    /// 標準偏差には下限を置く: 1分の数のばらつき（ポアソン分布の
    /// `sqrt(平均の数)`、少なくとも1件）より小さくしない。一定の頻度の
    /// 呼び出し元でも、分散が0になって、1件の違いで異常にならないように
    fn check_request_rate_anomaly(&self, current_rate: f64, baseline: &BehaviorBaseline) -> f64 {
        let mean_per_minute = baseline.avg_request_rate * 60.0;
        let std_dev_floor = mean_per_minute.max(1.0).sqrt() / 60.0;
        let std_dev = baseline.request_rate_std_dev.max(std_dev_floor);
        let z_score = (current_rate - baseline.avg_request_rate).max(0.0) / std_dev;
        // Z-scoreを0-1のスコアに正規化
        (z_score / (self.config.statistical_sensitivity * 2.0)).min(1.0)
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

    /// 覚えたパスが上限に達した後も、未知のパスは、覚える前の状態で判定して
    /// 検知する（PR #728のレビュー）
    #[tokio::test]
    async fn test_unknown_path_is_detected_after_the_paths_are_full() {
        let detector = BehavioralDetector::new().await.unwrap();
        let request = |path: String| RequestData {
            request_id: "r".to_string(),
            method: "GET".to_string(),
            path,
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some("192.0.2.1".parse().unwrap()),
            caller_id: Some("api_key:worker".to_string()),
            timestamp: Utc::now(),
        };
        // 毎回違うパスで、学習（`min_sample_size`）を終え、パスを上限まで埋める
        for i in 0..MAX_PATHS_PER_BASELINE {
            detector.analyze(&request(format!("/p/{i}"))).await.unwrap();
        }
        let result = detector
            .analyze(&request("/never-seen".to_string()))
            .await
            .unwrap();
        assert!(
            matches!(result.anomaly_types.as_slice(), types if types.iter().any(|t| matches!(t, AnomalyType::UnusualPath))),
            "{:?}",
            result.anomaly_types
        );
    }

    /// 覚えるパスが上限に達したら、いちばん少ないパスを忘れて、新しいパスを
    /// 覚える（#724）
    #[tokio::test]
    async fn test_baseline_paths_replace_the_rarest() {
        let detector = BehavioralDetector::new().await.unwrap();
        let mut baseline = BehaviorBaseline::new("user:test");
        let request = |path: &str| RequestData {
            request_id: "r".to_string(),
            method: "GET".to_string(),
            path: path.to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            caller_id: None,
            timestamp: Utc::now(),
        };
        // よく使うパスと、1回だけのパスで埋める
        for _ in 0..5 {
            detector.update_baseline(&mut baseline, &request("/mcp"), 1.0 / 60.0);
        }
        for i in 1..MAX_PATHS_PER_BASELINE {
            detector.update_baseline(&mut baseline, &request(&format!("/once/{i}")), 1.0 / 60.0);
        }
        assert_eq!(baseline.typical_paths.len(), MAX_PATHS_PER_BASELINE);
        detector.update_baseline(&mut baseline, &request("/new"), 1.0 / 60.0);
        assert_eq!(baseline.typical_paths.len(), MAX_PATHS_PER_BASELINE);
        assert!(baseline.typical_paths.contains_key("/new"));
        // よく使うパスは残る
        assert_eq!(baseline.typical_paths["/mcp"], 5.0);
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
            caller_id: None,
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
            // ヘッダーは、クライアントが何でも書けるので使わない（#543）
            headers: [("X-User-ID".to_string(), "someone-else".to_string())].into(),
            body: None,
            source_ip: Some("192.168.1.1".parse().unwrap()),
            caller_id: Some("user123".to_string()),
            timestamp: Utc::now(),
        };

        // 複数回リクエストを記録
        for i in 0..10 {
            request.request_id = format!("test-{:03}", i);
            let _ = detector.analyze(&request).await;
        }

        assert!(detector.get_baseline("user:someone-else").await.is_none());
        let baseline = detector.get_baseline("caller:user123").await;
        assert!(baseline.is_some());

        let baseline = baseline.unwrap();
        assert_eq!(baseline.sample_count, 10);
        assert!(baseline.typical_paths.contains_key("/api/products"));
    }

    /// 頻度は、呼び出し元ごとの実測値で学習する（#759。以前は常に毎分1件と
    /// して学習し、分散が0に近づいて、普通の通信でも異常スコアが1.0になった）
    #[tokio::test]
    async fn test_request_rate_baseline_learns_the_measured_rate() {
        let detector = BehavioralDetector::new().await.unwrap();
        let mut baseline = BehaviorBaseline::new("caller:steady");
        let request = RequestData {
            request_id: "r".to_string(),
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            caller_id: Some("steady".to_string()),
            timestamp: Utc::now(),
        };
        let per_minute = |n: f64| n / 60.0;
        // 毎分2件のペースを続ける
        for _ in 0..500 {
            detector.update_baseline(&mut baseline, &request, per_minute(2.0));
        }
        assert!((baseline.avg_request_rate - per_minute(2.0)).abs() < 1e-9);
        let threshold = detector.config.anomaly_threshold;
        // 同じペースや、1件多いだけでは異常にしない
        assert_eq!(
            detector.check_request_rate_anomaly(per_minute(2.0), &baseline),
            0.0
        );
        assert!(detector.check_request_rate_anomaly(per_minute(3.0), &baseline) < threshold);
        // 少ないのは異常にしない
        assert_eq!(detector.check_request_rate_anomaly(0.0, &baseline), 0.0);
        // 大きく増えたら異常にする
        assert!(detector.check_request_rate_anomaly(per_minute(30.0), &baseline) > threshold);
    }

    /// `analyze`は、今の頻度を、学習する前のベースラインと比べる（#759）
    #[tokio::test]
    async fn test_analyze_compares_the_current_rate_with_the_learned_baseline() {
        let detector = BehavioralDetector::new().await.unwrap();
        let now = Utc::now();
        let request = |caller: &str| RequestData {
            request_id: "r".to_string(),
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            caller_id: Some(caller.to_string()),
            timestamp: now,
        };
        // どちらも毎分2件を学習済み
        for caller in ["steady", "burst"] {
            let mut baseline = BehaviorBaseline::new(&format!("caller:{caller}"));
            baseline.sample_count = 1_000;
            baseline.avg_request_rate = 2.0 / 60.0;
            baseline.typical_paths.insert("/mcp".to_string(), 1_000.0);
            baseline.typical_hours.push(now.hour());
            detector
                .baselines
                .write()
                .await
                .insert(format!("caller:{caller}"), baseline);
        }
        // 直前の1分に、steadyは1件、burstは40件
        {
            let mut history = detector.request_history.write().await;
            history.push_back(RequestRecord {
                identifier: "caller:steady".to_string(),
                timestamp: now,
            });
            for _ in 0..40 {
                history.push_back(RequestRecord {
                    identifier: "caller:burst".to_string(),
                    timestamp: now,
                });
            }
        }

        let steady = detector.analyze(&request("steady")).await.unwrap();
        assert!(!steady.is_anomalous, "{:?}", steady.anomaly_types);

        let burst = detector.analyze(&request("burst")).await.unwrap();
        assert!(
            burst
                .anomaly_types
                .contains(&AnomalyType::AbnormalRequestRate),
            "{:?}",
            burst.anomaly_types
        );
    }
}
