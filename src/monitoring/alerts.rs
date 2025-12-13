//! アラート管理システム

use crate::monitoring::metrics::{MetricType, SystemMetrics};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// アラートレベル
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
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

impl AlertLevel {
    /// レベル名を取得
    pub fn as_str(&self) -> &str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        }
    }
}

/// アラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// アラートID
    pub id: String,
    /// レベル
    pub level: AlertLevel,
    /// メッセージ
    pub message: String,
    /// メトリクスタイプ
    pub metric_type: MetricType,
    /// 現在の値
    pub current_value: f64,
    /// 閾値
    pub threshold: f64,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 解決済みフラグ
    pub resolved: bool,
}

impl Alert {
    /// 新しいアラートを作成
    pub fn new(
        level: AlertLevel,
        message: impl Into<String>,
        metric_type: MetricType,
        current_value: f64,
        threshold: f64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            level,
            message: message.into(),
            metric_type,
            current_value,
            threshold,
            timestamp: Utc::now(),
            resolved: false,
        }
    }

    /// アラートを解決
    pub fn resolve(&mut self) {
        self.resolved = true;
    }
}

/// アラートルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// ルール名
    pub name: String,
    /// メトリクスタイプ
    pub metric_type: MetricType,
    /// 閾値
    pub threshold: f64,
    /// 比較演算子（true: 以上, false: 以下）
    pub greater_than: bool,
    /// アラートレベル
    pub level: AlertLevel,
    /// メッセージテンプレート
    pub message_template: String,
    /// 有効フラグ
    pub enabled: bool,
}

impl AlertRule {
    /// ルールをチェック
    pub fn check(&self, value: f64) -> Option<Alert> {
        if !self.enabled {
            return None;
        }

        let triggered = if self.greater_than {
            value > self.threshold
        } else {
            value < self.threshold
        };

        if triggered {
            let message = self
                .message_template
                .replace("{value}", &format!("{:.2}", value))
                .replace("{threshold}", &format!("{:.2}", self.threshold));

            Some(Alert::new(
                self.level,
                message,
                self.metric_type.clone(),
                value,
                self.threshold,
            ))
        } else {
            None
        }
    }
}

/// アラートマネージャー
pub struct AlertManager {
    rules: Arc<RwLock<Vec<AlertRule>>>,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    alert_history: Arc<RwLock<Vec<Alert>>>,
    max_history_size: usize,
}

impl AlertManager {
    /// 新しいアラートマネージャーを作成
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            max_history_size: 1000,
        }
    }

    /// デフォルトルールを追加
    pub async fn add_default_rules(&self) {
        let default_rules = vec![
            AlertRule {
                name: "High CPU Usage".to_string(),
                metric_type: MetricType::CpuUsage,
                threshold: 80.0,
                greater_than: true,
                level: AlertLevel::Warning,
                message_template: "CPU usage is high: {value}% (threshold: {threshold}%)"
                    .to_string(),
                enabled: true,
            },
            AlertRule {
                name: "Critical CPU Usage".to_string(),
                metric_type: MetricType::CpuUsage,
                threshold: 95.0,
                greater_than: true,
                level: AlertLevel::Critical,
                message_template: "CPU usage is critical: {value}% (threshold: {threshold}%)"
                    .to_string(),
                enabled: true,
            },
            AlertRule {
                name: "High Memory Usage".to_string(),
                metric_type: MetricType::MemoryUsage,
                threshold: 85.0,
                greater_than: true,
                level: AlertLevel::Warning,
                message_template: "Memory usage is high: {value}% (threshold: {threshold}%)"
                    .to_string(),
                enabled: true,
            },
            AlertRule {
                name: "High Error Rate".to_string(),
                metric_type: MetricType::ErrorRate,
                threshold: 5.0,
                greater_than: true,
                level: AlertLevel::Error,
                message_template: "Error rate is high: {value}% (threshold: {threshold}%)"
                    .to_string(),
                enabled: true,
            },
        ];

        let mut rules = self.rules.write().await;
        rules.extend(default_rules);
    }

    /// ルールを追加
    pub async fn add_rule(&self, rule: AlertRule) {
        let mut rules = self.rules.write().await;
        rules.push(rule);
    }

    /// システムメトリクスをチェック
    pub async fn check_metrics(&self, metrics: &SystemMetrics) -> Vec<Alert> {
        let mut new_alerts = Vec::new();
        let rules = self.rules.read().await;

        for rule in rules.iter() {
            let value = match &rule.metric_type {
                MetricType::CpuUsage => metrics.cpu_usage,
                MetricType::MemoryUsage => metrics.memory_usage,
                MetricType::ErrorRate => metrics.error_rate(),
                MetricType::ResponseTime => metrics.avg_response_time,
                _ => continue,
            };

            if let Some(alert) = rule.check(value) {
                new_alerts.push(alert.clone());

                let mut active = self.active_alerts.write().await;
                active.insert(alert.id.clone(), alert.clone());

                let mut history = self.alert_history.write().await;
                history.push(alert);
                if history.len() > self.max_history_size {
                    history.remove(0);
                }
            }
        }

        new_alerts
    }

    /// アクティブなアラートを取得
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let active = self.active_alerts.read().await;
        active.values().cloned().collect()
    }

    /// アラートを解決
    pub async fn resolve_alert(&self, alert_id: &str) -> bool {
        let mut active = self.active_alerts.write().await;
        if let Some(mut alert) = active.remove(alert_id) {
            alert.resolve();

            let mut history = self.alert_history.write().await;
            history.push(alert);

            true
        } else {
            false
        }
    }

    /// 全てのアラートを解決
    pub async fn resolve_all(&self) {
        let mut active = self.active_alerts.write().await;
        let mut history = self.alert_history.write().await;

        for (_, mut alert) in active.drain() {
            alert.resolve();
            history.push(alert);
        }

        let max_history_size = self.max_history_size;
        if history.len() > max_history_size {
            let drain_count = history.len() - max_history_size;
            history.drain(0..drain_count);
        }
    }

    /// アラート履歴を取得
    pub async fn get_alert_history(&self, limit: usize) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> AlertStats {
        let active = self.active_alerts.read().await;
        let history = self.alert_history.read().await;

        let mut stats = AlertStats {
            active_count: active.len(),
            total_count: history.len(),
            ..Default::default()
        };

        for alert in active.values() {
            match alert.level {
                AlertLevel::Info => stats.info_count += 1,
                AlertLevel::Warning => stats.warning_count += 1,
                AlertLevel::Error => stats.error_count += 1,
                AlertLevel::Critical => stats.critical_count += 1,
            }
        }

        stats
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

/// アラート統計
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AlertStats {
    pub active_count: usize,
    pub total_count: usize,
    pub info_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub critical_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_rule_check() {
        let rule = AlertRule {
            name: "Test".to_string(),
            metric_type: MetricType::CpuUsage,
            threshold: 80.0,
            greater_than: true,
            level: AlertLevel::Warning,
            message_template: "CPU: {value}%".to_string(),
            enabled: true,
        };

        // 閾値以上
        assert!(rule.check(85.0).is_some());

        // 閾値以下
        assert!(rule.check(70.0).is_none());
    }

    #[tokio::test]
    async fn test_alert_manager() {
        let manager = AlertManager::new();
        manager.add_default_rules().await;

        let mut metrics = SystemMetrics::new();
        metrics.cpu_usage = 90.0;

        let alerts = manager.check_metrics(&metrics).await;
        assert!(!alerts.is_empty());

        let active = manager.get_active_alerts().await;
        assert!(!active.is_empty());
    }
}
