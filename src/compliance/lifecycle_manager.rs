//! Lifecycle Manager
//!
//! データライフサイクル管理システム

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// データライフサイクル管理システム
pub struct LifecycleManager {
    /// 保持ポリシー
    policies: Arc<RwLock<HashMap<DataCategory, RetentionPolicy>>>,
    /// データエントリ
    data_entries: Arc<RwLock<Vec<DataEntry>>>,
}

/// データエントリ
#[derive(Debug, Clone)]
pub struct DataEntry {
    /// エントリID
    pub id: String,
    /// データ主体の識別子
    pub subject_id: String,
    /// データカテゴリ
    pub category: DataCategory,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 削除予定日
    pub deletion_date: DateTime<Utc>,
    /// ステータス
    pub status: DataStatus,
}

/// データステータス
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataStatus {
    /// アクティブ
    Active,
    /// アーカイブ済み
    Archived,
    /// 削除予定
    ScheduledForDeletion,
    /// 削除済み
    Deleted,
}

impl LifecycleManager {
    /// 新しいライフサイクル管理システムを作成
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Self::default_policies())),
            data_entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 保持ポリシーを追加
    pub async fn add_retention_policy(&self, policy: RetentionPolicy) -> Result<()> {
        let mut policies = self.policies.write().await;
        policies.insert(policy.data_category.clone(), policy);
        Ok(())
    }

    /// 保持ポリシーを取得
    pub async fn get_retention_policy(&self, category: &DataCategory) -> Result<RetentionPolicy> {
        let policies = self.policies.read().await;
        policies
            .get(category)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("No policy found for category: {:?}", category)))
    }

    /// データエントリを登録
    pub async fn register_data(
        &self,
        subject_id: String,
        category: DataCategory,
    ) -> Result<String> {
        let policies = self.policies.read().await;
        let policy = policies
            .get(&category)
            .ok_or_else(|| Error::NotFound(format!("No policy for category: {:?}", category)))?;

        let created_at = Utc::now();
        let deletion_date = created_at + chrono::Duration::days(policy.retention_days as i64);

        let entry = DataEntry {
            id: uuid::Uuid::new_v4().to_string(),
            subject_id,
            category,
            created_at,
            deletion_date,
            status: DataStatus::Active,
        };

        let id = entry.id.clone();

        let mut entries = self.data_entries.write().await;
        entries.push(entry);

        Ok(id)
    }

    /// 削除予定のデータを取得
    pub async fn get_data_for_deletion(&self) -> Vec<DataEntry> {
        let entries = self.data_entries.read().await;
        let now = Utc::now();

        entries
            .iter()
            .filter(|e| e.deletion_date <= now && e.status == DataStatus::Active)
            .cloned()
            .collect()
    }

    /// データを削除
    pub async fn delete_data(&self, entry_id: &str) -> Result<()> {
        let mut entries = self.data_entries.write().await;

        if let Some(entry) = entries.iter_mut().find(|e| e.id == entry_id) {
            entry.status = DataStatus::Deleted;
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Data entry not found: {}",
                entry_id
            )))
        }
    }

    /// データをアーカイブ
    pub async fn archive_data(&self, entry_id: &str) -> Result<()> {
        let mut entries = self.data_entries.write().await;

        if let Some(entry) = entries.iter_mut().find(|e| e.id == entry_id) {
            entry.status = DataStatus::Archived;
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Data entry not found: {}",
                entry_id
            )))
        }
    }

    /// 自動削除ジョブを実行
    pub async fn run_auto_deletion_job(&self) -> Result<usize> {
        let to_delete = self.get_data_for_deletion().await;
        let count = to_delete.len();

        for entry in to_delete {
            self.delete_data(&entry.id).await?;
        }

        Ok(count)
    }

    /// ライフサイクル統計を取得
    pub async fn get_statistics(&self) -> LifecycleStatistics {
        let entries = self.data_entries.read().await;

        let mut total = 0;
        let mut active = 0;
        let mut archived = 0;
        let mut scheduled = 0;
        let mut deleted = 0;
        let mut by_category: HashMap<String, usize> = HashMap::new();

        let now = Utc::now();

        for entry in entries.iter() {
            total += 1;

            match entry.status {
                DataStatus::Active => {
                    if entry.deletion_date <= now {
                        scheduled += 1;
                    } else {
                        active += 1;
                    }
                }
                DataStatus::Archived => archived += 1,
                DataStatus::ScheduledForDeletion => scheduled += 1,
                DataStatus::Deleted => deleted += 1,
            }

            *by_category
                .entry(format!("{:?}", entry.category))
                .or_insert(0) += 1;
        }

        LifecycleStatistics {
            total_entries: total,
            active_entries: active,
            archived_entries: archived,
            scheduled_for_deletion: scheduled,
            deleted_entries: deleted,
            entries_by_category: by_category,
        }
    }

    /// デフォルトポリシー
    fn default_policies() -> HashMap<DataCategory, RetentionPolicy> {
        vec![
            (
                DataCategory::PersonalIdentifiable,
                RetentionPolicy {
                    id: uuid::Uuid::new_v4().to_string(),
                    data_category: DataCategory::PersonalIdentifiable,
                    retention_days: 2557, // 7 years
                    reason: "Legal and tax obligations".to_string(),
                    legal_basis: LegalBasis::LegalObligation,
                    deletion_method: DeletionMethod::HardDelete,
                },
            ),
            (
                DataCategory::ContactInformation,
                RetentionPolicy {
                    id: uuid::Uuid::new_v4().to_string(),
                    data_category: DataCategory::ContactInformation,
                    retention_days: 1095, // 3 years
                    reason: "Marketing purposes".to_string(),
                    legal_basis: LegalBasis::Consent,
                    deletion_method: DeletionMethod::SoftDelete,
                },
            ),
            (
                DataCategory::Behavioral,
                RetentionPolicy {
                    id: uuid::Uuid::new_v4().to_string(),
                    data_category: DataCategory::Behavioral,
                    retention_days: 365, // 1 year
                    reason: "Analytics and improvement".to_string(),
                    legal_basis: LegalBasis::LegitimateInterests,
                    deletion_method: DeletionMethod::Anonymize,
                },
            ),
        ]
        .into_iter()
        .collect()
    }
}

/// ライフサイクル統計
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LifecycleStatistics {
    /// 総エントリ数
    pub total_entries: usize,
    /// アクティブエントリ数
    pub active_entries: usize,
    /// アーカイブ済みエントリ数
    pub archived_entries: usize,
    /// 削除予定エントリ数
    pub scheduled_for_deletion: usize,
    /// 削除済みエントリ数
    pub deleted_entries: usize,
    /// カテゴリ別エントリ数
    pub entries_by_category: HashMap<String, usize>,
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}
