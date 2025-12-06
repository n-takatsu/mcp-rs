use crate::security::auth::types::{AuthUser, AuthError};
use async_trait::async_trait;

/// ユーザーリポジトリのトレイト
/// 
/// 様々なストレージバックエンド(PostgreSQL, MySQL, Redis, In-memory)に対応できるよう、
/// 抽象化されたインターフェースを提供します。
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// 新しいユーザーを作成
    /// 
    /// # Arguments
    /// * `user` - 作成するユーザー情報
    /// * `password_hash` - パスワードハッシュ (OAuth2ユーザーの場合はNone)
    /// 
    /// # Errors
    /// * メールアドレスが既に存在する場合
    /// * データベース接続エラー
    async fn create_user(&self, user: &AuthUser, password_hash: Option<String>) -> Result<(), AuthError>;

    /// IDでユーザーを検索
    /// 
    /// # Arguments
    /// * `id` - ユーザーID
    /// 
    /// # Returns
    /// ユーザーが見つかった場合は`Some(AuthUser)`、見つからない場合は`None`
    async fn find_by_id(&self, id: &str) -> Result<Option<AuthUser>, AuthError>;

    /// メールアドレスでユーザーを検索
    /// 
    /// # Arguments
    /// * `email` - メールアドレス
    /// 
    /// # Returns
    /// ユーザーが見つかった場合は`Some(AuthUser)`、見つからない場合は`None`
    async fn find_by_email(&self, email: &str) -> Result<Option<AuthUser>, AuthError>;

    /// ユーザー情報を更新
    /// 
    /// # Arguments
    /// * `user` - 更新するユーザー情報
    /// 
    /// # Errors
    /// * ユーザーが見つからない場合
    /// * データベース接続エラー
    async fn update_user(&self, user: &AuthUser) -> Result<(), AuthError>;

    /// ユーザーを削除
    /// 
    /// # Arguments
    /// * `id` - ユーザーID
    /// 
    /// # Errors
    /// * ユーザーが見つからない場合
    /// * データベース接続エラー
    async fn delete_user(&self, id: &str) -> Result<(), AuthError>;

    /// パスワードを検証してユーザーを取得
    /// 
    /// # Arguments
    /// * `email` - メールアドレス
    /// * `password` - 検証するパスワード (平文)
    /// 
    /// # Returns
    /// 認証成功時は`Some(AuthUser)`、失敗時は`None`
    /// 
    /// # Security
    /// パスワードハッシュはArgon2で検証されます
    async fn verify_password(&self, email: &str, password: &str) -> Result<Option<AuthUser>, AuthError>;

    /// パスワードハッシュを取得
    /// 
    /// # Arguments
    /// * `user_id` - ユーザーID
    /// 
    /// # Returns
    /// パスワードハッシュが存在する場合は`Some(String)`、存在しない場合は`None`
    /// (OAuth2ユーザーなど、パスワードを持たないユーザーの場合)
    async fn get_password_hash(&self, user_id: &str) -> Result<Option<String>, AuthError>;

    /// パスワードハッシュを更新
    /// 
    /// # Arguments
    /// * `user_id` - ユーザーID
    /// * `password_hash` - 新しいパスワードハッシュ
    /// 
    /// # Errors
    /// * ユーザーが見つからない場合
    /// * データベース接続エラー
    async fn update_password_hash(&self, user_id: &str, password_hash: String) -> Result<(), AuthError>;
}

/// In-memoryユーザーリポジトリ (開発・テスト用)
/// 
/// **警告**: 本番環境では使用しないでください。
/// サーバー再起動時に全データが失われます。
pub mod memory;

/// PostgreSQLユーザーリポジトリ (本番環境推奨)
#[cfg(feature = "postgresql-backend")]
pub mod postgres;

/// Redisセッションストア (高速セッション管理)
#[cfg(feature = "redis-backend")]
pub mod redis;

// TODO: 将来的に実装予定
// - MySQLユーザーリポジトリ
