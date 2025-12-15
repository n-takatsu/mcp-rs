//! Load balancer implementation for WebSocket connections
//!
//! Provides various load balancing strategies including round-robin,
//! least connections, and weighted distribution.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Endpoint identifier
pub type EndpointId = String;

/// WebSocket endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// Unique endpoint ID
    pub id: EndpointId,
    /// WebSocket URL
    pub url: String,
    /// Weight for weighted load balancing
    pub weight: u32,
    /// Maximum concurrent connections
    pub max_connections: usize,
}

impl Endpoint {
    /// Creates a new endpoint
    pub fn new(id: EndpointId, url: String) -> Self {
        Self {
            id,
            url,
            weight: 1,
            max_connections: 1000,
        }
    }

    /// Sets weight
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Sets max connections
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
}

/// Load balancing strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BalancingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Least connections
    LeastConnections,
    /// Weighted round-robin
    WeightedRoundRobin,
    /// Random selection
    Random,
}

/// Load balancer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalancerConfig {
    /// Balancing strategy
    pub strategy: BalancingStrategy,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Failover threshold (consecutive failures)
    pub failover_threshold: u32,
    /// Enable session affinity
    pub session_affinity: bool,
}

impl Default for BalancerConfig {
    fn default() -> Self {
        Self {
            strategy: BalancingStrategy::RoundRobin,
            health_check_interval: Duration::from_secs(10),
            failover_threshold: 3,
            session_affinity: false,
        }
    }
}

/// Endpoint statistics
#[derive(Debug, Clone)]
pub struct EndpointStats {
    /// Endpoint ID
    pub endpoint_id: EndpointId,
    /// Current active connections
    pub active_connections: usize,
    /// Total requests handled
    pub total_requests: u64,
    /// Total failures
    pub total_failures: u64,
    /// Average response time (ms)
    pub avg_response_time: f64,
    /// Is endpoint healthy
    pub is_healthy: bool,
    /// Last health check time
    pub last_health_check: Option<Instant>,
}

impl EndpointStats {
    /// Creates new endpoint stats
    pub fn new(endpoint_id: EndpointId) -> Self {
        Self {
            endpoint_id,
            active_connections: 0,
            total_requests: 0,
            total_failures: 0,
            avg_response_time: 0.0,
            is_healthy: true,
            last_health_check: None,
        }
    }

    /// Calculates error rate (0.0 to 1.0)
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.total_failures as f64 / self.total_requests as f64
    }
}

/// Balancer statistics
#[derive(Debug, Clone)]
pub struct BalancerStats {
    /// Total endpoints
    pub total_endpoints: usize,
    /// Healthy endpoints
    pub healthy_endpoints: usize,
    /// Total requests
    pub total_requests: u64,
    /// Endpoint statistics
    pub endpoints: Vec<EndpointStats>,
}

/// Load balancer trait
pub trait LoadBalancer: Send + Sync {
    /// Selects an endpoint from available endpoints
    fn select_endpoint(&self, endpoints: &[Endpoint]) -> Option<Endpoint>;

    /// Reports endpoint health status
    fn report_health(&mut self, endpoint: &Endpoint, is_healthy: bool);

    /// Gets balancer statistics
    fn get_statistics(&self) -> BalancerStats;

    /// Increments connection count for endpoint
    fn increment_connections(&mut self, endpoint_id: &EndpointId);

    /// Decrements connection count for endpoint
    fn decrement_connections(&mut self, endpoint_id: &EndpointId);
}

/// Endpoint state
#[derive(Debug, Clone)]
struct EndpointState {
    endpoint: Endpoint,
    stats: EndpointStats,
    consecutive_failures: Arc<AtomicU32>,
    active_connections: Arc<AtomicUsize>,
    total_requests: Arc<AtomicU64>,
}

/// Load balancer manager implementation
pub struct BalancerManager {
    config: BalancerConfig,
    endpoints: Arc<RwLock<HashMap<EndpointId, EndpointState>>>,
    round_robin_index: Arc<AtomicUsize>,
}

impl BalancerManager {
    /// Creates a new balancer manager
    pub fn new(config: BalancerConfig) -> Self {
        Self {
            config,
            endpoints: Arc::new(RwLock::new(HashMap::new())),
            round_robin_index: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Registers an endpoint
    pub async fn register_endpoint(&self, endpoint: Endpoint) {
        let endpoint_id = endpoint.id.clone();
        let mut endpoints = self.endpoints.write().await;
        endpoints.insert(
            endpoint_id.clone(),
            EndpointState {
                endpoint,
                stats: EndpointStats::new(endpoint_id),
                consecutive_failures: Arc::new(AtomicU32::new(0)),
                active_connections: Arc::new(AtomicUsize::new(0)),
                total_requests: Arc::new(AtomicU64::new(0)),
            },
        );
    }

    /// Removes an endpoint
    pub async fn remove_endpoint(&self, endpoint_id: &EndpointId) {
        let mut endpoints = self.endpoints.write().await;
        endpoints.remove(endpoint_id);
    }

    /// Selects an endpoint from registered endpoints
    pub async fn select_endpoint(&self) -> Option<Endpoint> {
        let endpoints_map = self.endpoints.read().await;
        let all_endpoints: Vec<Endpoint> = endpoints_map
            .values()
            .map(|state| state.endpoint.clone())
            .collect();

        if all_endpoints.is_empty() {
            return None;
        }

        let healthy = self.filter_healthy(&all_endpoints).await;
        if healthy.is_empty() {
            return None;
        }

        match self.config.strategy {
            BalancingStrategy::RoundRobin => self.select_round_robin(&healthy),
            BalancingStrategy::LeastConnections => self.select_least_connections(&healthy).await,
            BalancingStrategy::WeightedRoundRobin => self.select_weighted_round_robin(&healthy),
            BalancingStrategy::Random => self.select_random(&healthy),
        }
    }

    /// Selects endpoint using round-robin strategy
    fn select_round_robin(&self, available: &[Endpoint]) -> Option<Endpoint> {
        if available.is_empty() {
            return None;
        }

        let index = self.round_robin_index.fetch_add(1, Ordering::Relaxed) % available.len();
        Some(available[index].clone())
    }

    /// Selects endpoint using least connections strategy
    async fn select_least_connections(&self, available: &[Endpoint]) -> Option<Endpoint> {
        if available.is_empty() {
            return None;
        }

        let endpoints = self.endpoints.read().await;
        let mut min_connections = usize::MAX;
        let mut selected = None;

        for endpoint in available {
            if let Some(state) = endpoints.get(&endpoint.id) {
                let connections = state.active_connections.load(Ordering::Relaxed);
                if connections < min_connections {
                    min_connections = connections;
                    selected = Some(endpoint.clone());
                }
            }
        }

        selected.or_else(|| Some(available[0].clone()))
    }

    /// Selects endpoint using weighted round-robin strategy
    fn select_weighted_round_robin(&self, available: &[Endpoint]) -> Option<Endpoint> {
        if available.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: u32 = available.iter().map(|e| e.weight).sum();
        if total_weight == 0 {
            return self.select_round_robin(available);
        }

        // Select based on weight
        let index = self.round_robin_index.fetch_add(1, Ordering::Relaxed);
        let target = (index as u32) % total_weight;
        let mut cumulative_weight = 0;

        for endpoint in available {
            cumulative_weight += endpoint.weight;
            if target < cumulative_weight {
                return Some(endpoint.clone());
            }
        }

        Some(available[0].clone())
    }

    /// Selects endpoint using random strategy
    fn select_random(&self, available: &[Endpoint]) -> Option<Endpoint> {
        if available.is_empty() {
            return None;
        }

        use std::collections::hash_map::RandomState;
        use std::hash::BuildHasher;

        let random_state = RandomState::new();
        let index = (random_state.hash_one(std::time::SystemTime::now()) as usize) % available.len();

        Some(available[index].clone())
    }

    /// Filters healthy endpoints
    async fn filter_healthy(&self, endpoints: &[Endpoint]) -> Vec<Endpoint> {
        let states = self.endpoints.read().await;
        endpoints
            .iter()
            .filter(|e| {
                states
                    .get(&e.id)
                    .map(|s| s.stats.is_healthy)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Gets statistics
    pub async fn get_statistics(&self) -> BalancerStats {
        let endpoints = self.endpoints.read().await;
        let endpoint_stats: Vec<_> = endpoints.values().map(|s| s.stats.clone()).collect();

        let healthy_count = endpoint_stats.iter().filter(|s| s.is_healthy).count();
        let total_requests = endpoint_stats.iter().map(|s| s.total_requests).sum();

        BalancerStats {
            total_endpoints: endpoints.len(),
            healthy_endpoints: healthy_count,
            total_requests,
            endpoints: endpoint_stats,
        }
    }

    /// Async version of select_endpoint
    pub async fn select_endpoint_async(&self, endpoints: &[Endpoint]) -> Option<Endpoint> {
        let healthy = self.filter_healthy(endpoints).await;
        if healthy.is_empty() {
            return None;
        }

        match self.config.strategy {
            BalancingStrategy::RoundRobin => self.select_round_robin(&healthy),
            BalancingStrategy::LeastConnections => self.select_least_connections(&healthy).await,
            BalancingStrategy::WeightedRoundRobin => self.select_weighted_round_robin(&healthy),
            BalancingStrategy::Random => self.select_random(&healthy),
        }
    }

    /// Reports endpoint health
    pub async fn report_health(&self, endpoint_id: &EndpointId, is_healthy: bool) {
        let mut endpoints = self.endpoints.write().await;
        if let Some(state) = endpoints.get_mut(endpoint_id) {
            state.stats.is_healthy = is_healthy;
            state.stats.last_health_check = Some(Instant::now());

            if is_healthy {
                state.consecutive_failures.store(0, Ordering::Relaxed);
            } else {
                let failures = state.consecutive_failures.fetch_add(1, Ordering::Relaxed);
                if failures >= self.config.failover_threshold {
                    state.stats.is_healthy = false;
                }
            }
        }
    }

    /// Reports endpoint health (async version, keeping for backward compatibility)
    pub async fn report_health_async(&self, endpoint: &Endpoint, is_healthy: bool) {
        self.report_health(&endpoint.id, is_healthy).await;
    }

    /// Increments connection count
    pub fn increment_connections(&self, endpoint_id: &EndpointId) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let endpoints = self.endpoints.read().await;
                if let Some(state) = endpoints.get(endpoint_id) {
                    state.active_connections.fetch_add(1, Ordering::Relaxed);
                    state.total_requests.fetch_add(1, Ordering::Relaxed);
                }
            })
        });
    }

    /// Decrements connection count
    pub fn decrement_connections(&self, endpoint_id: &EndpointId) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let endpoints = self.endpoints.read().await;
                if let Some(state) = endpoints.get(endpoint_id) {
                    state.active_connections.fetch_sub(1, Ordering::Relaxed);
                }
            })
        });
    }

    /// Increments connection count (async version)
    pub async fn increment_connections_async(&self, endpoint_id: &EndpointId) {
        let endpoints = self.endpoints.read().await;
        if let Some(state) = endpoints.get(endpoint_id) {
            state.active_connections.fetch_add(1, Ordering::Relaxed);
            state.total_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Decrements connection count (async version)
    pub async fn decrement_connections_async(&self, endpoint_id: &EndpointId) {
        let endpoints = self.endpoints.read().await;
        if let Some(state) = endpoints.get(endpoint_id) {
            state.active_connections.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl LoadBalancer for BalancerManager {
    fn select_endpoint(&self, endpoints: &[Endpoint]) -> Option<Endpoint> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.select_endpoint_async(endpoints).await })
        })
    }

    fn report_health(&mut self, endpoint: &Endpoint, is_healthy: bool) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.report_health_async(endpoint, is_healthy).await })
        });
    }

    fn get_statistics(&self) -> BalancerStats {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { self.get_statistics().await })
        })
    }

    fn increment_connections(&mut self, endpoint_id: &EndpointId) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.increment_connections_async(endpoint_id).await })
        });
    }

    fn decrement_connections(&mut self, endpoint_id: &EndpointId) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.decrement_connections_async(endpoint_id).await })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_creation() {
        let endpoint = Endpoint::new("ep1".to_string(), "ws://localhost:8080".to_string())
            .with_weight(2)
            .with_max_connections(500);

        assert_eq!(endpoint.weight, 2);
        assert_eq!(endpoint.max_connections, 500);
    }

    #[test]
    fn test_endpoint_stats_error_rate() {
        let mut stats = EndpointStats::new("ep1".to_string());
        stats.total_requests = 100;
        stats.total_failures = 5;

        assert_eq!(stats.error_rate(), 0.05);
    }

    #[tokio::test]
    async fn test_balancer_round_robin() {
        let config = BalancerConfig {
            strategy: BalancingStrategy::RoundRobin,
            ..Default::default()
        };
        let mut balancer = BalancerManager::new(config);

        let endpoints = vec![
            Endpoint::new("ep1".to_string(), "ws://server1".to_string()),
            Endpoint::new("ep2".to_string(), "ws://server2".to_string()),
            Endpoint::new("ep3".to_string(), "ws://server3".to_string()),
        ];

        for ep in endpoints.clone() {
            balancer.register_endpoint(ep);
        }

        // Should cycle through endpoints
        let selected1 = balancer.select_endpoint_async(&endpoints).await;
        let selected2 = balancer.select_endpoint_async(&endpoints).await;
        let selected3 = balancer.select_endpoint_async(&endpoints).await;

        assert!(selected1.is_some());
        assert!(selected2.is_some());
        assert!(selected3.is_some());
    }

    #[tokio::test]
    async fn test_balancer_least_connections() {
        let config = BalancerConfig {
            strategy: BalancingStrategy::LeastConnections,
            ..Default::default()
        };
        let mut balancer = BalancerManager::new(config);

        let endpoints = vec![
            Endpoint::new("ep1".to_string(), "ws://server1".to_string()),
            Endpoint::new("ep2".to_string(), "ws://server2".to_string()),
        ];

        for ep in endpoints.clone() {
            balancer.register_endpoint(ep);
        }

        // Simulate connections on ep1
        balancer
            .increment_connections_async(&"ep1".to_string())
            .await;
        balancer
            .increment_connections_async(&"ep1".to_string())
            .await;

        // Should select ep2 (fewer connections)
        let selected = balancer.select_endpoint_async(&endpoints).await;
        assert_eq!(selected.unwrap().id, "ep2");
    }

    #[tokio::test]
    async fn test_balancer_health_reporting() {
        let config = BalancerConfig::default();
        let mut balancer = BalancerManager::new(config);

        let endpoint = Endpoint::new("ep1".to_string(), "ws://server1".to_string());
        balancer.register_endpoint(endpoint.clone());

        // Report unhealthy
        balancer.report_health(&endpoint.id, false).await;

        let stats = balancer.get_statistics().await;
        assert_eq!(stats.healthy_endpoints, 0);
    }

    #[tokio::test]
    async fn test_balancer_weighted_round_robin() {
        let config = BalancerConfig {
            strategy: BalancingStrategy::WeightedRoundRobin,
            ..Default::default()
        };
        let mut balancer = BalancerManager::new(config);

        let endpoints = vec![
            Endpoint::new("ep1".to_string(), "ws://server1".to_string()).with_weight(3),
            Endpoint::new("ep2".to_string(), "ws://server2".to_string()).with_weight(1),
        ];

        for ep in endpoints.clone() {
            balancer.register_endpoint(ep);
        }

        // ep1 should be selected more often (75% of the time)
        let mut ep1_count = 0;
        for _ in 0..100 {
            if let Some(ep) = balancer.select_endpoint_async(&endpoints).await {
                if ep.id == "ep1" {
                    ep1_count += 1;
                }
            }
        }

        assert!(ep1_count > 60); // Should be around 75
    }
}
