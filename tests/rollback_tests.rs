//! Rollback Manager Tests
//!
//! ロールバック管理システムの包括的なテストスイート

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use mcp_rs::canary_deployment::{CanaryDeploymentManager, DeploymentState};
use mcp_rs::policy_config::PolicyConfig;
use mcp_rs::rollback::{
    MetricsSnapshot, PolicyMetrics, RollbackConfig, RollbackEvent, RollbackManager,
    SnapshotCreationReason, SystemMetrics,
};

fn create_test_policy() -> PolicyConfig {
    PolicyConfig {
        id: "test-policy".to_string(),
        name: "Test Policy".to_string(),
        version: "1.0.0".to_string(),
        description: Some("Test policy for rollback tests".to_string()),
        ..Default::default()
    }
}

fn create_test_metrics(error_rate: f64, response_time: f64) -> MetricsSnapshot {
    MetricsSnapshot {
        timestamp: chrono::Utc::now(),
        stable_metrics: PolicyMetrics {
            total_requests: 1000,
            successful_requests: 950,
            error_requests: 50,
            avg_response_time_ms: 120.0,
            error_rate: 2.0,
            ..Default::default()
        },
        canary_metrics: PolicyMetrics {
            total_requests: 200,
            successful_requests: (200.0 * (1.0 - error_rate / 100.0)) as u64,
            error_requests: (200.0 * (error_rate / 100.0)) as u64,
            avg_response_time_ms: response_time,
            error_rate,
            ..Default::default()
        },
        system_metrics: SystemMetrics::default(),
        custom_metrics: std::collections::HashMap::new(),
    }
}

#[tokio::test]
async fn test_rollback_manager_initialization() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 初期設定の確認
    let config = rollback_manager.get_config().await.unwrap();
    assert!(config.auto_rollback_enabled);
    assert_eq!(config.max_snapshots, 50);
    assert_eq!(config.error_rate_threshold, 5.0);
}

#[tokio::test]
async fn test_snapshot_creation() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // スナップショット作成
    let snapshot_id = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::Manual {
                created_by: "test".to_string(),
            },
        )
        .await
        .unwrap();

    assert!(!snapshot_id.is_empty());

    // 履歴の確認
    let history = rollback_manager.get_rollback_history().await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, snapshot_id);
}

#[tokio::test]
async fn test_multiple_snapshots() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 複数のスナップショットを作成
    let reasons = vec![
        SnapshotCreationReason::DeploymentStart,
        SnapshotCreationReason::ScheduledBackup,
        SnapshotCreationReason::Manual {
            created_by: "admin".to_string(),
        },
    ];

    for reason in reasons {
        let snapshot_id = rollback_manager
            .create_snapshot(
                policy.clone(),
                None,
                Default::default(),
                DeploymentState::Idle,
                reason,
            )
            .await
            .unwrap();
        assert!(!snapshot_id.is_empty());
    }

    let history = rollback_manager.get_rollback_history().await.unwrap();
    assert_eq!(history.len(), 3);
}

#[tokio::test]
async fn test_manual_rollback() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // スナップショット作成
    let snapshot_id = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::Manual {
                created_by: "test".to_string(),
            },
        )
        .await
        .unwrap();

    // 手動ロールバック実行
    let rollback_id = rollback_manager
        .initiate_manual_rollback(
            snapshot_id,
            "test_user".to_string(),
            "Test rollback".to_string(),
        )
        .await
        .unwrap();

    assert!(!rollback_id.is_empty());

    // メトリクスの確認
    let metrics = rollback_manager.get_rollback_metrics().await.unwrap();
    assert_eq!(metrics.manual_rollbacks, 1);
    assert_eq!(metrics.total_rollbacks, 1);
}

#[tokio::test]
async fn test_auto_rollback_trigger() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 事前にスナップショットを作成
    let _snapshot_id = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::DeploymentStart,
        )
        .await
        .unwrap();

    // 問題のあるメトリクスで自動ロールバックをトリガー
    let problematic_metrics = create_test_metrics(8.0, 600.0); // 8%エラー率、600ms応答時間

    let rollback_id = rollback_manager
        .trigger_auto_rollback("High error rate detected".to_string(), problematic_metrics)
        .await
        .unwrap();

    assert!(!rollback_id.is_empty());

    // メトリクスの確認
    let metrics = rollback_manager.get_rollback_metrics().await.unwrap();
    assert_eq!(metrics.auto_rollbacks, 1);
    assert_eq!(metrics.total_rollbacks, 1);
}

#[tokio::test]
async fn test_rollback_config_update() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 設定を更新
    let new_config = RollbackConfig {
        error_rate_threshold: 2.5,
        response_time_threshold_ms: 300,
        auto_rollback_enabled: false,
        ..Default::default()
    };

    rollback_manager
        .update_config(new_config.clone())
        .await
        .unwrap();

    // 設定の確認
    let updated_config = rollback_manager.get_config().await.unwrap();
    assert_eq!(updated_config.error_rate_threshold, 2.5);
    assert_eq!(updated_config.response_time_threshold_ms, 300);
    assert!(!updated_config.auto_rollback_enabled);
}

#[tokio::test]
async fn test_event_subscription() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // イベントサブスクリプション
    let mut event_receiver = rollback_manager.subscribe_events();

    // 別のタスクでイベントを受信
    let received_events = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_events_clone = received_events.clone();

    let event_handler = tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            received_events_clone.lock().await.push(event);
        }
    });

    // スナップショット作成（イベントをトリガー）
    let snapshot_id = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::Manual {
                created_by: "test".to_string(),
            },
        )
        .await
        .unwrap();

    // 少し待ってからイベントハンドラーを終了
    sleep(Duration::from_millis(100)).await;
    event_handler.abort();

    // イベントが受信されたことを確認
    let events = received_events.lock().await;
    assert!(!events.is_empty());

    if let Some(RollbackEvent::SnapshotCreated {
        snapshot_id: received_id,
        ..
    }) = events.first()
    {
        assert_eq!(received_id, &snapshot_id);
    } else {
        panic!("Expected SnapshotCreated event");
    }
}

#[tokio::test]
async fn test_snapshot_cleanup() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 設定を更新（最大スナップショット数を少なくする）
    let config = RollbackConfig {
        max_snapshots: 3,
        ..Default::default()
    };
    rollback_manager.update_config(config).await.unwrap();

    // 最大数を超えるスナップショットを作成
    for i in 0..5 {
        let _snapshot_id = rollback_manager
            .create_snapshot(
                policy.clone(),
                None,
                Default::default(),
                DeploymentState::Idle,
                SnapshotCreationReason::Manual {
                    created_by: format!("test_{}", i),
                },
            )
            .await
            .unwrap();
    }

    // 履歴のサイズが最大数以下であることを確認
    let history = rollback_manager.get_rollback_history().await.unwrap();
    assert!(history.len() <= 3);
}

#[tokio::test]
async fn test_metrics_collection() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 初期メトリクス
    let initial_metrics = rollback_manager.get_rollback_metrics().await.unwrap();
    assert_eq!(initial_metrics.total_rollbacks, 0);
    assert_eq!(initial_metrics.auto_rollbacks, 0);
    assert_eq!(initial_metrics.manual_rollbacks, 0);

    // スナップショット作成とロールバック実行
    let snapshot_id = rollback_manager
        .create_snapshot(
            policy.clone(),
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::Manual {
                created_by: "test".to_string(),
            },
        )
        .await
        .unwrap();

    let _rollback_id = rollback_manager
        .initiate_manual_rollback(
            snapshot_id,
            "test_user".to_string(),
            "Test rollback".to_string(),
        )
        .await
        .unwrap();

    // 自動ロールバックもテスト
    let _snapshot_id2 = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::DeploymentStart,
        )
        .await
        .unwrap();

    let problematic_metrics = create_test_metrics(10.0, 800.0);
    let _auto_rollback_id = rollback_manager
        .trigger_auto_rollback("Critical error rate".to_string(), problematic_metrics)
        .await
        .unwrap();

    // 最終メトリクス
    let final_metrics = rollback_manager.get_rollback_metrics().await.unwrap();
    assert_eq!(final_metrics.total_rollbacks, 2);
    assert_eq!(final_metrics.auto_rollbacks, 1);
    assert_eq!(final_metrics.manual_rollbacks, 1);
    assert!(final_metrics.success_rate > 0.0);
}

#[tokio::test]
async fn test_should_trigger_auto_rollback() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 正常なメトリクス
    let normal_metrics = create_test_metrics(2.0, 200.0);
    assert!(!rollback_manager
        .should_trigger_auto_rollback(&normal_metrics)
        .await
        .unwrap());

    // 高エラー率
    let high_error_metrics = create_test_metrics(8.0, 200.0);
    assert!(rollback_manager
        .should_trigger_auto_rollback(&high_error_metrics)
        .await
        .unwrap());

    // 高応答時間
    let slow_response_metrics = create_test_metrics(2.0, 1200.0); // 1200ms > 1000ms 閾値
    assert!(rollback_manager
        .should_trigger_auto_rollback(&slow_response_metrics)
        .await
        .unwrap());

    // 両方の問題
    let problematic_metrics = create_test_metrics(8.0, 1200.0);
    assert!(rollback_manager
        .should_trigger_auto_rollback(&problematic_metrics)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_monitoring_functionality() {
    let policy = create_test_policy();
    let canary_manager = Arc::new(CanaryDeploymentManager::new(policy.clone()));
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager));

    // 事前にスナップショットを作成
    let _snapshot_id = rollback_manager
        .create_snapshot(
            policy,
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::DeploymentStart,
        )
        .await
        .unwrap();

    // 監視開始
    rollback_manager.start_monitoring().await.unwrap();

    // 少し待機
    sleep(Duration::from_millis(100)).await;

    // 監視停止
    rollback_manager.stop_monitoring().await.unwrap();

    // テスト完了を確認（監視が正常に開始/停止されたことを確認）
}
