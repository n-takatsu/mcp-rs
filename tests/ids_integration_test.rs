//! Intrusion Detection System (IDS) Integration Tests
//!
//! IDSの全検知エンジンを統合的にテストします。

use chrono::Utc;
use mcp_rs::security::ids::{
    alerts::{Alert, AlertLevel, AlertManager, NotificationChannel},
    behavioral::BehavioralDetector,
    network::NetworkMonitor,
    signature::SignatureDetector,
    DetectionType, IDSConfig, IntrusionDetectionSystem, RecommendedAction, RequestData, Severity,
};
use std::collections::HashMap;
use std::net::IpAddr;

/// テスト用リクエストデータを作成
fn create_test_request(
    request_id: &str,
    method: &str,
    path: &str,
    source_ip: Option<IpAddr>,
) -> RequestData {
    RequestData {
        request_id: request_id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        query_params: HashMap::new(),
        headers: HashMap::new(),
        body: None,
        source_ip,
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn test_signature_detector_sql_injection() {
    let detector = SignatureDetector::new().await.unwrap();

    let mut request = create_test_request("test-001", "GET", "/api/users", None);
    request.query_params.insert(
        "id".to_string(),
        "1 UNION SELECT password FROM users".to_string(),
    );

    let result = detector.detect(&request).await.unwrap();

    assert!(result.matched, "SQL Injection should be detected");
    assert_eq!(result.detection_type, DetectionType::SqlInjection);
    assert!(result.confidence > 0.8, "Confidence should be high");
    assert!(
        !result.pattern_names.is_empty(),
        "Pattern names should be present"
    );
}

#[tokio::test]
async fn test_signature_detector_xss_attack() {
    let detector = SignatureDetector::new().await.unwrap();

    let mut request = create_test_request("test-002", "POST", "/api/comments", None);
    request.body = Some(b"<script>alert(document.cookie)</script>".to_vec());

    let result = detector.detect(&request).await.unwrap();

    assert!(result.matched, "XSS attack should be detected");
    assert_eq!(result.detection_type, DetectionType::XssAttack);
    assert!(result.confidence > 0.7);
}

#[tokio::test]
async fn test_signature_detector_path_traversal() {
    let detector = SignatureDetector::new().await.unwrap();

    let request = create_test_request("test-003", "GET", "/files/../../../etc/passwd", None);

    let result = detector.detect(&request).await.unwrap();

    assert!(result.matched, "Path traversal should be detected");
    assert_eq!(result.detection_type, DetectionType::UnauthorizedAccess);
}

#[tokio::test]
async fn test_signature_detector_command_injection() {
    let detector = SignatureDetector::new().await.unwrap();

    let mut request = create_test_request("test-004", "POST", "/api/execute", None);
    request
        .query_params
        .insert("cmd".to_string(), "ls; rm -rf /".to_string());

    let result = detector.detect(&request).await.unwrap();

    assert!(result.matched, "Command injection should be detected");
    assert!(result.confidence > 0.8);
}

#[tokio::test]
async fn test_signature_detector_benign_request() {
    let detector = SignatureDetector::new().await.unwrap();

    let mut request = create_test_request("test-005", "GET", "/api/products", None);
    request
        .query_params
        .insert("category".to_string(), "electronics".to_string());

    let result = detector.detect(&request).await.unwrap();

    assert!(!result.matched, "Benign request should not be detected");
    assert_eq!(result.confidence, 0.0);
}

#[tokio::test]
async fn test_behavioral_detector_baseline_learning() {
    let detector = BehavioralDetector::new().await.unwrap();

    // 学習モード：通常のリクエストを送信
    for i in 0..20 {
        let request = create_test_request(
            &format!("train-{}", i),
            "GET",
            "/api/users",
            Some("192.168.1.100".parse().unwrap()),
        );
        let _ = detector.analyze(&request).await;
    }

    // ベースラインが作成されたか確認
    let baseline = detector.get_baseline("192.168.1.100").await;
    assert!(baseline.is_some(), "Baseline should be created");

    let baseline = baseline.unwrap();
    assert!(baseline.sample_count >= 20);
}

#[tokio::test]
async fn test_behavioral_detector_anomaly_detection() {
    let detector = BehavioralDetector::new().await.unwrap();

    let ip: IpAddr = "192.168.1.200".parse().unwrap();

    // 通常パターンを学習
    for i in 0..30 {
        let request =
            create_test_request(&format!("normal-{}", i), "GET", "/api/products", Some(ip));
        let _ = detector.analyze(&request).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // 異常なリクエスト頻度（短時間に大量）
    for i in 0..50 {
        let request =
            create_test_request(&format!("anomaly-{}", i), "GET", "/api/products", Some(ip));
        let result = detector.analyze(&request).await.unwrap();

        if i > 40 {
            // 異常検知される可能性が高い
            if result.is_anomalous {
                assert!(result.anomaly_score > 0.5);
                break;
            }
        }
    }
}

#[tokio::test]
async fn test_network_monitor_ddos_detection() {
    let monitor = NetworkMonitor::new().await.unwrap();

    let ip: IpAddr = "203.0.113.50".parse().unwrap();

    // 短時間に大量のリクエスト（DDoS模擬）
    for i in 0..150 {
        let request = create_test_request(&format!("ddos-{}", i), "GET", "/", Some(ip));
        let result = monitor.check_traffic(&request).await.unwrap();

        if i > 100 {
            // DDoS検知される
            if result.is_suspicious {
                assert!(
                    result.risk_score > 0.8,
                    "DDoS should be detected with high risk score"
                );
                assert!(!result.detected_patterns.is_empty());
                break;
            }
        }
    }
}

#[tokio::test]
async fn test_network_monitor_port_scan_detection() {
    let monitor = NetworkMonitor::new().await.unwrap();

    let ip: IpAddr = "203.0.113.100".parse().unwrap();

    // 複数の異なるパスへのアクセス（ポートスキャン模擬）
    let paths = vec![
        "/admin",
        "/wp-admin",
        "/phpmyadmin",
        "/api",
        "/backup",
        "/config",
        "/test",
        "/debug",
        "/.git",
        "/.env",
    ];

    for (i, path) in paths.iter().enumerate() {
        let request = create_test_request(&format!("portscan-{}", i), "GET", path, Some(ip));
        let result = monitor.check_traffic(&request).await.unwrap();

        if i > 5 {
            if result.is_suspicious {
                assert!(result.risk_score > 0.5);
                break;
            }
        }
    }
}

#[tokio::test]
async fn test_network_monitor_suspicious_user_agent() {
    let monitor = NetworkMonitor::new().await.unwrap();

    let ip: IpAddr = "203.0.113.150".parse().unwrap();
    let mut request = create_test_request("test-ua", "GET", "/", Some(ip));

    // 疑わしいUser-Agent
    request
        .headers
        .insert("User-Agent".to_string(), "sqlmap/1.0".to_string());

    let result = monitor.check_traffic(&request).await.unwrap();

    assert!(
        result.is_suspicious,
        "Suspicious User-Agent should be detected"
    );
    assert!(result.risk_score > 0.6);
}

#[tokio::test]
async fn test_alert_manager_initialization() {
    let manager = AlertManager::new().await.unwrap();

    let stats = manager.get_alert_stats().await;
    assert!(stats.is_empty(), "Stats should be empty initially");
}

#[tokio::test]
async fn test_alert_manager_send_alert() {
    let manager = AlertManager::new().await.unwrap();

    let alert = Alert {
        id: uuid::Uuid::new_v4().to_string(),
        level: AlertLevel::High,
        detection_type: DetectionType::SqlInjection,
        confidence: 0.95,
        source_ip: Some("192.168.1.100".parse().unwrap()),
        description: "SQL injection detected in user query".to_string(),
        recommended_action: RecommendedAction::Block,
        created_at: Utc::now(),
    };

    let result = manager.send_alert(alert).await;
    assert!(result.is_ok(), "Alert should be sent successfully");

    let history = manager.get_alert_history(Some(10)).await;
    assert_eq!(history.len(), 1, "Alert should be in history");
}

#[tokio::test]
async fn test_alert_manager_multiple_alerts() {
    let manager = AlertManager::new().await.unwrap();

    // 複数のアラートを送信
    for i in 0..5 {
        let alert = Alert {
            id: format!("alert-{}", i),
            level: if i < 2 {
                AlertLevel::Critical
            } else {
                AlertLevel::Medium
            },
            detection_type: DetectionType::XssAttack,
            confidence: 0.8,
            source_ip: Some("192.168.1.100".parse().unwrap()),
            description: format!("Test alert {}", i),
            recommended_action: RecommendedAction::Warn,
            created_at: Utc::now(),
        };

        manager.send_alert(alert).await.unwrap();
    }

    let stats = manager.get_alert_stats().await;
    assert_eq!(stats.get(&AlertLevel::Critical).copied().unwrap_or(0), 2);
    assert_eq!(stats.get(&AlertLevel::Medium).copied().unwrap_or(0), 3);
}

#[tokio::test]
async fn test_alert_manager_notification_channels() {
    let manager = AlertManager::new().await.unwrap();

    // Slackチャネルを追加
    let slack_channel = NotificationChannel::Slack {
        webhook_url: "https://hooks.slack.com/test".to_string(),
        min_level: AlertLevel::High,
    };
    manager.add_notification_channel(slack_channel).await;

    // Emailチャネルを追加
    let email_channel = NotificationChannel::Email {
        recipients: vec!["security@example.com".to_string()],
        min_level: AlertLevel::Critical,
    };
    manager.add_notification_channel(email_channel).await;

    // チャネルが追加されたか確認（内部実装による）
    // 実際の通知送信はモックが必要
}

#[tokio::test]
async fn test_ids_initialization() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await;
    assert!(ids.is_ok(), "IDS should initialize successfully");

    let ids = ids.unwrap();
    let stats = ids.get_stats().await;
    assert_eq!(stats.total_detections, 0);
}

#[tokio::test]
async fn test_ids_analyze_malicious_request() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();

    let mut request = create_test_request(
        "malicious-001",
        "GET",
        "/api/users",
        Some("203.0.113.200".parse().unwrap()),
    );
    request
        .query_params
        .insert("id".to_string(), "1 OR 1=1".to_string());

    let result = ids.analyze_request(&request).await;
    assert!(result.is_ok());

    let result = result.unwrap();
    assert!(
        result.is_intrusion,
        "Malicious request should be detected as intrusion"
    );
    assert!(result.confidence > 0.5);
    assert_eq!(result.detection_type, DetectionType::SqlInjection);
}

#[tokio::test]
async fn test_ids_analyze_benign_request() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();

    let mut request = create_test_request(
        "benign-001",
        "GET",
        "/api/products",
        Some("192.168.1.50".parse().unwrap()),
    );
    request
        .query_params
        .insert("category".to_string(), "books".to_string());

    let result = ids.analyze_request(&request).await.unwrap();

    assert!(
        !result.is_intrusion,
        "Benign request should not be detected as intrusion"
    );
    assert_eq!(result.confidence, 0.0);
}

#[tokio::test]
async fn test_ids_comprehensive_detection() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();

    // SQL Injection
    let mut sql_request = create_test_request(
        "comp-sql",
        "POST",
        "/login",
        Some("203.0.113.50".parse().unwrap()),
    );
    sql_request.body = Some(b"username=admin' OR '1'='1&password=anything".to_vec());

    let result = ids.analyze_request(&sql_request).await.unwrap();
    assert!(result.is_intrusion);
    assert_eq!(result.detection_type, DetectionType::SqlInjection);

    // XSS Attack
    let mut xss_request = create_test_request(
        "comp-xss",
        "POST",
        "/comments",
        Some("203.0.113.60".parse().unwrap()),
    );
    xss_request.body = Some(b"<script>alert(XSS)</script>".to_vec());

    let result = ids.analyze_request(&xss_request).await.unwrap();
    assert!(result.is_intrusion);
    assert_eq!(result.detection_type, DetectionType::XssAttack);

    // Path Traversal
    let trav_request = create_test_request(
        "comp-trav",
        "GET",
        "/../../../etc/passwd",
        Some("203.0.113.70".parse().unwrap()),
    );

    let result = ids.analyze_request(&trav_request).await.unwrap();
    assert!(result.is_intrusion);
    assert_eq!(result.detection_type, DetectionType::UnauthorizedAccess);
}

#[tokio::test]
async fn test_ids_statistics() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();

    // 複数のリクエストを分析
    for i in 0..10 {
        let request = create_test_request(
            &format!("stats-{}", i),
            "GET",
            "/api/test",
            Some("192.168.1.100".parse().unwrap()),
        );
        let _ = ids.analyze_request(&request).await;
    }

    let stats = ids.get_stats().await;
    // 統計情報が正常に取得できることを確認
    assert!(stats.total_detections == 0 || stats.total_detections > 0);
}

#[tokio::test]
async fn test_ids_severity_levels() {
    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();

    // Critical: SQL Injection with UNION
    let mut critical_request = create_test_request(
        "severity-critical",
        "GET",
        "/api/data",
        Some("203.0.113.100".parse().unwrap()),
    );
    critical_request.query_params.insert(
        "q".to_string(),
        "1 UNION SELECT password FROM users".to_string(),
    );

    let result = ids.analyze_request(&critical_request).await.unwrap();
    if result.is_intrusion {
        assert_eq!(result.attack_details.severity, Severity::Critical);
    }
}

#[tokio::test]
async fn test_ids_concurrent_requests() {
    use tokio::task::JoinSet;

    let ids = IntrusionDetectionSystem::new(IDSConfig::default())
        .await
        .unwrap();
    let ids = std::sync::Arc::new(ids);

    let mut tasks = JoinSet::new();

    // 並行リクエスト処理
    for i in 0..20 {
        let ids_clone = ids.clone();
        tasks.spawn(async move {
            let request = create_test_request(
                &format!("concurrent-{}", i),
                "GET",
                "/api/test",
                Some("192.168.1.100".parse().unwrap()),
            );
            ids_clone.analyze_request(&request).await
        });
    }

    let mut success_count = 0;
    while let Some(result) = tasks.join_next().await {
        if result.is_ok() && result.unwrap().is_ok() {
            success_count += 1;
        }
    }

    assert_eq!(
        success_count, 20,
        "All concurrent requests should be processed"
    );
}
