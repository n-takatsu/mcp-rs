//! PostgreSQL ユーザーリポジトリ実装
//!
//! 本番環境推奨のデータベースバックエンド。
//! sqlxを使用した非同期クエリ実行とコネクションプーリング。

use super::UserRepository;
use crate::security::auth::types::{
    AuthError, AuthResult, AuthUser, PasswordHasher, Permission, Role,
};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::sync::Arc;

/// PostgreSQL ユーザーリポジトリ
pub struct PostgresUserRepository {
    pool: PgPool,
    password_hasher: PasswordHasher,
}

impl PostgresUserRepository {
    /// 新しいPostgreSQLリポジトリを作成
    ///
    /// # Arguments
    /// * `database_url` - PostgreSQL接続URL (例: "postgresql://user:pass@localhost/dbname")
    /// * `max_connections` - 最大接続数
    /// * `salt_rounds` - Argon2のコスト係数 (推奨: 12)
    pub async fn new(
        database_url: &str,
        max_connections: u32,
        salt_rounds: u32,
    ) -> Result<Self, AuthError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| AuthError::Internal(format!("Database connection failed: {}", e)))?;

        Ok(Self {
            pool,
            password_hasher: PasswordHasher::new(salt_rounds),
        })
    }

    /// マイグレーションを実行
    ///
    /// データベーススキーマを初期化します。
    pub async fn run_migrations(&self) -> Result<(), AuthError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                email TEXT UNIQUE,
                roles TEXT NOT NULL,
                permissions TEXT NOT NULL,
                provider TEXT NOT NULL,
                metadata TEXT NOT NULL,
                password_hash TEXT,
                created_at TIMESTAMP NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP NOT NULL DEFAULT NOW()
            );
            
            CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
            CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("Migration failed: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn create_user(
        &self,
        user: &AuthUser,
        password_hash: Option<String>,
    ) -> Result<(), AuthError> {
        // メールアドレスの重複チェック
        if let Some(ref email) = user.email {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
                    .bind(email)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| AuthError::Internal(format!("Email check failed: {}", e)))?;

            if exists {
                return Err(AuthError::UserAlreadyExists(email.clone()));
            }
        }

        // ロールとパーミッションをJSON文字列に変換
        let roles_json = serde_json::to_string(&user.roles)
            .map_err(|e| AuthError::Internal(format!("Roles serialization failed: {}", e)))?;
        let permissions_json = serde_json::to_string(&user.permissions)
            .map_err(|e| AuthError::Internal(format!("Permissions serialization failed: {}", e)))?;
        let provider_str = format!("{:?}", user.provider);
        let metadata_json = serde_json::to_string(&user.metadata)
            .map_err(|e| AuthError::Internal(format!("Metadata serialization failed: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, roles, permissions, provider, metadata, password_hash)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&roles_json)
        .bind(&permissions_json)
        .bind(&provider_str)
        .bind(&metadata_json)
        .bind(&password_hash)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("User creation failed: {}", e)))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<AuthUser>, AuthError> {
        let row = sqlx::query(
            r#"
            SELECT id, username, email, roles, permissions, provider, metadata
            FROM users WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("User lookup failed: {}", e)))?;

        if let Some(row) = row {
            let roles: std::collections::HashSet<Role> = serde_json::from_str(row.get("roles"))
                .map_err(|e| AuthError::Internal(format!("Roles deserialization failed: {}", e)))?;
            let permissions: std::collections::HashSet<Permission> =
                serde_json::from_str(row.get("permissions")).map_err(|e| {
                    AuthError::Internal(format!("Permissions deserialization failed: {}", e))
                })?;
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(row.get("metadata")).map_err(|e| {
                    AuthError::Internal(format!("Metadata deserialization failed: {}", e))
                })?;

            let provider_str: String = row.get("provider");
            let provider = match provider_str.as_str() {
                "Local" => crate::security::auth::types::AuthProvider::Local,
                "Google" => crate::security::auth::types::AuthProvider::Google,
                "GitHub" => crate::security::auth::types::AuthProvider::GitHub,
                "Microsoft" => crate::security::auth::types::AuthProvider::Microsoft,
                _ => crate::security::auth::types::AuthProvider::Local,
            };

            Ok(Some(AuthUser {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                roles,
                permissions,
                provider,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<AuthUser>, AuthError> {
        let row = sqlx::query(
            r#"
            SELECT id, username, email, roles, permissions, provider, metadata
            FROM users WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("User lookup failed: {}", e)))?;

        if let Some(row) = row {
            let roles: std::collections::HashSet<Role> = serde_json::from_str(row.get("roles"))
                .map_err(|e| AuthError::Internal(format!("Roles deserialization failed: {}", e)))?;
            let permissions: std::collections::HashSet<Permission> =
                serde_json::from_str(row.get("permissions")).map_err(|e| {
                    AuthError::Internal(format!("Permissions deserialization failed: {}", e))
                })?;
            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_str(row.get("metadata")).map_err(|e| {
                    AuthError::Internal(format!("Metadata deserialization failed: {}", e))
                })?;

            let provider_str: String = row.get("provider");
            let provider = match provider_str.as_str() {
                "Local" => crate::security::auth::types::AuthProvider::Local,
                "Google" => crate::security::auth::types::AuthProvider::Google,
                "GitHub" => crate::security::auth::types::AuthProvider::GitHub,
                "Microsoft" => crate::security::auth::types::AuthProvider::Microsoft,
                _ => crate::security::auth::types::AuthProvider::Local,
            };

            Ok(Some(AuthUser {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                roles,
                permissions,
                provider,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    async fn update_user(&self, user: &AuthUser) -> Result<(), AuthError> {
        let roles_json = serde_json::to_string(&user.roles)
            .map_err(|e| AuthError::Internal(format!("Roles serialization failed: {}", e)))?;
        let permissions_json = serde_json::to_string(&user.permissions)
            .map_err(|e| AuthError::Internal(format!("Permissions serialization failed: {}", e)))?;
        let provider_str = format!("{:?}", user.provider);
        let metadata_json = serde_json::to_string(&user.metadata)
            .map_err(|e| AuthError::Internal(format!("Metadata serialization failed: {}", e)))?;

        let result = sqlx::query(
            r#"
            UPDATE users
            SET username = $2, email = $3, roles = $4, permissions = $5, 
                provider = $6, metadata = $7, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&roles_json)
        .bind(&permissions_json)
        .bind(&provider_str)
        .bind(&metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("User update failed: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AuthError::UserNotFound(user.id.clone()));
        }

        Ok(())
    }

    async fn delete_user(&self, id: &str) -> Result<(), AuthError> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AuthError::Internal(format!("User deletion failed: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AuthError::UserNotFound(id.to_string()));
        }

        Ok(())
    }

    async fn verify_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<Option<AuthUser>, AuthError> {
        let row = sqlx::query(
            r#"
            SELECT id, username, email, roles, permissions, provider, metadata, password_hash
            FROM users WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Internal(format!("User lookup failed: {}", e)))?;

        if let Some(row) = row {
            let password_hash: Option<String> = row.get("password_hash");

            if let Some(hash) = password_hash {
                if self.password_hasher.verify(password, &hash)? {
                    let roles: std::collections::HashSet<Role> =
                        serde_json::from_str(row.get("roles")).map_err(|e| {
                            AuthError::Internal(format!("Roles deserialization failed: {}", e))
                        })?;
                    let permissions: std::collections::HashSet<Permission> =
                        serde_json::from_str(row.get("permissions")).map_err(|e| {
                            AuthError::Internal(format!(
                                "Permissions deserialization failed: {}",
                                e
                            ))
                        })?;
                    let metadata: std::collections::HashMap<String, String> =
                        serde_json::from_str(row.get("metadata")).map_err(|e| {
                            AuthError::Internal(format!("Metadata deserialization failed: {}", e))
                        })?;

                    let provider_str: String = row.get("provider");
                    let provider = match provider_str.as_str() {
                        "Local" => crate::security::auth::types::AuthProvider::Local,
                        "Google" => crate::security::auth::types::AuthProvider::Google,
                        "GitHub" => crate::security::auth::types::AuthProvider::GitHub,
                        "Microsoft" => crate::security::auth::types::AuthProvider::Microsoft,
                        _ => crate::security::auth::types::AuthProvider::Local,
                    };

                    return Ok(Some(AuthUser {
                        id: row.get("id"),
                        username: row.get("username"),
                        email: row.get("email"),
                        roles,
                        permissions,
                        provider,
                        metadata,
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn get_password_hash(&self, user_id: &str) -> Result<Option<String>, AuthError> {
        let hash: Option<String> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AuthError::Internal(format!("Password hash lookup failed: {}", e)))?;

        Ok(hash)
    }

    async fn update_password_hash(
        &self,
        user_id: &str,
        password_hash: String,
    ) -> Result<(), AuthError> {
        let result =
            sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
                .bind(user_id)
                .bind(&password_hash)
                .execute(&self.pool)
                .await
                .map_err(|e| AuthError::Internal(format!("Password hash update failed: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AuthError::UserNotFound(user_id.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::Role;

    // 注意: これらのテストはPostgreSQLサーバーが必要です
    // 環境変数 DATABASE_URL を設定してください
    // 例: DATABASE_URL=postgresql://postgres:password@localhost/test_db

    async fn setup_test_db() -> PostgresUserRepository {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:password@localhost/test_db".to_string());

        let repo = PostgresUserRepository::new(&database_url, 5, 12)
            .await
            .expect("Failed to connect to test database");

        repo.run_migrations()
            .await
            .expect("Failed to run migrations");

        // テーブルをクリア
        sqlx::query("DELETE FROM users")
            .execute(&repo.pool)
            .await
            .expect("Failed to clear users table");

        repo
    }

    fn create_test_user(id: &str, username: &str, email: &str) -> AuthUser {
        let mut user = AuthUser::new(id.to_string(), username.to_string());
        user.email = Some(email.to_string());
        user.roles.insert(Role::User);
        user
    }

    #[tokio::test]
    #[ignore] // DATABASE_URL設定が必要
    async fn test_create_and_find_user() {
        let repo = setup_test_db().await;
        let user = create_test_user("user1", "testuser", "test@example.com");
        let password_hash = Some("hash123".to_string());

        repo.create_user(&user, password_hash.clone())
            .await
            .unwrap();

        let found = repo.find_by_id("user1").await.unwrap().unwrap();
        assert_eq!(found.email, Some("test@example.com".to_string()));

        let found_by_email = repo
            .find_by_email("test@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_by_email.id, "user1");

        let hash = repo.get_password_hash("user1").await.unwrap().unwrap();
        assert_eq!(hash, "hash123");
    }

    #[tokio::test]
    #[ignore] // DATABASE_URL設定が必要
    async fn test_duplicate_email() {
        let repo = setup_test_db().await;
        let user1 = create_test_user("user1", "testuser1", "test@example.com");
        let user2 = create_test_user("user2", "testuser2", "test@example.com");

        repo.create_user(&user1, None).await.unwrap();
        let result = repo.create_user(&user2, None).await;

        assert!(matches!(result, Err(AuthError::UserAlreadyExists(_))));
    }

    #[tokio::test]
    #[ignore] // DATABASE_URL設定が必要
    async fn test_update_user() {
        let repo = setup_test_db().await;
        let mut user = create_test_user("user1", "testuser", "test@example.com");

        repo.create_user(&user, None).await.unwrap();

        user.username = "Updated Name".to_string();
        repo.update_user(&user).await.unwrap();

        let found = repo.find_by_id("user1").await.unwrap().unwrap();
        assert_eq!(found.username, "Updated Name");
    }

    #[tokio::test]
    #[ignore] // DATABASE_URL設定が必要
    async fn test_delete_user() {
        let repo = setup_test_db().await;
        let user = create_test_user("user1", "testuser", "test@example.com");

        repo.create_user(&user, None).await.unwrap();
        repo.delete_user("user1").await.unwrap();

        let found = repo.find_by_id("user1").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    #[ignore] // DATABASE_URL設定が必要
    async fn test_verify_password() {
        let repo = setup_test_db().await;
        let user = create_test_user("user1", "testuser", "test@example.com");
        let password = "TestPassword123!";
        let password_hash = repo.password_hasher.hash(password).unwrap();

        repo.create_user(&user, Some(password_hash)).await.unwrap();

        let verified = repo
            .verify_password("test@example.com", password)
            .await
            .unwrap();
        assert!(verified.is_some());

        let wrong = repo
            .verify_password("test@example.com", "wrong")
            .await
            .unwrap();
        assert!(wrong.is_none());
    }
}
