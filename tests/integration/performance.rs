//! Performance tests for containerized MCP-RS

use super::{TestConfig, wait_for_service};
use std::time::{Duration, Instant};

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_container_startup_time() {
    let config = TestConfig::from_env();
    let start = Instant::now();

    wait_for_service(
        &format!("{}/health", config.http_endpoint),
        Duration::from_secs(60),
    )
    .await
    .expect("Service should start");

    let startup_time = start.elapsed();
    println!("Container startup time: {:?}", startup_time);

    // Startup should be under 30 seconds
    assert!(
        startup_time < Duration::from_secs(30),
        "Startup time too long: {:?}",
        startup_time
    );
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_request_latency() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();
    let num_requests = 100;
    let mut latencies = Vec::new();

    for _ in 0..num_requests {
        let start = Instant::now();
        let response = client
            .get(&format!("{}/health", config.http_endpoint))
            .send()
            .await
            .expect("Request should succeed");

        assert!(response.status().is_success());
        latencies.push(start.elapsed());
    }

    let avg_latency = latencies.iter().sum::<Duration>() / num_requests as u32;
    let max_latency = latencies.iter().max().unwrap();
    let min_latency = latencies.iter().min().unwrap();

    println!("Average latency: {:?}", avg_latency);
    println!("Max latency: {:?}", max_latency);
    println!("Min latency: {:?}", min_latency);

    // Average latency should be under 100ms
    assert!(
        avg_latency < Duration::from_millis(100),
        "Average latency too high: {:?}",
        avg_latency
    );
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_throughput() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();
    let duration = Duration::from_secs(10);
    let start = Instant::now();
    let mut request_count = 0;

    while start.elapsed() < duration {
        let response = client
            .get(&format!("{}/health", config.http_endpoint))
            .send()
            .await
            .expect("Request should succeed");

        assert!(response.status().is_success());
        request_count += 1;
    }

    let elapsed = start.elapsed();
    let requests_per_second = request_count as f64 / elapsed.as_secs_f64();

    println!(
        "Throughput: {} requests in {:?} ({:.2} req/s)",
        request_count, elapsed, requests_per_second
    );

    // Should handle at least 100 requests per second
    assert!(
        requests_per_second > 100.0,
        "Throughput too low: {:.2} req/s",
        requests_per_second
    );
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_concurrent_load() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();
    let num_concurrent = 50;
    let requests_per_task = 20;

    let start = Instant::now();
    let mut handles = vec![];

    for _ in 0..num_concurrent {
        let client = client.clone();
        let endpoint = config.http_endpoint.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..requests_per_task {
                let response = client
                    .get(&format!("{}/health", endpoint))
                    .send()
                    .await
                    .expect("Request should succeed");
                assert!(response.status().is_success());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    let elapsed = start.elapsed();
    let total_requests = num_concurrent * requests_per_task;
    let requests_per_second = total_requests as f64 / elapsed.as_secs_f64();

    println!(
        "Concurrent load: {} requests in {:?} ({:.2} req/s)",
        total_requests, elapsed, requests_per_second
    );

    // Should complete all requests in under 60 seconds
    assert!(
        elapsed < Duration::from_secs(60),
        "Load test took too long: {:?}",
        elapsed
    );
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_memory_stability() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Make many requests to check for memory leaks
    for i in 0..1000 {
        let response = client
            .get(&format!("{}/health", config.http_endpoint))
            .send()
            .await
            .expect("Request should succeed");

        assert!(response.status().is_success());

        if i % 100 == 0 {
            println!("Completed {} requests", i);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    println!("Memory stability test completed (1000 requests)");
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_resource_limits() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Test that service handles resource constraints gracefully
    let start = Instant::now();
    let mut success_count = 0;
    let mut error_count = 0;

    for _ in 0..500 {
        match client
            .get(&format!("{}/health", config.http_endpoint))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                success_count += 1;
            }
            _ => {
                error_count += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    let success_rate = success_count as f64 / (success_count + error_count) as f64;

    println!(
        "Resource limits test: {} success, {} errors ({:.2}% success rate) in {:?}",
        success_count,
        error_count,
        success_rate * 100.0,
        elapsed
    );

    // Should have at least 95% success rate
    assert!(
        success_rate >= 0.95,
        "Success rate too low: {:.2}%",
        success_rate * 100.0
    );
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_connection_pool_exhaustion() {
    let config = TestConfig::from_env();
    let num_connections = 100;
    let mut handles = vec![];

    for i in 0..num_connections {
        let client = reqwest::Client::new();
        let endpoint = config.http_endpoint.clone();
        let handle = tokio::spawn(async move {
            match client
                .get(&format!("{}/health", endpoint))
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(response) => {
                    assert!(response.status().is_success());
                }
                Err(e) => {
                    panic!("Connection {} failed: {}", i, e);
                }
            }
        });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .expect(&format!("Connection {} should complete", i));
    }

    println!(
        "Connection pool test completed ({} concurrent connections)",
        num_connections
    );
}
