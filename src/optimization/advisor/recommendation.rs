//! Optimization Recommendation Types

use super::bottleneck::BottleneckType;
use serde::{Deserialize, Serialize};

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
    #[allow(clippy::too_many_arguments)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
