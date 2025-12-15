//! WebSocket Advanced Features Demo
//!
//! Demonstrates large file transfer, load balancing, and failover capabilities

use mcp_rs::error::Result;
use mcp_rs::transport::websocket::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== WebSocket Advanced Features Demo ===\n");

    demo_file_transfer().await?;
    demo_load_balancing().await?;
    demo_failover().await?;
    demo_integrated_scenario().await?;

    Ok(())
}

/// Demonstrates large file transfer functionality
async fn demo_file_transfer() -> Result<()> {
    println!("--- File Transfer Demo ---");

    let manager = TransferManager::new();

    // Configure transfer options
    let options = TransferOptions {
        chunk_size: 1024 * 1024, // 1MB chunks
        parallel_chunks: 4,
        compression: CompressionType::None,
        encryption: false,
        resume_support: true,
    };

    println!("Transfer Options:");
    println!("  Chunk Size: {} MB", options.chunk_size / (1024 * 1024));
    println!("  Parallel Chunks: {}", options.parallel_chunks);
    println!("  Compression: {:?}", options.compression);

    // Create transfer
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let file_size = 50 * 1024 * 1024; // 50MB

    manager
        .register_transfer(transfer_id.clone(), file_size, options)
        .await;

    println!("\nTransfer ID: {}", transfer_id);
    println!("File Size: {} MB", file_size / (1024 * 1024));

    // Simulate transfer progress
    manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;

    for i in 1..=10 {
        let bytes = (file_size * i) / 10;
        manager.update_progress(&transfer_id, bytes, Duration::from_millis(100)).await;

        let progress = manager.get_progress(&transfer_id).await.unwrap();
        println!(
            "Progress: {:.1}% | Speed: {:.2} MB/s",
            progress.percentage(),
            progress.speed / (1024.0 * 1024.0)
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Complete transfer
    manager
        .update_state(&transfer_id, TransferState::Completed)
        .await;

    println!("Transfer completed!\n");

    Ok(())
}

/// Demonstrates load balancing strategies
async fn demo_load_balancing() -> Result<()> {
    println!("--- Load Balancing Demo ---");

    // Test Round Robin
    println!("\n1. Round Robin Strategy:");
    let config = BalancerConfig {
        strategy: BalancingStrategy::RoundRobin,
        health_check_interval: Duration::from_secs(10),
        failover_threshold: 3,
        session_affinity: false,
    };

    let mut balancer = BalancerManager::new(config);

    let endpoints = vec![
        Endpoint::new("server1".to_string(), "ws://server1:8080".to_string()),
        Endpoint::new("server2".to_string(), "ws://server2:8080".to_string()),
        Endpoint::new("server3".to_string(), "ws://server3:8080".to_string()),
    ];

    for endpoint in endpoints.clone() {
        balancer.register_endpoint(endpoint);
    }

    println!("Registered endpoints:");
    for endpoint in &endpoints {
        println!("  - {} ({})", endpoint.id, endpoint.url);
    }

    println!("\nSelecting endpoints (Round Robin):");
    for _ in 0..6 {
        let selected = balancer.select_endpoint().await?;
        println!("  Selected: {}", selected.id);
    }

    // Test Least Connections
    println!("\n2. Least Connections Strategy:");
    let config = BalancerConfig {
        strategy: BalancingStrategy::LeastConnections,
        ..Default::default()
    };

    let mut balancer = BalancerManager::new(config);

    for endpoint in endpoints.clone() {
        balancer.register_endpoint(endpoint);
    }

    println!("Simulating connection load:");
    balancer.increment_connections("server1");
    balancer.increment_connections("server1");
    balancer.increment_connections("server2");

    for i in 1..=4 {
        let selected = balancer.select_endpoint().await?;
        println!("  Request {}: Selected {}", i, selected.id);
    }

    // Test Weighted Round Robin
    println!("\n3. Weighted Round Robin Strategy:");
    let config = BalancerConfig {
        strategy: BalancingStrategy::WeightedRoundRobin,
        ..Default::default()
    };

    let mut balancer = BalancerManager::new(config);

    let weighted_endpoints = vec![
        Endpoint::new("light".to_string(), "ws://light:8080".to_string()).with_weight(1),
        Endpoint::new("medium".to_string(), "ws://medium:8080".to_string()).with_weight(2),
        Endpoint::new("heavy".to_string(), "ws://heavy:8080".to_string()).with_weight(4),
    ];

    for endpoint in weighted_endpoints {
        balancer.register_endpoint(endpoint);
    }

    println!("Endpoint weights:");
    println!("  - light: 1");
    println!("  - medium: 2");
    println!("  - heavy: 4");

    println!("\nDistribution over 14 requests:");
    let mut counts = std::collections::HashMap::new();
    for _ in 0..14 {
        let selected = balancer.select_endpoint().await?;
        *counts.entry(selected.id).or_insert(0) += 1;
    }

    for (id, count) in counts {
        println!("  {}: {} requests", id, count);
    }

    // Health checking
    println!("\n4. Health Checking:");
    let config = BalancerConfig::default();
    let mut balancer = BalancerManager::new(config);

    let endpoint = Endpoint::new("test_server".to_string(), "ws://test:8080".to_string());
    balancer.register_endpoint(endpoint.clone());

    println!("Initial health: healthy");

    println!("Reporting failures:");
    for i in 1..=3 {
        balancer.report_health(&endpoint.id, false).await;
        println!("  Failure {}", i);
    }

    let stats = balancer.get_statistics().await;
    println!("Healthy endpoints: {}/{}", stats.healthy_endpoints, stats.total_endpoints);

    println!();

    Ok(())
}

/// Demonstrates failover functionality
async fn demo_failover() -> Result<()> {
    println!("--- Failover Demo ---");

    let config = FailoverConfig {
        max_retries: 3,
        initial_retry_delay: Duration::from_secs(1),
        max_retry_delay: Duration::from_secs(60),
        backoff_multiplier: 2.0,
        connection_timeout: Duration::from_secs(30),
        session_persistence: true,
    };

    let mut manager = FailoverManager::new(config);

    println!("Failover Configuration:");
    println!("  Max Retries: 3");
    println!("  Initial Delay: 1s");
    println!("  Backoff Multiplier: 2.0");
    println!("  Session Persistence: enabled");

    // Setup primary and backup endpoints
    let primary = Endpoint::new("primary".to_string(), "ws://primary:8080".to_string());
    let backup1 = Endpoint::new("backup1".to_string(), "ws://backup1:8080".to_string());
    let backup2 = Endpoint::new("backup2".to_string(), "ws://backup2:8080".to_string());

    manager.register_backup(primary.clone(), backup1.clone()).await?;
    manager.register_backup(primary.clone(), backup2.clone()).await?;

    println!("\nRegistered endpoints:");
    println!("  Primary: {} ({})", primary.id, primary.url);
    println!("  Backup 1: {} ({})", backup1.id, backup1.url);
    println!("  Backup 2: {} ({})", backup2.id, backup2.url);

    // Create session
    let mut session = SessionState::new("demo_session".to_string());
    session.add_pending_message("message1".to_string());
    session.add_pending_message("message2".to_string());
    session.metadata.insert("user_id".to_string(), "user123".to_string());

    manager.save_session(session.clone()).await;

    println!("\nSession created:");
    println!("  Session ID: {}", session.session_id);
    println!("  Pending Messages: {}", session.pending_messages.len());

    // Simulate primary failure
    println!("\n⚠️  Primary endpoint failed!");
    println!("Triggering failover...");

    let new_endpoint = manager.trigger_failover(&primary).await?;
    println!("✓ Failover completed to: {} ({})", new_endpoint.id, new_endpoint.url);

    // Restore session
    println!("\nRestoring session...");
    let restored = manager.restore_session("demo_session").await?;
    println!("✓ Session restored:");
    println!("  Session ID: {}", restored.session_id);
    println!("  Pending Messages: {}", restored.pending_messages.len());
    println!("  Metadata: {:?}", restored.metadata);

    // Show failover history
    println!("\nFailover History:");
    let history = manager.get_failover_history(5);
    for (i, event) in history.iter().enumerate() {
        println!(
            "  {}. {} → {} (Status: {:?})",
            i + 1,
            event.from_endpoint.id,
            event.to_endpoint.id,
            event.status
        );
    }

    println!();

    Ok(())
}

/// Demonstrates integrated scenario with all features
async fn demo_integrated_scenario() -> Result<()> {
    println!("--- Integrated Scenario: High-Availability File Transfer ---");

    // Setup load balancer
    let balancer_config = BalancerConfig {
        strategy: BalancingStrategy::LeastConnections,
        ..Default::default()
    };

    let mut balancer = BalancerManager::new(balancer_config);

    let server1 = Endpoint::new("server1".to_string(), "ws://server1:8080".to_string());
    let server2 = Endpoint::new("server2".to_string(), "ws://server2:8080".to_string());
    let server3 = Endpoint::new("server3".to_string(), "ws://server3:8080".to_string());

    balancer.register_endpoint(server1.clone());
    balancer.register_endpoint(server2.clone());
    balancer.register_endpoint(server3.clone());

    println!("1. Load Balancer initialized with 3 servers");

    // Setup failover
    let failover_config = FailoverConfig::default();
    let mut failover = FailoverManager::new(failover_config);

    failover.register_backup(server1.clone(), server2.clone()).await?;
    failover.register_backup(server1.clone(), server3.clone()).await?;

    println!("2. Failover configured for primary server");

    // Setup file transfer
    let transfer_manager = TransferManager::new();

    println!("3. Transfer Manager initialized");

    // Select server for transfer
    let selected_server = balancer.select_endpoint().await?;
    println!("\n✓ Selected server: {} ({})", selected_server.id, selected_server.url);

    balancer.increment_connections(&selected_server.id);

    // Create transfer
    let transfer_id = uuid::Uuid::new_v4().to_string();
    let options = TransferOptions {
        chunk_size: 2 * 1024 * 1024, // 2MB
        parallel_chunks: 8,
        compression: CompressionType::None,
        encryption: true,
        resume_support: true,
    };

    let file_size = 100 * 1024 * 1024; // 100MB

    transfer_manager
        .register_transfer(transfer_id.clone(), file_size, options)
        .await;

    println!("\n✓ Transfer registered:");
    println!("  Transfer ID: {}", transfer_id);
    println!("  File Size: 100 MB");
    println!("  Chunk Size: 2 MB");
    println!("  Parallel Chunks: 8");
    println!("  Encryption: enabled");

    // Start transfer
    transfer_manager
        .update_state(&transfer_id, TransferState::InProgress)
        .await;

    println!("\n📤 Transfer in progress...");

    // Simulate transfer with failure
    for i in 1..=5 {
        let bytes = (file_size * i) / 10;
        transfer_manager.update_progress(&transfer_id, bytes, Duration::from_millis(200)).await;

        let progress = transfer_manager.get_progress(&transfer_id).await.unwrap();
        println!(
            "  {:.1}% complete | Speed: {:.2} MB/s",
            progress.percentage(),
            progress.speed / (1024.0 * 1024.0)
        );

        // Simulate failure at 50%
        if i == 5 {
            println!("\n⚠️  Connection lost to {}!", selected_server.id);
            println!("Triggering failover...");

            let backup_server = failover.trigger_failover(&selected_server).await?;
            println!("✓ Failover to: {} ({})", backup_server.id, backup_server.url);

            balancer.decrement_connections(&selected_server.id);
            balancer.increment_connections(&backup_server.id);

            println!("Resuming transfer...");
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Complete remaining transfer
    for i in 6..=10 {
        let bytes = (file_size * i) / 10;
        transfer_manager.update_progress(&transfer_id, bytes, Duration::from_millis(200)).await;

        let progress = transfer_manager.get_progress(&transfer_id).await.unwrap();
        println!(
            "  {:.1}% complete | Speed: {:.2} MB/s",
            progress.percentage(),
            progress.speed / (1024.0 * 1024.0)
        );

        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    transfer_manager
        .update_state(&transfer_id, TransferState::Completed)
        .await;

    println!("\n✅ Transfer completed successfully!");

    // Show final statistics
    let balancer_stats = balancer.get_statistics().await;
    println!("\nFinal Statistics:");
    println!("  Active Endpoints: {}/{}", balancer_stats.healthy_endpoints, balancer_stats.total_endpoints);
    println!("  Total Requests: {}", balancer_stats.total_requests);

    for endpoint_stats in balancer_stats.endpoints {
        println!(
            "  {} - Connections: {}, Requests: {}",
            endpoint_stats.endpoint_id, endpoint_stats.active_connections, endpoint_stats.total_requests
        );
    }

    println!();

    Ok(())
}
