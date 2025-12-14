//! Anomaly Detection Types
//!
//! 異常検知用の型定義

use serde::{Deserialize, Serialize};

/// 異常検知アルゴリズム
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnomalyDetectionAlgorithm {
    /// Z-スコア法
    ZScore { threshold: f64 },
    /// IQR法（四分位範囲）
    Iqr { multiplier: f64 },
    /// 移動平均法
    MovingAverage { window: usize, threshold: f64 },
}

/// 異常スコア
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyScore {
    /// 異常度（0.0-1.0）
    pub score: f64,
    /// 異常フラグ
    pub is_anomaly: bool,
    /// 検知アルゴリズム
    pub algorithm: AnomalyDetectionAlgorithm,
    /// 説明
    pub explanation: String,
}

impl AnomalyScore {
    /// 新しい異常スコアを作成
    pub fn new(
        score: f64,
        is_anomaly: bool,
        algorithm: AnomalyDetectionAlgorithm,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            is_anomaly,
            algorithm,
            explanation: explanation.into(),
        }
    }
}
