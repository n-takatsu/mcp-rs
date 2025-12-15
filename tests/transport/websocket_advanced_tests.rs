//! Advanced WebSocket integration tests
//!
//! Tests for file transfer, load balancing, and failover functionality

use mcp_rs::transport::websocket::*;

#[tokio::test]
async fn test_transfer_manager_basic() {
    let manager = TransferManager::new();
    let transfer_id = uuid::Uuid::new_v4().to_string();

    let options = TransferOptions {
        chunk_size: 1024 * 1024, // 1MB
        parallel_chunks: 4,
        compression: CompressionType::None,
        encryption: false,
        resume_support: true,
    };

    manager
        .register_transfer(transfer_id.clone(), 10 * 1024 * 1024, options)
        .await;

    assert_eq!(manager.active_count().await, 0);
}

#[tokio::test]
async fn test_transfer_progress_tracking() {
    let manager = TransferManager::new();
    let transfer_id = uuid::Uuid::new_v4().to_string();

    let options = TransferOptions::default();

    manager
        .register_transfer(transfer_id.clone(), 100, options)
        .await;

    // Update progress to InProgress
    manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;

    assert_eq!(manager.active_count().await, 1);

    // Update progress to 50%
    manager
        .update_progress(&transfer_id, 50, std::time::Duration::from_secs(1))
        .await;

    let retrieved = manager.get_progress(&transfer_id).await.unwrap();
    assert!(retrieved.percentage() >= 49.0 && retrieved.percentage() <= 51.0);
}

#[tokio::test]
async fn test_transfer_state_transitions() {
    let manager = TransferManager::new();
    let transfer_id = uuid::Uuid::new_v4().to_string();

    let options = TransferOptions::default();

    manager
        .register_transfer(transfer_id.clone(), 1000, options)
        .await;

    // Pending -> InProgress
    manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;
    assert_eq!(manager.active_count().await, 1);

    // InProgress -> Paused
    manager
        .update_state(&transfer_id, TransferState::Paused)
        .await;
    assert_eq!(manager.active_count().await, 0);

    // Paused -> InProgress
    manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;
    assert_eq!(manager.active_count().await, 1);

    // InProgress -> Completed
    manager
        .update_state(&transfer_id, TransferState::Completed)
        .await;
    assert_eq!(manager.active_count().await, 0);
}

#[tokio::test]
async fn test_chunk_verification() {
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let data = vec![1, 2, 3, 4, 5];
    let chunk = FileChunk::new(transfer_id, 0, 1, data.clone());

    assert!(chunk.verify());
}

#[tokio::test]
async fn test_balancer_round_robin_strategy() {
    let config = BalancerConfig {
        strategy: BalancingStrategy::RoundRobin,
        health_check_interval: std::time::Duration::from_secs(10),
        failover_threshold: 3,
        session_affinity: false,
    };

    let mut manager = BalancerManager::new(config);

    let endpoint1 = Endpoint::new("ep1".to_string(), "ws://localhost:8081".to_string());
    let endpoint2 = Endpoint::new("ep2".to_string(), "ws://localhost:8082".to_string());
    let endpoint3 = Endpoint::new("ep3".to_string(), "ws://localhost:8083".to_string());

    manager.register_endpoint(endpoint1.clone()).await;
    manager.register_endpoint(endpoint2.clone()).await;
    manager.register_endpoint(endpoint3.clone()).await;

    let selected1 = manager.select_endpoint().await.unwrap();
    let selected2 = manager.select_endpoint().await.unwrap();
    let selected3 = manager.select_endpoint().await.unwrap();
    let selected4 = manager.select_endpoint().await.unwrap();

    assert_eq!(selected1.id, "ep1");
    assert_eq!(selected2.id, "ep2");
    assert_eq!(selected3.id, "ep3");
    assert_eq!(selected4.id, "ep1"); // Wraps around
}

#[tokio::test]
async fn test_balancer_least_connections() {
    let config = BalancerConfig {
        strategy: BalancingStrategy::LeastConnections,
        health_check_interval: std::time::Duration::from_secs(10),
        failover_threshold: 3,
        session_affinity: false,
    };

    let mut manager = BalancerManager::new(config);

    let endpoint1 = Endpoint::new("ep1".to_string(), "ws://localhost:8081".to_string());
    let endpoint2 = Endpoint::new("ep2".to_string(), "ws://localhost:8082".to_string());

    manager.register_endpoint(endpoint1.clone());
    manager.register_endpoint(endpoint2.clone());

    // First selection should be ep1 (both have 0 connections)
    let selected1 = manager.select_endpoint().await.unwrap();
    manager.increment_connections(&selected1.id);

    // Second selection should be ep2 (ep1 has 1, ep2 has 0)
    let selected2 = manager.select_endpoint().await.unwrap();
    assert_eq!(selected2.id, "ep2");
}

#[tokio::test]
async fn test_balancer_health_reporting() {
    let config = BalancerConfig::default();
    let mut manager = BalancerManager::new(config);

    let endpoint = Endpoint::new("ep1".to_string(), "ws://localhost:8081".to_string());
    manager.register_endpoint(endpoint.clone()).await;

    // Endpoint should be healthy initially
    let stats = manager.get_statistics().await;
    assert_eq!(stats.healthy_endpoints, 1);

    // Report failures
    manager.report_health(&endpoint.id, false).await;
    manager.report_health(&endpoint.id, false).await;
    manager.report_health(&endpoint.id, false).await;

    // After 3 failures, should be unhealthy
    let stats = manager.get_statistics().await;
    assert_eq!(stats.healthy_endpoints, 0);
}

#[tokio::test]
async fn test_failover_manager_register_backup() {
    let config = FailoverConfig::default();
    let mut manager = FailoverManager::new(config);

    let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
    let backup1 = Endpoint::new("backup1".to_string(), "ws://backup1".to_string());
    let backup2 = Endpoint::new("backup2".to_string(), "ws://backup2".to_string());

    manager
        .register_backup(primary.clone(), backup1.clone())
        .await
        .unwrap();
    manager
        .register_backup(primary.clone(), backup2.clone())
        .await
        .unwrap();

    let backups = manager.get_backups(&primary.id).await;
    assert_eq!(backups.len(), 2);
}

#[tokio::test]
async fn test_failover_trigger() {
    let config = FailoverConfig::default();
    let mut manager = FailoverManager::new(config);

    let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
    let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

    manager
        .register_backup(primary.clone(), backup.clone())
        .await
        .unwrap();

    let failover_endpoint = manager.trigger_failover(&primary).await.unwrap();
    assert_eq!(failover_endpoint.id, "backup");

    assert!(manager.is_failover_active(&primary));
}

#[tokio::test]
async fn test_failover_session_restoration() {
    let config = FailoverConfig {
        session_persistence: true,
        ..Default::default()
    };
    let manager = FailoverManager::new(config);

    let mut session = SessionState::new("session_123".to_string());
    session.add_pending_message("message1".to_string());
    session.add_pending_message("message2".to_string());

    manager.save_session(session.clone()).await;

    let restored = manager.restore_session("session_123").await.unwrap();
    assert_eq!(restored.session_id, "session_123");
    assert_eq!(restored.pending_messages.len(), 2);
}

#[tokio::test]
async fn test_failover_retry_with_exponential_backoff() {
    let config = FailoverConfig {
        max_retries: 5,
        initial_retry_delay: std::time::Duration::from_millis(100),
        max_retry_delay: std::time::Duration::from_secs(10),
        backoff_multiplier: 2.0,
        ..Default::default()
    };
    let mut manager = FailoverManager::new(config);

    let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
    let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

    manager
        .register_backup(primary.clone(), backup)
        .await
        .unwrap();

    // Trigger multiple failovers to test retry count
    manager.trigger_failover(&primary).await.unwrap();
    assert_eq!(manager.get_retry_count(&primary.id).await, 1);

    manager.trigger_failover(&primary).await.unwrap();
    assert_eq!(manager.get_retry_count(&primary.id).await, 2);
}

#[tokio::test]
async fn test_integration_transfer_with_balancer() {
    // Create transfer manager
    let transfer_manager = TransferManager::new();

    // Create load balancer
    let config = BalancerConfig::default();
    let mut balancer = BalancerManager::new(config);

    let endpoint1 = Endpoint::new("ep1".to_string(), "ws://localhost:8081".to_string());
    let endpoint2 = Endpoint::new("ep2".to_string(), "ws://localhost:8082".to_string());

    balancer.register_endpoint(endpoint1);
    balancer.register_endpoint(endpoint2);

    // Select endpoint for transfer
    let selected = balancer.select_endpoint().await.unwrap();

    // Register transfer with selected endpoint
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let options = TransferOptions::default();

    transfer_manager
        .register_transfer(transfer_id.clone(), 1024 * 1024, options)
        .await;

    // Simulate transfer start
    transfer_manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;

    balancer.increment_connections(&selected.id);

    assert_eq!(transfer_manager.active_count().await, 1);
}

#[tokio::test]
async fn test_integration_failover_with_session_restoration() {
    let failover_config = FailoverConfig {
        session_persistence: true,
        ..Default::default()
    };
    let mut failover_manager = FailoverManager::new(failover_config);

    let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
    let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

    failover_manager
        .register_backup(primary.clone(), backup)
        .await
        .unwrap();

    // Create session
    let mut session = SessionState::new("session_456".to_string());
    session.add_pending_message("important_message".to_string());
    failover_manager.save_session(session.clone()).await;

    // Trigger failover
    let new_endpoint = failover_manager.trigger_failover(&primary).await.unwrap();
    assert_eq!(new_endpoint.id, "backup");

    // Restore session
    let restored = failover_manager.restore_session("session_456").await.unwrap();
    assert_eq!(restored.pending_messages.len(), 1);
}

#[tokio::test]
async fn test_weighted_round_robin_distribution() {
    let config = BalancerConfig {
        strategy: BalancingStrategy::WeightedRoundRobin,
        ..Default::default()
    };

    let mut manager = BalancerManager::new(config);

    let endpoint1 = Endpoint::new("ep1".to_string(), "ws://localhost:8081".to_string())
        .with_weight(1);
    let endpoint2 = Endpoint::new("ep2".to_string(), "ws://localhost:8082".to_string())
        .with_weight(3);

    manager.register_endpoint(endpoint1).await;
    manager.register_endpoint(endpoint2).await;

    let mut ep1_count = 0;
    let mut ep2_count = 0;

    // Test distribution over 100 selections
    for _ in 0..100 {
        let selected = manager.select_endpoint().await.unwrap();
        if selected.id == "ep1" {
            ep1_count += 1;
        } else {
            ep2_count += 1;
        }
    }

    // ep2 should be selected ~3x more than ep1
    assert!(ep2_count > ep1_count * 2);
}
