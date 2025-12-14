//! Prediction Types
//!
//! 予測分析用の型定義

use serde::{Deserialize, Serialize};

/// トレンド方向
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrendDirection {
    /// 上昇トレンド
    Increasing,
    /// 下降トレンド
    Decreasing,
    /// 安定
    Stable,
}

/// 予測結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    /// 予測値
    pub predicted_value: f64,
    /// トレンド方向
    pub trend: TrendDirection,
    /// トレンド強度（0.0-1.0）
    pub trend_strength: f64,
    /// 信頼度（0.0-1.0）
    pub confidence: f64,
}

impl PredictionResult {
    /// 新しい予測結果を作成
    pub fn new(
        predicted_value: f64,
        trend: TrendDirection,
        trend_strength: f64,
        confidence: f64,
    ) -> Self {
        Self {
            predicted_value,
            trend,
            trend_strength: trend_strength.clamp(0.0, 1.0),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}
