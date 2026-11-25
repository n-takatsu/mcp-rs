//! PostgreSQL Database Engine
//!
//! Provides PostgreSQL-specific implementations for database operations
//! including prepared statements, transactions, and JSON support.

pub mod connection;
pub mod json_support;
pub mod pg_connection;
pub mod prepared;
pub mod transaction;

pub use connection::{PostgreSqlConfig, PostgreSqlPool};
pub use json_support::PostgreSqlJsonSupport;
pub use pg_connection::PostgreSqlConnection;
pub use prepared::PostgreSqlPreparedStatement;
pub use transaction::PostgreSqlTransaction;

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseType, HealthStatus},
};
use async_trait::async_trait;
use std::sync::Arc;

// 接続の設定のキー。MySQL（#527）と共有するので、`engines`に置いた
pub use super::{OPTION_BYTE_LIMIT, OPTION_FETCH_LIMIT, OPTION_SSL_ROOT_CERT};

/// 接続先（ホスト・ポート・データベース・ユーザー。PR #767のレビュー）。
/// エンジンを作った設定と違う接続先で`connect`されたら断るために比べる
/// （パスワードは持たない）
#[derive(Clone, Debug, PartialEq, Eq)]
struct PoolTarget {
    host: String,
    port: u16,
    database: String,
    username: String,
}

impl PoolTarget {
    fn of(config: &DatabaseConfig) -> Self {
        Self {
            host: config.connection.host.clone(),
            port: config.connection.port,
            database: config.connection.database.clone(),
            username: config.connection.username.clone(),
        }
    }
}

impl std::fmt::Display for PoolTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{}:{}/{}",
            self.username, self.host, self.port, self.database
        )
    }
}

/// TLSの設定（`ssl_mode`・`ssl_root_cert`、#774）。接続先が同じでも、
/// エンジンを作った設定とTLSの設定が違う`connect`は断るために比べる（共有の
/// プールは、エンジンの設定のTLSでつなぐため）
#[derive(Clone, Debug, PartialEq, Eq)]
struct PoolTls {
    ssl_mode: Option<String>,
    ssl_root_cert: Option<String>,
}

impl PoolTls {
    fn of(config: &DatabaseConfig) -> Self {
        Self {
            ssl_mode: config.connection.ssl_mode.clone(),
            ssl_root_cert: config.connection.options.get(OPTION_SSL_ROOT_CERT).cloned(),
        }
    }
}

/// PostgreSQL Database Engine
///
/// Main entry point for PostgreSQL database operations
#[derive(Clone)]
pub struct PostgreSqlEngine {
    config: DatabaseConfig,
    /// 接続のプール。最初の`connect`（か`health_check`・`get_version`）で、
    /// エンジンの設定の接続先に作り、以後の接続で共有する（#453）。以前は
    /// `connect`のたびに新しいプールを作っていた
    pool: Arc<tokio::sync::OnceCell<sqlx::PgPool>>,
}

impl PostgreSqlEngine {
    /// Create a new PostgreSQL engine
    pub async fn new(
        config: crate::handlers::database::types::DatabaseConfig,
    ) -> Result<Self, crate::handlers::database::types::DatabaseError> {
        Ok(Self {
            config,
            pool: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// 共有のプール。まだ無ければ、エンジンの設定で作る（つなぐ）
    async fn shared_pool(&self) -> Result<sqlx::PgPool, DatabaseError> {
        self.pool_for(&self.config).await
    }

    /// `config`の接続先の、共有のプール。まだ無ければ`config`で作る。
    /// エンジンを作った設定と違う接続先なら断る（PR #767のレビュー）。
    /// プールは1つなので、断らないと、Bへの操作がAのプールで動き、接続の
    /// 情報にはBが出る。また、プールはいつもエンジンの設定の接続先なので、
    /// `health_check`・`get_version`が確かめる接続先とも合う
    async fn pool_for(&self, config: &DatabaseConfig) -> Result<sqlx::PgPool, DatabaseError> {
        let (engine_target, target) = (PoolTarget::of(&self.config), PoolTarget::of(config));
        if target != engine_target {
            return Err(DatabaseError::ConfigurationError(format!(
                "this PostgreSQL engine was created for {engine_target}; create another engine to connect to {target}"
            )));
        }
        // TLSの設定が違えば断る（#774。以前は、弱いTLSの設定で作ったプールを
        // `verify-full`の`connect`にも返した）
        if PoolTls::of(config) != PoolTls::of(&self.config) {
            return Err(DatabaseError::ConfigurationError(format!(
                "this PostgreSQL engine was created with other TLS settings (ssl_mode, ssl_root_cert) for {engine_target}; create another engine to connect with these TLS settings"
            )));
        }
        Ok(self
            .pool
            .get_or_try_init(|| pg_connection::connect_pool(config))
            .await?
            .clone())
    }

    /// `health_check`・`get_version`の待ち時間の上限（`query_timeout`秒。
    /// 0なら1秒）
    fn check_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.config.features.query_timeout).max(1))
    }

    /// プールを用意し（まだ無ければつなぎ）、`run`を、期限の中で実行する
    async fn with_pool<T, F, Fut>(&self, run: F) -> Result<T, DatabaseError>
    where
        F: FnOnce(sqlx::PgPool) -> Fut,
        Fut: std::future::Future<Output = Result<T, DatabaseError>>,
    {
        let timeout = self.check_timeout();
        tokio::time::timeout(timeout, async { run(self.shared_pool().await?).await })
            .await
            .map_err(|_| {
                DatabaseError::TimeoutError(format!(
                    "PostgreSQL did not respond within {} seconds",
                    timeout.as_secs()
                ))
            })?
    }
}

#[async_trait]
impl DatabaseEngine for PostgreSqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let pool = self.pool_for(config).await?;
        let limit = |key: &str| {
            config
                .connection
                .options
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
        };
        let conn = PostgreSqlConnection::with_pool(
            pool,
            config.connection.database.clone(),
            config.connection.username.clone(),
        )
        .with_statement_timeout(std::time::Duration::from_secs(u64::from(
            config.features.query_timeout,
        )))
        .with_fetch_limit(limit(OPTION_FETCH_LIMIT))
        .with_byte_limit(limit(OPTION_BYTE_LIMIT));
        Ok(Box::new(conn))
    }

    /// 接続先に`SELECT 1`を送り、答えれば`Healthy`（#758）。つなげない・
    /// 答えない・期限（`query_timeout`）を過ぎたときは`Critical`と、その
    /// 理由。まだつないでいなければ、エンジンの設定でつなぐ。以前は
    /// データベースに触れず、常に`Healthy`を返した
    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        use crate::handlers::database::types::HealthStatusType;
        use chrono::Utc;

        let started = std::time::Instant::now();
        let result = self
            .with_pool(|pool| async move {
                sqlx::query("SELECT 1")
                    .execute(&pool)
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                Ok(pool)
            })
            .await;
        let response_time_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let connection_count = self.pool.get().map_or(0, |pool| pool.size());
        let (status, error_message) = match result {
            Ok(_) => (HealthStatusType::Healthy, None),
            Err(e) => (HealthStatusType::Critical, Some(e.to_string())),
        };
        Ok(HealthStatus {
            status,
            last_check: Utc::now(),
            response_time_ms,
            error_message,
            connection_count,
            active_transactions: 0,
        })
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
            DatabaseFeature::JsonSupport,
            DatabaseFeature::Acid,
        ]
    }

    fn validate_config(&self, _config: &DatabaseConfig) -> Result<(), DatabaseError> {
        Ok(())
    }

    /// 接続先の版（`SHOW server_version`）を`PostgreSQL 16.4`の形で返す
    /// （#758）。つなげない・読めないときはエラー。以前は接続先によらず
    /// `PostgreSQL 15 (Mock)`を返した
    async fn get_version(&self) -> Result<String, DatabaseError> {
        let version: String = self
            .with_pool(|pool| async move {
                sqlx::query_scalar("SHOW server_version")
                    .fetch_one(&pool)
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
            })
            .await?;
        Ok(format!("PostgreSQL {}", version.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::HealthStatusType;

    /// 誰も待ち受けていない（つなげない）接続先の設定
    fn unreachable_config() -> DatabaseConfig {
        let mut config = DatabaseConfig {
            database_type: DatabaseType::PostgreSQL,
            ..Default::default()
        };
        config.connection.host = "127.0.0.1".to_string();
        // 予約されたポート（待ち受けているものは無い）
        config.connection.port = 1;
        config.connection.timeout_seconds = 1;
        config.features.query_timeout = 2;
        config
    }

    /// つなげないときは`Healthy`でなく`Critical`と理由を返す（#758）
    #[tokio::test]
    async fn test_health_check_reports_an_unreachable_server() {
        let engine = PostgreSqlEngine::new(unreachable_config()).await.unwrap();
        let health = engine.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatusType::Critical);
        assert!(health.error_message.is_some(), "{health:?}");
        assert_eq!(health.connection_count, 0);
    }

    /// エンジンを作った設定と違う接続先で`connect`されたら断る。同じ接続先
    /// なら共有のプールを使う（PR #767のレビュー）
    #[tokio::test]
    async fn test_connect_refuses_another_target_than_the_engine() {
        let config = unreachable_config();
        let engine = PostgreSqlEngine::new(config.clone()).await.unwrap();
        // つながずにプールを用意する（`connect_lazy`は接続しない）
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy_with(sqlx::postgres::PgConnectOptions::new());
        engine
            .pool
            .set(pool)
            .unwrap_or_else(|_| panic!("the pool was already set"));

        assert!(engine.connect(&config).await.is_ok());

        let mut other = config.clone();
        other.connection.database = "other".to_string();
        let result = engine.connect(&other).await;
        assert!(
            matches!(result, Err(DatabaseError::ConfigurationError(ref m)) if m.contains("/other")),
            "{:?}",
            result.err()
        );
    }

    /// 接続先が同じでも、TLSの設定（`ssl_mode`・`ssl_root_cert`）が
    /// エンジンと違う`connect`は断る（#774）。つなぐ前に断るので、プールも
    /// 作らない
    #[tokio::test]
    async fn test_connect_refuses_other_tls_settings_than_the_engine() {
        let config = unreachable_config();
        let engine = PostgreSqlEngine::new(config.clone()).await.unwrap();

        let mut verify_full = config.clone();
        verify_full.connection.ssl_mode = Some("verify-full".to_string());
        let mut with_root_cert = config.clone();
        with_root_cert
            .connection
            .options
            .insert(OPTION_SSL_ROOT_CERT.to_string(), "/etc/ca.pem".to_string());
        for other in [verify_full, with_root_cert] {
            let result = engine.connect(&other).await;
            assert!(
                matches!(result, Err(DatabaseError::ConfigurationError(ref m)) if m.contains("TLS")),
                "{:?}",
                result.err()
            );
        }
        assert!(engine.pool.get().is_none());
    }

    /// エンジンを作った設定（A）と違う接続先（B）で最初に`connect`されても、
    /// プールをBに作らないので、`health_check`・`get_version`はAを確かめる
    /// （PR #767の2回目のレビュー。以前はBのプールができ、Aとの比べで
    /// `ConfigurationError`になった）
    #[tokio::test]
    async fn test_first_connect_to_another_target_does_not_break_health_check() {
        let config = unreachable_config();
        let engine = PostgreSqlEngine::new(config.clone()).await.unwrap();
        let mut other = config.clone();
        other.connection.database = "other".to_string();

        let result = engine.connect(&other).await;
        assert!(
            matches!(result, Err(DatabaseError::ConfigurationError(_))),
            "{:?}",
            result.err()
        );
        assert!(engine.pool.get().is_none());

        // Aはつなげない設定なので、つなげない理由で`Critical`・エラーになる
        // （接続先の比べのエラーではない）
        let health = engine.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatusType::Critical);
        let message = health.error_message.unwrap_or_default();
        assert!(!message.contains("create another engine"), "{message}");
        let version = engine.get_version().await;
        assert!(
            !matches!(version, Err(DatabaseError::ConfigurationError(_))),
            "{version:?}"
        );
    }

    /// つなげないときは、決まった版でなくエラーを返す（#758）
    #[tokio::test]
    async fn test_get_version_fails_without_a_server() {
        let engine = PostgreSqlEngine::new(unreachable_config()).await.unwrap();
        let result = engine.get_version().await;
        assert!(result.is_err(), "{result:?}");
    }
}
