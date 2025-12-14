//! Alert Manager
//!
//! アラート管理システム

use super::types::*;
use crate::error::Result;
use std::collections::HashMap;

/// アラート管理システム
pub struct AlertManager {
    /// アラート一覧
    alerts: Vec<Alert>,
    /// 深刻度別カウント
    severity_counts: HashMap<AlertSeverity, u64>,
}

impl AlertManager {
    /// 新しいアラート管理システムを作成
    pub fn new() -> Self {
        Self {
            alerts: Vec::new(),
            severity_counts: HashMap::new(),
        }
    }

    /// アラートを追加
    pub async fn add_alert(&mut self, alert: Alert) -> Result<()> {
        // 深刻度カウントを更新
        *self
            .severity_counts
            .entry(alert.severity.clone())
            .or_insert(0) += 1;

        // 重複排除チェック
        let is_duplicate = self.is_duplicate(&alert);
        if is_duplicate {
            // 既存のアラートを見つけて更新
            let similar_id = self
                .alerts
                .iter()
                .find(|a| self.is_similar(a, &alert))
                .map(|a| a.id.clone());

            if let Some(id) = similar_id {
                if let Some(existing) = self.alerts.iter_mut().find(|a| a.id == id) {
                    existing.description = format!(
                        "{}\n\n[重複アラート] {}",
                        existing.description, alert.description
                    );
                }
            }
        } else {
            self.alerts.push(alert);
        }

        Ok(())
    }

    /// アラートを取得
    pub fn get_alert(&self, alert_id: &str) -> Option<&Alert> {
        self.alerts.iter().find(|a| a.id == alert_id)
    }

    /// 全アラートを取得
    pub fn get_all_alerts(&self) -> &[Alert] {
        &self.alerts
    }

    /// 深刻度でフィルタリング
    pub fn get_alerts_by_severity(&self, severity: AlertSeverity) -> Vec<&Alert> {
        self.alerts
            .iter()
            .filter(|a| a.severity == severity)
            .collect()
    }

    /// ステータスでフィルタリング
    pub fn get_alerts_by_status(&self, status: AlertStatus) -> Vec<&Alert> {
        self.alerts.iter().filter(|a| a.status == status).collect()
    }

    /// アラートステータスを更新
    pub fn update_alert_status(&mut self, alert_id: &str, new_status: AlertStatus) -> Result<()> {
        if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.status = new_status;
            Ok(())
        } else {
            Err(crate::error::Error::NotFound(format!(
                "Alert not found: {}",
                alert_id
            )))
        }
    }

    /// 新規アラート数を取得
    pub fn get_new_alerts_count(&self) -> usize {
        self.alerts
            .iter()
            .filter(|a| a.status == AlertStatus::New)
            .count()
    }

    /// 高深刻度アラート数を取得
    pub async fn get_high_severity_count(&self) -> u64 {
        self.alerts
            .iter()
            .filter(|a| matches!(a.severity, AlertSeverity::High | AlertSeverity::Critical))
            .count() as u64
    }

    /// 総アラート数を取得
    pub async fn get_alert_count(&self) -> u64 {
        self.alerts.len() as u64
    }

    /// 重複チェック
    fn is_duplicate(&self, new_alert: &Alert) -> bool {
        self.alerts.iter().any(|a| self.is_similar(a, new_alert))
    }

    /// アラートの類似性をチェック
    fn is_similar(&self, alert1: &Alert, alert2: &Alert) -> bool {
        // 同じソース、同じタイトル、5分以内
        alert1.source == alert2.source
            && alert1.title == alert2.title
            && alert2
                .timestamp
                .signed_duration_since(alert1.timestamp)
                .num_minutes()
                < 5
    }

    /// アラート統計を取得
    pub fn get_statistics(&self) -> AlertStatistics {
        let mut by_severity = HashMap::new();
        let mut by_status = HashMap::new();

        for alert in &self.alerts {
            *by_severity
                .entry(format!("{:?}", alert.severity))
                .or_insert(0) += 1;
            *by_status.entry(format!("{:?}", alert.status)).or_insert(0) += 1;
        }

        AlertStatistics {
            total_alerts: self.alerts.len(),
            new_alerts: self.get_new_alerts_count(),
            investigating_alerts: self.get_alerts_by_status(AlertStatus::Investigating).len(),
            resolved_alerts: self.get_alerts_by_status(AlertStatus::Resolved).len(),
            false_positive_alerts: self.get_alerts_by_status(AlertStatus::FalsePositive).len(),
            by_severity,
            by_status,
        }
    }

    /// 優先度順にアラートを取得
    pub fn get_prioritized_alerts(&self) -> Vec<&Alert> {
        let mut alerts: Vec<&Alert> = self.alerts.iter().collect();

        // 深刻度とステータスで優先度をソート
        alerts.sort_by(|a, b| {
            // 深刻度で降順
            let severity_cmp = b.severity.cmp(&a.severity);
            if severity_cmp != std::cmp::Ordering::Equal {
                return severity_cmp;
            }

            // ステータスで並べ替え（New > Investigating > Responding）
            let a_priority = match a.status {
                AlertStatus::New => 3,
                AlertStatus::Investigating => 2,
                AlertStatus::Responding => 1,
                _ => 0,
            };
            let b_priority = match b.status {
                AlertStatus::New => 3,
                AlertStatus::Investigating => 2,
                AlertStatus::Responding => 1,
                _ => 0,
            };

            b_priority.cmp(&a_priority)
        });

        alerts
    }
}

/// アラート統計
#[derive(Debug, Clone)]
pub struct AlertStatistics {
    /// 総アラート数
    pub total_alerts: usize,
    /// 新規アラート数
    pub new_alerts: usize,
    /// 調査中アラート数
    pub investigating_alerts: usize,
    /// 解決済みアラート数
    pub resolved_alerts: usize,
    /// 偽陽性アラート数
    pub false_positive_alerts: usize,
    /// 深刻度別カウント
    pub by_severity: HashMap<String, usize>,
    /// ステータス別カウント
    pub by_status: HashMap<String, usize>,
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}
