use super::UserRepository;
use crate::security::auth::types::{AuthError, AuthUser, PasswordHasher};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memoryユーザーリポジトリ
///
/// HashMapを使用したシンプルなユーザーストレージ実装。
/// 開発・テスト用途のみで使用してください。
///
/// # 制限事項
/// - サーバー再起動時にデータが失われます
/// - スケールアウトできません (単一インスタンスのみ)
/// - 本番環境では使用しないでください
#[derive(Clone)]
pub struct InMemoryUserRepository {
    users: Arc<RwLock<HashMap<String, StoredUser>>>,
    password_hasher: PasswordHasher,
}

#[derive(Debug, Clone)]
struct StoredUser {
    user: AuthUser,
    password_hash: Option<String>,
}

impl InMemoryUserRepository {
    /// 新しいIn-memoryリポジトリを作成
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            password_hasher: PasswordHasher::new(12),
        }
    }
}

impl Default for InMemoryUserRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn create_user(
        &self,
        user: &AuthUser,
        password_hash: Option<String>,
    ) -> Result<(), AuthError> {
        let mut users = self.users.write().await;

        // メールアドレスの重複チェック
        if let Some(ref email) = user.email {
            if users.values().any(|u| u.user.email.as_ref() == Some(email)) {
                return Err(AuthError::UserAlreadyExists(email.clone()));
            }
        }

        users.insert(
            user.id.clone(),
            StoredUser {
                user: user.clone(),
                password_hash,
            },
        );

        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<AuthUser>, AuthError> {
        let users = self.users.read().await;
        Ok(users.get(id).map(|stored| stored.user.clone()))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<AuthUser>, AuthError> {
        let users = self.users.read().await;
        Ok(users
            .values()
            .find(|stored| stored.user.email.as_deref() == Some(email))
            .map(|stored| stored.user.clone()))
    }

    async fn update_user(&self, user: &AuthUser) -> Result<(), AuthError> {
        let mut users = self.users.write().await;

        if let Some(stored) = users.get_mut(&user.id) {
            stored.user = user.clone();
            Ok(())
        } else {
            Err(AuthError::UserNotFound(user.id.clone()))
        }
    }

    async fn delete_user(&self, id: &str) -> Result<(), AuthError> {
        let mut users = self.users.write().await;

        if users.remove(id).is_some() {
            Ok(())
        } else {
            Err(AuthError::UserNotFound(id.to_string()))
        }
    }

    async fn verify_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<Option<AuthUser>, AuthError> {
        let users = self.users.read().await;

        if let Some(stored) = users
            .values()
            .find(|u| u.user.email.as_deref() == Some(email))
        {
            if let Some(hash) = &stored.password_hash {
                if self.password_hasher.verify(password, hash)? {
                    Ok(Some(stored.user.clone()))
                } else {
                    Ok(None)
                }
            } else {
                // OAuth2ユーザーなど、パスワードを持たないユーザー
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn get_password_hash(&self, user_id: &str) -> Result<Option<String>, AuthError> {
        let users = self.users.read().await;
        Ok(users
            .get(user_id)
            .and_then(|stored| stored.password_hash.clone()))
    }

    async fn update_password_hash(
        &self,
        user_id: &str,
        password_hash: String,
    ) -> Result<(), AuthError> {
        let mut users = self.users.write().await;

        if let Some(stored) = users.get_mut(user_id) {
            stored.password_hash = Some(password_hash);
            Ok(())
        } else {
            Err(AuthError::UserNotFound(user_id.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::Role;

    fn create_test_user(id: &str, username: &str, email: &str) -> AuthUser {
        let mut user = AuthUser::new(id.to_string(), username.to_string());
        user.email = Some(email.to_string());
        user.roles.insert(Role::User);
        user
    }

    #[tokio::test]
    async fn test_create_and_find_user() {
        let repo = InMemoryUserRepository::new();
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
    async fn test_duplicate_email() {
        let repo = InMemoryUserRepository::new();
        let user1 = create_test_user("user1", "testuser1", "test@example.com");
        let user2 = create_test_user("user2", "testuser2", "test@example.com");

        repo.create_user(&user1, None).await.unwrap();
        let result = repo.create_user(&user2, None).await;

        assert!(matches!(result, Err(AuthError::UserAlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_update_user() {
        let repo = InMemoryUserRepository::new();
        let mut user = create_test_user("user1", "testuser", "test@example.com");

        repo.create_user(&user, None).await.unwrap();

        user.username = "Updated Name".to_string();
        repo.update_user(&user).await.unwrap();

        let found = repo.find_by_id("user1").await.unwrap().unwrap();
        assert_eq!(found.username, "Updated Name");
    }

    #[tokio::test]
    async fn test_delete_user() {
        let repo = InMemoryUserRepository::new();
        let user = create_test_user("user1", "testuser", "test@example.com");

        repo.create_user(&user, None).await.unwrap();
        repo.delete_user("user1").await.unwrap();

        let found = repo.find_by_id("user1").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_verify_password() {
        let repo = InMemoryUserRepository::new();
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

    #[tokio::test]
    async fn test_update_password_hash() {
        let repo = InMemoryUserRepository::new();
        let user = create_test_user("user1", "testuser", "test@example.com");
        let old_hash = "old_hash";
        let new_hash = "new_hash";

        repo.create_user(&user, Some(old_hash.to_string()))
            .await
            .unwrap();
        repo.update_password_hash("user1", new_hash.to_string())
            .await
            .unwrap();

        let hash = repo.get_password_hash("user1").await.unwrap().unwrap();
        assert_eq!(hash, new_hash);
    }
}
