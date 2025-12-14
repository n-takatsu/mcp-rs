//! Alert Condition Types

use crate::monitoring::{MetricPoint, MetricType};
use serde::{Deserialize, Serialize};

/// アラート条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    /// 閾値超過
    Threshold {
        metric_type: MetricType,
        threshold: f64,
        comparison: Comparison,
    },
    /// レート変化
    RateChange {
        metric_type: MetricType,
        rate: f64,
        window_seconds: u64,
    },
    /// 異常検知
    Anomaly { metric_type: MetricType, score: f64 },
}

/// 比較演算子
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Comparison {
    /// より大きい
    GreaterThan,
    /// より小さい
    LessThan,
    /// 等しい
    Equal,
}

impl Comparison {
    /// 比較を評価
    pub fn evaluate(&self, value: f64, threshold: f64) -> bool {
        match self {
            Comparison::GreaterThan => value > threshold,
            Comparison::LessThan => value < threshold,
            Comparison::Equal => (value - threshold).abs() < f64::EPSILON,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comparison_evaluate() {
        assert!(Comparison::GreaterThan.evaluate(10.0, 5.0));
        assert!(!Comparison::GreaterThan.evaluate(5.0, 10.0));
        assert!(Comparison::LessThan.evaluate(5.0, 10.0));
        assert!(!Comparison::LessThan.evaluate(10.0, 5.0));
    }
}
