//! Anomaly Detector Implementation
//!
//! 異常検知器の実装

use super::types::{AnomalyDetectionAlgorithm, AnomalyScore};
use std::collections::VecDeque;

/// 異常検知器
pub struct AnomalyDetector {
    /// 履歴データバッファ
    history: VecDeque<f64>,
    /// 最大履歴サイズ
    max_history: usize,
    /// 検知アルゴリズム
    algorithm: AnomalyDetectionAlgorithm,
}

impl AnomalyDetector {
    /// 新しい異常検知器を作成
    pub fn new(max_history: usize, algorithm: AnomalyDetectionAlgorithm) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            algorithm,
        }
    }

    /// データポイントを追加
    pub fn add_point(&mut self, value: f64) {
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(value);
    }

    /// 異常を検知
    pub fn detect(&self, value: f64) -> AnomalyScore {
        match &self.algorithm {
            AnomalyDetectionAlgorithm::ZScore { threshold } => {
                self.detect_zscore(value, *threshold)
            }
            AnomalyDetectionAlgorithm::Iqr { multiplier } => self.detect_iqr(value, *multiplier),
            AnomalyDetectionAlgorithm::MovingAverage { window, threshold } => {
                self.detect_moving_average(value, *window, *threshold)
            }
        }
    }

    /// Z-スコア法による異常検知
    fn detect_zscore(&self, value: f64, threshold: f64) -> AnomalyScore {
        if self.history.len() < 2 {
            return AnomalyScore::new(0.0, false, self.algorithm.clone(), "Insufficient data");
        }

        let mean = self.history.iter().sum::<f64>() / self.history.len() as f64;
        let variance = self.history.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
            / self.history.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev < f64::EPSILON {
            return AnomalyScore::new(0.0, false, self.algorithm.clone(), "Zero variance");
        }

        let z_score = ((value - mean) / std_dev).abs();
        let is_anomaly = z_score > threshold;

        AnomalyScore::new(
            (z_score / (threshold * 2.0)).min(1.0),
            is_anomaly,
            self.algorithm.clone(),
            format!("Z-score: {:.2}, threshold: {:.2}", z_score, threshold),
        )
    }

    /// IQR法による異常検知
    fn detect_iqr(&self, value: f64, multiplier: f64) -> AnomalyScore {
        if self.history.len() < 4 {
            return AnomalyScore::new(
                0.0,
                false,
                self.algorithm.clone(),
                "Insufficient data for IQR",
            );
        }

        let mut sorted: Vec<f64> = self.history.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let n = sorted.len();
        let q1 = sorted[n / 4];
        let q3 = sorted[(3 * n) / 4];
        let iqr = q3 - q1;

        let lower_bound = q1 - multiplier * iqr;
        let upper_bound = q3 + multiplier * iqr;

        let is_anomaly = value < lower_bound || value > upper_bound;
        let distance = if value < lower_bound {
            lower_bound - value
        } else if value > upper_bound {
            value - upper_bound
        } else {
            0.0
        };

        let score = (distance / iqr).min(1.0);

        AnomalyScore::new(
            score,
            is_anomaly,
            self.algorithm.clone(),
            format!(
                "IQR: [{:.2}, {:.2}], value: {:.2}",
                lower_bound, upper_bound, value
            ),
        )
    }

    /// 移動平均法による異常検知
    fn detect_moving_average(&self, value: f64, window: usize, threshold: f64) -> AnomalyScore {
        if self.history.len() < window {
            return AnomalyScore::new(
                0.0,
                false,
                self.algorithm.clone(),
                "Insufficient data for moving average",
            );
        }

        let recent: Vec<f64> = self.history.iter().rev().take(window).copied().collect();
        let ma = recent.iter().sum::<f64>() / recent.len() as f64;

        let deviation = ((value - ma) / ma * 100.0).abs();
        let is_anomaly = deviation > threshold;

        AnomalyScore::new(
            (deviation / (threshold * 2.0)).min(1.0),
            is_anomaly,
            self.algorithm.clone(),
            format!("MA: {:.2}, deviation: {:.2}%", ma, deviation),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anomaly_detector_creation() {
        let detector =
            AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });
        assert_eq!(detector.max_history, 100);
    }

    #[test]
    fn test_add_point() {
        let mut detector =
            AnomalyDetector::new(3, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });
        detector.add_point(10.0);
        detector.add_point(20.0);
        detector.add_point(30.0);

        assert_eq!(detector.history.len(), 3);

        detector.add_point(40.0);
        assert_eq!(detector.history.len(), 3);
        assert_eq!(detector.history[0], 20.0);
    }

    #[test]
    fn test_detect_zscore_normal() {
        let mut detector =
            AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });

        // 正常データを追加
        for i in 0..10 {
            detector.add_point(50.0 + i as f64);
        }

        let score = detector.detect(55.0);
        assert!(!score.is_anomaly);
    }

    #[test]
    fn test_detect_zscore_anomaly() {
        let mut detector =
            AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });

        // 正常データを追加（分散を持たせる）
        for i in 0..10 {
            detector.add_point(48.0 + i as f64);
        }

        // 異常値を検知
        let score = detector.detect(150.0);
        assert!(score.is_anomaly);
    }

    #[test]
    fn test_detect_iqr() {
        let mut detector =
            AnomalyDetector::new(100, AnomalyDetectionAlgorithm::Iqr { multiplier: 1.5 });

        for i in 0..20 {
            detector.add_point(50.0 + i as f64);
        }

        let score_normal = detector.detect(60.0);
        assert!(!score_normal.is_anomaly);

        let score_anomaly = detector.detect(200.0);
        assert!(score_anomaly.is_anomaly);
    }

    #[test]
    fn test_detect_moving_average() {
        let mut detector = AnomalyDetector::new(
            100,
            AnomalyDetectionAlgorithm::MovingAverage {
                window: 5,
                threshold: 20.0,
            },
        );

        for _ in 0..10 {
            detector.add_point(50.0);
        }

        let score_normal = detector.detect(55.0);
        assert!(!score_normal.is_anomaly);

        let score_anomaly = detector.detect(100.0);
        assert!(score_anomaly.is_anomaly);
    }
}
