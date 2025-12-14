//! Alert Rule Types

use super::alert::AlertLevel;
use super::condition::{AlertCondition, Comparison};
use crate::monitoring::MetricPoint;
use serde::{Deserialize, Serialize};

/// アラートルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// ルールID
    pub id: String,
    /// ルール名
    pub name: String,
    /// 条件
    pub condition: AlertCondition,
    /// アラートレベル
    pub level: AlertLevel,
    /// 有効フラグ
    pub enabled: bool,
}

impl AlertRule {
    /// 新しいアラートルールを作成
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        condition: AlertCondition,
        level: AlertLevel,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            condition,
            level,
            enabled: true,
        }
    }

    /// ルールを評価
    pub fn evaluate(&self, metric: &MetricPoint) -> bool {
        if !self.enabled {
            return false;
        }

        match &self.condition {
            AlertCondition::Threshold {
                metric_type,
                threshold,
                comparison,
            } => {
                if &metric.metric_type != metric_type {
                    return false;
                }
                comparison.evaluate(metric.value, *threshold)
            }
            AlertCondition::RateChange { metric_type, .. } => {
                // 簡易実装: レート変化の計算は省略
                &metric.metric_type == metric_type
            }
            AlertCondition::Anomaly { metric_type, .. } => {
                // 簡易実装: 異常スコアの評価は省略
                &metric.metric_type == metric_type
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::MetricType;

    #[test]
    fn test_alert_rule_evaluate() {
        let rule = AlertRule::new(
            "rule-1",
            "CPU High",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
            AlertLevel::Warning,
        );

        let metric_high = MetricPoint::new(MetricType::Cpu, 90.0);
        let metric_low = MetricPoint::new(MetricType::Cpu, 50.0);

        assert!(rule.evaluate(&metric_high));
        assert!(!rule.evaluate(&metric_low));
    }
}
