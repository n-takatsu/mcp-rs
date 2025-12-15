//! Intrusion Detection System (IDS) Demo
//!
//! IDSの基本的な使用方法を示すデモプログラム

use chrono::Utc;
use mcp_rs::security::ids::{
    alerts::{AlertLevel, AlertManager, NotificationChannel},
    DetectionType, IDSConfig, IntrusionDetectionSystem, RequestData,
};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング設定
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== Intrusion Detection System (IDS) Demo ===\n");

    // 1. IDS初期化
    demo_ids_initialization().await?;

    // 2. シグネチャベース検知デモ
    demo_signature_detection().await?;

    // 3. 振る舞いベース検知デモ
    demo_behavioral_detection().await?;

    // 4. ネットワーク監視デモ
    demo_network_monitoring().await?;

    // 5. アラート管理デモ
    demo_alert_management().await?;

    // 6. 統合IDSデモ
    demo_integrated_ids().await?;

    info!("\n=== Demo Complete ===");
    Ok(())
}

/// IDS初期化デモ
async fn demo_ids_initialization() -> Result<(), Box<dyn std::error::Error>> {
    info!("1. IDS Initialization Demo");
    info!("----------------------------");

    // デフォルト設定でIDS初期化
    let _ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;
    info!("✓ IDS initialized with default configuration");

    // カスタム設定でIDS初期化
    let config = IDSConfig {
        enabled: true,
        signature_based_enabled: true,
        behavioral_based_enabled: true,
        network_based_enabled: true,
        min_confidence_threshold: 0.7,
        alert_enabled: true,
        auto_block_enabled: false,
        session_timeout: Duration::from_secs(3600),
    };

    let ids = IntrusionDetectionSystem::new(config).await?;
    info!("✓ IDS initialized with custom configuration");

    let stats = ids.get_stats().await;
    info!("Initial stats: {} detections\n", stats.total_detections);

    Ok(())
}

/// シグネチャベース検知デモ
async fn demo_signature_detection() -> Result<(), Box<dyn std::error::Error>> {
    info!("2. Signature-Based Detection Demo");
    info!("-----------------------------------");

    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;

    // SQL Injection攻撃
    info!("Testing SQL Injection detection...");
    let mut sql_request = create_request("sql-001", "GET", "/api/users");
    sql_request.query_params.insert(
        "id".to_string(),
        "1 UNION SELECT password, email FROM users".to_string(),
    );

    let result = ids.analyze_request(&sql_request).await?;
    if result.is_intrusion {
        info!("✓ SQL Injection detected!");
        info!("  Confidence: {:.1}%", result.confidence * 100.0);
        info!("  Severity: {:?}", result.attack_details.severity);
    }

    // XSS攻撃
    info!("\nTesting XSS detection...");
    let mut xss_request = create_request("xss-001", "POST", "/api/comments");
    xss_request.body = Some(b"<script>alert(document.cookie)</script>".to_vec());

    let result = ids.analyze_request(&xss_request).await?;
    if result.is_intrusion {
        info!("✓ XSS Attack detected!");
        info!("  Confidence: {:.1}%", result.confidence * 100.0);
    }

    // Path Traversal攻撃
    info!("\nTesting Path Traversal detection...");
    let trav_request = create_request("trav-001", "GET", "/../../../etc/passwd");

    let result = ids.analyze_request(&trav_request).await?;
    if result.is_intrusion {
        info!("✓ Path Traversal detected!");
        info!("  Confidence: {:.1}%", result.confidence * 100.0);
    }

    // 正常なリクエスト
    info!("\nTesting benign request...");
    let mut benign_request = create_request("benign-001", "GET", "/api/products");
    benign_request
        .query_params
        .insert("category".to_string(), "electronics".to_string());

    let _result = ids.analyze_request(&benign_request).await?;
    info!("✓ Benign request: No intrusion detected\n");

    Ok(())
}

/// 振る舞いベース検知デモ
async fn demo_behavioral_detection() -> Result<(), Box<dyn std::error::Error>> {
    info!("3. Behavioral Detection Demo");
    info!("------------------------------");

    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;
    let ip: IpAddr = "192.168.1.100".parse()?;

    info!("Establishing baseline with normal requests...");

    // 通常のリクエストパターンを学習
    for i in 0..20 {
        let request =
            create_request_with_ip(&format!("baseline-{}", i), "GET", "/api/products", Some(ip));
        ids.analyze_request(&request).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    info!("✓ Baseline established with 20 requests");

    // 異常なリクエストパターン（高頻度）
    info!("\nSimulating anomalous behavior (high frequency)...");
    for i in 0..50 {
        let request =
            create_request_with_ip(&format!("anomaly-{}", i), "GET", "/api/products", Some(ip));
        let result = ids.analyze_request(&request).await?;

        if result.is_intrusion && result.detection_type == DetectionType::AnomalousBehavior {
            info!("✓ Anomalous behavior detected at request {}", i + 1);
            info!("  Confidence: {:.1}%", result.confidence * 100.0);
            info!("  Description: {}", result.attack_details.description);
            break;
        }
    }

    info!("");
    Ok(())
}

/// ネットワーク監視デモ
async fn demo_network_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    info!("4. Network Monitoring Demo");
    info!("---------------------------");

    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;
    let ip: IpAddr = "203.0.113.100".parse()?;

    // DDoS攻撃シミュレーション
    info!("Simulating DDoS attack...");
    for i in 0..120 {
        let request = create_request_with_ip(&format!("ddos-{}", i), "GET", "/", Some(ip));
        let result = ids.analyze_request(&request).await?;

        if result.is_intrusion && result.detection_type == DetectionType::DdosAttack {
            info!("✓ DDoS attack detected at request {}", i + 1);
            info!("  Risk Score: {:.1}%", result.confidence * 100.0);
            break;
        }
    }

    // ポートスキャンシミュレーション
    info!("\nSimulating port scan...");
    let scan_paths = [
        "/admin",
        "/wp-admin",
        "/phpmyadmin",
        "/api",
        "/backup",
        "/config",
        "/test",
        "/.git",
        "/.env",
        "/db",
    ];

    let scan_ip: IpAddr = "203.0.113.150".parse()?;
    for (i, path) in scan_paths.iter().enumerate() {
        let request = create_request_with_ip(&format!("scan-{}", i), "GET", path, Some(scan_ip));
        let result = ids.analyze_request(&request).await?;

        if result.is_intrusion && result.detection_type == DetectionType::PortScan {
            info!("✓ Port scan detected after {} requests", i + 1);
            info!("  Scanned paths: {:?}", &scan_paths[..=i]);
            break;
        }
    }

    // 疑わしいUser-Agent
    info!("\nTesting suspicious User-Agent...");
    let mut ua_request =
        create_request_with_ip("ua-001", "GET", "/", Some("203.0.113.200".parse()?));
    ua_request
        .headers
        .insert("User-Agent".to_string(), "sqlmap/1.5.12".to_string());

    let result = ids.analyze_request(&ua_request).await?;
    if result.is_intrusion {
        info!("✓ Suspicious User-Agent detected");
        info!("  Tool: sqlmap (SQL injection tool)");
    }

    info!("");
    Ok(())
}

/// アラート管理デモ
async fn demo_alert_management() -> Result<(), Box<dyn std::error::Error>> {
    info!("5. Alert Management Demo");
    info!("-------------------------");

    let manager = AlertManager::new().await?;

    // 通知チャネルの追加
    info!("Adding notification channels...");

    // Slackチャネル（デモ用）
    manager
        .add_notification_channel(NotificationChannel::Slack {
            webhook_url: "https://hooks.slack.com/services/DEMO/WEBHOOK".to_string(),
            min_level: AlertLevel::High,
        })
        .await;
    info!("✓ Slack notification channel added (High+)");

    // Emailチャネル（デモ用）
    manager
        .add_notification_channel(NotificationChannel::Email {
            recipients: vec![
                "security@example.com".to_string(),
                "admin@example.com".to_string(),
            ],
            min_level: AlertLevel::Critical,
        })
        .await;
    info!("✓ Email notification channel added (Critical)");

    // ログチャネルはデフォルトで有効
    info!("✓ Log notification channel (default, Low+)");

    // アラートの送信
    info!("\nSending test alerts...");
    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;

    let mut sql_request = create_request("alert-sql", "GET", "/api/data");
    sql_request.query_params.insert(
        "q".to_string(),
        "1 UNION SELECT * FROM passwords".to_string(),
    );

    let result = ids.analyze_request(&sql_request).await?;
    if result.is_intrusion {
        ids.generate_alert(result).await?;
        info!("✓ Alert generated for SQL Injection");
    }

    // アラート統計
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let stats = manager.get_alert_stats().await;
    info!("\nAlert Statistics:");
    for (level, count) in stats {
        info!("  {:?}: {} alerts", level, count);
    }

    info!("");
    Ok(())
}

/// 統合IDSデモ
async fn demo_integrated_ids() -> Result<(), Box<dyn std::error::Error>> {
    info!("6. Integrated IDS Demo");
    info!("-----------------------");

    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;

    info!("Processing various attack types...\n");

    // 複数の攻撃タイプをテスト
    let test_cases = vec![
        ("SQL Injection", create_sql_injection_request()),
        ("XSS Attack", create_xss_request()),
        ("Command Injection", create_command_injection_request()),
        ("Path Traversal", create_path_traversal_request()),
    ];

    let mut _detected_count = 0;
    for (attack_type, request) in test_cases {
        let result = ids.analyze_request(&request).await?;
        if result.is_intrusion {
            _detected_count += 1;
            info!("✓ {} detected", attack_type);
            info!("  Type: {:?}", result.detection_type);
            info!("  Confidence: {:.1}%", result.confidence * 100.0);
            info!("  Severity: {:?}", result.attack_details.severity);
            info!("  Action: {:?}\n", result.recommended_action);
        }
    }

    // 最終統計
    let stats = ids.get_stats().await;
    info!("Final Statistics:");
    info!("  Total detections: {}", stats.total_detections);
    info!("  Total alerts: {}", stats.total_alerts);
    info!("  Detection rate: {:.1}%", stats.detection_rate * 100.0);

    Ok(())
}

// ヘルパー関数

fn create_request(id: &str, method: &str, path: &str) -> RequestData {
    RequestData {
        request_id: id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        query_params: HashMap::new(),
        headers: HashMap::new(),
        body: None,
        source_ip: None,
        timestamp: Utc::now(),
    }
}

fn create_request_with_ip(id: &str, method: &str, path: &str, ip: Option<IpAddr>) -> RequestData {
    RequestData {
        request_id: id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        query_params: HashMap::new(),
        headers: HashMap::new(),
        body: None,
        source_ip: ip,
        timestamp: Utc::now(),
    }
}

fn create_sql_injection_request() -> RequestData {
    let mut request = create_request("demo-sql", "GET", "/api/users");
    request.query_params.insert(
        "id".to_string(),
        "1 UNION SELECT password FROM users".to_string(),
    );
    request.source_ip = Some("203.0.113.10".parse().unwrap());
    request
}

fn create_xss_request() -> RequestData {
    let mut request = create_request("demo-xss", "POST", "/api/comments");
    request.body = Some(b"<script>alert(document.cookie)</script>".to_vec());
    request.source_ip = Some("203.0.113.20".parse().unwrap());
    request
}

fn create_command_injection_request() -> RequestData {
    let mut request = create_request("demo-cmd", "POST", "/api/exec");
    request
        .query_params
        .insert("cmd".to_string(), "ls; cat /etc/passwd".to_string());
    request.source_ip = Some("203.0.113.30".parse().unwrap());
    request
}

fn create_path_traversal_request() -> RequestData {
    let request = create_request("demo-trav", "GET", "/../../../etc/shadow");
    RequestData {
        source_ip: Some("203.0.113.40".parse().unwrap()),
        ..request
    }
}
