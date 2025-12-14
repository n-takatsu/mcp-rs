//! Correlation Engine
//!
//! セキュリティイベント相関分析エンジン

use super::types::*;
use crate::error::Result;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 相関分析エンジン
pub struct CorrelationEngine {
    /// イベント履歴
    event_history: Arc<RwLock<Vec<AuditLogEntry>>>,
    /// 検出された相関イベント
    correlated_events: Arc<RwLock<Vec<CorrelatedEvent>>>,
}

impl CorrelationEngine {
    /// 新しい相関分析エンジンを作成
    pub fn new() -> Self {
        Self {
            event_history: Arc::new(RwLock::new(Vec::new())),
            correlated_events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// イベントを分析して相関を検出
    pub async fn analyze(&self, entry: &AuditLogEntry) -> Result<Vec<CorrelatedEvent>> {
        // イベント履歴に追加
        {
            let mut history = self.event_history.write().await;
            history.push(entry.clone());

            // 古いイベントをクリーンアップ（7日以上前）
            let cutoff = Utc::now() - Duration::days(7);
            history.retain(|e| e.timestamp > cutoff);
        }

        let mut correlated = Vec::new();

        // Kill Chain分析
        if let Some(event) = self.analyze_kill_chain(entry).await? {
            correlated.push(event);
        }

        // 攻撃シナリオ再構築
        if let Some(event) = self.reconstruct_attack_scenario(entry).await? {
            correlated.push(event);
        }

        // 複数イベントの相関
        if let Some(event) = self.correlate_multiple_events(entry).await? {
            correlated.push(event);
        }

        // 検出された相関イベントを保存
        if !correlated.is_empty() {
            let mut events = self.correlated_events.write().await;
            events.extend(correlated.clone());
        }

        Ok(correlated)
    }

    /// Kill Chain分析
    async fn analyze_kill_chain(&self, entry: &AuditLogEntry) -> Result<Option<CorrelatedEvent>> {
        let history = self.event_history.read().await;

        // ユーザーの最近のアクション履歴を取得
        let user_actions: Vec<&AuditLogEntry> = history
            .iter()
            .filter(|e| {
                e.user_id == entry.user_id
                    && entry.timestamp.signed_duration_since(e.timestamp) < Duration::hours(24)
            })
            .collect();

        if user_actions.len() < 3 {
            return Ok(None);
        }

        // Kill Chainパターンを検出
        let has_reconnaissance = user_actions
            .iter()
            .any(|e| e.action == "list_users" || e.action == "list_resources");
        let has_access_attempt = user_actions
            .iter()
            .any(|e| e.action == "login_attempt" || e.action == "auth_attempt");
        let has_privilege_escalation = user_actions
            .iter()
            .any(|e| e.action == "role_change" || e.action == "sudo_command");
        let has_data_access = user_actions
            .iter()
            .any(|e| e.action == "data_export" || e.action == "data_download");

        let chain_stages = [
            has_reconnaissance,
            has_access_attempt,
            has_privilege_escalation,
            has_data_access,
        ]
        .iter()
        .filter(|&&stage| stage)
        .count();

        if chain_stages >= 3 {
            let related_logs: Vec<String> = user_actions.iter().map(|e| e.id.clone()).collect();

            Ok(Some(CorrelatedEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                related_logs,
                attack_scenario: Some(AttackScenario::Exfiltration),
                confidence: 85,
                description: format!(
                    "ユーザー {} のアクションがKill Chainパターンに一致（{}段階）",
                    entry.user_id, chain_stages
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 攻撃シナリオ再構築
    async fn reconstruct_attack_scenario(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<CorrelatedEvent>> {
        let history = self.event_history.read().await;

        // 同一IPアドレスからの複数ユーザーアクセス
        if let Some(ip) = &entry.ip_address {
            let same_ip_users: Vec<&AuditLogEntry> = history
                .iter()
                .filter(|e| {
                    e.ip_address.as_ref() == Some(ip)
                        && e.user_id != entry.user_id
                        && entry.timestamp.signed_duration_since(e.timestamp) < Duration::hours(1)
                })
                .collect();

            if same_ip_users.len() >= 3 {
                let related_logs: Vec<String> =
                    same_ip_users.iter().map(|e| e.id.clone()).collect();

                return Ok(Some(CorrelatedEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp: entry.timestamp,
                    related_logs,
                    attack_scenario: Some(AttackScenario::LateralMovement),
                    confidence: 80,
                    description: format!(
                        "同一IPアドレス {} から{}人のユーザーが短時間にアクセス（横方向移動の可能性）",
                        ip,
                        same_ip_users.len() + 1
                    ),
                }));
            }
        }

        Ok(None)
    }

    /// 複数イベントの相関
    async fn correlate_multiple_events(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<CorrelatedEvent>> {
        let history = self.event_history.read().await;

        // 失敗後の成功パターン
        let failed_attempts: Vec<&AuditLogEntry> = history
            .iter()
            .filter(|e| {
                e.user_id == entry.user_id
                    && e.action == entry.action
                    && e.result == ActionResult::Failure
                    && entry.timestamp.signed_duration_since(e.timestamp) < Duration::minutes(30)
            })
            .collect();

        if !failed_attempts.is_empty() && entry.result == ActionResult::Success {
            let related_logs: Vec<String> = failed_attempts.iter().map(|e| e.id.clone()).collect();

            return Ok(Some(CorrelatedEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                related_logs,
                attack_scenario: Some(AttackScenario::CredentialAccess),
                confidence: 75,
                description: format!(
                    "ユーザー {} が{}回の失敗後にアクセス成功（ブルートフォース攻撃の可能性）",
                    entry.user_id,
                    failed_attempts.len()
                ),
            }));
        }

        Ok(None)
    }

    /// 相関イベント数を取得
    pub async fn get_correlated_count(&self) -> usize {
        self.correlated_events.read().await.len()
    }
}

impl Default for CorrelationEngine {
    fn default() -> Self {
        Self::new()
    }
}
