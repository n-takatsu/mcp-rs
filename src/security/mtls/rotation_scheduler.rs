//! Certificate Rotation Scheduler
//!
//! 証明書ローテーションスケジューラーの実装

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// ローテーションスケジューラー
pub struct RotationScheduler {
    /// ローテーション設定
    config: RotationConfig,
    /// スケジュールされたローテーション
    scheduled: Arc<RwLock<HashMap<String, ScheduledRotation>>>,
    /// ローテーション履歴
    history: Arc<RwLock<Vec<RotationEvent>>>,
}

/// スケジュールされたローテーション
#[derive(Debug, Clone)]
pub(crate) struct ScheduledRotation {
    /// 証明書シリアル番号
    serial_number: String,
    /// スケジュール日時
    scheduled_at: DateTime<Utc>,
    /// 有効期限
    expires_at: DateTime<Utc>,
    /// 自動ローテーション有効
    auto_rotate: bool,
}

impl RotationScheduler {
    /// 新しいローテーションスケジューラーを作成
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config,
            scheduled: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// ローテーションをスケジュール
    pub async fn schedule_rotation(
        &self,
        serial_number: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        if !self.config.enable_auto_rotation {
            return Ok(());
        }

        let days_before = Duration::days(self.config.rotation_days_before_expiry as i64);
        let scheduled_at = expires_at - days_before;

        let mut scheduled = self.scheduled.write().await;
        scheduled.insert(
            serial_number.to_string(),
            ScheduledRotation {
                serial_number: serial_number.to_string(),
                scheduled_at,
                expires_at,
                auto_rotate: true,
            },
        );

        Ok(())
    }

    /// 実行対象のローテーションをチェック
    pub async fn check_due_rotations(&self) -> Vec<String> {
        let scheduled = self.scheduled.read().await;
        let now = Utc::now();

        scheduled
            .values()
            .filter(|rotation| rotation.auto_rotate && rotation.scheduled_at <= now)
            .map(|rotation| rotation.serial_number.clone())
            .collect()
    }

    /// ローテーションを実行
    pub async fn execute_rotation(
        &self,
        old_serial: &str,
        new_serial: &str,
    ) -> Result<RotationEvent> {
        let event = RotationEvent {
            id: format!("rotation-{}-{}", old_serial, new_serial),
            old_serial_number: old_serial.to_string(),
            new_serial_number: new_serial.to_string(),
            rotated_at: Utc::now(),
            status: RotationStatus::Success,
            error: None,
        };

        // 履歴に記録
        let mut history = self.history.write().await;
        history.push(event.clone());

        // スケジュールから削除
        let mut scheduled = self.scheduled.write().await;
        scheduled.remove(old_serial);

        Ok(event)
    }

    /// スケジュールをキャンセル
    pub async fn cancel_rotation(&self, serial_number: &str) -> Result<()> {
        let mut scheduled = self.scheduled.write().await;
        scheduled.remove(serial_number);
        Ok(())
    }

    /// 期限切れ間近の証明書を取得（シリアル番号のリスト）
    pub async fn get_expiring_soon(&self, days: u32) -> Vec<String> {
        let scheduled = self.scheduled.read().await;
        let threshold = Utc::now() + Duration::days(days as i64);

        scheduled
            .values()
            .filter(|rotation| rotation.expires_at <= threshold)
            .map(|rotation| rotation.serial_number.clone())
            .collect()
    }

    /// ローテーション履歴を取得
    pub async fn get_history(&self, limit: usize) -> Vec<RotationEvent> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> RotationStatistics {
        let scheduled = self.scheduled.read().await;
        let history = self.history.read().await;

        let completed = history
            .iter()
            .filter(|e| e.status == RotationStatus::Success)
            .count();
        let failed = history
            .iter()
            .filter(|e| e.status == RotationStatus::Failed)
            .count();

        RotationStatistics {
            total_scheduled: scheduled.len(),
            total_completed: completed,
            total_failed: failed,
            expiring_soon_7days: self.count_expiring_soon(&scheduled, 7),
            expiring_soon_30days: self.count_expiring_soon(&scheduled, 30),
        }
    }

    /// 期限切れ間近の証明書数をカウント
    fn count_expiring_soon(
        &self,
        scheduled: &HashMap<String, ScheduledRotation>,
        days: i64,
    ) -> usize {
        let threshold = Utc::now() + Duration::days(days);
        scheduled
            .values()
            .filter(|rotation| rotation.expires_at <= threshold)
            .count()
    }
}

#[allow(clippy::derivable_impls)]
impl Default for RotationScheduler {
    fn default() -> Self {
        Self {
            config: RotationConfig::default(),
            scheduled: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

/// ローテーション統計情報
#[derive(Debug, Clone)]
pub struct RotationStatistics {
    /// スケジュール済み総数
    pub total_scheduled: usize,
    /// 完了総数
    pub total_completed: usize,
    /// 失敗総数
    pub total_failed: usize,
    /// 7日以内に期限切れ
    pub expiring_soon_7days: usize,
    /// 30日以内に期限切れ
    pub expiring_soon_30days: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schedule_rotation() {
        let scheduler = RotationScheduler::default();
        let expires_at = Utc::now() + Duration::days(40);

        scheduler
            .schedule_rotation("123456", expires_at)
            .await
            .unwrap();

        let scheduled = scheduler.scheduled.read().await;
        assert_eq!(scheduled.len(), 1);
        assert!(scheduled.contains_key("123456"));
    }

    #[tokio::test]
    async fn test_check_due_rotations() {
        let scheduler = RotationScheduler::default();
        // 過去の日付でスケジュール（即座に実行対象）
        let expires_at = Utc::now() + Duration::days(1);

        scheduler
            .schedule_rotation("123456", expires_at)
            .await
            .unwrap();

        let due = scheduler.check_due_rotations().await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0], "123456");
    }

    #[tokio::test]
    async fn test_execute_rotation() {
        let scheduler = RotationScheduler::default();
        let expires_at = Utc::now() + Duration::days(40);

        scheduler
            .schedule_rotation("old123", expires_at)
            .await
            .unwrap();

        let event = scheduler
            .execute_rotation("old123", "new456")
            .await
            .unwrap();

        assert_eq!(event.old_serial_number, "old123");
        assert_eq!(event.new_serial_number, "new456");
        assert_eq!(event.status, RotationStatus::Success);

        // スケジュールから削除されていることを確認
        let scheduled = scheduler.scheduled.read().await;
        assert!(!scheduled.contains_key("old123"));

        // 履歴に記録されていることを確認
        let history = scheduler.history.read().await;
        assert_eq!(history.len(), 1);
    }
}
