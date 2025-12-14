//! Alert Manager Implementation

use super::alert::{Alert, AlertLevel};
use super::rule::AlertRule;
use crate::monitoring::MetricPoint;
use std::sync::Arc;
use tokio::sync::RwLock;

/// アラートマネージャー
pub struct AlertManager {
    /// アラートルール
    rules: Arc<RwLock<Vec<AlertRule>>>,
    /// アクティブアラート
    active_alerts: Arc<RwLock<Vec<Alert>>>,
}

impl AlertManager {
    /// 新しいアラートマネージャーを作成
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            active_alerts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// ルールを追加
    pub async fn add_rule(&self, rule: AlertRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
    }

    /// メトリクスを評価してアラートを生成
    pub async fn evaluate_metrics(&self, metrics: &[MetricPoint]) -> Vec<Alert> {
        let rules = self.rules.read().await;
        let mut new_alerts = Vec::new();

        for metric in metrics {
            for rule in rules.iter() {
                if rule.evaluate(metric) {
                    let alert = Alert::new(
                        uuid::Uuid::new_v4().to_string(),
                        &rule.name,
                        rule.level.clone(),
                        format!("Alert triggered: {} = {}", rule.name, metric.value),
                        rule.condition.clone(),
                    );
                    new_alerts.push(alert);
                }
            }
        }

        // アクティブアラートに追加
        let mut active = self.active_alerts.write().await;
        active.extend(new_alerts.clone());

        new_alerts
    }

    /// アクティブアラートを取得
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let alerts = self.active_alerts.read().await;
        alerts.clone()
    }

    /// アラートを確認
    pub async fn acknowledge_alert(&self, alert_id: &str) {
        let mut alerts = self.active_alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledge();
        }
    }

    /// 確認済みアラートをクリア
    pub async fn clear_acknowledged(&self) {
        let mut alerts = self.active_alerts.write().await;
        alerts.retain(|a| !a.acknowledged);
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::alerts::condition::{AlertCondition, Comparison};
    use crate::monitoring::MetricType;

    #[tokio::test]
    async fn test_alert_manager_add_rule() {
        let manager = AlertManager::new();

        manager
            .add_rule(AlertRule::new(
                "rule-1",
                "Test Rule",
                AlertCondition::Threshold {
                    metric_type: MetricType::Cpu,
                    threshold: 80.0,
                    comparison: Comparison::GreaterThan,
                },
                AlertLevel::Warning,
            ))
            .await;

        let rules = manager.rules.read().await;
        assert_eq!(rules.len(), 1);
    }

    #[tokio::test]
    async fn test_evaluate_metrics() {
        let manager = AlertManager::new();

        manager
            .add_rule(AlertRule::new(
                "rule-1",
                "CPU High",
                AlertCondition::Threshold {
                    metric_type: MetricType::Cpu,
                    threshold: 80.0,
                    comparison: Comparison::GreaterThan,
                },
                AlertLevel::Warning,
            ))
            .await;

        let metrics = vec![MetricPoint::new(MetricType::Cpu, 90.0)];
        let alerts = manager.evaluate_metrics(&metrics).await;

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, AlertLevel::Warning);
    }

    #[tokio::test]
    async fn test_acknowledge_and_clear() {
        let manager = AlertManager::new();

        manager
            .add_rule(AlertRule::new(
                "rule-1",
                "Test",
                AlertCondition::Threshold {
                    metric_type: MetricType::Cpu,
                    threshold: 80.0,
                    comparison: Comparison::GreaterThan,
                },
                AlertLevel::Info,
            ))
            .await;

        let metrics = vec![MetricPoint::new(MetricType::Cpu, 90.0)];
        let alerts = manager.evaluate_metrics(&metrics).await;

        let alert_id = alerts[0].id.clone();
        manager.acknowledge_alert(&alert_id).await;

        let active = manager.get_active_alerts().await;
        assert_eq!(active.len(), 1);
        assert!(active[0].acknowledged);

        manager.clear_acknowledged().await;
        let active = manager.get_active_alerts().await;
        assert_eq!(active.len(), 0);
    }
}
