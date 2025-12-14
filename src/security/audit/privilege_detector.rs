//! Privilege Escalation Detector
//!
//! 権限昇格検知システム

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 権限昇格検知器
pub struct PrivilegeDetector {
    /// ユーザーごとの権限履歴
    role_history: Arc<RwLock<HashMap<String, Vec<RoleChange>>>>,
    /// 分析済みログ数
    analyzed_count: Arc<RwLock<u64>>,
    /// 検出数
    detection_count: Arc<RwLock<u64>>,
}

/// ロール変更履歴
#[derive(Debug, Clone)]
struct RoleChange {
    timestamp: DateTime<Utc>,
    from_role: String,
    to_role: String,
    action: String,
}

impl PrivilegeDetector {
    /// 新しい権限昇格検知器を作成
    pub fn new() -> Self {
        Self {
            role_history: Arc::new(RwLock::new(HashMap::new())),
            analyzed_count: Arc::new(RwLock::new(0)),
            detection_count: Arc::new(RwLock::new(0)),
        }
    }

    /// 権限昇格を検知
    pub async fn detect(&self, entry: &AuditLogEntry) -> Result<Option<PrivilegeEscalationEvent>> {
        // 分析カウント更新
        {
            let mut count = self.analyzed_count.write().await;
            *count += 1;
        }

        // 権限関連のアクションのみ処理
        if !self.is_privilege_action(&entry.action) {
            return Ok(None);
        }

        // ロール変更の検出
        if let Some(event) = self.detect_role_change(entry).await? {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 異常な権限使用の検出
        if let Some(event) = self.detect_abnormal_usage(entry).await? {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 横方向移動の検出
        if let Some(event) = self.detect_lateral_movement(entry).await? {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 複数権限取得の検出
        if let Some(event) = self.detect_multiple_grants(entry).await? {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        Ok(None)
    }

    /// 権限関連のアクションか判定
    fn is_privilege_action(&self, action: &str) -> bool {
        action.contains("role")
            || action.contains("permission")
            || action.contains("admin")
            || action.contains("grant")
            || action.contains("sudo")
            || action.contains("impersonate")
    }

    /// ロール変更を検出
    async fn detect_role_change(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<PrivilegeEscalationEvent>> {
        if !entry.action.contains("role") {
            return Ok(None);
        }

        // リソースフィールドから情報を抽出（"from:user to:admin"形式）
        let mut from_role = entry.details.get("from_role").cloned().unwrap_or_default();
        let mut to_role = entry.details.get("to_role").cloned().unwrap_or_default();

        if from_role.is_empty() && entry.resource.contains("from:") {
            if let Some(from_start) = entry.resource.find("from:") {
                let rest = &entry.resource[from_start + 5..];
                if let Some(end) = rest.find(" ") {
                    from_role = rest[..end].to_string();
                } else if let Some(end) = rest.find(",") {
                    from_role = rest[..end].to_string();
                }
            }
        }

        if to_role.is_empty() && entry.resource.contains("to:") {
            if let Some(to_start) = entry.resource.find("to:") {
                let rest = &entry.resource[to_start + 3..];
                if let Some(end) = rest.find(" ") {
                    to_role = rest[..end].to_string();
                } else if let Some(end) = rest.find(",") {
                    to_role = rest[..end].to_string();
                } else {
                    to_role = rest.to_string();
                }
            }
        }

        // ロール履歴を記録
        {
            let mut history = self.role_history.write().await;
            let user_history = history
                .entry(entry.user_id.clone())
                .or_insert_with(Vec::new);
            user_history.push(RoleChange {
                timestamp: entry.timestamp,
                from_role: from_role.clone(),
                to_role: to_role.clone(),
                action: entry.action.clone(),
            });
        }

        // 管理者権限への昇格を検出
        let risk_score = self.calculate_role_risk(&from_role, &to_role);

        if risk_score > 50 {
            Ok(Some(PrivilegeEscalationEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                user_id: entry.user_id.clone(),
                from_role: from_role.clone(),
                to_role: to_role.clone(),
                escalation_type: EscalationType::RoleChange,
                risk_score,
                description: format!(
                    "ユーザー {} がロール {} から {} へ昇格しました",
                    entry.user_id, from_role, to_role
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 異常な権限使用を検出
    async fn detect_abnormal_usage(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<PrivilegeEscalationEvent>> {
        // 深夜の管理者アクション
        let hour = entry.timestamp.hour();
        let is_unusual_time = !(6..=22).contains(&hour);

        // 管理者アクションかつ異常な時間帯
        let has_admin_role = entry
            .details
            .get("role")
            .map(|r| r.contains("admin"))
            .unwrap_or(false);
        if (entry.action.contains("admin") || has_admin_role)
            && is_unusual_time
            && entry.result == ActionResult::Success
        {
            Ok(Some(PrivilegeEscalationEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                user_id: entry.user_id.clone(),
                from_role: "unknown".to_string(),
                to_role: "admin".to_string(),
                escalation_type: EscalationType::AbnormalUsage,
                risk_score: 70,
                description: format!(
                    "ユーザー {} が異常な時間帯（{}時）に管理者アクセスを実行しました",
                    entry.user_id, hour
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 横方向移動を検出
    async fn detect_lateral_movement(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<PrivilegeEscalationEvent>> {
        if entry.action != "impersonate_user" {
            return Ok(None);
        }

        let target_user = entry
            .details
            .get("target_user")
            .cloned()
            .unwrap_or_default();

        Ok(Some(PrivilegeEscalationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: entry.timestamp,
            user_id: entry.user_id.clone(),
            from_role: "user".to_string(),
            to_role: target_user.clone(),
            escalation_type: EscalationType::LateralMovement,
            risk_score: 80,
            description: format!(
                "ユーザー {} が {} に成りすまして横方向移動を試みました",
                entry.user_id, target_user
            ),
        }))
    }

    /// 複数権限取得を検出
    async fn detect_multiple_grants(
        &self,
        entry: &AuditLogEntry,
    ) -> Result<Option<PrivilegeEscalationEvent>> {
        if entry.action != "permission_grant" {
            return Ok(None);
        }

        // 過去1時間のロール変更をチェック
        let history = self.role_history.read().await;
        if let Some(user_history) = history.get(&entry.user_id) {
            let recent_changes = user_history
                .iter()
                .filter(|change| {
                    entry.timestamp.signed_duration_since(change.timestamp) < Duration::hours(1)
                })
                .count();

            if recent_changes >= 3 {
                return Ok(Some(PrivilegeEscalationEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp: entry.timestamp,
                    user_id: entry.user_id.clone(),
                    from_role: "user".to_string(),
                    to_role: "multiple".to_string(),
                    escalation_type: EscalationType::MultipleGrants,
                    risk_score: 85,
                    description: format!(
                        "ユーザー {} が過去1時間に{}回の権限変更を受けました",
                        entry.user_id, recent_changes
                    ),
                }));
            }
        }

        Ok(None)
    }

    /// ロールのリスクスコアを計算
    fn calculate_role_risk(&self, from_role: &str, to_role: &str) -> u8 {
        let from_level = self.role_level(from_role);
        let to_level = self.role_level(to_role);

        let diff = to_level.saturating_sub(from_level);

        match diff {
            0 => 0,
            1 => 40,
            2 => 70,
            _ => 90,
        }
    }

    /// ロールのレベルを返す
    fn role_level(&self, role: &str) -> u8 {
        match role {
            "admin" | "superuser" | "root" => 3,
            "moderator" | "manager" => 2,
            "user" | "member" => 1,
            _ => 0,
        }
    }

    /// 検出数をインクリメント
    async fn increment_detection_count(&self) {
        let mut count = self.detection_count.write().await;
        *count += 1;
    }

    /// 分析済みログ数を取得
    pub async fn get_analyzed_count(&self) -> u64 {
        *self.analyzed_count.read().await
    }

    /// 検出数を取得
    pub async fn get_detection_count(&self) -> u64 {
        *self.detection_count.read().await
    }
}

impl Default for PrivilegeDetector {
    fn default() -> Self {
        Self::new()
    }
}
