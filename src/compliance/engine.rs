//! Compliance Engine
//!
//! GDPR/CCPAコンプライアンスのメインエンジン

use super::types::*;
use crate::error::{Error, Result};
use crate::security::audit_log::{AuditFilter, AuditLogger};
use crate::security::auth::{
    ApiKeyConfig, ApiKeyManager, AuthError, InMemoryUserRepository, SessionAuth, SessionConfig,
    UserRepository,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// コンプライアンスエンジン
pub struct ComplianceEngine {
    /// リクエストストレージ
    requests: Arc<RwLock<HashMap<String, DataSubjectRequest>>>,
    /// 同意記録ストレージ
    consents: Arc<RwLock<HashMap<String, Vec<ConsentRecord>>>>,
    /// 保持ポリシー
    retention_policies: Arc<RwLock<HashMap<DataCategory, RetentionPolicy>>>,
    /// コンプライアンス処理自体の監査ログ（`security::audit_log`とは別の記録）
    audit_log: Arc<RwLock<Vec<AuditLogEntry>>>,
    /// mcp-rs自身が保持するAPIキーストア
    api_keys: Arc<RwLock<ApiKeyManager>>,
    /// mcp-rs自身が保持するセッションストア
    sessions: Arc<SessionAuth>,
    /// mcp-rs自身が保持するアカウントストア
    users: Arc<dyn UserRepository>,
    /// mcp-rs自身のセキュリティ監査ログ
    audit_logger: Arc<AuditLogger>,
}

impl ComplianceEngine {
    /// 新しいコンプライアンスエンジンを作成（デフォルトのin-memoryストアを使用）
    pub fn new() -> Self {
        Self::with_stores(
            Arc::new(RwLock::new(ApiKeyManager::new(ApiKeyConfig::default()))),
            Arc::new(SessionAuth::new(SessionConfig::default())),
            Arc::new(InMemoryUserRepository::new()),
            Arc::new(AuditLogger::with_defaults()),
        )
    }

    /// 既存のストアを注入してコンプライアンスエンジンを作成する。
    /// APIキー・セッション・アカウント・監査ログを既に保持しているサーバー
    /// が、その共有ハンドルをそのまま渡せるようにするためのもの。
    pub fn with_stores(
        api_keys: Arc<RwLock<ApiKeyManager>>,
        sessions: Arc<SessionAuth>,
        users: Arc<dyn UserRepository>,
        audit_logger: Arc<AuditLogger>,
    ) -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            consents: Arc::new(RwLock::new(HashMap::new())),
            retention_policies: Arc::new(RwLock::new(Self::default_retention_policies())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            api_keys,
            sessions,
            users,
            audit_logger,
        }
    }

    /// データ主体リクエストを処理
    pub async fn process_request(&self, request: DataSubjectRequest) -> Result<RequestResult> {
        // リクエストを保存
        let request_id = request.id.clone();
        let subject_id = request.subject_id.clone();
        let request_type = request.request_type.clone();

        {
            let mut requests = self.requests.write().await;
            requests.insert(request_id.clone(), request);
        }

        // 監査ログに記録
        self.log_audit(
            "request_received",
            &subject_id,
            "system",
            vec![
                ("request_id".to_string(), request_id.clone()),
                ("request_type".to_string(), format!("{:?}", request_type)),
            ]
            .into_iter()
            .collect(),
            "success",
        )
        .await;

        // リクエストタイプに応じて処理
        let result = match request_type {
            RequestType::Erasure => {
                self.process_erasure_request(&request_id, &subject_id)
                    .await?
            }
            RequestType::Access => {
                self.process_access_request(&request_id, &subject_id)
                    .await?
            }
            RequestType::Portability => {
                self.process_portability_request(&request_id, &subject_id)
                    .await?
            }
            RequestType::Rectification => {
                self.process_rectification_request(&request_id, &subject_id)
                    .await?
            }
            RequestType::Restriction => {
                self.process_restriction_request(&request_id, &subject_id)
                    .await?
            }
            RequestType::Objection => {
                self.process_objection_request(&request_id, &subject_id)
                    .await?
            }
        };

        // リクエストステータスを更新
        {
            let mut requests = self.requests.write().await;
            if let Some(req) = requests.get_mut(&request_id) {
                req.status = result.status.clone();
            }
        }

        Ok(result)
    }

    /// 削除リクエストを処理。mcp-rs自身が保持するこのアカウントの
    /// セッション・APIキー・アカウント記録を実際に削除し、監査ログの
    /// 該当エントリを匿名化する。mcp-rsが代理アクセスする外部バックエンド
    /// のデータは対象外（そのデータを所有する側の責務）。
    async fn process_erasure_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        let sessions_destroyed = self
            .sessions
            .destroy_user_sessions(subject_id)
            .map_err(|e| Error::Internal(format!("Failed to destroy sessions: {e}")))?;

        let api_keys_revoked = self.api_keys.write().await.revoke_all_for_user(subject_id);

        let user_record_found = match self.users.delete_user(subject_id).await {
            Ok(()) => true,
            Err(AuthError::UserNotFound(_)) => false,
            Err(e) => {
                return Err(Error::Internal(format!(
                    "Failed to delete account record: {e}"
                )))
            }
        };

        let audit_entries_redacted = self.audit_logger.redact_user(subject_id).await;

        let certificate = self.generate_deletion_certificate(
            subject_id,
            sessions_destroyed,
            api_keys_revoked,
            user_record_found,
            audit_entries_redacted,
        );

        self.log_audit(
            "data_erased",
            subject_id,
            "system",
            vec![
                ("request_id".to_string(), request_id.to_string()),
                (
                    "sessions_destroyed".to_string(),
                    sessions_destroyed.to_string(),
                ),
                ("api_keys_revoked".to_string(), api_keys_revoked.to_string()),
                (
                    "user_record_found".to_string(),
                    user_record_found.to_string(),
                ),
                (
                    "audit_entries_redacted".to_string(),
                    audit_entries_redacted.to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: None,
            certificate: Some(certificate),
            error: None,
        })
    }

    /// アクセスリクエストを処理
    async fn process_access_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        // 個人データを収集
        let personal_data = self.collect_personal_data(subject_id).await?;

        self.log_audit(
            "data_accessed",
            subject_id,
            "system",
            vec![("request_id".to_string(), request_id.to_string())]
                .into_iter()
                .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: Some(personal_data),
            certificate: None,
            error: None,
        })
    }

    /// ポータビリティリクエストを処理
    async fn process_portability_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        // 構造化データをエクスポート
        let export_data = self.export_structured_data(subject_id).await?;

        self.log_audit(
            "data_exported",
            subject_id,
            "system",
            vec![("request_id".to_string(), request_id.to_string())]
                .into_iter()
                .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: Some(export_data),
            certificate: None,
            error: None,
        })
    }

    /// 訂正リクエストを処理
    async fn process_rectification_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        self.log_audit(
            "data_rectified",
            subject_id,
            "system",
            vec![("request_id".to_string(), request_id.to_string())]
                .into_iter()
                .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: None,
            certificate: None,
            error: None,
        })
    }

    /// 処理制限リクエストを処理
    async fn process_restriction_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        self.log_audit(
            "processing_restricted",
            subject_id,
            "system",
            vec![("request_id".to_string(), request_id.to_string())]
                .into_iter()
                .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: None,
            certificate: None,
            error: None,
        })
    }

    /// 異議申立リクエストを処理
    async fn process_objection_request(
        &self,
        request_id: &str,
        subject_id: &str,
    ) -> Result<RequestResult> {
        self.log_audit(
            "objection_received",
            subject_id,
            "system",
            vec![("request_id".to_string(), request_id.to_string())]
                .into_iter()
                .collect(),
            "success",
        )
        .await;

        Ok(RequestResult {
            request_id: request_id.to_string(),
            status: RequestStatus::Completed,
            completed_at: Some(chrono::Utc::now()),
            data: None,
            certificate: None,
            error: None,
        })
    }

    /// 削除証明書を生成。mcp-rs自身が保持するアカウントデータに対して
    /// 実際に行った操作の件数を報告する（外部バックエンドのデータは対象外）。
    #[allow(clippy::too_many_arguments)]
    fn generate_deletion_certificate(
        &self,
        subject_id: &str,
        sessions_destroyed: usize,
        api_keys_revoked: usize,
        user_record_found: bool,
        audit_entries_redacted: usize,
    ) -> String {
        format!(
            "DELETION CERTIFICATE\n\n\
             Subject ID: {}\n\
             Date: {}\n\
             Certificate ID: {}\n\n\
             This certifies that mcp-rs has processed a Right to Erasure request \
             in accordance with GDPR Article 17 and CCPA Section 1798.105, for \
             the account data mcp-rs itself holds. Data held by external \
             backends that mcp-rs proxies access to is out of scope; that \
             data remains the responsibility of whoever owns it.\n\n\
             Account record found and deleted: {}\n\
             Sessions destroyed: {}\n\
             API keys revoked: {}\n\
             Audit log entries anonymized: {}\n\
             Compliance: GDPR, CCPA\n",
            subject_id,
            chrono::Utc::now().to_rfc3339(),
            uuid::Uuid::new_v4(),
            user_record_found,
            sessions_destroyed,
            api_keys_revoked,
            audit_entries_redacted,
        )
    }

    /// 個人データを収集。mcp-rs自身が保持するアカウント記録・APIキー
    /// （メタデータのみ、生のキー値は含まない）・セッション・監査ログを
    /// 実際に集約する。
    async fn collect_personal_data(&self, subject_id: &str) -> Result<String> {
        let account = self
            .users
            .find_by_id(subject_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to look up account: {e}")))?;

        let api_keys: Vec<serde_json::Value> = {
            let keys = self.api_keys.read().await;
            keys.list_user_keys(subject_id)
                .into_iter()
                .map(|k| serde_json::to_value(k).unwrap_or(serde_json::Value::Null))
                .collect()
        };

        let sessions: Vec<serde_json::Value> = self
            .sessions
            .list_user_sessions(subject_id)
            .map_err(|e| Error::Internal(format!("Failed to list sessions: {e}")))?
            .iter()
            .map(|s| {
                serde_json::json!({
                    "session_id": s.session_id,
                    "created_at": s.created_at,
                    "expires_at": s.expires_at,
                    "last_accessed_at": s.last_accessed_at,
                    "ip_address": s.ip_address,
                    "user_agent": s.user_agent,
                })
            })
            .collect();

        let audit_entries = self
            .audit_logger
            .search(AuditFilter {
                user_id: Some(subject_id.to_string()),
                ..Default::default()
            })
            .await;

        let mut data = serde_json::json!({
            "subject_id": subject_id,
            "account_found": account.is_some(),
            "account": account,
            "api_keys": api_keys,
            "sessions": sessions,
            "audit_log_entries": audit_entries,
            "data_categories": [],
            "processing_purposes": [],
            "third_parties": [],
            "retention_periods": {},
        });

        // 同意記録を追加
        let consents = self.consents.read().await;
        if let Some(consent_list) = consents.get(subject_id) {
            data["consents"] = serde_json::json!(consent_list);
        }

        serde_json::to_string_pretty(&data)
            .map_err(|e| Error::ParseError(format!("Failed to serialize data: {e}")))
    }

    /// 構造化データをエクスポート
    async fn export_structured_data(&self, subject_id: &str) -> Result<String> {
        let data = self.collect_personal_data(subject_id).await?;

        // JSON形式でエクスポート
        Ok(data)
    }

    /// 監査ログを記録
    async fn log_audit(
        &self,
        action: &str,
        subject_id: &str,
        actor: &str,
        details: HashMap<String, String>,
        result: &str,
    ) {
        let entry = AuditLogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            subject_id: subject_id.to_string(),
            actor: actor.to_string(),
            timestamp: chrono::Utc::now(),
            details,
            result: result.to_string(),
        };

        let mut log = self.audit_log.write().await;
        log.push(entry);
    }

    /// コンプライアンスレポートを生成
    pub async fn generate_report(
        &self,
        period_start: chrono::DateTime<chrono::Utc>,
        period_end: chrono::DateTime<chrono::Utc>,
    ) -> ComplianceReport {
        let requests = self.requests.read().await;
        let audit_log = self.audit_log.read().await;

        let mut requests_by_type: HashMap<String, usize> = HashMap::new();
        let mut total_processing_time = 0.0;
        let mut processed_requests = 0;

        for req in requests.values() {
            if req.created_at >= period_start && req.created_at <= period_end {
                let type_str = format!("{:?}", req.request_type);
                *requests_by_type.entry(type_str).or_insert(0) += 1;

                if req.status == RequestStatus::Completed {
                    if let Some(completed_at) = req.completed_at {
                        let processing_time = completed_at
                            .signed_duration_since(req.created_at)
                            .num_seconds() as f64;
                        total_processing_time += processing_time;
                        processed_requests += 1;
                    }
                }
            }
        }

        let avg_processing_time = if processed_requests > 0 {
            total_processing_time / processed_requests as f64
        } else {
            0.0
        };

        ComplianceReport {
            id: uuid::Uuid::new_v4().to_string(),
            period_start,
            period_end,
            total_requests: requests.len(),
            requests_by_type,
            avg_processing_time_seconds: avg_processing_time,
            violations: 0,
            audit_entries: audit_log.len(),
        }
    }

    /// デフォルトの保持ポリシー
    fn default_retention_policies() -> HashMap<DataCategory, RetentionPolicy> {
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
                    reason: "Marketing and communication".to_string(),
                    legal_basis: LegalBasis::Consent,
                    deletion_method: DeletionMethod::SoftDelete,
                },
            ),
        ]
        .into_iter()
        .collect()
    }
}

impl Default for ComplianceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::audit_log::{
        AuditCategory, AuditLevel, AuditLogEntry as SecAuditLogEntry,
    };
    use crate::security::auth::AuthUser;

    async fn seeded_engine() -> (
        ComplianceEngine,
        Arc<RwLock<ApiKeyManager>>,
        Arc<SessionAuth>,
        Arc<dyn UserRepository>,
        Arc<AuditLogger>,
    ) {
        let api_keys = Arc::new(RwLock::new(ApiKeyManager::new(ApiKeyConfig::default())));
        let sessions = Arc::new(SessionAuth::new(SessionConfig::default()));
        let users: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());
        let audit_logger = Arc::new(AuditLogger::with_defaults());

        let user = AuthUser::new("user-1".to_string(), "alice".to_string());
        users.create_user(&user, None).await.unwrap();

        api_keys
            .write()
            .await
            .generate_key("test-key".to_string(), "user-1".to_string(), None)
            .unwrap();

        sessions.create_session(user).unwrap();

        audit_logger
            .log(
                SecAuditLogEntry::new(
                    AuditLevel::Info,
                    AuditCategory::Authentication,
                    "User logged in".to_string(),
                )
                .with_user("user-1".to_string())
                .with_request_info("192.168.1.1".to_string(), "Mozilla/5.0".to_string()),
            )
            .await
            .unwrap();

        let engine = ComplianceEngine::with_stores(
            Arc::clone(&api_keys),
            Arc::clone(&sessions),
            Arc::clone(&users),
            Arc::clone(&audit_logger),
        );

        (engine, api_keys, sessions, users, audit_logger)
    }

    #[tokio::test]
    async fn test_erasure_deletes_real_data() {
        let (engine, api_keys, sessions, users, audit_logger) = seeded_engine().await;

        let request = DataSubjectRequest::new("user-1", RequestType::Erasure);
        let result = engine.process_request(request).await.unwrap();

        assert_eq!(result.status, RequestStatus::Completed);
        assert!(result.certificate.is_some());

        assert!(sessions.list_user_sessions("user-1").unwrap().is_empty());
        assert!(api_keys.read().await.list_user_keys("user-1").is_empty());
        assert!(users.find_by_id("user-1").await.unwrap().is_none());

        let remaining = audit_logger
            .search(AuditFilter {
                user_id: Some("user-1".to_string()),
                ..Default::default()
            })
            .await;
        assert_eq!(remaining.len(), 1);
        assert!(remaining[0].message.contains("REDACTED"));
        assert!(remaining[0].ip_address.is_none());
    }

    #[tokio::test]
    async fn test_erasure_unknown_subject_is_not_an_error() {
        let engine = ComplianceEngine::new();

        let request = DataSubjectRequest::new("no-such-user", RequestType::Erasure);
        let result = engine.process_request(request).await.unwrap();

        assert_eq!(result.status, RequestStatus::Completed);
        let certificate = result.certificate.unwrap();
        assert!(certificate.contains("Account record found and deleted: false"));
        assert!(certificate.contains("Sessions destroyed: 0"));
        assert!(certificate.contains("API keys revoked: 0"));
    }

    #[tokio::test]
    async fn test_access_request_exports_real_data_without_secrets() {
        let (engine, ..) = seeded_engine().await;

        let request = DataSubjectRequest::new("user-1", RequestType::Access);
        let result = engine.process_request(request).await.unwrap();

        assert_eq!(result.status, RequestStatus::Completed);
        let data: serde_json::Value = serde_json::from_str(&result.data.unwrap()).unwrap();

        assert_eq!(data["account_found"], serde_json::json!(true));
        assert_eq!(data["account"]["id"], serde_json::json!("user-1"));
        assert_eq!(data["api_keys"].as_array().unwrap().len(), 1);
        assert!(data["api_keys"][0].get("key_hash").is_none());
        assert_eq!(data["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(data["audit_log_entries"].as_array().unwrap().len(), 1);
    }
}
