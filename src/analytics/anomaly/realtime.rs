//! Realtime Anomaly Detection
//!
//! リアルタイム異常検知器

use super::detector::AnomalyDetector;
use super::types::AnomalyScore;
use crate::monitoring::MetricPoint;
use std::collections::HashMap;

/// リアルタイム異常検知器
pub struct RealtimeAnomalyDetector {
    /// メトリクスタイプごとの検知器
    detectors: HashMap<String, AnomalyDetector>,
}

impl RealtimeAnomalyDetector {
    /// 新しいリアルタイム異常検知器を作成
    pub fn new() -> Self {
        Self {
            detectors: HashMap::new(),
        }
    }

    /// 検知器を登録
    pub fn register_detector(&mut self, metric_name: impl Into<String>, detector: AnomalyDetector) {
        self.detectors.insert(metric_name.into(), detector);
    }

    /// メトリクスを評価
    pub fn evaluate(&mut self, metric: &MetricPoint) -> Option<AnomalyScore> {
        let metric_name = format!("{:?}", metric.metric_type);
        let detector = self.detectors.get_mut(&metric_name)?;

        let score = detector.detect(metric.value);
        detector.add_point(metric.value);

        Some(score)
    }
}

impl Default for RealtimeAnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::anomaly::AnomalyDetectionAlgorithm;
    use crate::monitoring::MetricType;

    #[test]
    fn test_realtime_anomaly_detector() {
        let mut rt_detector = RealtimeAnomalyDetector::new();

        rt_detector.register_detector(
            "Cpu",
            AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 }),
        );

        let metric = MetricPoint::new(MetricType::Cpu, 50.0);
        let score = rt_detector.evaluate(&metric);
        assert!(score.is_some());
    }
}
