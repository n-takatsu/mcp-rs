//! Trend Detection
//!
//! トレンド検出器の実装

use super::types::TrendDirection;
use crate::monitoring::MetricPoint;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::MetricType;

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
}
