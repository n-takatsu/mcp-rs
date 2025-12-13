//! 異常検知システム

use crate::monitoring::metrics::{MetricStats, MetricType, MetricValue};
use serde::{Deserialize, Serialize};

/// 異常検知結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResult {
    /// 異常フラグ
    pub is_anomaly: bool,
    /// 異常スコア（0.0-1.0）
    pub score: f64,
    /// 理由
    pub reason: String,
}

/// 異常検知器
pub struct AnomalyDetector {
    /// Z-scoreの閾値
    zscore_threshold: f64,
}

impl AnomalyDetector {
    /// 新しい検知器を作成
    pub fn new() -> Self {
        Self {
            zscore_threshold: 3.0, // 3シグマルール
        }
    }

    /// Z-scoreベースの異常検知
    pub fn detect_zscore(&self, value: f64, stats: &MetricStats) -> AnomalyResult {
        if stats.std_dev == 0.0 {
            return AnomalyResult {
                is_anomaly: false,
                score: 0.0,
                reason: "Insufficient variance".to_string(),
            };
        }

        let zscore = ((value - stats.mean) / stats.std_dev).abs();
        let is_anomaly = zscore > self.zscore_threshold;

        AnomalyResult {
            is_anomaly,
            score: (zscore / self.zscore_threshold).min(1.0),
            reason: format!("Z-score: {:.2}", zscore),
        }
    }

    /// IQRベースの異常検知
    pub fn detect_iqr(&self, values: &[f64]) -> Vec<AnomalyResult> {
        if values.len() < 4 {
            return values
                .iter()
                .map(|_| AnomalyResult {
                    is_anomaly: false,
                    score: 0.0,
                    reason: "Insufficient data".to_string(),
                })
                .collect();
        }

        let stats = MetricStats::from_values(values.to_vec());
        let iqr = stats.p95 - stats.p50;
        let lower_bound = stats.p50 - 1.5 * iqr;
        let upper_bound = stats.p95 + 1.5 * iqr;

        values
            .iter()
            .map(|&v| {
                let is_anomaly = v < lower_bound || v > upper_bound;
                let distance = if v < lower_bound {
                    lower_bound - v
                } else if v > upper_bound {
                    v - upper_bound
                } else {
                    0.0
                };

                AnomalyResult {
                    is_anomaly,
                    score: (distance / iqr).min(1.0),
                    reason: format!(
                        "Value: {:.2}, Range: [{:.2}, {:.2}]",
                        v, lower_bound, upper_bound
                    ),
                }
            })
            .collect()
    }

    /// 移動平均ベースの異常検知
    pub fn detect_moving_average(
        &self,
        recent_values: &[f64],
        window_size: usize,
    ) -> AnomalyResult {
        if recent_values.len() < window_size {
            return AnomalyResult {
                is_anomaly: false,
                score: 0.0,
                reason: "Insufficient data".to_string(),
            };
        }

        let ma: f64 =
            recent_values.iter().rev().take(window_size).sum::<f64>() / window_size as f64;
        let current = recent_values.last().copied().unwrap_or(0.0);
        let deviation = (current - ma).abs();
        let relative_deviation = if ma != 0.0 { deviation / ma } else { 0.0 };

        let is_anomaly = relative_deviation > 0.5; // 50%以上の偏差

        AnomalyResult {
            is_anomaly,
            score: relative_deviation.min(1.0),
            reason: format!(
                "MA: {:.2}, Current: {:.2}, Deviation: {:.2}%",
                ma,
                current,
                relative_deviation * 100.0
            ),
        }
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_detection() {
        let detector = AnomalyDetector::new();
        let values = vec![10.0, 12.0, 11.0, 13.0, 12.0];
        let stats = MetricStats::from_values(values);

        // 正常値
        let result = detector.detect_zscore(12.0, &stats);
        assert!(!result.is_anomaly);

        // 異常値
        let result = detector.detect_zscore(50.0, &stats);
        assert!(result.is_anomaly);
    }

    #[test]
    fn test_iqr_detection() {
        let detector = AnomalyDetector::new();
        let values = vec![10.0, 12.0, 11.0, 13.0, 100.0]; // 100.0は異常値

        let results = detector.detect_iqr(&values);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_moving_average_detection() {
        let detector = AnomalyDetector::new();
        let values = vec![10.0, 11.0, 12.0, 11.0, 50.0]; // 50.0は異常値

        let result = detector.detect_moving_average(&values, 4);
        assert!(result.is_anomaly);
    }
}
