//! Machine Learning Anomaly Detection
//!
//! 機械学習を活用した高度な異常検知システム。
//!
//! ## 主要機能
//!
//! - **特徴量抽出**: リクエストデータから機械学習用の特徴量を生成
//! - **ベースライン学習**: 正常パターンを学習してベースラインモデルを構築
//! - **異常検知**: 統計的手法とML手法による異常パターン検出
//! - **モデル管理**: モデルのバージョニング、更新、パフォーマンス監視
//!
//! ## 使用例
//!
//! ```rust,no_run
//! use mcp_rs::security::ids::ml::{MLAnomalyDetector, MLConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ml_detector = MLAnomalyDetector::new(MLConfig::default()).await?;
//!
//! // 学習モード
//! ml_detector.train(&training_data).await?;
//!
//! // 異常検知
//! let result = ml_detector.detect(&request_data).await?;
//! if result.is_anomaly {
//!     println!("異常検知: スコア={}", result.anomaly_score);
//! }
//! # Ok(())
//! # }
//! ```

pub mod detector;
pub mod features;
pub mod models;

use crate::error::McpError;
use crate::security::ids::RequestData;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use detector::MLAnomalyDetector;
pub use features::FeatureExtractor;

/// ML異常検知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLConfig {
    /// 特徴量の次元数
    pub feature_dimensions: usize,
    /// 異常スコアのしきい値（0.0-1.0）
    pub anomaly_threshold: f64,
    /// 学習に必要な最小サンプル数
    pub min_training_samples: usize,
    /// モデル更新間隔（秒）
    pub model_update_interval_secs: u64,
    /// 統計的感度（Z-scoreのしきい値）
    pub statistical_sensitivity: f64,
    /// オンライン学習の有効化
    pub online_learning: bool,
    /// モデルの保存パス
    pub model_save_path: Option<String>,
}

impl Default for MLConfig {
    fn default() -> Self {
        Self {
            feature_dimensions: 20,
            anomaly_threshold: 0.7,
            min_training_samples: 100,
            model_update_interval_secs: 3600, // 1時間
            statistical_sensitivity: 3.0,     // 3-sigma
            online_learning: true,
            model_save_path: Some("models/ml_anomaly.bin".to_string()),
        }
    }
}

/// ML異常検知結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLDetectionResult {
    /// 異常フラグ
    pub is_anomaly: bool,
    /// 異常スコア（0.0-1.0）
    pub anomaly_score: f64,
    /// 信頼度（0.0-1.0）
    pub confidence: f64,
    /// 検知手法
    pub detection_method: DetectionMethod,
    /// 異常パターンの説明
    pub explanation: String,
    /// 特徴量の重要度
    pub feature_importance: HashMap<String, f64>,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}

/// 検知手法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionMethod {
    /// 統計的異常検知
    Statistical,
    /// Isolation Forest
    IsolationForest,
    /// One-Class SVM
    OneClassSVM,
    /// K-means Clustering
    KMeansClustering,
    /// アンサンブル（複数手法の組み合わせ）
    Ensemble,
}

/// 特徴量ベクトル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    /// 特徴量データ
    pub features: Vec<f64>,
    /// 特徴量名
    pub feature_names: Vec<String>,
    /// 元のリクエストID
    pub request_id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}

/// モデル統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    /// トレーニングサンプル数
    pub training_samples: usize,
    /// 最終更新日時
    pub last_updated: DateTime<Utc>,
    /// モデルバージョン
    pub model_version: u32,
    /// 検知精度（テストデータでの評価）
    pub accuracy: Option<f64>,
    /// 誤検知率
    pub false_positive_rate: Option<f64>,
    /// 真陽性率
    pub true_positive_rate: Option<f64>,
}
