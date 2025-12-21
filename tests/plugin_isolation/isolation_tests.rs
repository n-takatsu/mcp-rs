//! プラグイン隔離システムの統合テスト

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::*;

#[tokio::test]
async fn test_inter_plugin_communication_with_rules() -> Result<(), McpError> {
    // プラグイン間通信コントローラーを作成
    let config = InterPluginCommConfig {
        default_rate_limit: 10,
        max_queue_size: 100,
        message_timeout_seconds: 5,
        max_retries: 3,
        history_retention_seconds: 3600,
    };

    let comm_controller = InterPluginCommunicationController::new(config).await?;

    let plugin_a = Uuid::new_v4();
    let plugin_b = Uuid::new_v4();
    let plugin_c = Uuid::new_v4();

    // A -> B の通信を許可
    let rule_ab = CommunicationRule {
        source_plugin: plugin_a,
        target_plugin: plugin_b,
        allowed_message_types: vec!["data".to_string(), "command".to_string()],
        priority: 1,
    };
    comm_controller.add_rule(rule_ab).await?;

    // B -> C の通信を許可
    let rule_bc = CommunicationRule {
        source_plugin: plugin_b,
        target_plugin: plugin_c,
        allowed_message_types: vec!["result".to_string()],
        priority: 1,
    };
    comm_controller.add_rule(rule_bc).await?;

    // A -> B へメッセージ送信（成功するはず）
    let msg_id = comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "data".to_string(),
            vec![1, 2, 3, 4],
            1,
        )
        .await?;

    assert_ne!(msg_id, Uuid::nil());

    // メッセージを受信
    let received = comm_controller.receive_message(plugin_b).await?;
    assert!(received.is_some());

    let message = received.unwrap();
    assert_eq!(message.from_plugin, plugin_a);
    assert_eq!(message.to_plugin, plugin_b);
    assert_eq!(message.message_type, "data");

    // A -> C への直接通信は失敗するはず（ルールがない）
    let result = comm_controller
        .send_message(
            plugin_a,
            plugin_c,
            "data".to_string(),
            vec![],
            1,
        )
        .await;

    assert!(result.is_err());

    // 統計を確認
    let stats = comm_controller.get_stats().await?;
    assert_eq!(stats.total_rules, 2);
    assert_eq!(stats.active_rules, 2);
    assert!(stats.successful_messages > 0);

    Ok(())
}

#[tokio::test]
async fn test_error_handler_with_recovery() -> Result<(), McpError> {
    let config = ErrorHandlingConfig {
        max_history_size: 1000,
        history_retention_seconds: 3600,
        auto_recovery_enabled: true,
        consecutive_error_threshold: 3,
        critical_error_threshold: 2,
    };

    let error_handler = PluginErrorHandler::new(config).await?;
    let plugin_id = Uuid::new_v4();

    // 軽微なエラー
    let action = error_handler
        .handle_error(
            plugin_id,
            ErrorCategory::NetworkError,
            "NET_TIMEOUT".to_string(),
            "Network timeout occurred".to_string(),
            None,
            HashMap::new(),
        )
        .await?;

    // ネットワークエラーなので再起動アクションになるはず
    assert!(matches!(action, RecoveryAction::Restart { .. }));

    // クリティカルエラー
    let action = error_handler
        .handle_error(
            plugin_id,
            ErrorCategory::SecurityViolation,
            "SEC_VIOLATION".to_string(),
            "Unauthorized access attempt".to_string(),
            None,
            HashMap::new(),
        )
        .await?;

    // セキュリティ違反なので隔離アクションになるはず
    assert!(matches!(action, RecoveryAction::Quarantine));

    // エラー統計を確認
    let stats = error_handler.get_error_stats(plugin_id).await?;
    assert_eq!(stats.total_errors, 2);
    assert!(stats.errors_by_category.contains_key(&ErrorCategory::NetworkError));
    assert!(stats
        .errors_by_category
        .contains_key(&ErrorCategory::SecurityViolation));

    Ok(())
}

#[tokio::test]
async fn test_error_handler_consecutive_errors() -> Result<(), McpError> {
    let config = ErrorHandlingConfig {
        max_history_size: 1000,
        history_retention_seconds: 3600,
        auto_recovery_enabled: true,
        consecutive_error_threshold: 3,
        critical_error_threshold: 5,
    };

    let error_handler = PluginErrorHandler::new(config).await?;
    let plugin_id = Uuid::new_v4();

    // 3回連続でエラーを発生させる
    for i in 0..3 {
        error_handler
            .handle_error(
                plugin_id,
                ErrorCategory::Timeout,
                format!("TIMEOUT_{}", i),
                "Operation timed out".to_string(),
                None,
                HashMap::new(),
            )
            .await?;
    }

    // 次のエラーで閾値を超えるので隔離になるはず
    let action = error_handler
        .handle_error(
            plugin_id,
            ErrorCategory::Timeout,
            "TIMEOUT_FINAL".to_string(),
            "Operation timed out again".to_string(),
            None,
            HashMap::new(),
        )
        .await?;

    assert!(matches!(action, RecoveryAction::Quarantine));

    Ok(())
}

#[tokio::test]
async fn test_rate_limiting_in_communication() -> Result<(), McpError> {
    let config = InterPluginCommConfig {
        default_rate_limit: 3, // 1秒に3メッセージまで
        max_queue_size: 100,
        message_timeout_seconds: 5,
        max_retries: 3,
        history_retention_seconds: 3600,
    };

    let comm_controller = InterPluginCommunicationController::new(config).await?;

    let plugin_a = Uuid::new_v4();
    let plugin_b = Uuid::new_v4();

    // ルールを追加
    let rule = CommunicationRule {
        source_plugin: plugin_a,
        target_plugin: plugin_b,
        allowed_message_types: vec!["test".to_string()],
        priority: 1,
    };
    comm_controller.add_rule(rule).await?;

    // 3つのメッセージは成功するはず
    for i in 0..3 {
        let result = comm_controller
            .send_message(
                plugin_a,
                plugin_b,
                "test".to_string(),
                vec![i],
                1,
            )
            .await;
        assert!(result.is_ok(), "Message {} should succeed", i);
    }

    // 4つ目はレート制限でエラーになるはず
    let result = comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "test".to_string(),
            vec![4],
            1,
        )
        .await;

    assert!(result.is_err(), "Message 4 should fail due to rate limiting");

    // 1秒待つとリセットされるはず
    sleep(Duration::from_secs(1)).await;

    let result = comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "test".to_string(),
            vec![5],
            1,
        )
        .await;

    assert!(result.is_ok(), "Message should succeed after rate limit reset");

    Ok(())
}

#[tokio::test]
async fn test_message_priority_queueing() -> Result<(), McpError> {
    let config = InterPluginCommConfig::default();
    let comm_controller = InterPluginCommunicationController::new(config).await?;

    let plugin_a = Uuid::new_v4();
    let plugin_b = Uuid::new_v4();

    // ルールを追加
    let rule = CommunicationRule {
        source_plugin: plugin_a,
        target_plugin: plugin_b,
        allowed_message_types: vec!["test".to_string()],
        priority: 1,
    };
    comm_controller.add_rule(rule).await?;

    // 異なる優先度でメッセージを送信
    comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "test".to_string(),
            vec![1], // 低優先度
            1,
        )
        .await?;

    comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "test".to_string(),
            vec![3], // 高優先度
            10,
        )
        .await?;

    comm_controller
        .send_message(
            plugin_a,
            plugin_b,
            "test".to_string(),
            vec![2], // 中優先度
            5,
        )
        .await?;

    // メッセージは優先度順（高→低）で受信されるはず
    let msg1 = comm_controller.receive_message(plugin_b).await?;
    assert!(msg1.is_some());
    assert_eq!(msg1.unwrap().payload, vec![3]); // 最高優先度

    let msg2 = comm_controller.receive_message(plugin_b).await?;
    assert!(msg2.is_some());
    assert_eq!(msg2.unwrap().payload, vec![2]); // 中優先度

    let msg3 = comm_controller.receive_message(plugin_b).await?;
    assert!(msg3.is_some());
    assert_eq!(msg3.unwrap().payload, vec![1]); // 最低優先度

    Ok(())
}

#[tokio::test]
async fn test_error_callback_registration() -> Result<(), McpError> {
    let config = ErrorHandlingConfig::default();
    let error_handler = PluginErrorHandler::new(config).await?;

    // エラーカウンターを共有
    let error_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let error_count_clone = error_count.clone();

    // コールバックを登録
    let callback = Arc::new(move |error: &PluginError| {
        error_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!("Error callback: {:?}", error.category);
        Ok(())
    });

    error_handler.register_callback(callback).await?;

    let plugin_id = Uuid::new_v4();

    // エラーを発生させる
    error_handler
        .handle_error(
            plugin_id,
            ErrorCategory::NetworkError,
            "NET_001".to_string(),
            "Test error".to_string(),
            None,
            HashMap::new(),
        )
        .await?;

    // コールバックが呼ばれたか確認
    let count = error_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
async fn test_error_history_retention() -> Result<(), McpError> {
    let config = ErrorHandlingConfig {
        max_history_size: 5, // 最大5エラーまで
        history_retention_seconds: 3600,
        auto_recovery_enabled: true,
        consecutive_error_threshold: 10,
        critical_error_threshold: 10,
    };

    let error_handler = PluginErrorHandler::new(config).await?;
    let plugin_id = Uuid::new_v4();

    // 10個のエラーを発生させる
    for i in 0..10 {
        error_handler
            .handle_error(
                plugin_id,
                ErrorCategory::NetworkError,
                format!("ERR_{}", i),
                "Test error".to_string(),
                None,
                HashMap::new(),
            )
            .await?;
    }

    // 履歴は最大5個までのはず
    let history = error_handler.get_error_history(Some(plugin_id), None, None).await?;
    assert!(history.len() <= 5);

    Ok(())
}

#[tokio::test]
async fn test_communication_history() -> Result<(), McpError> {
    let config = InterPluginCommConfig::default();
    let comm_controller = InterPluginCommunicationController::new(config).await?;

    let plugin_a = Uuid::new_v4();
    let plugin_b = Uuid::new_v4();

    // ルールを追加
    let rule = CommunicationRule {
        source_plugin: plugin_a,
        target_plugin: plugin_b,
        allowed_message_types: vec!["test".to_string()],
        priority: 1,
    };
    comm_controller.add_rule(rule).await?;

    // メッセージを送受信
    comm_controller
        .send_message(plugin_a, plugin_b, "test".to_string(), vec![1], 1)
        .await?;

    comm_controller.receive_message(plugin_b).await?;

    // 履歴を確認
    let history = comm_controller
        .get_communication_history(Some(plugin_a), Some(10))
        .await?;

    assert!(!history.is_empty());
    assert!(history
        .iter()
        .any(|e| e.event_type == CommEventType::MessageSent));
    assert!(history
        .iter()
        .any(|e| e.event_type == CommEventType::MessageReceived));

    Ok(())
}
