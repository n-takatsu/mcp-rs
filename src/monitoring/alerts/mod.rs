//! Alerts Module
//!
//! アラートシステム

use crate::monitoring::{MetricPoint, MetricType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

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

    #[test]
    fn test_comparison_evaluate() {
        assert!(Comparison::GreaterThan.evaluate(10.0, 5.0));
        assert!(!Comparison::GreaterThan.evaluate(5.0, 10.0));
        assert!(Comparison::LessThan.evaluate(5.0, 10.0));
        assert!(!Comparison::LessThan.evaluate(10.0, 5.0));
    }

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
