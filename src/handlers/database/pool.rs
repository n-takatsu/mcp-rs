//! Database Connection Pool Management
//!
//! データベース接続プールの管理と最適化

use super::engine::DatabaseEngine;
use super::types::{DatabaseConfig, DatabaseError, HealthStatus, HealthStatusType, PoolConfig};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Semaphore};

/// 接続プール情報
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub pool_id: String,
    pub total_connections: u32,
    pub active_connections: u32,
    pub idle_connections: u32,
    pub pending_requests: u32,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

/// プール化された接続
pub struct PooledConnection {
    connection: Box<dyn super::engine::DatabaseConnection>,
    created_at: DateTime<Utc>,
    last_used: DateTime<Utc>,
    usage_count: u64,
    pool: Arc<ConnectionPool>,
}

impl PooledConnection {
    /// 接続を使用としてマーク
    pub fn mark_used(&mut self) {
        self.last_used = Utc::now();
        self.usage_count += 1;
    }

    /// 接続の年齢を取得（秒）
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.created_at).num_seconds()
    }

    /// 最後の使用からの経過時間（秒）
    pub fn idle_seconds(&self) -> i64 {
        (Utc::now() - self.last_used).num_seconds()
    }

    /// 内部接続への参照を取得
    pub fn inner(&self) -> &dyn super::engine::DatabaseConnection {
        self.connection.as_ref()
    }

    /// 内部接続への可変参照を取得
    pub fn inner_mut(&mut self) -> &mut dyn super::engine::DatabaseConnection {
        self.connection.as_mut()
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        // 接続がDropされたときにプールに返却
        let pool = self.pool.clone();
        let connection = std::mem::replace(
            &mut self.connection,
            Box::new(DummyConnection) as Box<dyn super::engine::DatabaseConnection>,
        );

        tokio::spawn(async move {
            pool.return_connection(connection).await;
        });
    }
}

/// ダミー接続（Drop時の一時的な置き換え用）
struct DummyConnection;

#[async_trait]
impl super::engine::DatabaseConnection for DummyConnection {
    async fn query(
        &self,
        _sql: &str,
        _params: &[super::types::Value],
    ) -> Result<super::types::QueryResult, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn execute(
        &self,
        _sql: &str,
        _params: &[super::types::Value],
    ) -> Result<super::types::ExecuteResult, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn super::engine::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn get_schema(&self) -> Result<super::types::DatabaseSchema, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn get_table_schema(
        &self,
        _table_name: &str,
    ) -> Result<super::types::TableInfo, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn prepare(
        &self,
        _sql: &str,
    ) -> Result<Box<dyn super::engine::PreparedStatement>, DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        Err(DatabaseError::ConnectionFailed(
            "Dummy connection".to_string(),
        ))
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn connection_info(&self) -> super::engine::ConnectionInfo {
        super::engine::ConnectionInfo {
            connection_id: "dummy".to_string(),
            database_name: "dummy".to_string(),
            user_name: "dummy".to_string(),
            server_version: "0.0.0".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

/// 接続プール
#[derive(Clone)]
pub struct ConnectionPool {
    engine: Arc<dyn DatabaseEngine>,
    config: PoolConfig,
    connections: Arc<RwLock<VecDeque<Box<dyn super::engine::DatabaseConnection>>>>,
    semaphore: Arc<Semaphore>,
    active_count: Arc<RwLock<u32>>,
    pool_info: Arc<RwLock<PoolInfo>>,
}

impl ConnectionPool {
    /// 新しい接続プールを作成
    pub async fn new(
        engine: Arc<dyn DatabaseEngine>,
        db_config: &DatabaseConfig,
    ) -> Result<Arc<Self>, DatabaseError> {
        let pool_config = db_config.pool.clone();
        let pool_id = uuid::Uuid::new_v4().to_string();

        let pool = Arc::new(Self {
            engine,
            config: pool_config.clone(),
            connections: Arc::new(RwLock::new(VecDeque::new())),
            semaphore: Arc::new(Semaphore::new(pool_config.max_connections as usize)),
            active_count: Arc::new(RwLock::new(0)),
            pool_info: Arc::new(RwLock::new(PoolInfo {
                pool_id,
                total_connections: 0,
                active_connections: 0,
                idle_connections: 0,
                pending_requests: 0,
                created_at: Utc::now(),
                last_activity: Utc::now(),
            })),
        });

        // 最小接続数まで接続を作成
        pool.ensure_min_connections(db_config).await?;

        // バックグラウンドタスクを開始
        pool.start_maintenance_task(db_config.clone());

        Ok(pool)
    }

    /// 接続を取得
    pub async fn get_connection(
        &self,
        db_config: &DatabaseConfig,
    ) -> Result<PooledConnection, DatabaseError> {
        // セマフォで接続数を制限
        let _permit =
            self.semaphore.acquire().await.map_err(|e| {
                DatabaseError::PoolError(format!("Failed to acquire permit: {}", e))
            })?;

        // プール内の接続を試行
        if let Some(connection) = self.try_get_pooled_connection().await {
            return Ok(PooledConnection {
                connection,
                created_at: Utc::now(),
                last_used: Utc::now(),
                usage_count: 0,
                pool: Arc::new(ConnectionPool::clone_from_arc(&Arc::new(self.clone()))),
            });
        }

        // 新しい接続を作成
        let connection = self.create_new_connection(db_config).await?;

        // アクティブ接続数を増加
        {
            let mut active_count = self.active_count.write().await;
            *active_count += 1;
        }

        // プール情報を更新
        self.update_pool_info().await;

        Ok(PooledConnection {
            connection,
            created_at: Utc::now(),
            last_used: Utc::now(),
            usage_count: 0,
            pool: Arc::new(ConnectionPool::clone_from_arc(&Arc::new(self.clone()))),
        })
    }

    /// プールから接続を取得を試行
    async fn try_get_pooled_connection(
        &self,
    ) -> Option<Box<dyn super::engine::DatabaseConnection>> {
        let mut connections = self.connections.write().await;

        // 健全な接続を探す
        while let Some(connection) = connections.pop_front() {
            // 接続の健全性をチェック
            if connection.ping().await.is_ok() {
                return Some(connection);
            }
            // 不健全な接続は破棄
        }

        None
    }

    /// 新しい接続を作成
    async fn create_new_connection(
        &self,
        db_config: &DatabaseConfig,
    ) -> Result<Box<dyn super::engine::DatabaseConnection>, DatabaseError> {
        // タイムアウト付きで接続作成
        let timeout = Duration::from_secs(self.config.connection_timeout as u64);

        tokio::time::timeout(timeout, self.engine.connect(db_config))
            .await
            .map_err(|_| DatabaseError::TimeoutError("Connection creation timeout".to_string()))?
    }

    /// 接続をプールに返却
    pub async fn return_connection(&self, connection: Box<dyn super::engine::DatabaseConnection>) {
        let mut connections = self.connections.write().await;

        // プールの容量をチェック
        if connections.len() < self.config.max_connections as usize {
            connections.push_back(connection);
        }
        // 容量オーバーの場合は接続を破棄

        // アクティブ接続数を減少
        {
            let mut active_count = self.active_count.write().await;
            if *active_count > 0 {
                *active_count -= 1;
            }
        }

        self.update_pool_info().await;
    }

    /// 最小接続数を確保
    async fn ensure_min_connections(
        &self,
        db_config: &DatabaseConfig,
    ) -> Result<(), DatabaseError> {
        let mut connections = self.connections.write().await;

        while connections.len() < self.config.min_connections as usize {
            match self.create_new_connection(db_config).await {
                Ok(connection) => connections.push_back(connection),
                Err(e) => {
                    tracing::error!("Failed to create minimum connection: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// プール情報を更新
    async fn update_pool_info(&self) {
        let connections = self.connections.read().await;
        let active_count = self.active_count.read().await;

        let mut pool_info = self.pool_info.write().await;
        pool_info.total_connections = connections.len() as u32 + *active_count;
        pool_info.active_connections = *active_count;
        pool_info.idle_connections = connections.len() as u32;
        pool_info.last_activity = Utc::now();
    }

    /// プール情報を取得
    pub async fn get_pool_info(&self) -> PoolInfo {
        self.pool_info.read().await.clone()
    }

    /// プールの健全性チェック
    pub async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        let pool_info = self.get_pool_info().await;
        let engine_health = self.engine.health_check().await?;

        let status = if pool_info.total_connections == 0 {
            HealthStatusType::Critical
        } else if pool_info.idle_connections == 0
            && pool_info.active_connections >= self.config.max_connections
        {
            HealthStatusType::Warning
        } else {
            HealthStatusType::Healthy
        };

        Ok(HealthStatus {
            status,
            last_check: Utc::now(),
            response_time_ms: engine_health.response_time_ms,
            error_message: None,
            connection_count: pool_info.total_connections,
            active_transactions: 0, // TODO: トランザクション数を追跡
        })
    }

    /// メンテナンスタスクを開始
    fn start_maintenance_task(&self, db_config: DatabaseConfig) {
        let pool = Arc::new(ConnectionPool::clone_from_arc(&Arc::new(self.clone())));

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 1分間隔

            loop {
                interval.tick().await;

                // 古い接続を削除
                pool.cleanup_old_connections().await;

                // 最小接続数を確保
                if let Err(e) = pool.ensure_min_connections(&db_config).await {
                    tracing::error!("Failed to ensure minimum connections: {}", e);
                }
            }
        });
    }

    /// 古い接続をクリーンアップ
    async fn cleanup_old_connections(&self) {
        let mut connections = self.connections.write().await;
        let _max_lifetime = self.config.max_lifetime as i64;
        let _idle_timeout = self.config.idle_timeout as i64;

        connections.retain(|_connection| {
            // 接続情報を取得（実際にはタイムスタンプを保持する必要がある）
            // ここでは簡略化
            true // TODO: 実際の年齢とアイドル時間をチェック
        });
    }

    /// Arcからの複製用ヘルパー
    fn clone_from_arc(arc_self: &Arc<Self>) -> Self {
        Self {
            engine: arc_self.engine.clone(),
            config: arc_self.config.clone(),
            connections: arc_self.connections.clone(),
            semaphore: Arc::new(Semaphore::new(arc_self.config.max_connections as usize)),
            active_count: arc_self.active_count.clone(),
            pool_info: arc_self.pool_info.clone(),
        }
    }
}

/// プールマネージャー
/// 複数のプールを管理
pub struct PoolManager {
    pools: Arc<RwLock<std::collections::HashMap<String, Arc<ConnectionPool>>>>,
}

impl Default for PoolManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// プールを作成・登録
    pub async fn create_pool(
        &self,
        pool_id: String,
        engine: Arc<dyn DatabaseEngine>,
        config: &DatabaseConfig,
    ) -> Result<Arc<ConnectionPool>, DatabaseError> {
        let pool = ConnectionPool::new(engine, config).await?;

        let mut pools = self.pools.write().await;
        pools.insert(pool_id, pool.clone());

        Ok(pool)
    }

    /// プールを取得
    pub async fn get_pool(&self, pool_id: &str) -> Option<Arc<ConnectionPool>> {
        let pools = self.pools.read().await;
        pools.get(pool_id).cloned()
    }

    /// プールを削除
    pub async fn remove_pool(&self, pool_id: &str) -> bool {
        let mut pools = self.pools.write().await;
        pools.remove(pool_id).is_some()
    }

    /// 全プールの健全性チェック
    pub async fn health_check_all(&self) -> Vec<(String, HealthStatus)> {
        let pools = self.pools.read().await;
        let mut results = Vec::new();

        for (id, pool) in pools.iter() {
            match pool.health_check().await {
                Ok(status) => results.push((id.clone(), status)),
                Err(e) => {
                    results.push((
                        id.clone(),
                        HealthStatus {
                            status: HealthStatusType::Critical,
                            last_check: Utc::now(),
                            response_time_ms: 0,
                            error_message: Some(e.to_string()),
                            connection_count: 0,
                            active_transactions: 0,
                        },
                    ));
                }
            }
        }

        results
    }
}
