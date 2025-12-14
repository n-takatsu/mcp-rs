//! Alert Types

use super::condition::AlertCondition;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// アラートレベル
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    /// 情報
    Info,
    /// 警告
    Warning,
    /// エラー
    Error,
    /// 致命的
    Critical,
}

/// アラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// アラートID
    pub id: String,
    /// アラート名
    pub name: String,
    /// レベル
    pub level: AlertLevel,
    /// メッセージ
    pub message: String,
    /// 条件
    pub condition: AlertCondition,
    /// タイムスタンプ
    pub timestamp: SystemTime,
    /// 確認済みフラグ
    pub acknowledged: bool,
}

impl Alert {
    /// 新しいアラートを作成
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        level: AlertLevel,
        message: impl Into<String>,
        condition: AlertCondition,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            level,
            message: message.into(),
            condition,
            timestamp: SystemTime::now(),
            acknowledged: false,
        }
    }

    /// アラートを確認
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::alerts::condition::Comparison;
    use crate::monitoring::MetricType;

    #[test]
    fn test_alert_creation() {
        let alert = Alert::new(
            "alert-1",
            "CPU High",
            AlertLevel::Warning,
            "CPU usage is high",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
        );

        assert_eq!(alert.id, "alert-1");
        assert_eq!(alert.level, AlertLevel::Warning);
        assert!(!alert.acknowledged);
    }

    #[test]
    fn test_alert_acknowledge() {
        let mut alert = Alert::new(
            "alert-1",
            "Test",
            AlertLevel::Info,
            "Test message",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
        );

        alert.acknowledge();
        assert!(alert.acknowledged);
    }
}
