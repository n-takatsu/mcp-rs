//! Column Encryption RBAC Integration
//!
//! カラムレベル暗号化のロールベースアクセス制御を実装します。

use crate::error::{Error, Result};
use crate::security::auth::types::{AuthUser, Permission, Role};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 暗号化操作の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionOperation {
    /// 暗号化
    Encrypt,
    /// 復号
    Decrypt,
    /// 鍵ローテーション
    RotateKey,
}

impl std::fmt::Display for EncryptionOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionOperation::Encrypt => write!(f, "encrypt"),
            EncryptionOperation::Decrypt => write!(f, "decrypt"),
            EncryptionOperation::RotateKey => write!(f, "rotate_key"),
        }
    }
}

/// カラム暗号化権限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnEncryptionPermission {
    pub role_name: String,
    pub table_name: String,
    pub column_name: String,
    pub can_encrypt: bool,
    pub can_decrypt: bool,
    pub can_rotate_key: bool,
}

/// 監査ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionAuditLog {
    pub user_id: String,
    pub operation: EncryptionOperation,
    pub table_name: String,
    pub column_name: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub request_ip: Option<String>,
    pub user_agent: Option<String>,
}

/// 権限キャッシュキー
type PermissionCacheKey = (String, String, String, EncryptionOperation);

/// カラム暗号化RBAC管理
pub struct ColumnEncryptionRbac {
    pool: PgPool,
    /// 権限キャッシュ: (role, table, column, operation) -> bool
    permission_cache: Arc<RwLock<HashMap<PermissionCacheKey, bool>>>,
}

impl ColumnEncryptionRbac {
    /// 新しいRBAC管理を作成
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            permission_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// ユーザーが指定された操作を実行できるか確認
    pub async fn check_permission(
        &self,
        user: &AuthUser,
        table: &str,
        column: &str,
        operation: EncryptionOperation,
    ) -> Result<bool> {
        // Admin は常に全権限を持つ
        if user.is_admin() {
            debug!(
                "Admin user {} has permission for {:?} on {}.{}",
                user.username, operation, table, column
            );
            return Ok(true);
        }

        // 各ロールについて権限をチェック
        for role in &user.roles {
            let role_name = match role {
                Role::Admin => "Admin",
                Role::User => "User",
                Role::Guest => "Guest",
                Role::Custom(name) => name.as_str(),
            };

            // キャッシュをチェック
            let cache_key = (
                role_name.to_string(),
                table.to_string(),
                column.to_string(),
                operation,
            );

            {
                let cache = self.permission_cache.read().await;
                if let Some(&has_permission) = cache.get(&cache_key) {
                    if has_permission {
                        debug!(
                            "Permission found in cache for role {} on {}.{}",
                            role_name, table, column
                        );
                        return Ok(true);
                    }
                }
            }

            // DBから権限を取得
            if self
                .check_role_permission_db(role_name, table, column, operation)
                .await?
            {
                // キャッシュに追加
                let mut cache = self.permission_cache.write().await;
                cache.insert(cache_key, true);
                return Ok(true);
            }
        }

        // パーミッションベースでもチェック
        let permission = Permission::new(format!("{}.{}", table, column), operation.to_string());
        if user.has_permission(&permission) {
            return Ok(true);
        }

        warn!(
            "User {} does not have permission for {:?} on {}.{}",
            user.username, operation, table, column
        );
        Ok(false)
    }

    /// DBから権限をチェック（内部使用）
    async fn check_role_permission_db(
        &self,
        role_name: &str,
        table: &str,
        column: &str,
        operation: EncryptionOperation,
    ) -> Result<bool> {
        // ワイルドカード一致または完全一致をチェック
        let result = sqlx::query_as::<_, (Option<bool>, Option<bool>, Option<bool>)>(
            r#"
            SELECT can_encrypt, can_decrypt, can_rotate_key
            FROM column_encryption_permissions
            WHERE role_name = $1
              AND (table_name = $2 OR table_name = '*')
              AND (column_name = $3 OR column_name = '*')
            ORDER BY 
                CASE WHEN table_name = '*' THEN 1 ELSE 0 END,
                CASE WHEN column_name = '*' THEN 1 ELSE 0 END
            LIMIT 1
            "#,
        )
        .bind(role_name)
        .bind(table)
        .bind(column)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Database error: {}", e)))?;

        if let Some((can_encrypt, can_decrypt, can_rotate_key)) = result {
            let has_permission = match operation {
                EncryptionOperation::Encrypt => can_encrypt.unwrap_or(false),
                EncryptionOperation::Decrypt => can_decrypt.unwrap_or(false),
                EncryptionOperation::RotateKey => can_rotate_key.unwrap_or(false),
            };
            Ok(has_permission)
        } else {
            Ok(false)
        }
    }

    /// 監査ログを記録
    pub async fn audit_log(&self, log: &EncryptionAuditLog) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO encryption_audit_log 
            (user_id, operation, table_name, column_name, success, error_message, request_ip, user_agent)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(&log.user_id)
        .bind(log.operation.to_string())
        .bind(&log.table_name)
        .bind(&log.column_name)
        .bind(log.success)
        .bind(&log.error_message)
        .bind(&log.request_ip)
        .bind(&log.user_agent)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Database error: {}", e)))?;

        if log.success {
            debug!(
                "Audit log: User {} successfully performed {:?} on {}.{}",
                log.user_id, log.operation, log.table_name, log.column_name
            );
        } else {
            warn!(
                "Audit log: User {} failed to perform {:?} on {}.{}: {}",
                log.user_id,
                log.operation,
                log.table_name,
                log.column_name,
                log.error_message.as_deref().unwrap_or("Unknown error")
            );
        }

        Ok(())
    }

    /// 権限を付与
    pub async fn grant_permission(
        &self,
        role_name: &str,
        table: &str,
        column: &str,
        can_encrypt: bool,
        can_decrypt: bool,
        can_rotate_key: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO column_encryption_permissions 
            (role_name, table_name, column_name, can_encrypt, can_decrypt, can_rotate_key)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (role_name, table_name, column_name)
            DO UPDATE SET
                can_encrypt = EXCLUDED.can_encrypt,
                can_decrypt = EXCLUDED.can_decrypt,
                can_rotate_key = EXCLUDED.can_rotate_key
            "#,
        )
        .bind(role_name)
        .bind(table)
        .bind(column)
        .bind(can_encrypt)
        .bind(can_decrypt)
        .bind(can_rotate_key)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Database error: {}", e)))?;

        // キャッシュをクリア
        self.clear_cache().await;

        info!(
            "Granted permission for role {} on {}.{}: encrypt={}, decrypt={}, rotate={}",
            role_name, table, column, can_encrypt, can_decrypt, can_rotate_key
        );

        Ok(())
    }

    /// 権限を取り消し
    pub async fn revoke_permission(
        &self,
        role_name: &str,
        table: &str,
        column: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM column_encryption_permissions WHERE role_name = $1 AND table_name = $2 AND column_name = $3"
        )
        .bind(role_name)
        .bind(table)
        .bind(column)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Database error: {}", e)))?;

        // キャッシュをクリア
        self.clear_cache().await;

        info!(
            "Revoked permission for role {} on {}.{}",
            role_name, table, column
        );

        Ok(())
    }

    /// キャッシュをクリア
    pub async fn clear_cache(&self) {
        let mut cache = self.permission_cache.write().await;
        cache.clear();
        debug!("Permission cache cleared");
    }

    /// 監査ログを取得
    pub async fn get_audit_logs(
        &self,
        user_id: Option<&str>,
        table: Option<&str>,
        limit: i64,
    ) -> Result<Vec<EncryptionAuditLog>> {
        type LogRow = (
            String,
            String,
            String,
            String,
            bool,
            Option<String>,
            Option<String>,
            Option<String>,
        );

        let logs: Vec<LogRow> = match (user_id, table) {
            (Some(uid), Some(tbl)) => {
                sqlx::query_as(
                    r#"
                    SELECT user_id, operation, table_name, column_name, success, 
                           error_message, request_ip, user_agent
                    FROM encryption_audit_log
                    WHERE user_id = $1 AND table_name = $2
                    ORDER BY timestamp DESC
                    LIMIT $3
                    "#,
                )
                .bind(uid)
                .bind(tbl)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (Some(uid), None) => {
                sqlx::query_as(
                    r#"
                    SELECT user_id, operation, table_name, column_name, success, 
                           error_message, request_ip, user_agent
                    FROM encryption_audit_log
                    WHERE user_id = $1
                    ORDER BY timestamp DESC
                    LIMIT $2
                    "#,
                )
                .bind(uid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (None, Some(tbl)) => {
                sqlx::query_as(
                    r#"
                    SELECT user_id, operation, table_name, column_name, success, 
                           error_message, request_ip, user_agent
                    FROM encryption_audit_log
                    WHERE table_name = $1
                    ORDER BY timestamp DESC
                    LIMIT $2
                    "#,
                )
                .bind(tbl)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (None, None) => {
                sqlx::query_as(
                    r#"
                    SELECT user_id, operation, table_name, column_name, success, 
                           error_message, request_ip, user_agent
                    FROM encryption_audit_log
                    ORDER BY timestamp DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| Error::Internal(format!("Database error: {}", e)))?;

        let result = logs
            .into_iter()
            .map(
                |(
                    user_id,
                    operation,
                    table_name,
                    column_name,
                    success,
                    error_message,
                    request_ip,
                    user_agent,
                )| {
                    let operation = match operation.as_str() {
                        "encrypt" => EncryptionOperation::Encrypt,
                        "decrypt" => EncryptionOperation::Decrypt,
                        "rotate_key" => EncryptionOperation::RotateKey,
                        _ => EncryptionOperation::Decrypt, // fallback
                    };

                    EncryptionAuditLog {
                        user_id,
                        operation,
                        table_name,
                        column_name,
                        success,
                        error_message,
                        request_ip,
                        user_agent,
                    }
                },
            )
            .collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_operation_display() {
        assert_eq!(EncryptionOperation::Encrypt.to_string(), "encrypt");
        assert_eq!(EncryptionOperation::Decrypt.to_string(), "decrypt");
        assert_eq!(EncryptionOperation::RotateKey.to_string(), "rotate_key");
    }
}
