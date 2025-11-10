//! Database Load Balancing and Read/Write Splitting
//!
//! データベースの負荷分散と読み書き分離機能

use super::engine::DatabaseConnection;
use super::types::{DatabaseConfig, DatabaseError, ExecuteResult, QueryResult, QueryType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 負荷分散戦略
#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    /// ラウンドロビン
    RoundRobin,
    /// 重み付きラウンドロビン
    WeightedRoundRobin(HashMap<String, u32>),
    /// 最小接続数
    LeastConnections,
    /// ランダム選択
    Random,
    /// レスポンス時間ベース
    ResponseTime,
    /// 健全性優先
    HealthBased,
}

/// データベースエンドポイント
#[derive(Debug, Clone)]
pub struct DatabaseEndpoint {
    pub id: String,
    pub config: DatabaseConfig,
    pub role: DatabaseRole,
    pub weight: u32,
    pub max_connections: u32,
    pub current_connections: Arc<AtomicUsize>,
    pub avg_response_time_ms: Arc<AtomicUsize>,
    pub health_score: Arc<AtomicUsize>, // 0-100
}

/// データベースの役割
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DatabaseRole {
    Master,      // 読み書き両方
    Slave,       // 読み取り専用
    ReadReplica, // 読み取り専用レプリカ
    WriteOnly,   // 書き込み専用（特殊用途）
}

impl DatabaseEndpoint {
    pub fn new(id: String, config: DatabaseConfig, role: DatabaseRole, weight: u32) -> Self {
        Self {
            id,
            config,
            role,
            weight,
            max_connections: 100,
            current_connections: Arc::new(AtomicUsize::new(0)),
            avg_response_time_ms: Arc::new(AtomicUsize::new(0)),
            health_score: Arc::new(AtomicUsize::new(100)),
        }
    }

    /// 現在の負荷レベル (0.0-1.0)
    pub fn load_level(&self) -> f64 {
        let current = self.current_connections.load(Ordering::Relaxed);
        current as f64 / self.max_connections as f64
    }

    /// エンドポイントが利用可能かどうか
    pub fn is_available(&self) -> bool {
        let health = self.health_score.load(Ordering::Relaxed);
        let load = self.load_level();

        health >= 50 && load < 0.95
    }

    /// 接続を記録
    pub fn record_connection(&self) {
        self.current_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// 接続解放を記録
    pub fn record_disconnection(&self) {
        self.current_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// レスポンス時間を更新
    pub fn update_response_time(&self, response_time_ms: u64) {
        let current = self.avg_response_time_ms.load(Ordering::Relaxed);
        // 簡単な移動平均 (α = 0.1)
        let new_avg = (current as f64 * 0.9 + response_time_ms as f64 * 0.1) as usize;
        self.avg_response_time_ms.store(new_avg, Ordering::Relaxed);
    }

    /// 健全性スコアを更新
    pub fn update_health_score(&self, score: u8) {
        self.health_score.store(score as usize, Ordering::Relaxed);
    }
}

/// 負荷分散マネージャー
pub struct LoadBalancer {
    strategy: LoadBalancingStrategy,
    endpoints: Arc<RwLock<Vec<DatabaseEndpoint>>>,
    round_robin_counter: AtomicUsize,
}

impl LoadBalancer {
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            strategy,
            endpoints: Arc::new(RwLock::new(Vec::new())),
            round_robin_counter: AtomicUsize::new(0),
        }
    }

    /// エンドポイントを追加
    pub async fn add_endpoint(&self, endpoint: DatabaseEndpoint) {
        let mut endpoints = self.endpoints.write().await;
        endpoints.push(endpoint);
        info!("Added database endpoint: {}", endpoints.last().unwrap().id);
    }

    /// エンドポイントを削除
    pub async fn remove_endpoint(&self, endpoint_id: &str) {
        let mut endpoints = self.endpoints.write().await;
        endpoints.retain(|e| e.id != endpoint_id);
        info!("Removed database endpoint: {}", endpoint_id);
    }

    /// 読み取り用エンドポイントを選択
    pub async fn select_read_endpoint(&self) -> Result<DatabaseEndpoint, DatabaseError> {
        let endpoints = self.endpoints.read().await;
        let read_endpoints: Vec<_> = endpoints
            .iter()
            .filter(|e| matches!(e.role, DatabaseRole::Slave | DatabaseRole::ReadReplica))
            .filter(|e| e.is_available())
            .collect();

        if read_endpoints.is_empty() {
            return Err(DatabaseError::NoAvailableEndpoints(
                "No read endpoints available".to_string(),
            ));
        }

        self.select_endpoint_by_strategy(&read_endpoints)
            .cloned()
            .ok_or_else(|| {
                DatabaseError::NoAvailableEndpoints("Failed to select read endpoint".to_string())
            })
    }

    /// 書き込み用エンドポイントを選択
    pub async fn select_write_endpoint(&self) -> Result<DatabaseEndpoint, DatabaseError> {
        let endpoints = self.endpoints.read().await;
        let write_endpoints: Vec<_> = endpoints
            .iter()
            .filter(|e| matches!(e.role, DatabaseRole::Master | DatabaseRole::WriteOnly))
            .filter(|e| e.is_available())
            .collect();

        if write_endpoints.is_empty() {
            return Err(DatabaseError::NoAvailableEndpoints(
                "No write endpoints available".to_string(),
            ));
        }

        self.select_endpoint_by_strategy(&write_endpoints)
            .cloned()
            .ok_or_else(|| {
                DatabaseError::NoAvailableEndpoints("Failed to select write endpoint".to_string())
            })
    }

    /// 戦略に基づいてエンドポイントを選択
    fn select_endpoint_by_strategy<'a>(
        &self,
        endpoints: &[&'a DatabaseEndpoint],
    ) -> Option<&'a DatabaseEndpoint> {
        if endpoints.is_empty() {
            return None;
        }

        match &self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let index =
                    self.round_robin_counter.fetch_add(1, Ordering::Relaxed) % endpoints.len();
                Some(endpoints[index])
            }
            LoadBalancingStrategy::WeightedRoundRobin(weights) => {
                // 重み付きラウンドロビン（簡略化版）
                let mut total_weight = 0;
                for endpoint in endpoints {
                    total_weight += weights.get(&endpoint.id).unwrap_or(&endpoint.weight);
                }

                let target = self.round_robin_counter.fetch_add(1, Ordering::Relaxed)
                    % total_weight as usize;
                let mut current_weight = 0;

                for endpoint in endpoints {
                    current_weight += weights.get(&endpoint.id).unwrap_or(&endpoint.weight);
                    if target < current_weight as usize {
                        return Some(endpoint);
                    }
                }
                Some(endpoints[0])
            }
            LoadBalancingStrategy::LeastConnections => endpoints
                .iter()
                .min_by_key(|e| e.current_connections.load(Ordering::Relaxed))
                .copied(),
            LoadBalancingStrategy::Random => {
                // シンプルなランダム実装（randクレートの代わり）
                let index = (std::ptr::addr_of!(endpoints) as usize) % endpoints.len();
                Some(endpoints[index])
            }
            LoadBalancingStrategy::ResponseTime => endpoints
                .iter()
                .min_by_key(|e| e.avg_response_time_ms.load(Ordering::Relaxed))
                .copied(),
            LoadBalancingStrategy::HealthBased => endpoints
                .iter()
                .max_by_key(|e| e.health_score.load(Ordering::Relaxed))
                .copied(),
        }
    }

    /// 全エンドポイントの統計情報を取得
    pub async fn get_statistics(&self) -> LoadBalancerStats {
        let endpoints = self.endpoints.read().await;
        let mut stats = LoadBalancerStats {
            total_endpoints: endpoints.len(),
            available_endpoints: 0,
            total_connections: 0,
            avg_response_time_ms: 0,
            avg_health_score: 0,
            endpoints_by_role: HashMap::new(),
        };

        let mut total_response_time = 0;
        let mut total_health = 0;

        for endpoint in endpoints.iter() {
            if endpoint.is_available() {
                stats.available_endpoints += 1;
            }

            stats.total_connections += endpoint.current_connections.load(Ordering::Relaxed);
            total_response_time += endpoint.avg_response_time_ms.load(Ordering::Relaxed);
            total_health += endpoint.health_score.load(Ordering::Relaxed);

            *stats
                .endpoints_by_role
                .entry(endpoint.role.clone())
                .or_insert(0) += 1;
        }

        if !endpoints.is_empty() {
            stats.avg_response_time_ms = total_response_time / endpoints.len();
            stats.avg_health_score = total_health / endpoints.len();
        }

        stats
    }
}

/// 読み書き分離マネージャー
pub struct ReadWriteSplitter {
    load_balancer: Arc<LoadBalancer>,
    read_preference: ReadPreference,
}

/// 読み取り設定
#[derive(Debug, Clone)]
pub enum ReadPreference {
    /// 常にマスターから読み取り
    Primary,
    /// 可能な限りレプリカから読み取り
    Secondary,
    /// レプリカ優先、利用不可時はマスター
    SecondaryPreferred,
    /// マスター優先、利用不可時はレプリカ
    PrimaryPreferred,
    /// 最も近い（低レイテンシ）から読み取り
    Nearest,
}

impl ReadWriteSplitter {
    pub fn new(load_balancer: Arc<LoadBalancer>, read_preference: ReadPreference) -> Self {
        Self {
            load_balancer,
            read_preference,
        }
    }

    /// クエリタイプに基づいて適切なエンドポイントを選択
    pub async fn select_endpoint_for_query(
        &self,
        query_type: QueryType,
    ) -> Result<DatabaseEndpoint, DatabaseError> {
        match query_type {
            QueryType::Select => self.select_read_endpoint().await,
            QueryType::Insert | QueryType::Update | QueryType::Delete => {
                self.load_balancer.select_write_endpoint().await
            }
            QueryType::Transaction => {
                // トランザクションは一般的にマスターで実行
                self.load_balancer.select_write_endpoint().await
            }
            QueryType::Ddl => {
                // DDL操作はマスターでのみ実行
                self.load_balancer.select_write_endpoint().await
            }
            QueryType::Unknown => {
                // 不明なクエリは安全のためマスターで実行
                warn!("Unknown query type, routing to master");
                self.load_balancer.select_write_endpoint().await
            }
            // 他のクエリタイプもマスターで実行
            _ => self.load_balancer.select_write_endpoint().await,
        }
    }

    /// 読み取り設定に基づいて読み取りエンドポイントを選択
    async fn select_read_endpoint(&self) -> Result<DatabaseEndpoint, DatabaseError> {
        match self.read_preference {
            ReadPreference::Primary => self.load_balancer.select_write_endpoint().await,
            ReadPreference::Secondary => self.load_balancer.select_read_endpoint().await,
            ReadPreference::SecondaryPreferred => {
                match self.load_balancer.select_read_endpoint().await {
                    Ok(endpoint) => Ok(endpoint),
                    Err(_) => {
                        debug!("No secondary available, falling back to primary");
                        self.load_balancer.select_write_endpoint().await
                    }
                }
            }
            ReadPreference::PrimaryPreferred => {
                match self.load_balancer.select_write_endpoint().await {
                    Ok(endpoint) => Ok(endpoint),
                    Err(_) => {
                        debug!("No primary available, falling back to secondary");
                        self.load_balancer.select_read_endpoint().await
                    }
                }
            }
            ReadPreference::Nearest => {
                // レスポンス時間ベースで選択
                let endpoints = self.load_balancer.endpoints.read().await;
                let best_endpoint = endpoints
                    .iter()
                    .filter(|e| e.is_available())
                    .min_by_key(|e| e.avg_response_time_ms.load(Ordering::Relaxed))
                    .cloned();

                best_endpoint.ok_or_else(|| {
                    DatabaseError::NoAvailableEndpoints(
                        "No available endpoints for nearest read".to_string(),
                    )
                })
            }
        }
    }
}

/// 負荷分散統計情報
#[derive(Debug, Clone)]
pub struct LoadBalancerStats {
    pub total_endpoints: usize,
    pub available_endpoints: usize,
    pub total_connections: usize,
    pub avg_response_time_ms: usize,
    pub avg_health_score: usize,
    pub endpoints_by_role: HashMap<DatabaseRole, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_endpoint(
        id: &str,
        role: DatabaseRole,
        connections: usize,
        response_time: usize,
    ) -> DatabaseEndpoint {
        let endpoint = DatabaseEndpoint::new(
            id.to_string(),
            DatabaseConfig::default(),
            role,
            100, // max_connections を100に設定
        );
        endpoint
            .current_connections
            .store(connections, Ordering::Relaxed);
        endpoint
            .avg_response_time_ms
            .store(response_time, Ordering::Relaxed);
        endpoint
    }

    #[tokio::test]
    async fn test_round_robin_load_balancing() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);

        let endpoint1 = create_test_endpoint("db1", DatabaseRole::Slave, 10, 50);
        let endpoint2 = create_test_endpoint("db2", DatabaseRole::Slave, 20, 100);

        lb.add_endpoint(endpoint1).await;
        lb.add_endpoint(endpoint2).await;

        // ラウンドロビンで交互に選択されることを確認
        let first = lb.select_read_endpoint().await.unwrap();
        let second = lb.select_read_endpoint().await.unwrap();

        assert_ne!(first.id, second.id);
    }

    #[tokio::test]
    async fn test_least_connections_strategy() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::LeastConnections);

        let endpoint1 = create_test_endpoint("db1", DatabaseRole::Slave, 10, 50);
        let endpoint2 = create_test_endpoint("db2", DatabaseRole::Slave, 5, 100);

        lb.add_endpoint(endpoint1).await;
        lb.add_endpoint(endpoint2).await;

        // 接続数の少ないエンドポイントが選択されることを確認
        let selected = lb.select_read_endpoint().await.unwrap();
        assert_eq!(selected.id, "db2");
    }

    #[tokio::test]
    async fn test_response_time_strategy() {
        let lb = LoadBalancer::new(LoadBalancingStrategy::ResponseTime);

        let endpoint1 = create_test_endpoint("db1", DatabaseRole::Slave, 10, 100);
        let endpoint2 = create_test_endpoint("db2", DatabaseRole::Slave, 20, 50);

        lb.add_endpoint(endpoint1).await;
        lb.add_endpoint(endpoint2).await;

        // レスポンス時間の短いエンドポイントが選択されることを確認
        let selected = lb.select_read_endpoint().await.unwrap();
        assert_eq!(selected.id, "db2");
    }

    #[tokio::test]
    async fn test_read_write_splitting() {
        let lb = Arc::new(LoadBalancer::new(LoadBalancingStrategy::RoundRobin));
        let splitter = ReadWriteSplitter::new(Arc::clone(&lb), ReadPreference::SecondaryPreferred);

        let master = create_test_endpoint("master", DatabaseRole::Master, 0, 50);
        let slave = create_test_endpoint("slave", DatabaseRole::Slave, 0, 60);

        lb.add_endpoint(master).await;
        lb.add_endpoint(slave).await;

        // 読み取りクエリはスレーブに
        let read_endpoint = splitter
            .select_endpoint_for_query(QueryType::Select)
            .await
            .unwrap();
        assert_eq!(read_endpoint.role, DatabaseRole::Slave);

        // 書き込みクエリはマスターに
        let write_endpoint = splitter
            .select_endpoint_for_query(QueryType::Insert)
            .await
            .unwrap();
        assert_eq!(write_endpoint.role, DatabaseRole::Master);
    }

    #[tokio::test]
    async fn test_endpoint_availability() {
        let endpoint = create_test_endpoint("test", DatabaseRole::Master, 50, 50);

        // 正常な状態では利用可能
        assert!(endpoint.is_available());

        // 健全性が低下すると利用不可
        endpoint.update_health_score(30);
        assert!(!endpoint.is_available());

        // 負荷が高すぎると利用不可
        endpoint.update_health_score(80);
        endpoint.current_connections.store(96, Ordering::Relaxed); // 96/100 = 96% > 95%
        assert!(!endpoint.is_available());
    }
}
