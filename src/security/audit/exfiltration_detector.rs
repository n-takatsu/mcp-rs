//! Data Exfiltration Detector
//!
//! データ流出検知システム

use super::types::*;
use crate::error::Result;
use chrono::{DateTime, Duration, Timelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// データ流出検知器
pub struct ExfiltrationDetector {
    /// ユーザーごとのアクセス履歴
    access_history: Arc<RwLock<HashMap<String, Vec<DataAccess>>>>,
    /// 検出数
    detection_count: Arc<RwLock<u64>>,
}

/// データアクセス履歴
#[derive(Debug, Clone)]
struct DataAccess {
    timestamp: DateTime<Utc>,
    data_volume: u64,
    data_type: String,
    action: String,
}

impl ExfiltrationDetector {
    /// 新しいデータ流出検知器を作成
    pub fn new() -> Self {
        Self {
            access_history: Arc::new(RwLock::new(HashMap::new())),
            detection_count: Arc::new(RwLock::new(0)),
        }
    }

    /// データ流出を検知
    pub async fn detect(&self, entry: &AuditLogEntry) -> Result<Option<ExfiltrationEvent>> {
        // データアクセス関連のアクションのみ処理
        if !self.is_data_action(&entry.action) {
            return Ok(None);
        }

        // データボリュームを取得
        let data_volume = entry
            .details
            .get("data_volume")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let data_type = entry
            .details
            .get("data_type")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        // 大量データアクセスの検出（履歴記録前に実行）
        let mass_access_event = self
            .detect_mass_access(entry, data_volume, &data_type)
            .await?;

        // アクセス履歴を記録
        {
            let mut history = self.access_history.write().await;
            let user_history = history
                .entry(entry.user_id.clone())
                .or_insert_with(Vec::new);
            user_history.push(DataAccess {
                timestamp: entry.timestamp,
                data_volume,
                data_type: data_type.clone(),
                action: entry.action.clone(),
            });
        }

        // 大量データアクセスが検出された場合
        if let Some(event) = mass_access_event {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 異常なエクスポートの検出
        if let Some(event) = self
            .detect_abnormal_export(entry, data_volume, &data_type)
            .await?
        {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 機密データアクセスの検出
        if let Some(event) = self
            .detect_sensitive_access(entry, data_volume, &data_type)
            .await?
        {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        // 異常な時間帯アクセスの検出
        if let Some(event) = self
            .detect_unusual_time_access(entry, data_volume, &data_type)
            .await?
        {
            self.increment_detection_count().await;
            return Ok(Some(event));
        }

        Ok(None)
    }

    /// データアクセス関連のアクションか判定
    fn is_data_action(&self, action: &str) -> bool {
        action.contains("export")
            || action.contains("download")
            || action.contains("copy")
            || action.contains("read")
            || action.contains("access")
            || action.contains("query")
            || action.contains("api")
    }

    /// 大量データアクセスを検出
    async fn detect_mass_access(
        &self,
        entry: &AuditLogEntry,
        data_volume: u64,
        data_type: &str,
    ) -> Result<Option<ExfiltrationEvent>> {
        // 過去24時間の平均アクセス量を計算
        let avg_volume = self.calculate_average_volume(&entry.user_id).await;

        // 通常の100倍以上のアクセス、または初回で1MB以上
        let is_large_volume = data_volume >= 1_000_000; // 1MB以上
        let is_unusual_multiplier = avg_volume > 0 && data_volume > avg_volume * 100;

        if (avg_volume == 0 || is_unusual_multiplier) && is_large_volume {
            Ok(Some(ExfiltrationEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                user_id: entry.user_id.clone(),
                data_volume,
                data_type: data_type.to_string(),
                pattern: ExfiltrationPattern::MassAccess,
                risk_score: 90,
                description: format!(
                    "ユーザー {} が通常の100倍以上（{}バイト）のデータにアクセスしました",
                    entry.user_id, data_volume
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 異常なエクスポートを検出
    async fn detect_abnormal_export(
        &self,
        entry: &AuditLogEntry,
        data_volume: u64,
        data_type: &str,
    ) -> Result<Option<ExfiltrationEvent>> {
        if !entry.action.contains("export") {
            return Ok(None);
        }

        // 深夜のエクスポート
        let hour = entry.timestamp.hour();
        let is_unusual_time = !(6..=22).contains(&hour);

        // 大量データの深夜エクスポート
        if is_unusual_time && data_volume > 10_000_000 {
            // 10MB以上
            Ok(Some(ExfiltrationEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                user_id: entry.user_id.clone(),
                data_volume,
                data_type: data_type.to_string(),
                pattern: ExfiltrationPattern::AbnormalExport,
                risk_score: 85,
                description: format!(
                    "ユーザー {} が深夜（{}時）に大量データ（{}バイト）をエクスポートしました",
                    entry.user_id, hour, data_volume
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 機密データアクセスを検出
    async fn detect_sensitive_access(
        &self,
        entry: &AuditLogEntry,
        data_volume: u64,
        data_type: &str,
    ) -> Result<Option<ExfiltrationEvent>> {
        let sensitive_types = ["password", "credit_card", "ssn", "api_key", "token"];

        if sensitive_types
            .iter()
            .any(|t| data_type.contains(t) || entry.resource.contains(t))
        {
            Ok(Some(ExfiltrationEvent {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: entry.timestamp,
                user_id: entry.user_id.clone(),
                data_volume,
                data_type: data_type.to_string(),
                pattern: ExfiltrationPattern::SensitiveDataAccess,
                risk_score: 95,
                description: format!(
                    "ユーザー {} が機密データ（{}）にアクセスしました",
                    entry.user_id, data_type
                ),
            }))
        } else {
            Ok(None)
        }
    }

    /// 異常な時間帯アクセスを検出
    async fn detect_unusual_time_access(
        &self,
        entry: &AuditLogEntry,
        data_volume: u64,
        data_type: &str,
    ) -> Result<Option<ExfiltrationEvent>> {
        let hour = entry.timestamp.hour();
        let is_unusual_time = !(6..=22).contains(&hour);

        // 過去のアクセスパターンを確認
        let history = self.access_history.read().await;
        if let Some(user_history) = history.get(&entry.user_id) {
            let usual_hours: Vec<u32> = user_history
                .iter()
                .filter(|access| {
                    entry
                        .timestamp
                        .signed_duration_since(access.timestamp)
                        .num_days()
                        < 30
                })
                .map(|access| access.timestamp.hour())
                .collect();

            let unusual_hour_count = usual_hours
                .iter()
                .filter(|&&h| !(6..=22).contains(&h))
                .count();
            let total_count = usual_hours.len();

            // 通常アクセスの10%未満が異常時間帯の場合
            if is_unusual_time
                && total_count > 10
                && (unusual_hour_count as f64 / total_count as f64) < 0.1
                && data_volume > 1_000_000
            {
                Ok(Some(ExfiltrationEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp: entry.timestamp,
                    user_id: entry.user_id.clone(),
                    data_volume,
                    data_type: data_type.to_string(),
                    pattern: ExfiltrationPattern::UnusualTime,
                    risk_score: 75,
                    description: format!(
                        "ユーザー {} が通常と異なる時間帯（{}時）にデータアクセスを実行しました",
                        entry.user_id, hour
                    ),
                }))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// 平均データ量を計算
    async fn calculate_average_volume(&self, user_id: &str) -> u64 {
        let history = self.access_history.read().await;
        if let Some(user_history) = history.get(user_id) {
            if user_history.is_empty() {
                return 1000; // デフォルト値
            }

            let total: u64 = user_history.iter().map(|a| a.data_volume).sum();
            total / user_history.len() as u64
        } else {
            1000 // デフォルト値
        }
    }

    /// 検出数をインクリメント
    async fn increment_detection_count(&self) {
        let mut count = self.detection_count.write().await;
        *count += 1;
    }

    /// 検出数を取得
    pub async fn get_detection_count(&self) -> u64 {
        *self.detection_count.read().await
    }
}

impl Default for ExfiltrationDetector {
    fn default() -> Self {
        Self::new()
    }
}
