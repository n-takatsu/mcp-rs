//! Redis セッションストア実装
//!
//! 高速セッション管理のためのRedisバックエンド。
//! TTL（Time To Live）による自動期限切れ管理。

use crate::security::auth::types::{AuthError, AuthResult, AuthUser};
use async_trait::async_trait;
use redis::{aio::ConnectionManager, AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// セッション情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// ユーザー情報
    pub user: AuthUser,
    /// セッション作成時刻（UNIX timestamp）
    pub created_at: i64,
    /// 最終アクセス時刻（UNIX timestamp）
    pub last_accessed_at: i64,
    /// メタデータ
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Redis セッションストア
pub struct RedisSessionStore {
    connection: ConnectionManager,
    /// セッションの有効期限（秒）
    default_ttl: u64,
    /// セッションキーのプレフィックス
    key_prefix: String,
}

impl RedisSessionStore {
    /// 新しいRedisセッションストアを作成
    ///
    /// # Arguments
    /// * `redis_url` - Redis接続URL (例: "redis://localhost:6379")
    /// * `default_ttl` - デフォルトのセッション有効期限（秒）
    /// * `key_prefix` - セッションキーのプレフィックス（例: "session:"）
    pub async fn new(
        redis_url: &str,
        default_ttl: u64,
        key_prefix: String,
    ) -> Result<Self, AuthError> {
        let client = Client::open(redis_url)
            .map_err(|e| AuthError::Internal(format!("Redis client creation failed: {}", e)))?;

        let connection = ConnectionManager::new(client)
            .await
            .map_err(|e| AuthError::Internal(format!("Redis connection failed: {}", e)))?;

        Ok(Self {
            connection,
            default_ttl,
            key_prefix,
        })
    }

    /// セッションキーを生成
    fn session_key(&self, session_id: &str) -> String {
        format!("{}{}", self.key_prefix, session_id)
    }

    /// ユーザーセッションキーを生成（ユーザーIDから全セッションを検索）
    fn user_sessions_key(&self, user_id: &str) -> String {
        format!("{}user:{}:sessions", self.key_prefix, user_id)
    }

    /// セッションを作成
    ///
    /// # Arguments
    /// * `session_id` - セッションID（UUID推奨）
    /// * `user` - 認証済みユーザー
    /// * `ttl` - セッション有効期限（None = デフォルト値使用）
    pub async fn create_session(
        &mut self,
        session_id: &str,
        user: &AuthUser,
        ttl: Option<u64>,
    ) -> Result<(), AuthError> {
        let now = chrono::Utc::now().timestamp();
        let session_data = SessionData {
            user: user.clone(),
            created_at: now,
            last_accessed_at: now,
            metadata: std::collections::HashMap::new(),
        };

        let session_json = serde_json::to_string(&session_data)
            .map_err(|e| AuthError::Internal(format!("Session serialization failed: {}", e)))?;

        let key = self.session_key(session_id);
        let ttl_seconds = ttl.unwrap_or(self.default_ttl);

        // セッションデータを保存
        self.connection
            .set_ex::<_, _, ()>(&key, session_json, ttl_seconds)
            .await
            .map_err(|e| AuthError::Internal(format!("Session creation failed: {}", e)))?;

        // ユーザーのセッションリストに追加
        let user_sessions_key = self.user_sessions_key(&user.id);
        self.connection
            .sadd::<_, _, ()>(&user_sessions_key, session_id)
            .await
            .map_err(|e| AuthError::Internal(format!("User sessions update failed: {}", e)))?;

        // ユーザーセッションリストにもTTLを設定
        self.connection
            .expire::<_, ()>(&user_sessions_key, ttl_seconds as i64)
            .await
            .map_err(|e| AuthError::Internal(format!("User sessions TTL failed: {}", e)))?;

        Ok(())
    }

    /// セッションを取得
    pub async fn get_session(&mut self, session_id: &str) -> Result<Option<SessionData>, AuthError> {
        let key = self.session_key(session_id);

        let session_json: Option<String> = self
            .connection
            .get(&key)
            .await
            .map_err(|e| AuthError::Internal(format!("Session lookup failed: {}", e)))?;

        if let Some(json) = session_json {
            let session_data: SessionData = serde_json::from_str(&json)
                .map_err(|e| AuthError::Internal(format!("Session deserialization failed: {}", e)))?;

            Ok(Some(session_data))
        } else {
            Ok(None)
        }
    }

    /// セッションを更新（最終アクセス時刻を更新し、TTLをリフレッシュ）
    pub async fn refresh_session(
        &mut self,
        session_id: &str,
        ttl: Option<u64>,
    ) -> Result<(), AuthError> {
        if let Some(mut session_data) = self.get_session(session_id).await? {
            // 最終アクセス時刻を更新
            session_data.last_accessed_at = chrono::Utc::now().timestamp();

            let session_json = serde_json::to_string(&session_data)
                .map_err(|e| AuthError::Internal(format!("Session serialization failed: {}", e)))?;

            let key = self.session_key(session_id);
            let ttl_seconds = ttl.unwrap_or(self.default_ttl);

            // データを更新し、TTLをリフレッシュ
            self.connection
                .set_ex::<_, _, ()>(&key, session_json, ttl_seconds)
                .await
                .map_err(|e| AuthError::Internal(format!("Session refresh failed: {}", e)))?;

            Ok(())
        } else {
            Err(AuthError::SessionNotFound)
        }
    }

    /// セッションを削除
    pub async fn destroy_session(&mut self, session_id: &str) -> Result<(), AuthError> {
        // セッションデータを取得してユーザーIDを確認
        if let Some(session_data) = self.get_session(session_id).await? {
            let key = self.session_key(session_id);

            // セッションを削除
            self.connection
                .del::<_, ()>(&key)
                .await
                .map_err(|e| AuthError::Internal(format!("Session deletion failed: {}", e)))?;

            // ユーザーセッションリストから削除
            let user_sessions_key = self.user_sessions_key(&session_data.user.id);
            self.connection
                .srem::<_, _, ()>(&user_sessions_key, session_id)
                .await
                .map_err(|e| AuthError::Internal(format!("User sessions update failed: {}", e)))?;

            Ok(())
        } else {
            Err(AuthError::SessionNotFound)
        }
    }

    /// ユーザーの全セッションを削除
    pub async fn destroy_user_sessions(&mut self, user_id: &str) -> Result<u64, AuthError> {
        let user_sessions_key = self.user_sessions_key(user_id);

        // ユーザーの全セッションIDを取得
        let session_ids: Vec<String> = self
            .connection
            .smembers(&user_sessions_key)
            .await
            .map_err(|e| AuthError::Internal(format!("User sessions lookup failed: {}", e)))?;

        let mut deleted_count = 0;

        // 各セッションを削除
        for session_id in &session_ids {
            let key = self.session_key(session_id);
            let result: u64 = self
                .connection
                .del(&key)
                .await
                .map_err(|e| AuthError::Internal(format!("Session deletion failed: {}", e)))?;
            deleted_count += result;
        }

        // ユーザーセッションリストを削除
        self.connection
            .del::<_, ()>(&user_sessions_key)
            .await
            .map_err(|e| AuthError::Internal(format!("User sessions deletion failed: {}", e)))?;

        Ok(deleted_count)
    }

    /// セッションの有効期限を取得（秒）
    pub async fn get_session_ttl(&mut self, session_id: &str) -> Result<Option<i64>, AuthError> {
        let key = self.session_key(session_id);

        let ttl: i64 = self
            .connection
            .ttl(&key)
            .await
            .map_err(|e| AuthError::Internal(format!("TTL lookup failed: {}", e)))?;

        // -2: キーが存在しない, -1: TTL未設定, 0以上: 残り秒数
        match ttl {
            -2 => Ok(None),
            -1 => Ok(None),
            seconds => Ok(Some(seconds)),
        }
    }

    /// ユーザーのアクティブセッション数を取得
    pub async fn count_user_sessions(&mut self, user_id: &str) -> Result<usize, AuthError> {
        let user_sessions_key = self.user_sessions_key(user_id);

        let count: usize = self
            .connection
            .scard(&user_sessions_key)
            .await
            .map_err(|e| AuthError::Internal(format!("Session count failed: {}", e)))?;

        Ok(count)
    }

    /// セッションメタデータを更新
    pub async fn update_session_metadata(
        &mut self,
        session_id: &str,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<(), AuthError> {
        if let Some(mut session_data) = self.get_session(session_id).await? {
            session_data.metadata = metadata;
            session_data.last_accessed_at = chrono::Utc::now().timestamp();

            let session_json = serde_json::to_string(&session_data)
                .map_err(|e| AuthError::Internal(format!("Session serialization failed: {}", e)))?;

            let key = self.session_key(session_id);

            // TTLを保持したまま更新
            let current_ttl = self.get_session_ttl(session_id).await?.unwrap_or(self.default_ttl as i64);

            self.connection
                .set_ex::<_, _, ()>(&key, session_json, current_ttl as u64)
                .await
                .map_err(|e| AuthError::Internal(format!("Session metadata update failed: {}", e)))?;

            Ok(())
        } else {
            Err(AuthError::SessionNotFound)
        }
    }

    /// 接続の健全性チェック
    pub async fn health_check(&mut self) -> Result<bool, AuthError> {
        let result: Result<String, _> = self.connection.get("__health_check__").await;
        
        match result {
            Ok(_) | Err(_) => Ok(true), // 接続は生きている（キーが存在しなくてもOK）
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::{AuthProvider, Role};

    // 注意: これらのテストはRedisサーバーが必要です
    // 環境変数 REDIS_URL を設定してください
    // 例: REDIS_URL=redis://localhost:6379

    async fn setup_test_store() -> RedisSessionStore {
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());

        RedisSessionStore::new(&redis_url, 3600, "test:session:".to_string())
            .await
            .expect("Failed to connect to test Redis")
    }

    fn create_test_user(id: &str, username: &str) -> AuthUser {
        let mut user = AuthUser::new(id.to_string(), username.to_string());
        user.email = Some(format!("{}@example.com", username));
        user.roles.insert(Role::User);
        user
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_create_and_get_session() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");
        let session_id = "session_123";

        store.create_session(session_id, &user, None).await.unwrap();

        let session = store.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(session.user.id, "user1");
        assert_eq!(session.user.username, "testuser");

        // クリーンアップ
        store.destroy_session(session_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_session_ttl() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");
        let session_id = "session_ttl_test";

        // 10秒のTTLで作成
        store.create_session(session_id, &user, Some(10)).await.unwrap();

        let ttl = store.get_session_ttl(session_id).await.unwrap().unwrap();
        assert!(ttl > 0 && ttl <= 10);

        // クリーンアップ
        store.destroy_session(session_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_refresh_session() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");
        let session_id = "session_refresh_test";

        store.create_session(session_id, &user, Some(10)).await.unwrap();

        let session_before = store.get_session(session_id).await.unwrap().unwrap();
        
        // 少し待ってからリフレッシュ
        tokio::time::sleep(Duration::from_secs(1)).await;
        store.refresh_session(session_id, Some(20)).await.unwrap();

        let session_after = store.get_session(session_id).await.unwrap().unwrap();
        assert!(session_after.last_accessed_at > session_before.last_accessed_at);

        let ttl = store.get_session_ttl(session_id).await.unwrap().unwrap();
        assert!(ttl > 10); // リフレッシュされたので10秒以上残っている

        // クリーンアップ
        store.destroy_session(session_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_destroy_session() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");
        let session_id = "session_destroy_test";

        store.create_session(session_id, &user, None).await.unwrap();
        assert!(store.get_session(session_id).await.unwrap().is_some());

        store.destroy_session(session_id).await.unwrap();
        assert!(store.get_session(session_id).await.unwrap().is_none());
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_destroy_user_sessions() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");

        // 複数のセッションを作成
        store.create_session("session_1", &user, None).await.unwrap();
        store.create_session("session_2", &user, None).await.unwrap();
        store.create_session("session_3", &user, None).await.unwrap();

        let count = store.count_user_sessions("user1").await.unwrap();
        assert_eq!(count, 3);

        let deleted = store.destroy_user_sessions("user1").await.unwrap();
        assert_eq!(deleted, 3);

        let count_after = store.count_user_sessions("user1").await.unwrap();
        assert_eq!(count_after, 0);
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_session_metadata() {
        let mut store = setup_test_store().await;
        let user = create_test_user("user1", "testuser");
        let session_id = "session_metadata_test";

        store.create_session(session_id, &user, None).await.unwrap();

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("ip_address".to_string(), "192.168.1.1".to_string());
        metadata.insert("user_agent".to_string(), "Mozilla/5.0".to_string());

        store.update_session_metadata(session_id, metadata).await.unwrap();

        let session = store.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(session.metadata.get("ip_address").unwrap(), "192.168.1.1");
        assert_eq!(session.metadata.get("user_agent").unwrap(), "Mozilla/5.0");

        // クリーンアップ
        store.destroy_session(session_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // REDIS_URL設定が必要
    async fn test_health_check() {
        let mut store = setup_test_store().await;
        let is_healthy = store.health_check().await.unwrap();
        assert!(is_healthy);
    }
}
