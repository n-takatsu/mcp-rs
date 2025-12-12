//! ポリシーロールバック管理システム
//!
//! ポリシー更新の失敗時に安全にロールバック

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::error::McpError;
use crate::policy_config::PolicyConfig;

/// ロールバックマネージャー
///
/// ポリシーのロールバックポイントを管理し、安全なロールバックを提供
pub struct RollbackManager {
    /// ロールバックポイントの履歴
    rollback_points: Arc<RwLock<VecDeque<RollbackPoint>>>,
    /// 最大保持するロールバックポイント数
    max_rollback_points: usize,
    /// 現在アクティブなポリシー
    active_policy: Arc<RwLock<PolicyConfig>>,
}

/// ロールバックポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPoint {
    /// ロールバックポイントID
    pub id: String,
    /// ポリシースナップショット
    pub policy: PolicyConfig,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 説明
    pub description: String,
    /// タグ（検索用）
    pub tags: Vec<String>,
}

impl RollbackPoint {
    /// 新しいロールバックポイントを作成
    pub fn new(policy: PolicyConfig, description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            policy,
            created_at: Utc::now(),
            description: description.into(),
            tags: vec![],
        }
    }

    /// タグを追加
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

impl RollbackManager {
    /// 新しいロールバックマネージャーを作成
    ///
    /// # 引数
    /// * `initial_policy` - 初期ポリシー
    /// * `max_rollback_points` - 最大保持するロールバックポイント数
    pub fn new(initial_policy: PolicyConfig, max_rollback_points: usize) -> Self {
        let mut points = VecDeque::new();
        points.push_back(RollbackPoint::new(
            initial_policy.clone(),
            "初期ポリシー",
        ));

        Self {
            rollback_points: Arc::new(RwLock::new(points)),
            max_rollback_points,
            active_policy: Arc::new(RwLock::new(initial_policy)),
        }
    }

    /// ロールバックポイントを作成
    ///
    /// # 引数
    /// * `policy` - 保存するポリシー
    /// * `description` - 説明
    pub async fn create_rollback_point(
        &self,
        policy: PolicyConfig,
        description: impl Into<String>,
    ) -> Result<String, McpError> {
        let point = RollbackPoint::new(policy, description);
        let point_id = point.id.clone();

        let mut points = self.rollback_points.write().await;

        // 最大数を超える場合は古いものを削除
        while points.len() >= self.max_rollback_points {
            if let Some(removed) = points.pop_front() {
                info!("古いロールバックポイントを削除: {}", removed.id);
            }
        }

        points.push_back(point);
        info!("ロールバックポイント作成: {}", point_id);

        Ok(point_id)
    }

    /// 指定したロールバックポイントにロールバック
    ///
    /// # 引数
    /// * `point_id` - ロールバックポイントID
    pub async fn rollback_to_point(&self, point_id: &str) -> Result<(), McpError> {
        info!("ロールバックポイントへ復元中: {}", point_id);

        let points = self.rollback_points.read().await;

        // ロールバックポイントを検索
        let point = points
            .iter()
            .find(|p| p.id == point_id)
            .ok_or_else(|| {
                McpError::NotFound(format!("ロールバックポイントが見つかりません: {}", point_id))
            })?;

        // ポリシーを復元
        let mut active = self.active_policy.write().await;
        *active = point.policy.clone();

        info!("ロールバック成功: {} に復元", point_id);

        Ok(())
    }

    /// 最新のロールバックポイントにロールバック
    pub async fn rollback_to_latest(&self) -> Result<(), McpError> {
        info!("最新のロールバックポイントに復元中...");

        let points = self.rollback_points.read().await;

        let point = points.back().ok_or_else(|| {
            McpError::NotFound("ロールバックポイントが存在しません".to_string())
        })?;

        // ポリシーを復元
        let mut active = self.active_policy.write().await;
        *active = point.policy.clone();

        info!("最新ロールバック成功: {}", point.id);

        Ok(())
    }

    /// N個前のロールバックポイントにロールバック
    ///
    /// # 引数
    /// * `steps` - 何個前に戻るか（1 = 直前）
    pub async fn rollback_n_steps(&self, steps: usize) -> Result<(), McpError> {
        if steps == 0 {
            return Err(McpError::Validation(
                "ロールバックステップは1以上である必要があります".to_string(),
            ));
        }

        info!("{}ステップ前にロールバック中...", steps);

        let points = self.rollback_points.read().await;

        if points.len() <= steps {
            return Err(McpError::NotFound(format!(
                "{}ステップ前のロールバックポイントが存在しません（利用可能: {}）",
                steps,
                points.len() - 1
            )));
        }

        let index = points.len() - 1 - steps;
        let point = &points[index];

        // ポリシーを復元
        let mut active = self.active_policy.write().await;
        *active = point.policy.clone();

        info!("{}ステップ前にロールバック成功: {}", steps, point.id);

        Ok(())
    }

    /// 全てのロールバックポイントを取得
    pub async fn list_rollback_points(&self) -> Vec<RollbackPoint> {
        self.rollback_points.read().await.iter().cloned().collect()
    }

    /// 特定のタグを持つロールバックポイントを検索
    pub async fn find_by_tag(&self, tag: &str) -> Vec<RollbackPoint> {
        self.rollback_points
            .read()
            .await
            .iter()
            .filter(|p| p.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// 現在のポリシーを取得
    pub async fn get_active_policy(&self) -> PolicyConfig {
        self.active_policy.read().await.clone()
    }

    /// ロールバックポイント数を取得
    pub async fn count_rollback_points(&self) -> usize {
        self.rollback_points.read().await.len()
    }

    /// 古いロールバックポイントをクリーンアップ
    ///
    /// # 引数
    /// * `older_than_days` - 何日より古いものを削除するか
    pub async fn cleanup_old_points(&self, older_than_days: i64) -> Result<usize, McpError> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
        let mut points = self.rollback_points.write().await;

        let original_len = points.len();

        // 古いポイントをフィルタリング（最新は常に保持）
        let mut new_points: VecDeque<RollbackPoint> = points
            .iter()
            .filter(|p| p.created_at > cutoff)
            .cloned()
            .collect();

        // 最新のポイントが削除された場合は復元
        if new_points.is_empty() && !points.is_empty() {
            if let Some(latest) = points.back() {
                new_points.push_back(latest.clone());
                warn!("全てのポイントが古いため、最新を保持");
            }
        }

        let removed = original_len - new_points.len();
        *points = new_points;

        info!("{}個の古いロールバックポイントを削除", removed);

        Ok(removed)
    }

    /// ポリシーを更新し、自動的にロールバックポイントを作成
    pub async fn update_with_rollback_point(
        &self,
        new_policy: PolicyConfig,
        description: impl Into<String>,
    ) -> Result<String, McpError> {
        // 現在のポリシーをロールバックポイントとして保存
        let current = self.active_policy.read().await.clone();
        let point_id = self.create_rollback_point(current, description).await?;

        // 新しいポリシーを適用
        let mut active = self.active_policy.write().await;
        *active = new_policy;

        Ok(point_id)
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
    async fn test_create_rollback_point() {
        let initial = create_test_policy("1.0.0");
        let manager = RollbackManager::new(initial, 10);

        let policy = create_test_policy("1.1.0");
        let result = manager.create_rollback_point(policy, "Test point").await;

        assert!(result.is_ok());
        assert_eq!(manager.count_rollback_points().await, 2);
    }

    #[tokio::test]
    async fn test_rollback_to_point() {
        let initial = create_test_policy("1.0.0");
        let manager = RollbackManager::new(initial.clone(), 10);

        let v2 = create_test_policy("2.0.0");
        let point_id = manager
            .create_rollback_point(v2.clone(), "Version 2.0.0")
            .await
            .unwrap();

        // v2を適用
        manager
            .update_with_rollback_point(create_test_policy("3.0.0"), "Version 3.0.0")
            .await
            .unwrap();

        // v2にロールバック
        manager.rollback_to_point(&point_id).await.unwrap();

        let active = manager.get_active_policy().await;
        assert_eq!(active.version, "2.0.0");
    }

    #[tokio::test]
    async fn test_rollback_n_steps() {
        let initial = create_test_policy("1.0.0");
        let manager = RollbackManager::new(initial, 10);

        // v2, v3を追加
        manager
            .create_rollback_point(create_test_policy("2.0.0"), "v2")
            .await
            .unwrap();
        manager
            .create_rollback_point(create_test_policy("3.0.0"), "v3")
            .await
            .unwrap();

        // 1ステップ前（v2）にロールバック
        manager.rollback_n_steps(1).await.unwrap();

        let active = manager.get_active_policy().await;
        assert_eq!(active.version, "2.0.0");
    }

    #[tokio::test]
    async fn test_max_rollback_points() {
        let initial = create_test_policy("1.0.0");
        let manager = RollbackManager::new(initial, 3);

        // 5個追加（最大3個なので古い2個は削除される）
        for i in 2..=6 {
            manager
                .create_rollback_point(create_test_policy(&format!("{}.0.0", i)), "test")
                .await
                .unwrap();
        }

        assert_eq!(manager.count_rollback_points().await, 3);
    }

    #[tokio::test]
    async fn test_find_by_tag() {
        let initial = create_test_policy("1.0.0");
        let manager = RollbackManager::new(initial, 10);

        let mut point = RollbackPoint::new(create_test_policy("2.0.0"), "Tagged point");
        point.tags = vec!["stable".to_string(), "tested".to_string()];

        manager
            .rollback_points
            .write()
            .await
            .push_back(point.clone());

        let found = manager.find_by_tag("stable").await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].policy.version, "2.0.0");
    }
}
