//! Prediction Analytics Module
//!
//! 予測分析システム

use crate::monitoring::MetricPoint;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 時系列予測モデル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPredictor {
    /// 履歴データバッファ
    history: VecDeque<f64>,
    /// 最大履歴サイズ
    max_history: usize,
}

impl TimeSeriesPredictor {
    /// 新しい予測器を作成
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
        }
    }

    /// データポイントを追加
    pub fn add_point(&mut self, value: f64) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(value);
    }

    /// 移動平均予測
    pub fn predict_moving_average(&self, window: usize) -> Option<f64> {
        if self.history.len() < window {
            return None;
        }

        let sum: f64 = self.history.iter().rev().take(window).sum();
        Some(sum / window as f64)
    }

    /// 線形回帰予測
    pub fn predict_linear_regression(&self) -> Option<f64> {
        let n = self.history.len();
        if n < 2 {
            return None;
        }

        let x_values: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y_values: Vec<f64> = self.history.iter().copied().collect();

        let x_mean = x_values.iter().sum::<f64>() / n as f64;
        let y_mean = y_values.iter().sum::<f64>() / n as f64;

        let mut numerator = 0.0;
        let mut denominator = 0.0;

        for i in 0..n {
            let x_diff = x_values[i] - x_mean;
            let y_diff = y_values[i] - y_mean;
            numerator += x_diff * y_diff;
            denominator += x_diff * x_diff;
        }

        if denominator.abs() < f64::EPSILON {
            return None;
        }

        let slope = numerator / denominator;
        let intercept = y_mean - slope * x_mean;

        // 次のポイントを予測
        Some(slope * n as f64 + intercept)
    }

    /// 指数平滑化予測
    pub fn predict_exponential_smoothing(&self, alpha: f64) -> Option<f64> {
        if self.history.is_empty() {
            return None;
        }

        let mut forecast = self.history[0];
        for &value in self.history.iter().skip(1) {
            forecast = alpha * value + (1.0 - alpha) * forecast;
        }

        Some(forecast)
    }
}

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

/// トレンド検出器
pub struct TrendDetector {
    /// 安定判定の閾値
    stability_threshold: f64,
}

impl TrendDetector {
    /// 新しいトレンド検出器を作成
    pub fn new(stability_threshold: f64) -> Self {
        Self {
            stability_threshold,
        }
    }

    /// トレンドを検出
    pub fn detect_trend(&self, metrics: &[MetricPoint]) -> Option<TrendDirection> {
        if metrics.len() < 2 {
            return None;
        }

        let values: Vec<f64> = metrics.iter().map(|m| m.value).collect();
        let first = values.first()?;
        let last = values.last()?;

        let change_rate = (last - first) / first * 100.0;

        if change_rate.abs() < self.stability_threshold {
            Some(TrendDirection::Stable)
        } else if change_rate > 0.0 {
            Some(TrendDirection::Increasing)
        } else {
            Some(TrendDirection::Decreasing)
        }
    }

    /// トレンド強度を計算
    pub fn calculate_trend_strength(&self, metrics: &[MetricPoint]) -> Option<f64> {
        if metrics.len() < 2 {
            return None;
        }

        let values: Vec<f64> = metrics.iter().map(|m| m.value).collect();
        let n = values.len();

        // 線形回帰の決定係数（R²）を計算
        let x_values: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let x_mean = x_values.iter().sum::<f64>() / n as f64;
        let y_mean = values.iter().sum::<f64>() / n as f64;

        let mut ss_res = 0.0;
        let mut ss_tot = 0.0;
        let mut numerator = 0.0;
        let mut denominator = 0.0;

        for i in 0..n {
            let x_diff = x_values[i] - x_mean;
            let y_diff = values[i] - y_mean;
            numerator += x_diff * y_diff;
            denominator += x_diff * x_diff;
        }

        if denominator.abs() < f64::EPSILON {
            return None;
        }

        let slope = numerator / denominator;
        let intercept = y_mean - slope * x_mean;

        for i in 0..n {
            let predicted = slope * x_values[i] + intercept;
            ss_res += (values[i] - predicted).powi(2);
            ss_tot += (values[i] - y_mean).powi(2);
        }

        if ss_tot.abs() < f64::EPSILON {
            return None;
        }

        Some(1.0 - (ss_res / ss_tot))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::MetricType;

    #[test]
    fn test_time_series_predictor_creation() {
        let predictor = TimeSeriesPredictor::new(10);
        assert_eq!(predictor.max_history, 10);
    }

    #[test]
    fn test_add_point() {
        let mut predictor = TimeSeriesPredictor::new(3);
        predictor.add_point(10.0);
        predictor.add_point(20.0);
        predictor.add_point(30.0);

        assert_eq!(predictor.history.len(), 3);

        predictor.add_point(40.0);
        assert_eq!(predictor.history.len(), 3);
        assert_eq!(predictor.history[0], 20.0);
    }

    #[test]
    fn test_predict_moving_average() {
        let mut predictor = TimeSeriesPredictor::new(10);
        predictor.add_point(10.0);
        predictor.add_point(20.0);
        predictor.add_point(30.0);

        let prediction = predictor.predict_moving_average(3);
        assert_eq!(prediction, Some(20.0));
    }

    #[test]
    fn test_predict_linear_regression() {
        let mut predictor = TimeSeriesPredictor::new(10);
        for i in 1..=5 {
            predictor.add_point(i as f64 * 10.0);
        }

        let prediction = predictor.predict_linear_regression();
        assert!(prediction.is_some());
        assert!(prediction.unwrap() > 50.0);
    }

    #[test]
    fn test_predict_exponential_smoothing() {
        let mut predictor = TimeSeriesPredictor::new(10);
        predictor.add_point(10.0);
        predictor.add_point(20.0);
        predictor.add_point(30.0);

        let prediction = predictor.predict_exponential_smoothing(0.5);
        assert!(prediction.is_some());
    }

    #[test]
    fn test_trend_detector() {
        let detector = TrendDetector::new(5.0);

        let increasing_metrics = vec![
            MetricPoint::new(MetricType::Cpu, 10.0),
            MetricPoint::new(MetricType::Cpu, 20.0),
            MetricPoint::new(MetricType::Cpu, 30.0),
        ];

        let trend = detector.detect_trend(&increasing_metrics);
        assert_eq!(trend, Some(TrendDirection::Increasing));
    }

    #[test]
    fn test_trend_strength() {
        let detector = TrendDetector::new(5.0);

        let linear_metrics = vec![
            MetricPoint::new(MetricType::Cpu, 10.0),
            MetricPoint::new(MetricType::Cpu, 20.0),
            MetricPoint::new(MetricType::Cpu, 30.0),
        ];

        let strength = detector.calculate_trend_strength(&linear_metrics);
        assert!(strength.is_some());
        assert!(strength.unwrap() > 0.9); // 線形データなのでR²は高い
    }

    #[test]
    fn test_prediction_result() {
        let result = PredictionResult::new(50.0, TrendDirection::Increasing, 0.8, 0.9);
        assert_eq!(result.predicted_value, 50.0);
        assert_eq!(result.trend, TrendDirection::Increasing);
        assert_eq!(result.trend_strength, 0.8);
        assert_eq!(result.confidence, 0.9);
    }
}
