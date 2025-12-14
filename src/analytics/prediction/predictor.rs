//! Time Series Predictor
//!
//! 時系列予測器の実装

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
