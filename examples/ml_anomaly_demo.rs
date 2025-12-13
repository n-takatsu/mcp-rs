//! ML Anomaly Detection Demo
//!
//! 機械学習ベースの異常検知システムのデモプログラム
//!
//! 実行方法:
//! ```bash
//! cargo run --example ml_anomaly_demo --features ml-anomaly-detection
//! ```

use chrono::Utc;
use mcp_rs::security::ids::ml::{MLAnomalyDetector, MLConfig};
use mcp_rs::security::ids::RequestData;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング初期化
    env_logger::init();

    println!("=== ML Anomaly Detection Demo ===\n");

    // 1. ML検知器の初期化
    println!("1. Initializing ML Anomaly Detector...");
    let config = MLConfig {
        anomaly_threshold: 0.7,
        min_training_samples: 50,
        online_learning: true,
        ..Default::default()
    };
    let detector = MLAnomalyDetector::new(config).await?;
    println!("✓ Detector initialized\n");

    // 2. 正常なトレーニングデータを生成
    println!("2. Generating training data...");
    let mut training_data = Vec::new();

    // 正常なAPIリクエスト
    for i in 0..100 {
        training_data.push(create_request(
            &format!("train-{}", i),
            "GET",
            &format!("/api/users/{}", i % 10),
            HashMap::new(),
        ));
    }

    println!("✓ Generated {} training samples\n", training_data.len());

    // 3. モデルをトレーニング
    println!("3. Training model...");
    detector.train(&training_data).await?;
    println!("✓ Training completed\n");

    // 4. 正常リクエストのテスト
    println!("4. Testing normal requests:");
    test_normal_requests(&detector).await?;

    // 5. 異常リクエストのテスト
    println!("\n5. Testing anomalous requests:");
    test_anomalous_requests(&detector).await?;

    // 6. 統計情報の表示
    println!("\n6. Model Statistics:");
    let stats = detector.get_stats().await;
    println!("  Training samples: {}", stats.training_samples);
    println!("  Model version: {}", stats.model_version);
    println!("  Last updated: {}", stats.last_updated);

    println!("\n=== Demo completed successfully ===");
    Ok(())
}

/// 正常リクエストのテスト
async fn test_normal_requests(
    detector: &MLAnomalyDetector,
) -> Result<(), Box<dyn std::error::Error>> {
    let requests = vec![
        create_request("normal-1", "GET", "/api/users/5", HashMap::new()),
        create_request("normal-2", "GET", "/api/users/8", HashMap::new()),
        create_request("normal-3", "GET", "/api/users/2", HashMap::new()),
    ];

    for request in requests {
        let result = detector.detect(&request).await?;
        println!(
            "  {} [{}] - Score: {:.3}, Anomaly: {}",
            request.path, request.request_id, result.anomaly_score, result.is_anomaly
        );
    }

    Ok(())
}

/// 異常リクエストのテスト
async fn test_anomalous_requests(
    detector: &MLAnomalyDetector,
) -> Result<(), Box<dyn std::error::Error>> {
    let requests = vec![
        // SQL Injection
        create_request(
            "sqli",
            "GET",
            "/api/users?id=1' UNION SELECT * FROM passwords--",
            HashMap::new(),
        ),
        // XSS Attack
        create_request(
            "xss",
            "POST",
            "/api/comments",
            vec![(
                "comment".to_string(),
                "<script>alert('XSS')</script>".to_string(),
            )]
            .into_iter()
            .collect(),
        ),
        // Path Traversal
        create_request(
            "traversal",
            "GET",
            "/api/files?path=../../../../etc/passwd",
            HashMap::new(),
        ),
        // 異常に長いパス
        create_request(
            "long-path",
            "GET",
            &format!("/api/{}", "a".repeat(1000)),
            HashMap::new(),
        ),
    ];

    for request in requests {
        let result = detector.detect(&request).await?;
        println!(
            "  {} [{}] - Score: {:.3}, Anomaly: {}",
            request.path.chars().take(50).collect::<String>(),
            request.request_id,
            result.anomaly_score,
            result.is_anomaly
        );

        if result.is_anomaly {
            println!("    Explanation: {}", result.explanation);
        }
    }

    Ok(())
}

/// テスト用リクエストを作成
fn create_request(
    id: &str,
    method: &str,
    path: &str,
    query_params: HashMap<String, String>,
) -> RequestData {
    RequestData {
        request_id: id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        query_params,
        headers: HashMap::new(),
        body: None,
        source_ip: Some("192.168.1.100".parse().unwrap()),
        timestamp: Utc::now(),
    }
}
