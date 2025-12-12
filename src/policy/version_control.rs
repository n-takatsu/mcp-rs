//! ポリシーバージョン管理システム
//!
//! ポリシーのバージョン履歴とdiff機能を提供

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::McpError;
use crate::policy_config::PolicyConfig;

/// バージョン管理マネージャー
pub struct VersionManager {
    /// バージョン履歴
    versions: Arc<RwLock<Vec<PolicyVersion>>>,
    /// 現在のバージョンID
    current_version_id: Arc<RwLock<String>>,
    /// 最大保持バージョン数
    max_versions: usize,
}

/// ポリシーバージョン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    /// バージョンID
    pub id: String,
    /// ポリシースナップショット
    pub policy: PolicyConfig,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 作成者
    pub created_by: String,
    /// 変更理由
    pub change_reason: String,
    /// 親バージョンID
    pub parent_id: Option<String>,
    /// タグ
    pub tags: Vec<String>,
}

/// バージョン間の差分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    /// 変更されたフィールド
    pub changes: Vec<FieldChange>,
    /// 変更概要
    pub summary: String,
}

/// フィールド変更
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    /// フィールドパス（例: "security.encryption.algorithm"）
    pub field_path: String,
    /// 旧値
    pub old_value: Option<String>,
    /// 新値
    pub new_value: Option<String>,
    /// 変更種別
    pub change_type: ChangeType,
}

/// 変更種別
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    /// 追加
    Added,
    /// 変更
    Modified,
    /// 削除
    Removed,
}

impl VersionManager {
    /// 新しいバージョンマネージャーを作成
    pub fn new(initial_policy: PolicyConfig, max_versions: usize) -> Self {
        let version = PolicyVersion {
            id: uuid::Uuid::new_v4().to_string(),
            policy: initial_policy,
            created_at: Utc::now(),
            created_by: "system".to_string(),
            change_reason: "初期バージョン".to_string(),
            parent_id: None,
            tags: vec!["initial".to_string()],
        };

        let version_id = version.id.clone();

        Self {
            versions: Arc::new(RwLock::new(vec![version])),
            current_version_id: Arc::new(RwLock::new(version_id)),
            max_versions,
        }
    }

    /// 新しいバージョンを作成
    pub async fn create_version(
        &self,
        policy: PolicyConfig,
        created_by: String,
        change_reason: String,
    ) -> Result<String, McpError> {
        let current_id = self.current_version_id.read().await.clone();

        let version = PolicyVersion {
            id: uuid::Uuid::new_v4().to_string(),
            policy,
            created_at: Utc::now(),
            created_by,
            change_reason,
            parent_id: Some(current_id),
            tags: vec![],
        };

        let version_id = version.id.clone();

        let mut versions = self.versions.write().await;

        // 最大数を超える場合は古いものを削除（初期バージョンは保持）
        while versions.len() >= self.max_versions && versions.len() > 1 {
            // 初期バージョン以外で最も古いものを削除
            if versions.len() > 1 {
                versions.remove(1);
                info!("古いバージョンを削除");
            }
        }

        versions.push(version);

        // 現在のバージョンIDを更新
        let mut current = self.current_version_id.write().await;
        *current = version_id.clone();

        info!("新しいバージョン作成: {}", version_id);

        Ok(version_id)
    }

    /// 指定したバージョンを取得
    pub async fn get_version(&self, version_id: &str) -> Result<PolicyVersion, McpError> {
        let versions = self.versions.read().await;

        versions
            .iter()
            .find(|v| v.id == version_id)
            .cloned()
            .ok_or_else(|| {
                McpError::NotFound(format!("バージョンが見つかりません: {}", version_id))
            })
    }

    /// 現在のバージョンを取得
    pub async fn get_current_version(&self) -> Result<PolicyVersion, McpError> {
        let version_id = self.current_version_id.read().await.clone();
        self.get_version(&version_id).await
    }

    /// 全バージョンを取得
    pub async fn list_versions(&self) -> Vec<PolicyVersion> {
        self.versions.read().await.clone()
    }

    /// 2つのバージョン間の差分を計算
    pub async fn diff(
        &self,
        old_version_id: &str,
        new_version_id: &str,
    ) -> Result<VersionDiff, McpError> {
        let old_version = self.get_version(old_version_id).await?;
        let new_version = self.get_version(new_version_id).await?;

        let changes = self.compute_policy_diff(&old_version.policy, &new_version.policy);

        let summary = format!("{}個のフィールドが変更されました", changes.len());

        Ok(VersionDiff { changes, summary })
    }

    /// ポリシー間の差分を計算
    fn compute_policy_diff(&self, old: &PolicyConfig, new: &PolicyConfig) -> Vec<FieldChange> {
        let mut changes = Vec::new();

        // バージョンの変更
        if old.version != new.version {
            changes.push(FieldChange {
                field_path: "version".to_string(),
                old_value: Some(old.version.clone()),
                new_value: Some(new.version.clone()),
                change_type: ChangeType::Modified,
            });
        }

        // セキュリティ設定の変更
        self.diff_security_config(&old.security, &new.security, &mut changes);

        // 監視設定の変更
        self.diff_monitoring_config(&old.monitoring, &new.monitoring, &mut changes);

        // 認証設定の変更
        self.diff_auth_config(&old.authentication, &new.authentication, &mut changes);

        changes
    }

    fn diff_security_config(
        &self,
        old: &crate::policy_config::SecurityPolicyConfig,
        new: &crate::policy_config::SecurityPolicyConfig,
        changes: &mut Vec<FieldChange>,
    ) {
        if old.enabled != new.enabled {
            changes.push(FieldChange {
                field_path: "security.enabled".to_string(),
                old_value: Some(old.enabled.to_string()),
                new_value: Some(new.enabled.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        // 暗号化設定
        if old.encryption.algorithm != new.encryption.algorithm {
            changes.push(FieldChange {
                field_path: "security.encryption.algorithm".to_string(),
                old_value: Some(old.encryption.algorithm.clone()),
                new_value: Some(new.encryption.algorithm.clone()),
                change_type: ChangeType::Modified,
            });
        }

        // TLS設定
        if old.tls.min_version != new.tls.min_version {
            changes.push(FieldChange {
                field_path: "security.tls.min_version".to_string(),
                old_value: Some(old.tls.min_version.clone()),
                new_value: Some(new.tls.min_version.clone()),
                change_type: ChangeType::Modified,
            });
        }

        // レート制限
        if old.rate_limiting.requests_per_minute != new.rate_limiting.requests_per_minute {
            changes.push(FieldChange {
                field_path: "security.rate_limiting.requests_per_minute".to_string(),
                old_value: Some(old.rate_limiting.requests_per_minute.to_string()),
                new_value: Some(new.rate_limiting.requests_per_minute.to_string()),
                change_type: ChangeType::Modified,
            });
        }
    }

    fn diff_monitoring_config(
        &self,
        old: &crate::policy_config::MonitoringPolicyConfig,
        new: &crate::policy_config::MonitoringPolicyConfig,
        changes: &mut Vec<FieldChange>,
    ) {
        if old.interval_seconds != new.interval_seconds {
            changes.push(FieldChange {
                field_path: "monitoring.interval_seconds".to_string(),
                old_value: Some(old.interval_seconds.to_string()),
                new_value: Some(new.interval_seconds.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        if old.log_level != new.log_level {
            changes.push(FieldChange {
                field_path: "monitoring.log_level".to_string(),
                old_value: Some(old.log_level.clone()),
                new_value: Some(new.log_level.clone()),
                change_type: ChangeType::Modified,
            });
        }
    }

    fn diff_auth_config(
        &self,
        old: &crate::policy_config::AuthenticationPolicyConfig,
        new: &crate::policy_config::AuthenticationPolicyConfig,
        changes: &mut Vec<FieldChange>,
    ) {
        if old.session_timeout_seconds != new.session_timeout_seconds {
            changes.push(FieldChange {
                field_path: "authentication.session_timeout_seconds".to_string(),
                old_value: Some(old.session_timeout_seconds.to_string()),
                new_value: Some(new.session_timeout_seconds.to_string()),
                change_type: ChangeType::Modified,
            });
        }

        if old.method != new.method {
            changes.push(FieldChange {
                field_path: "authentication.method".to_string(),
                old_value: Some(old.method.clone()),
                new_value: Some(new.method.clone()),
                change_type: ChangeType::Modified,
            });
        }

        if old.require_mfa != new.require_mfa {
            changes.push(FieldChange {
                field_path: "authentication.require_mfa".to_string(),
                old_value: Some(old.require_mfa.to_string()),
                new_value: Some(new.require_mfa.to_string()),
                change_type: ChangeType::Modified,
            });
        }
    }

    /// タグを追加
    pub async fn add_tag(&self, version_id: &str, tag: String) -> Result<(), McpError> {
        let mut versions = self.versions.write().await;

        let version = versions
            .iter_mut()
            .find(|v| v.id == version_id)
            .ok_or_else(|| {
                McpError::NotFound(format!("バージョンが見つかりません: {}", version_id))
            })?;

        if !version.tags.contains(&tag) {
            version.tags.push(tag.clone());
            info!("タグ追加: {} -> {}", version_id, tag);
        }

        Ok(())
    }

    /// タグで検索
    pub async fn find_by_tag(&self, tag: &str) -> Vec<PolicyVersion> {
        self.versions
            .read()
            .await
            .iter()
            .filter(|v| v.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// バージョン履歴を取得（親子関係）
    pub async fn get_version_history(
        &self,
        version_id: &str,
    ) -> Result<Vec<PolicyVersion>, McpError> {
        let versions = self.versions.read().await;
        let mut history = Vec::new();

        let mut current_id = Some(version_id.to_string());

        while let Some(id) = current_id {
            let version = versions
                .iter()
                .find(|v| v.id == id)
                .ok_or_else(|| McpError::NotFound(format!("バージョンが見つかりません: {}", id)))?;

            history.push(version.clone());
            current_id = version.parent_id.clone();
        }

        Ok(history)
    }

    /// バージョン数を取得
    pub async fn count_versions(&self) -> usize {
        self.versions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_config::*;
    use std::collections::HashMap;

    fn create_test_policy(version: &str) -> PolicyConfig {
        PolicyConfig {
            id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            version: version.to_string(),
            description: Some("Test policy".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            security: SecurityPolicyConfig {
                enabled: true,
                encryption: EncryptionConfig {
                    algorithm: "AES-256".to_string(),
                    key_size: 256,
                    pbkdf2_iterations: 100000,
                },
                tls: TlsConfig {
                    enforce: true,
                    min_version: "1.2".to_string(),
                    cipher_suites: vec!["TLS_AES_256_GCM_SHA384".to_string()],
                },
                input_validation: InputValidationConfig {
                    enabled: true,
                    max_input_length: 1024,
                    sql_injection_protection: true,
                    xss_protection: true,
                },
                rate_limiting: RateLimitingConfig {
                    enabled: true,
                    requests_per_minute: 6000,
                    burst_size: 10,
                },
            },
            monitoring: MonitoringPolicyConfig {
                interval_seconds: 60,
                alerts_enabled: true,
                log_level: "info".to_string(),
                metrics: MetricsConfig {
                    enabled: true,
                    sampling_rate: 1.0,
                    buffer_size: 1000,
                },
            },
            authentication: AuthenticationPolicyConfig {
                enabled: true,
                method: "password".to_string(),
                session_timeout_seconds: 1800,
                require_mfa: false,
            },
            custom: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_create_version() {
        let initial = create_test_policy("1.0.0");
        let manager = VersionManager::new(initial, 10);

        let v2 = create_test_policy("2.0.0");
        let result = manager
            .create_version(v2, "user1".to_string(), "Major update".to_string())
            .await;

        assert!(result.is_ok());
        assert_eq!(manager.count_versions().await, 2);
    }

    #[tokio::test]
    async fn test_get_current_version() {
        let initial = create_test_policy("1.0.0");
        let manager = VersionManager::new(initial, 10);

        let current = manager.get_current_version().await.unwrap();
        assert_eq!(current.policy.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_version_diff() {
        let initial = create_test_policy("1.0.0");
        let manager = VersionManager::new(initial, 10);

        let old_version_id = manager.get_current_version().await.unwrap().id;

        let mut v2 = create_test_policy("2.0.0");
        v2.security.encryption.algorithm = "AES-128".to_string();

        let new_version_id = manager
            .create_version(v2, "user1".to_string(), "Algorithm change".to_string())
            .await
            .unwrap();

        let diff = manager
            .diff(&old_version_id, &new_version_id)
            .await
            .unwrap();

        assert!(diff.changes.len() >= 2); // version + algorithm
        assert!(diff.changes.iter().any(|c| c.field_path == "version"));
        assert!(diff
            .changes
            .iter()
            .any(|c| c.field_path == "security.encryption.algorithm"));
    }

    #[tokio::test]
    async fn test_add_tag() {
        let initial = create_test_policy("1.0.0");
        let manager = VersionManager::new(initial, 10);

        let version_id = manager.get_current_version().await.unwrap().id;

        manager
            .add_tag(&version_id, "stable".to_string())
            .await
            .unwrap();

        let found = manager.find_by_tag("stable").await;
        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn test_version_history() {
        let initial = create_test_policy("1.0.0");
        let manager = VersionManager::new(initial, 10);

        let _v2_id = manager
            .create_version(
                create_test_policy("2.0.0"),
                "user1".to_string(),
                "v2".to_string(),
            )
            .await
            .unwrap();

        let v3_id = manager
            .create_version(
                create_test_policy("3.0.0"),
                "user1".to_string(),
                "v3".to_string(),
            )
            .await
            .unwrap();

        let history = manager.get_version_history(&v3_id).await.unwrap();

        assert_eq!(history.len(), 3); // v3 -> v2 -> v1
        assert_eq!(history[0].policy.version, "3.0.0");
        assert_eq!(history[1].policy.version, "2.0.0");
        assert_eq!(history[2].policy.version, "1.0.0");
    }
}
