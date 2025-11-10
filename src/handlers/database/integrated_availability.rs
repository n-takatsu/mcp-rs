//! Integrated Database Availability Management
//!
//! データベース可用性の統合管理システム

use super::{
    availability::{AvailabilityManager, AvailabilityConfig},
    retry::{ExecutionStrategy, RetryStrategy, TimeoutStrategy},
    loadbalancer::{LoadBalancer, ReadWriteSplitter, LoadBalancingStrategy, ReadPreference, DatabaseEndpoint, DatabaseRole},
    safety::SafetyManager,
    types::{DatabaseConfig, DatabaseError, QueryResult, ExecuteResult, QueryType},
};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// 包括的なデータベース可用性システム
pub struct IntegratedAvailabilitySystem {
    /// 可用性管理
    availability_manager: Arc<AvailabilityManager>,
    /// 負荷分散
    load_balancer: Arc<LoadBalancer>,
    /// 読み書き分離
    read_write_splitter: Arc<ReadWriteSplitter>,
    /// 実行戦略（リトライ・タイムアウト）
    execution_strategy: ExecutionStrategy,
    /// 安全性管理
    safety_manager: Arc<SafetyManager>,
    /// 設定
    config: IntegratedConfig,
}

impl IntegratedAvailabilitySystem {
    /// 新しい統合可用性システムを作成
    pub async fn new(config: IntegratedConfig) -> Result<Self, DatabaseError> {
        // 各コンポーネントを初期化
        let availability_manager = Arc::new(
            AvailabilityManager::new(config.availability.clone()).await
        );
        
        let load_balancer = Arc::new(
            LoadBalancer::new(config.load_balancing_strategy.clone())
        );
        
        let read_write_splitter = Arc::new(
            ReadWriteSplitter::new(
                Arc::clone(&load_balancer),
                config.read_preference.clone()
            )
        );

        let safety_manager = Arc::new(
            SafetyManager::new(config.safety.clone()).await
        );

        // エンドポイントを負荷分散器に追加
        for endpoint in &config.database_endpoints {
            load_balancer.add_endpoint(endpoint.clone()).await;
        }

        let system = Self {
            availability_manager,
            load_balancer,
            read_write_splitter,
            execution_strategy: config.execution_strategy.clone(),
            safety_manager,
            config,
        };

        // 監視開始
        system.start_monitoring().await;

        Ok(system)
    }

    /// 監視システムを開始
    async fn start_monitoring(&self) {
        // 可用性監視を開始
        self.availability_manager.start_monitoring().await;
        
        info!("Integrated availability system monitoring started");
    }

    /// 安全で高可用性なクエリ実行
    pub async fn execute_query(&self, sql: &str, params: &[super::types::Value], query_type: QueryType) -> Result<QueryResult, DatabaseError> {
        // 1. 安全性チェック
        self.safety_manager.safe_execute(|| async {
            // 2. 適切なエンドポイント選択
            let endpoint = self.read_write_splitter
                .select_endpoint_for_query(query_type)
                .await?;

            // 3. リトライとタイムアウト付きで実行
            self.execution_strategy.execute_query(|| async {
                self.execute_on_endpoint(&endpoint, sql, params).await
            }).await
        }).await
    }

    /// 安全で高可用性なコマンド実行
    pub async fn execute_command(&self, sql: &str, params: &[super::types::Value]) -> Result<ExecuteResult, DatabaseError> {
        // 1. 安全性チェック
        self.safety_manager.safe_execute(|| async {
            // 2. 書き込みエンドポイント選択
            let endpoint = self.load_balancer.select_write_endpoint().await?;

            // 3. リトライとタイムアウト付きで実行
            self.execution_strategy.execute_command(|| async {
                self.execute_command_on_endpoint(&endpoint, sql, params).await
            }).await
        }).await
    }

    /// 特定のエンドポイントでクエリを実行
    async fn execute_on_endpoint(&self, endpoint: &DatabaseEndpoint, sql: &str, params: &[super::types::Value]) -> Result<QueryResult, DatabaseError> {
        // 接続記録
        endpoint.record_connection();
        
        let start_time = std::time::Instant::now();
        
        // 実際のクエリ実行（ここではプレースホルダー）
        let result = self.mock_query_execution(sql, params).await;
        
        let execution_time = start_time.elapsed();
        endpoint.update_response_time(execution_time.as_millis() as u64);
        endpoint.record_disconnection();
        
        result
    }

    /// 特定のエンドポイントでコマンドを実行
    async fn execute_command_on_endpoint(&self, endpoint: &DatabaseEndpoint, sql: &str, params: &[super::types::Value]) -> Result<ExecuteResult, DatabaseError> {
        // 接続記録
        endpoint.record_connection();
        
        let start_time = std::time::Instant::now();
        
        // 実際のコマンド実行（ここではプレースホルダー）
        let result = self.mock_command_execution(sql, params).await;
        
        let execution_time = start_time.elapsed();
        endpoint.update_response_time(execution_time.as_millis() as u64);
        endpoint.record_disconnection();
        
        result
    }

    // プレースホルダーメソッド（実際の実装では実際のDB操作）
    async fn mock_query_execution(&self, _sql: &str, _params: &[super::types::Value]) -> Result<QueryResult, DatabaseError> {
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            total_rows: Some(0),
            execution_time_ms: 10,
        })
    }

    async fn mock_command_execution(&self, _sql: &str, _params: &[super::types::Value]) -> Result<ExecuteResult, DatabaseError> {
        Ok(ExecuteResult {
            rows_affected: 1,
            last_insert_id: None,
            execution_time_ms: 10,
        })
    }

    /// システム統計情報を取得
    pub async fn get_system_stats(&self) -> SystemStats {
        let load_balancer_stats = self.load_balancer.get_statistics().await;
        
        SystemStats {
            total_endpoints: load_balancer_stats.total_endpoints,
            available_endpoints: load_balancer_stats.available_endpoints,
            total_connections: load_balancer_stats.total_connections,
            avg_response_time_ms: load_balancer_stats.avg_response_time_ms,
            avg_health_score: load_balancer_stats.avg_health_score,
            endpoints_by_role: load_balancer_stats.endpoints_by_role,
            safety_stats: self.safety_manager.get_stats().await,
        }
    }

    /// エンドポイントを動的に追加
    pub async fn add_endpoint(&self, endpoint: DatabaseEndpoint) -> Result<(), DatabaseError> {
        self.load_balancer.add_endpoint(endpoint).await;
        info!("Dynamically added new database endpoint");
        Ok(())
    }

    /// エンドポイントを動的に削除
    pub async fn remove_endpoint(&self, endpoint_id: &str) -> Result<(), DatabaseError> {
        self.load_balancer.remove_endpoint(endpoint_id).await;
        info!("Dynamically removed database endpoint: {}", endpoint_id);
        Ok(())
    }

    /// 手動フェイルオーバーを実行
    pub async fn manual_failover(&self, from_endpoint: &str, to_endpoint: &str) -> Result<(), DatabaseError> {
        info!("Executing manual failover from {} to {}", from_endpoint, to_endpoint);
        
        // 元のエンドポイントを無効化
        self.remove_endpoint(from_endpoint).await?;
        
        // フェイルオーバー処理のロジックをここに実装
        // 実際の実装では、接続の再配置、トランザクションの処理等が必要
        
        Ok(())
    }
}

/// 統合システム設定
#[derive(Debug, Clone)]
pub struct IntegratedConfig {
    /// 可用性設定
    pub availability: AvailabilityConfig,
    /// 安全性設定
    pub safety: SafetyConfig,
    /// 負荷分散戦略
    pub load_balancing_strategy: LoadBalancingStrategy,
    /// 読み取り設定
    pub read_preference: ReadPreference,
    /// 実行戦略
    pub execution_strategy: ExecutionStrategy,
    /// データベースエンドポイント
    pub database_endpoints: Vec<DatabaseEndpoint>,
}

impl Default for IntegratedConfig {
    fn default() -> Self {
        Self {
            availability: AvailabilityConfig::default(),
            safety: SafetyConfig::default(),
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            read_preference: ReadPreference::SecondaryPreferred,
            execution_strategy: ExecutionStrategy::default(),
            database_endpoints: vec![
                DatabaseEndpoint::new(
                    "primary".to_string(),
                    DatabaseConfig::default(),
                    DatabaseRole::Master,
                    1,
                ),
            ],
        }
    }
}

/// システム統計情報
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub total_endpoints: usize,
    pub available_endpoints: usize,
    pub total_connections: usize,
    pub avg_response_time_ms: usize,
    pub avg_health_score: usize,
    pub endpoints_by_role: HashMap<DatabaseRole, usize>,
    pub safety_stats: super::safety::SafetyStats,
}

/// 便利なビルダーパターン
pub struct AvailabilitySystemBuilder {
    config: IntegratedConfig,
}

impl AvailabilitySystemBuilder {
    pub fn new() -> Self {
        Self {
            config: IntegratedConfig::default(),
        }
    }

    pub fn with_load_balancing_strategy(mut self, strategy: LoadBalancingStrategy) -> Self {
        self.config.load_balancing_strategy = strategy;
        self
    }

    pub fn with_read_preference(mut self, preference: ReadPreference) -> Self {
        self.config.read_preference = preference;
        self
    }

    pub fn with_execution_strategy(mut self, strategy: ExecutionStrategy) -> Self {
        self.config.execution_strategy = strategy;
        self
    }

    pub fn add_database_endpoint(mut self, endpoint: DatabaseEndpoint) -> Self {
        self.config.database_endpoints.push(endpoint);
        self
    }

    pub async fn build(self) -> Result<IntegratedAvailabilitySystem, DatabaseError> {
        IntegratedAvailabilitySystem::new(self.config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integrated_system_creation() {
        let system = AvailabilitySystemBuilder::new()
            .with_load_balancing_strategy(LoadBalancingStrategy::LeastConnections)
            .with_read_preference(ReadPreference::SecondaryPreferred)
            .add_database_endpoint(DatabaseEndpoint::new(
                "replica1".to_string(),
                DatabaseConfig::default(),
                DatabaseRole::Slave,
                1,
            ))
            .build()
            .await;

        assert!(system.is_ok());
    }

    #[tokio::test]
    async fn test_query_execution_flow() {
        let system = AvailabilitySystemBuilder::new()
            .build()
            .await
            .unwrap();

        let result = system.execute_query(
            "SELECT * FROM users",
            &[],
            QueryType::Select
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_command_execution_flow() {
        let system = AvailabilitySystemBuilder::new()
            .build()
            .await
            .unwrap();

        let result = system.execute_command(
            "INSERT INTO users (name) VALUES (?)",
            &[super::types::Value::String("test".to_string())]
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dynamic_endpoint_management() {
        let system = AvailabilitySystemBuilder::new()
            .build()
            .await
            .unwrap();

        // エンドポイント追加
        let new_endpoint = DatabaseEndpoint::new(
            "dynamic1".to_string(),
            DatabaseConfig::default(),
            DatabaseRole::Slave,
            1,
        );

        assert!(system.add_endpoint(new_endpoint).await.is_ok());

        // エンドポイント削除
        assert!(system.remove_endpoint("dynamic1").await.is_ok());
    }
}