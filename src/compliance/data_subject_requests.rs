//! Data Subject Requests
//!
//! データ主体リクエストの処理システム

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// データ主体リクエスト処理システム
pub struct DataSubjectRequestHandler {
    /// リクエストストレージ
    requests: Arc<RwLock<HashMap<String, DataSubjectRequest>>>,
    /// 処理履歴
    history: Arc<RwLock<Vec<RequestHistory>>>,
}

/// リクエスト処理履歴
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestHistory {
    /// リクエストID
    pub request_id: String,
    /// アクション
    pub action: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 詳細
    pub details: String,
}

impl DataSubjectRequestHandler {
    /// 新しいリクエストハンドラーを作成
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// リクエストを提出
    pub async fn submit_request(&self, request: DataSubjectRequest) -> Result<String> {
        let request_id = request.id.clone();

        // 重複チェック
        {
            let requests = self.requests.read().await;
            if requests.contains_key(&request_id) {
                return Err(Error::AlreadyExists(format!(
                    "Request already exists: {}",
                    request_id
                )));
            }
        }

        // リクエストを保存
        {
            let mut requests = self.requests.write().await;
            requests.insert(request_id.clone(), request.clone());
        }

        // 履歴に記録
        self.add_history(
            &request_id,
            "submitted",
            format!("Request type: {:?}", request.request_type),
        )
        .await;

        Ok(request_id)
    }

    /// リクエストステータスを更新
    pub async fn update_status(&self, request_id: &str, new_status: RequestStatus) -> Result<()> {
        let mut requests = self.requests.write().await;

        if let Some(request) = requests.get_mut(request_id) {
            let old_status = request.status.clone();
            request.status = new_status.clone();

            drop(requests); // ロックを解放

            self.add_history(
                request_id,
                "status_updated",
                format!("From {:?} to {:?}", old_status, new_status),
            )
            .await;

            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Request not found: {}",
                request_id
            )))
        }
    }

    /// リクエストを取得
    pub async fn get_request(&self, request_id: &str) -> Result<DataSubjectRequest> {
        let requests = self.requests.read().await;
        requests
            .get(request_id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("Request not found: {}", request_id)))
    }

    /// データ主体の全リクエストを取得
    pub async fn get_requests_by_subject(&self, subject_id: &str) -> Vec<DataSubjectRequest> {
        let requests = self.requests.read().await;
        requests
            .values()
            .filter(|r| r.subject_id == subject_id)
            .cloned()
            .collect()
    }

    /// 期限切れリクエストを取得
    pub async fn get_overdue_requests(&self) -> Vec<DataSubjectRequest> {
        let requests = self.requests.read().await;
        let now = Utc::now();

        requests
            .values()
            .filter(|r| {
                r.deadline < now
                    && r.status != RequestStatus::Completed
                    && r.status != RequestStatus::Rejected
            })
            .cloned()
            .collect()
    }

    /// リクエスト期限を延長
    pub async fn extend_deadline(
        &self,
        request_id: &str,
        additional_days: i64,
    ) -> Result<DateTime<Utc>> {
        let mut requests = self.requests.write().await;

        if let Some(request) = requests.get_mut(request_id) {
            request.deadline += chrono::Duration::days(additional_days);
            request.status = RequestStatus::Extended;

            let new_deadline = request.deadline;

            drop(requests); // ロックを解放

            self.add_history(
                request_id,
                "deadline_extended",
                format!("Extended by {} days", additional_days),
            )
            .await;

            Ok(new_deadline)
        } else {
            Err(Error::NotFound(format!(
                "Request not found: {}",
                request_id
            )))
        }
    }

    /// リクエストを拒否
    pub async fn reject_request(&self, request_id: &str, reason: String) -> Result<()> {
        self.update_status(request_id, RequestStatus::Rejected)
            .await?;

        self.add_history(request_id, "rejected", reason).await;

        Ok(())
    }

    /// リクエスト統計を取得
    pub async fn get_statistics(&self) -> RequestStatistics {
        let requests = self.requests.read().await;

        let mut total = 0;
        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut overdue = 0;

        let now = Utc::now();

        for request in requests.values() {
            total += 1;

            *by_status
                .entry(format!("{:?}", request.status))
                .or_insert(0) += 1;

            *by_type
                .entry(format!("{:?}", request.request_type))
                .or_insert(0) += 1;

            if request.deadline < now
                && request.status != RequestStatus::Completed
                && request.status != RequestStatus::Rejected
            {
                overdue += 1;
            }
        }

        RequestStatistics {
            total_requests: total,
            requests_by_status: by_status,
            requests_by_type: by_type,
            overdue_requests: overdue,
        }
    }

    /// 履歴に追加
    async fn add_history(&self, request_id: &str, action: &str, details: String) {
        let entry = RequestHistory {
            request_id: request_id.to_string(),
            action: action.to_string(),
            timestamp: Utc::now(),
            details,
        };

        let mut history = self.history.write().await;
        history.push(entry);
    }

    /// リクエスト履歴を取得
    pub async fn get_request_history(&self, request_id: &str) -> Vec<RequestHistory> {
        let history = self.history.read().await;
        history
            .iter()
            .filter(|h| h.request_id == request_id)
            .cloned()
            .collect()
    }
}

/// リクエスト統計
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestStatistics {
    /// 総リクエスト数
    pub total_requests: usize,
    /// ステータス別リクエスト数
    pub requests_by_status: HashMap<String, usize>,
    /// タイプ別リクエスト数
    pub requests_by_type: HashMap<String, usize>,
    /// 期限切れリクエスト数
    pub overdue_requests: usize,
}

impl Default for DataSubjectRequestHandler {
    fn default() -> Self {
        Self::new()
    }
}
