//! ポリシーシステム統合テスト
//!
//! DynamicPolicyUpdater、HotReloadManager、RollbackManager、VersionManagerの
//! 統合動作を検証する

use chrono::Utc;
use mcp_rs::policy::{
    dynamic_updater::{DynamicPolicyUpdater, UpdateConfig},
    hot_reload::{HotReloadManager, ReloadStrategy},
    rollback::RollbackManager,
    version_control::VersionManager,
};
use mcp_rs::policy_config::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// テスト用ポリシー作成
fn create_test_policy(version: &str) -> PolicyConfig {
    PolicyConfig {
        id: "test-policy".to_string(),
        name: "Test Policy".to_string(),
        version: version.to_string(),
        description: Some("Test policy".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        security: SecurityPolicyConfig {
            enabled: true,
            encryption: EncryptionConfig {
                algorithm: "AES-256".to_string(),
                key_size: 256,
                pbkdf2_iterations: 100000,
            },
            tls: TlsConfig {
                enforce: true,
                min_version: "1.2".to_string(),
                cipher_suites: vec!["TLS_AES_256_GCM_SHA384".to_string()],
            },
            input_validation: InputValidationConfig {
                enabled: true,
                max_input_length: 1024,
                sql_injection_protection: true,
                xss_protection: true,
            },
            rate_limiting: RateLimitingConfig {
                enabled: true,
                requests_per_minute: 6000,
                burst_size: 10,
            },
        },
        monitoring: MonitoringPolicyConfig {
            interval_seconds: 60,
            alerts_enabled: true,
            log_level: "info".to_string(),
            metrics: MetricsConfig {
                enabled: true,
                sampling_rate: 1.0,
                buffer_size: 1000,
            },
        },
        authentication: AuthenticationPolicyConfig {
            enabled: true,
            method: "password".to_string(),
            session_timeout_seconds: 1800,
            require_mfa: false,
        },
        custom: HashMap::new(),
    }
}

#[tokio::test]
async fn test_end_to_end_policy_update_workflow() {
    // 統合シナリオ: ポリシー更新 → ロールバック → バージョン確認

    let initial_policy = create_test_policy("1.0.0");

    // 1. DynamicPolicyUpdaterで更新
    let updater = DynamicPolicyUpdater::new(initial_policy.clone(), UpdateConfig::default());

    let v2_policy = create_test_policy("2.0.0");
    let result = updater.update_policy(v2_policy.clone()).await;
    assert!(result.is_ok(), "ポリシー更新に失敗");

    let active = updater.get_active_policy().await;
    assert_eq!(active.version, "2.0.0", "バージョンが更新されていない");

    // 2. RollbackManagerでロールバック
    let rollback_mgr = RollbackManager::new(v2_policy.clone(), 10);
    let v3_policy = create_test_policy("3.0.0");
    let point_id = rollback_mgr
        .update_with_rollback_point(v3_policy.clone(), "v3 update")
        .await
        .unwrap();

    // v2にロールバック
    rollback_mgr.rollback_to_point(&point_id).await.unwrap();
    let restored = rollback_mgr.get_active_policy().await;
    assert_eq!(
        restored.version, "2.0.0",
        "ロールバックが正しく動作していない"
    );

    // 3. VersionManagerでバージョン履歴確認
    let version_mgr = VersionManager::new(initial_policy, 10);
    let _v2_id = version_mgr
        .create_version(v2_policy, "user1".to_string(), "v2 update".to_string())
        .await
        .unwrap();
    let v3_id = version_mgr
        .create_version(v3_policy, "user1".to_string(), "v3 update".to_string())
        .await
        .unwrap();

    let history = version_mgr.get_version_history(&v3_id).await.unwrap();
    assert_eq!(history.len(), 3, "バージョン履歴の長さが不正");
    assert_eq!(history[0].policy.version, "3.0.0");
    assert_eq!(history[1].policy.version, "2.0.0");
    assert_eq!(history[2].policy.version, "1.0.0");
}

#[tokio::test]
async fn test_hot_reload_with_rollback() {
    // ホットリロード失敗時のロールバック統合

    let initial_policy = create_test_policy("1.0.0");
    let rollback_mgr = Arc::new(RollbackManager::new(initial_policy.clone(), 10));

    // HotReloadManagerで更新
    let reload_mgr = HotReloadManager::new(
        initial_policy.clone(),
        ReloadStrategy::Graceful {
            grace_period_secs: 1,
        },
    );

    let v2_policy = create_test_policy("2.0.0");
    let result = reload_mgr.reload(v2_policy.clone()).await;
    assert!(result.is_ok(), "ホットリロードに失敗");

    let reload_result = result.unwrap();
    assert!(reload_result.success, "リロードが成功していない");
    assert!(
        reload_result.elapsed_ms >= 1000,
        "グレースフル期間が守られていない"
    );

    // ロールバックポイント作成
    rollback_mgr
        .create_rollback_point(v2_policy, "v2 checkpoint")
        .await
        .unwrap();

    let points = rollback_mgr.list_rollback_points().await;
    assert_eq!(points.len(), 2, "ロールバックポイント数が不正");
}

#[tokio::test]
async fn test_concurrent_policy_updates() {
    // 並行更新の整合性検証

    let initial_policy = create_test_policy("1.0.0");
    let updater = Arc::new(DynamicPolicyUpdater::new(
        initial_policy,
        UpdateConfig::default(),
    ));

    let mut handles = vec![];

    // 10個の並行更新
    for i in 1..=10 {
        let updater_clone = Arc::clone(&updater);
        let handle = tokio::spawn(async move {
            let policy = create_test_policy(&format!("1.{}.0", i));
            updater_clone.update_policy(policy).await
        });
        handles.push(handle);
    }

    // 全て完了を待つ
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "タスク実行エラー");
    }

    // 最終的なポリシーを確認（最後の更新が反映されているはず）
    let final_policy = updater.get_active_policy().await;
    assert!(final_policy.version.starts_with("1."), "バージョンが不正");
}

#[tokio::test]
async fn test_gradual_rollout_stages() {
    // 段階的ロールアウトの統合テスト

    let initial_policy = create_test_policy("1.0.0");
    let reload_mgr = HotReloadManager::new(
        initial_policy,
        ReloadStrategy::Gradual {
            stages: 3,
            stage_delay_secs: 1,
        },
    );

    let new_policy = create_test_policy("2.0.0");
    let start = std::time::Instant::now();
    let result = reload_mgr.reload(new_policy).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "段階的ロールアウトに失敗");
    assert!(
        elapsed.as_secs() >= 2,
        "段階遅延が守られていない（期待: >=2秒, 実際: {:?}）",
        elapsed
    );

    let reload_result = result.unwrap();
    assert!(reload_result.success, "リロードが成功していない");
}

#[tokio::test]
async fn test_version_diff_integration() {
    // バージョン差分計算の統合テスト

    let initial_policy = create_test_policy("1.0.0");
    let version_mgr = VersionManager::new(initial_policy, 10);

    let v1_id = version_mgr.get_current_version().await.unwrap().id;

    // v2: 暗号化アルゴリズム変更
    let mut v2_policy = create_test_policy("2.0.0");
    v2_policy.security.encryption.algorithm = "AES-128".to_string();
    let v2_id = version_mgr
        .create_version(
            v2_policy,
            "admin".to_string(),
            "Downgrade encryption".to_string(),
        )
        .await
        .unwrap();

    // v3: レート制限変更
    let mut v3_policy = create_test_policy("3.0.0");
    v3_policy.security.rate_limiting.requests_per_minute = 12000;
    let v3_id = version_mgr
        .create_version(
            v3_policy,
            "admin".to_string(),
            "Increase rate limit".to_string(),
        )
        .await
        .unwrap();

    // v1 -> v2 の差分
    let diff_v1_v2 = version_mgr.diff(&v1_id, &v2_id).await.unwrap();
    assert!(
        diff_v1_v2.changes.len() >= 2,
        "差分数が不足（期待: >=2, 実際: {}）",
        diff_v1_v2.changes.len()
    );
    assert!(
        diff_v1_v2
            .changes
            .iter()
            .any(|c| c.field_path.contains("encryption.algorithm")),
        "暗号化アルゴリズム変更が検出されていない"
    );

    // v2 -> v3 の差分
    let diff_v2_v3 = version_mgr.diff(&v2_id, &v3_id).await.unwrap();
    assert!(
        diff_v2_v3
            .changes
            .iter()
            .any(|c| c.field_path.contains("rate_limiting.requests_per_minute")),
        "レート制限変更が検出されていない"
    );
}

#[tokio::test]
async fn test_rollback_with_version_tagging() {
    // ロールバックとバージョンタグ付けの統合

    let initial_policy = create_test_policy("1.0.0");
    let version_mgr = VersionManager::new(initial_policy.clone(), 10);

    // 複数バージョン作成
    let v2_id = version_mgr
        .create_version(
            create_test_policy("2.0.0"),
            "dev".to_string(),
            "Development version".to_string(),
        )
        .await
        .unwrap();

    let v3_id = version_mgr
        .create_version(
            create_test_policy("3.0.0"),
            "qa".to_string(),
            "QA version".to_string(),
        )
        .await
        .unwrap();

    let v4_id = version_mgr
        .create_version(
            create_test_policy("4.0.0"),
            "prod".to_string(),
            "Production version".to_string(),
        )
        .await
        .unwrap();

    // タグ付け
    version_mgr
        .add_tag(&v2_id, "dev".to_string())
        .await
        .unwrap();
    version_mgr.add_tag(&v3_id, "qa".to_string()).await.unwrap();
    version_mgr
        .add_tag(&v3_id, "stable".to_string())
        .await
        .unwrap();
    version_mgr
        .add_tag(&v4_id, "production".to_string())
        .await
        .unwrap();

    // タグで検索
    let stable_versions = version_mgr.find_by_tag("stable").await;
    assert_eq!(stable_versions.len(), 1, "stable タグが1つであるべき");
    assert_eq!(stable_versions[0].policy.version, "3.0.0");

    // RollbackManagerで stable バージョンに戻す
    let rollback_mgr = RollbackManager::new(create_test_policy("4.0.0"), 10);
    rollback_mgr
        .create_rollback_point(create_test_policy("3.0.0"), "stable checkpoint")
        .await
        .unwrap();
    rollback_mgr
        .create_rollback_point(create_test_policy("4.0.0"), "production checkpoint")
        .await
        .unwrap();

    // 1ステップ前（v3）にロールバック
    rollback_mgr.rollback_n_steps(1).await.unwrap();
    let restored = rollback_mgr.get_active_policy().await;
    assert_eq!(restored.version, "3.0.0", "stable バージョンに戻っていない");
}

#[tokio::test]
async fn test_update_event_subscription() {
    // 更新イベント購読の統合テスト

    let initial_policy = create_test_policy("1.0.0");
    let updater = DynamicPolicyUpdater::new(initial_policy, UpdateConfig::default());

    // イベント購読
    let mut receiver = updater.subscribe();

    // 別タスクで更新実行
    let updater_clone = Arc::new(updater);
    let updater_task = Arc::clone(&updater_clone);
    let update_handle = tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        let policy = create_test_policy("2.0.0");
        updater_task.update_policy(policy).await
    });

    // イベント受信（タイムアウト付き）
    let mut event_count = 0;
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout && event_count < 3 {
        match tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await {
            Ok(Ok(event)) => {
                println!("受信イベント: {:?}", event.event_type);
                event_count += 1;
            }
            Ok(Err(_)) => break, // チャネルクローズ
            Err(_) => break,     // タイムアウト
        }
    }

    update_handle.await.unwrap().unwrap();

    assert!(
        event_count >= 2,
        "イベントが不足（期待: >=2, 実際: {}）",
        event_count
    );
}

#[tokio::test]
async fn test_rollback_cleanup() {
    // ロールバックポイントのクリーンアップ統合テスト

    let initial_policy = create_test_policy("1.0.0");
    let rollback_mgr = RollbackManager::new(initial_policy, 5);

    // 10個のロールバックポイント作成（最大5個なので5個残るはず）
    for i in 1..=10 {
        rollback_mgr
            .create_rollback_point(create_test_policy(&format!("{}.0.0", i)), format!("v{}", i))
            .await
            .unwrap();
    }

    let count = rollback_mgr.count_rollback_points().await;
    assert_eq!(count, 5, "最大保持数が守られていない");

    // 古いポイントのクリーンアップ（0日より古い = 全て古い）
    let _removed = rollback_mgr.cleanup_old_points(0).await.unwrap();

    // 最新は常に保持されるので、1つは残る
    let remaining = rollback_mgr.count_rollback_points().await;
    assert_eq!(remaining, 1, "最新ポイントが保持されていない");
}

#[tokio::test]
async fn test_policy_statistics() {
    // 更新統計情報の統合テスト

    let initial_policy = create_test_policy("1.0.0");
    let updater = DynamicPolicyUpdater::new(initial_policy, UpdateConfig::default());

    // 複数回更新
    for i in 1..=5 {
        let policy = create_test_policy(&format!("1.{}.0", i));
        updater.update_policy(policy).await.unwrap();
    }

    let stats = updater.get_statistics().await;

    assert_eq!(stats.total_updates, 5, "更新回数が不正");
    assert_eq!(stats.successful_updates, 5, "成功回数が不正");
    assert_eq!(stats.failed_updates, 0, "失敗回数が不正");
    assert!(
        (stats.success_rate - 100.0).abs() < 0.001,
        "成功率が不正（期待: 100.0%, 実際: {}%）",
        stats.success_rate
    );
}
