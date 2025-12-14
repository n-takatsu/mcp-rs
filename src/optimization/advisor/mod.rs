//! Optimization Advisor Module
//!
//! パフォーマンス最適化アドバイザーシステム

use crate::monitoring::MetricPoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ボトルネックタイプ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BottleneckType {
    /// CPU
    Cpu,
    /// メモリ
    Memory,
    /// ネットワーク
    Network,
    /// データベース
    Database,
    /// ディスクI/O
    DiskIo,
}

/// ボトルネック
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    /// ボトルネックタイプ
    pub bottleneck_type: BottleneckType,
    /// 深刻度（0.0-1.0）
    pub severity: f64,
    /// 説明
    pub description: String,
    /// 影響範囲
    pub impact: String,
    /// 検出メトリクス
    pub detected_value: f64,
    /// 閾値
    pub threshold: f64,
}

impl Bottleneck {
    /// 新しいボトルネックを作成
    pub fn new(
        bottleneck_type: BottleneckType,
        severity: f64,
        description: impl Into<String>,
        impact: impl Into<String>,
        detected_value: f64,
        threshold: f64,
    ) -> Self {
        Self {
            bottleneck_type,
            severity: severity.clamp(0.0, 1.0),
            description: description.into(),
            impact: impact.into(),
            detected_value,
            threshold,
        }
    }
}

/// 最適化提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    /// 提案ID
    pub id: String,
    /// タイトル
    pub title: String,
    /// 説明
    pub description: String,
    /// 対象ボトルネック
    pub target_bottleneck: BottleneckType,
    /// 優先度（1-5、5が最高）
    pub priority: u8,
    /// 推定効果（%）
    pub estimated_improvement: f64,
    /// 実装難易度（1-5、5が最難）
    pub implementation_difficulty: u8,
    /// 推定コスト
    pub estimated_cost: f64,
    /// 推定ROI
    pub estimated_roi: f64,
}

impl OptimizationRecommendation {
    /// 新しい最適化提案を作成
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        target_bottleneck: BottleneckType,
        priority: u8,
        estimated_improvement: f64,
        implementation_difficulty: u8,
        estimated_cost: f64,
    ) -> Self {
        let estimated_roi = if estimated_cost > 0.0 {
            estimated_improvement / estimated_cost
        } else {
            0.0
        };

        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            target_bottleneck,
            priority: priority.clamp(1, 5),
            estimated_improvement,
            implementation_difficulty: implementation_difficulty.clamp(1, 5),
            estimated_cost,
            estimated_roi,
        }
    }
}

/// 最適化アドバイザー
pub struct OptimizationAdvisor {
    /// ボトルネック検出閾値
    thresholds: HashMap<BottleneckType, f64>,
    /// 推奨事項テンプレート
    recommendations_db: Vec<OptimizationRecommendation>,
}

impl OptimizationAdvisor {
    /// 新しい最適化アドバイザーを作成
    pub fn new() -> Self {
        let mut thresholds = HashMap::new();
        thresholds.insert(BottleneckType::Cpu, 80.0);
        thresholds.insert(BottleneckType::Memory, 85.0);
        thresholds.insert(BottleneckType::Network, 75.0);
        thresholds.insert(BottleneckType::Database, 70.0);
        thresholds.insert(BottleneckType::DiskIo, 80.0);

        Self {
            thresholds,
            recommendations_db: Self::initialize_recommendations(),
        }
    }

    /// 閾値を設定
    pub fn set_threshold(&mut self, bottleneck_type: BottleneckType, threshold: f64) {
        self.thresholds.insert(bottleneck_type, threshold);
    }

    /// ボトルネックを検出
    pub fn detect_bottlenecks(&self, metrics: &[MetricPoint]) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        for metric in metrics {
            let (bottleneck_type, threshold) = match metric.metric_type {
                crate::monitoring::MetricType::Cpu => {
                    (BottleneckType::Cpu, *self.thresholds.get(&BottleneckType::Cpu).unwrap())
                }
                crate::monitoring::MetricType::Memory => (
                    BottleneckType::Memory,
                    *self.thresholds.get(&BottleneckType::Memory).unwrap(),
                ),
                crate::monitoring::MetricType::Network => (
                    BottleneckType::Network,
                    *self.thresholds.get(&BottleneckType::Network).unwrap(),
                ),
                _ => continue,
            };

            if metric.value > threshold {
                let severity = ((metric.value - threshold) / threshold).min(1.0);
                bottlenecks.push(Bottleneck::new(
                    bottleneck_type.clone(),
                    severity,
                    format!("{:?} usage is high: {:.2}%", bottleneck_type, metric.value),
                    format!("System performance degradation due to {:?} bottleneck", bottleneck_type),
                    metric.value,
                    threshold,
                ));
            }
        }

        bottlenecks
    }

    /// 最適化提案を生成
    pub fn generate_recommendations(&self, bottlenecks: &[Bottleneck]) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        for bottleneck in bottlenecks {
            let matching_recs: Vec<_> = self
                .recommendations_db
                .iter()
                .filter(|rec| rec.target_bottleneck == bottleneck.bottleneck_type)
                .cloned()
                .collect();

            recommendations.extend(matching_recs);
        }

        // 優先度でソート
        recommendations.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.estimated_roi.partial_cmp(&a.estimated_roi).unwrap()));

        recommendations
    }

    /// 推奨事項データベースを初期化
    fn initialize_recommendations() -> Vec<OptimizationRecommendation> {
        vec![
            OptimizationRecommendation::new(
                "cpu-01",
                "CPU Optimization: Enable Caching",
                "Implement caching layer to reduce CPU-intensive computations",
                BottleneckType::Cpu,
                5,
                30.0,
                2,
                1000.0,
            ),
            OptimizationRecommendation::new(
                "cpu-02",
                "CPU Optimization: Optimize Algorithms",
                "Review and optimize critical algorithm implementations",
                BottleneckType::Cpu,
                4,
                20.0,
                4,
                5000.0,
            ),
            OptimizationRecommendation::new(
                "mem-01",
                "Memory Optimization: Reduce Memory Footprint",
                "Optimize data structures and reduce memory allocations",
                BottleneckType::Memory,
                5,
                25.0,
                3,
                2000.0,
            ),
            OptimizationRecommendation::new(
                "mem-02",
                "Memory Optimization: Implement Memory Pooling",
                "Use object pooling to reduce allocation overhead",
                BottleneckType::Memory,
                4,
                15.0,
                3,
                1500.0,
            ),
            OptimizationRecommendation::new(
                "net-01",
                "Network Optimization: Enable Compression",
                "Enable gzip/brotli compression for network traffic",
                BottleneckType::Network,
                5,
                40.0,
                1,
                500.0,
            ),
            OptimizationRecommendation::new(
                "net-02",
                "Network Optimization: CDN Integration",
                "Use CDN for static assets to reduce network load",
                BottleneckType::Network,
                4,
                35.0,
                2,
                3000.0,
            ),
        ]
    }
}

impl Default for OptimizationAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::MetricType;

    #[test]
    fn test_bottleneck_creation() {
        let bottleneck = Bottleneck::new(
            BottleneckType::Cpu,
            0.8,
            "High CPU usage",
            "Performance degradation",
            90.0,
            80.0,
        );

        assert_eq!(bottleneck.bottleneck_type, BottleneckType::Cpu);
        assert_eq!(bottleneck.severity, 0.8);
    }

    #[test]
    fn test_optimization_recommendation_roi() {
        let rec = OptimizationRecommendation::new(
            "test-01",
            "Test Optimization",
            "Test description",
            BottleneckType::Cpu,
            5,
            30.0,
            2,
            1000.0,
        );

        assert_eq!(rec.estimated_roi, 30.0 / 1000.0);
    }

    #[test]
    fn test_optimization_advisor_creation() {
        let advisor = OptimizationAdvisor::new();
        assert!(advisor.thresholds.contains_key(&BottleneckType::Cpu));
    }

    #[test]
    fn test_set_threshold() {
        let mut advisor = OptimizationAdvisor::new();
        advisor.set_threshold(BottleneckType::Cpu, 70.0);

        assert_eq!(*advisor.thresholds.get(&BottleneckType::Cpu).unwrap(), 70.0);
    }

    #[test]
    fn test_detect_bottlenecks() {
        let advisor = OptimizationAdvisor::new();

        let metrics = vec![
            MetricPoint::new(MetricType::Cpu, 90.0),
            MetricPoint::new(MetricType::Memory, 50.0),
        ];

        let bottlenecks = advisor.detect_bottlenecks(&metrics);

        assert_eq!(bottlenecks.len(), 1);
        assert_eq!(bottlenecks[0].bottleneck_type, BottleneckType::Cpu);
    }

    #[test]
    fn test_generate_recommendations() {
        let advisor = OptimizationAdvisor::new();

        let bottlenecks = vec![Bottleneck::new(
            BottleneckType::Cpu,
            0.8,
            "High CPU",
            "Performance issue",
            90.0,
            80.0,
        )];

        let recommendations = advisor.generate_recommendations(&bottlenecks);

        assert!(!recommendations.is_empty());
        assert!(recommendations
            .iter()
            .all(|r| r.target_bottleneck == BottleneckType::Cpu));
    }

    #[test]
    fn test_recommendations_sorted_by_priority() {
        let advisor = OptimizationAdvisor::new();

        let bottlenecks = vec![Bottleneck::new(
            BottleneckType::Cpu,
            0.8,
            "High CPU",
            "Performance issue",
            90.0,
            80.0,
        )];

        let recommendations = advisor.generate_recommendations(&bottlenecks);

        // 優先度の高い順にソートされていることを確認
        for i in 0..recommendations.len() - 1 {
            assert!(recommendations[i].priority >= recommendations[i + 1].priority);
        }
    }

    #[test]
    fn test_priority_clamping() {
        let rec = OptimizationRecommendation::new(
            "test-01",
            "Test",
            "Desc",
            BottleneckType::Cpu,
            10, // 5を超える
            10.0,
            0, // 1未満
            100.0,
        );

        assert_eq!(rec.priority, 5);
        assert_eq!(rec.implementation_difficulty, 1);
    }
}
