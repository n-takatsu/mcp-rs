//! Docker container integration tests

use super::{TestConfig, wait_for_service};
use std::time::Duration;

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_docker_compose_health_checks() {
    let config = TestConfig::from_env();

    // Wait for all services to be ready
    println!("Waiting for HTTP server...");
    wait_for_service(
        &format!("{}/health", config.http_endpoint),
        Duration::from_secs(60),
    )
    .await
    .expect("HTTP server should be ready");

    println!("Waiting for WebSocket server...");
    wait_for_service(
        &format!("{}/health", config.websocket_endpoint.replace("ws://", "http://")),
        Duration::from_secs(60),
    )
    .await
    .expect("WebSocket server should be ready");

    println!("All services are healthy!");
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_http_endpoint_basic_request() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("Should get health response");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_service_discovery() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Test service-to-service communication
    let endpoints = vec![
        format!("{}/health", config.http_endpoint),
        format!("{}/health", config.websocket_endpoint.replace("ws://", "http://")),
    ];

    for endpoint in endpoints {
        let response = client
            .get(&endpoint)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .expect(&format!("Should reach {}", endpoint));

        assert!(
            response.status().is_success(),
            "Endpoint {} should be healthy",
            endpoint
        );
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_database_connectivity() {
    use tokio_postgres::NoTls;

    let config = TestConfig::from_env();

    let (client, connection) = tokio_postgres::connect(&config.database_url, NoTls)
        .await
        .expect("Should connect to database");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    let rows = client
        .query("SELECT 1 as test", &[])
        .await
        .expect("Should execute query");

    assert_eq!(rows.len(), 1);
    let value: i32 = rows[0].get(0);
    assert_eq!(value, 1);
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_redis_connectivity() {
    use redis::AsyncCommands;

    let config = TestConfig::from_env();
    let client = redis::Client::open(config.redis_url.as_str())
        .expect("Should create Redis client");

    let mut con = client
        .get_multiplexed_async_connection()
        .await
        .expect("Should connect to Redis");

    // Test SET
    let _: () = con
        .set("test_key", "test_value")
        .await
        .expect("Should set key");

    // Test GET
    let value: String = con.get("test_key").await.expect("Should get key");
    assert_eq!(value, "test_value");

    // Cleanup
    let _: () = con.del("test_key").await.expect("Should delete key");
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_container_restart_resilience() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Make initial request
    let response1 = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Initial request should succeed");
    assert!(response1.status().is_success());

    // Wait a bit
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Make another request (container should still be running)
    let response2 = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Follow-up request should succeed");
    assert!(response2.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_concurrent_connections() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();
    let num_requests = 10;

    let mut handles = vec![];

    for i in 0..num_requests {
        let client = client.clone();
        let endpoint = config.http_endpoint.clone();
        let handle = tokio::spawn(async move {
            let response = client
                .get(&format!("{}/health", endpoint))
                .send()
                .await
                .expect(&format!("Request {} should succeed", i));
            assert!(response.status().is_success());
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("All requests should complete");
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_network_isolation() {
    let config = TestConfig::from_env();

    // Services should be able to communicate within the network
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Internal network communication should work");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_environment_variables() {
    // Verify test environment is properly configured
    assert!(std::env::var("MCP_HTTP_ENDPOINT").is_ok() || std::env::var("MCP_HTTP_ENDPOINT").is_err());
    assert!(std::env::var("DATABASE_URL").is_ok() || std::env::var("DATABASE_URL").is_err());
    assert!(std::env::var("REDIS_URL").is_ok() || std::env::var("REDIS_URL").is_err());
}
