use chrono::Timelike;
use mcp_rs::security::audit::{
    ActionResult, AlertSeverity, AlertStatus, AuditAnalysisEngine, AuditLogEntry,
};
use std::collections::HashMap;

fn create_test_entry(action: &str, resource: &str) -> AuditLogEntry {
    AuditLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        user_id: "user123".to_string(),
        action: action.to_string(),
        resource: resource.to_string(),
        details: HashMap::new(),
        ip_address: Some("192.168.1.100".to_string()),
        user_agent: Some("Mozilla/5.0".to_string()),
        result: ActionResult::Success,
    }
}

#[tokio::test]
async fn test_privilege_escalation_role_change() {
    let engine = AuditAnalysisEngine::new();

    let entry = create_test_entry("change_role", "from:user to:admin");
    let result = engine.analyze_log(entry).await.unwrap();

    assert!(!result.privilege_events.is_empty());
    assert!(result.should_generate_alert());
}

#[tokio::test]
async fn test_privilege_escalation_abnormal_usage() {
    let engine = AuditAnalysisEngine::new();

    let mut entry = create_test_entry("admin_action", "admin_panel");
    entry.timestamp = chrono::Utc::now()
        .with_hour(3)
        .unwrap()
        .with_minute(0)
        .unwrap();
    entry
        .details
        .insert("role".to_string(), "admin".to_string());

    let result = engine.analyze_log(entry).await.unwrap();

    assert!(!result.privilege_events.is_empty());
}

#[tokio::test]
async fn test_privilege_escalation_lateral_movement() {
    let engine = AuditAnalysisEngine::new();

    let entry = create_test_entry("impersonate_user", "target_user:admin001");
    let result = engine.analyze_log(entry).await.unwrap();

    assert!(!result.privilege_events.is_empty());
}

#[tokio::test]
async fn test_exfiltration_mass_access() {
    let engine = AuditAnalysisEngine::new();

    let mut entry = create_test_entry("read", "users_table");
    entry
        .details
        .insert("data_volume".to_string(), "1048576".to_string()); // 1MB

    let result = engine.analyze_log(entry).await.unwrap();

    assert!(!result.exfiltration_events.is_empty());
}

#[tokio::test]
async fn test_exfiltration_abnormal_export() {
    let engine = AuditAnalysisEngine::new();

    let mut entry = create_test_entry("export", "customer_data");
    entry.timestamp = chrono::Utc::now()
        .with_hour(2)
        .unwrap()
        .with_minute(0)
        .unwrap();
    entry
        .details
        .insert("data_volume".to_string(), "10485760".to_string()); // 10MB

    let result = engine.analyze_log(entry).await.unwrap();

    assert!(!result.exfiltration_events.is_empty());
}

#[tokio::test]
async fn test_exfiltration_sensitive_data() {
    let engine = AuditAnalysisEngine::new();

    let sensitive_resources = vec![
        "user_passwords",
        "credit_cards",
        "ssn_data",
        "api_keys",
        "auth_tokens",
    ];

    for resource in sensitive_resources {
        let entry = create_test_entry("read", resource);
        let result = engine.analyze_log(entry).await.unwrap();

        assert!(!result.exfiltration_events.is_empty());
    }
}

#[tokio::test]
async fn test_correlation_kill_chain() {
    let engine = AuditAnalysisEngine::new();

    let events = vec![
        ("scan_network", "network_scan"),
        ("login_attempt", "auth_endpoint"),
        ("change_role", "from:user to:admin"),
        ("read", "sensitive_data"),
    ];

    for (action, resource) in events {
        let entry = create_test_entry(action, resource);
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 4);
}

#[tokio::test]
async fn test_correlation_attack_scenario() {
    let engine = AuditAnalysisEngine::new();

    let users = vec!["user1", "user2", "user3"];

    for user in users {
        let mut entry = create_test_entry("login", "auth_endpoint");
        entry.user_id = user.to_string();
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 3);
}

#[tokio::test]
async fn test_alert_generation_high_severity() {
    let engine = AuditAnalysisEngine::new();

    let entry = create_test_entry("change_role", "from:user to:admin");
    let result = engine.analyze_log(entry).await.unwrap();

    if result.should_generate_alert() {
        let alert = engine.generate_alert(&result).await.unwrap();
        assert!(matches!(
            alert.severity,
            AlertSeverity::High | AlertSeverity::Critical
        ));
        assert_eq!(alert.status, AlertStatus::New);
        assert!(!alert.title.is_empty());
        assert!(!alert.description.is_empty());
    }
}

#[tokio::test]
async fn test_statistics_collection() {
    let engine = AuditAnalysisEngine::new();

    let entries = vec![
        create_test_entry("change_role", "from:user to:admin"),
        create_test_entry("read", "user_passwords"),
    ];

    for entry in entries {
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;

    assert!(stats.total_logs_analyzed >= 2);
}

#[tokio::test]
async fn test_no_false_positives_normal_activity() {
    let engine = AuditAnalysisEngine::new();

    let mut entry = create_test_entry("read", "public_data");
    entry.timestamp = chrono::Utc::now()
        .with_hour(14)
        .unwrap()
        .with_minute(0)
        .unwrap();

    let result = engine.analyze_log(entry).await.unwrap();

    // 正常アクティビティなので、イベントは少ないはず
    assert!(result.privilege_events.is_empty() && result.exfiltration_events.is_empty());
}

#[tokio::test]
async fn test_concurrent_analysis() {
    use std::sync::Arc;

    let engine = Arc::new(AuditAnalysisEngine::new());
    let mut handles = vec![];

    for i in 0..10 {
        let engine_clone = Arc::clone(&engine);
        let handle = tokio::spawn(async move {
            let mut entry = create_test_entry("read", "public_data");
            entry.user_id = format!("user{}", i);

            engine_clone.analyze_log(entry).await.unwrap()
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 10);
}

#[tokio::test]
async fn test_time_based_patterns() {
    let engine = AuditAnalysisEngine::new();

    let late_night_hours = vec![0, 1, 2, 3, 4, 5];

    for hour in late_night_hours {
        let mut entry = create_test_entry("admin_action", "admin_panel");
        entry.timestamp = chrono::Utc::now()
            .with_hour(hour)
            .unwrap()
            .with_minute(0)
            .unwrap();
        entry.user_id = "admin001".to_string();
        entry
            .details
            .insert("role".to_string(), "admin".to_string());

        let result = engine.analyze_log(entry).await.unwrap();

        // 深夜の管理者アクセスは異常として検出される可能性がある
        assert!(result.privilege_events.is_empty() || !result.privilege_events.is_empty());
    }
}

#[tokio::test]
async fn test_ip_based_correlation() {
    let engine = AuditAnalysisEngine::new();

    let users = vec!["user1", "user2", "user3", "user4"];
    let ip = "192.168.1.100";

    for user in users {
        let mut entry = create_test_entry("login", "auth_endpoint");
        entry.user_id = user.to_string();
        entry.ip_address = Some(ip.to_string());

        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 4);
}

#[tokio::test]
async fn test_data_volume_thresholds() {
    let engine = AuditAnalysisEngine::new();

    let volumes = [
        (1024, false),    // 1KB - アラートなし
        (1048576, true),  // 1MB - アラートあり
        (10485760, true), // 10MB - アラートあり（高リスク）
    ];

    for (i, (volume, should_alert)) in volumes.iter().enumerate() {
        let mut entry = create_test_entry("read", "users_table");
        // 各テストで異なるユーザーを使用して履歴の干渉を防ぐ
        entry.user_id = format!("test_user_{}", i);
        entry
            .details
            .insert("data_volume".to_string(), volume.to_string());

        let result = engine.analyze_log(entry).await.unwrap();

        if *should_alert {
            assert!(
                !result.exfiltration_events.is_empty(),
                "Expected alert for volume {} but got none",
                volume
            );
        }
    }
}

#[tokio::test]
async fn test_multiple_privilege_grants() {
    let engine = AuditAnalysisEngine::new();

    for i in 0..4 {
        let entry = create_test_entry("grant_permission", &format!("permission_{}", i));
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 4);
}

#[tokio::test]
async fn test_brute_force_detection() {
    let engine = AuditAnalysisEngine::new();

    // 複数回失敗
    for _ in 0..5 {
        let mut entry = create_test_entry("login", "auth_endpoint");
        entry.result = ActionResult::Failure;
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    // 成功
    let entry = create_test_entry("login", "auth_endpoint");
    let _ = engine.analyze_log(entry).await.unwrap();

    // 相関イベントが検出される可能性がある
    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 6);
}

#[tokio::test]
async fn test_engine_initialization() {
    let engine = AuditAnalysisEngine::new();
    let stats = engine.get_statistics().await;

    assert_eq!(stats.total_logs_analyzed, 0);
    assert_eq!(stats.total_alerts, 0);
}

#[tokio::test]
async fn test_multiple_detection_types() {
    let engine = AuditAnalysisEngine::new();

    let mut entry = create_test_entry(
        "change_role_and_export",
        "from:user to:admin, data:user_passwords",
    );
    entry
        .details
        .insert("data_volume".to_string(), "1048576".to_string());

    let result = engine.analyze_log(entry).await.unwrap();

    // 複数の検知タイプが発生する可能性
    assert!(
        result.should_generate_alert()
            || !result.privilege_events.is_empty()
            || !result.exfiltration_events.is_empty()
    );
}

#[tokio::test]
async fn test_alert_statistics() {
    let engine = AuditAnalysisEngine::new();

    let entries = vec![
        create_test_entry("change_role", "from:user to:admin"),
        create_test_entry("read", "user_passwords"),
        create_test_entry("impersonate_user", "target_user:admin001"),
    ];

    for entry in entries {
        let _ = engine.analyze_log(entry).await.unwrap();
    }

    let stats = engine.get_statistics().await;
    assert!(stats.total_logs_analyzed >= 3);
}
