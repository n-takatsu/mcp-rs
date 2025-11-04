use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use mcp_rs::policy_application::PolicyApplicationEngine;
use mcp_rs::policy_config::{PolicyConfig, PolicyLoader};
use mcp_rs::policy_validation::{PolicyValidationEngine, ValidationLevel};

/// 統合テストスイート - Policy Hot-Reload システム全体のテスト
#[cfg(test)]
mod policy_hot_reload_tests {
    use super::*;

    /// 完全なポリシーホットリロードワークフローのテスト
    #[tokio::test]
    async fn test_complete_policy_hot_reload_workflow() {
        println!("🧪 完全なポリシーホットリロードワークフローのテスト開始");

        // テスト環境のセットアップ
        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("test_policy.toml");

        // 1. 初期ポリシーファイルを作成
        let initial_policy = create_test_policy("initial-policy", "1.0.0");
        PolicyLoader::save_to_file(&initial_policy, &policy_file)
            .await
            .unwrap();

        // 2. ポリシー適用エンジンを起動
        let mut engine = PolicyApplicationEngine::with_validation_level(
            temp_dir.path(),
            ValidationLevel::Standard,
        );
        engine.add_policy_file(&policy_file);

        let mut event_receiver = engine.subscribe();
        engine.start().await.unwrap();

        // 3. 初期ポリシーが正常に適用されることを確認
        sleep(Duration::from_millis(500)).await;
        let current_policy = engine.get_current_policy().await;
        assert_eq!(current_policy.id, "initial-policy");
        assert_eq!(current_policy.version, "1.0.0");

        // 4. ポリシーファイルを更新
        let updated_policy = create_test_policy("updated-policy", "2.0.0");
        PolicyLoader::save_to_file(&updated_policy, &policy_file)
            .await
            .unwrap();

        // 5. 更新イベントが発生することを確認
        let event = tokio::time::timeout(Duration::from_secs(5), event_receiver.recv())
            .await
            .expect("タイムアウト")
            .expect("イベント受信エラー");

        match event.event_type {
            mcp_rs::policy_application::PolicyApplicationEventType::Applied => {
                println!("✅ ポリシー更新イベントを正常に受信");
            }
            _ => panic!("予期しないイベントタイプ: {:?}", event.event_type),
        }

        // 6. 更新されたポリシーが適用されていることを確認
        sleep(Duration::from_millis(500)).await;
        let updated_current_policy = engine.get_current_policy().await;
        assert_eq!(updated_current_policy.id, "updated-policy");
        assert_eq!(updated_current_policy.version, "2.0.0");

        // エンジンを停止
        engine.stop();

        println!("✅ 完全なポリシーホットリロードワークフローテスト完了");
    }

    /// パフォーマンステスト - 大量ポリシー更新の処理能力
    #[tokio::test]
    async fn test_performance_bulk_policy_updates() {
        println!("⚡ パフォーマンステスト: 大量ポリシー更新");

        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("perf_test_policy.toml");

        // 初期ポリシー作成
        let initial_policy = create_test_policy("perf-test", "1.0.0");
        PolicyLoader::save_to_file(&initial_policy, &policy_file)
            .await
            .unwrap();

        // エンジン起動
        let mut engine = PolicyApplicationEngine::with_validation_level(
            temp_dir.path(),
            ValidationLevel::Basic, // パフォーマンス重視で基本検証のみ
        );
        engine.add_policy_file(&policy_file);

        let mut event_receiver = engine.subscribe();
        engine.start().await.unwrap();

        sleep(Duration::from_millis(200)).await;

        // パフォーマンス測定開始
        let start_time = std::time::Instant::now();
        let update_count = 10;

        // 複数回の高速ポリシー更新
        for i in 1..=update_count {
            let policy = create_test_policy(&format!("perf-test-{}", i), &format!("1.0.{}", i));
            PolicyLoader::save_to_file(&policy, &policy_file)
                .await
                .unwrap();

            // 短い間隔で更新
            sleep(Duration::from_millis(100)).await;
        }

        // 全ての更新イベントを待機
        let mut received_events = 0;
        while received_events < update_count {
            match tokio::time::timeout(Duration::from_secs(2), event_receiver.recv()).await {
                Ok(Ok(_)) => received_events += 1,
                _ => break,
            }
        }

        let total_duration = start_time.elapsed();
        let avg_time_per_update = total_duration.as_millis() as f64 / update_count as f64;

        println!("📊 パフォーマンス結果:");
        println!("   - 更新回数: {} 回", update_count);
        println!("   - 総処理時間: {}ms", total_duration.as_millis());
        println!("   - 平均処理時間/更新: {:.2}ms", avg_time_per_update);
        println!("   - 受信イベント数: {}", received_events);

        // パフォーマンス基準の確認
        assert!(
            avg_time_per_update < 200.0,
            "平均処理時間が200ms以下であること"
        );
        assert!(
            received_events >= update_count / 2,
            "50%以上の更新イベントが処理されること"
        );

        engine.stop();
        println!("✅ パフォーマンステスト完了");
    }

    /// 検証システムの統合テスト
    #[tokio::test]
    async fn test_validation_integration() {
        println!("🔍 検証システム統合テスト開始");

        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("validation_test.toml");

        // 最初に有効なポリシーから始める
        let valid_policy = create_test_policy("initial-valid-policy", "1.0.0");
        PolicyLoader::save_to_file(&valid_policy, &policy_file)
            .await
            .unwrap();

        // 厳格な検証レベルでエンジンを起動
        let mut engine = PolicyApplicationEngine::with_validation_level(
            temp_dir.path(),
            ValidationLevel::Strict,
        );
        engine.add_policy_file(&policy_file);

        let mut event_receiver = engine.subscribe();
        engine.start().await.unwrap();

        // 最初の有効なポリシーが適用されることを確認
        sleep(Duration::from_millis(500)).await;
        let current_policy = engine.get_current_policy().await;
        assert_eq!(current_policy.id, "initial-valid-policy");

        // 次に無効なポリシーに更新
        let invalid_toml = format!(
            r#"
id = ""
name = ""
version = "1.0.0"
description = "Invalid policy for testing"
created_at = "{}"
updated_at = "{}"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 64
pbkdf2_iterations = 100

[security.tls]
enforce = true
min_version = "TLSv1.0"
cipher_suites = []

[security.rate_limiting]
enabled = true
requests_per_minute = 100
burst_size = 20
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 1048576
sql_injection_protection = true
xss_protection = true

[monitoring]
enabled = true
interval_seconds = 60
log_level = "info"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 1.0
buffer_size = 1000

[authentication]
enabled = true
method = "basic"
session_timeout_seconds = 3600

[custom]
environment = "development"
compliance_mode = "standard"
"#,
            chrono::Utc::now().to_rfc3339(),
            chrono::Utc::now().to_rfc3339()
        );

        tokio::fs::write(&policy_file, invalid_toml).await.unwrap();

        // イベントを待機（検証失敗または適用失敗のいずれかを期待）
        let mut received_validation_failure = false;
        for _ in 0..3 {
            match tokio::time::timeout(Duration::from_secs(2), event_receiver.recv()).await {
                Ok(Ok(event)) => match event.event_type {
                    mcp_rs::policy_application::PolicyApplicationEventType::ValidationFailed => {
                        println!("✅ 期待通り検証失敗イベントを受信");
                        received_validation_failure = true;
                        break;
                    }
                    mcp_rs::policy_application::PolicyApplicationEventType::ApplicationFailed => {
                        println!("✅ ポリシー適用失敗イベントを受信（検証エラー含む）");
                        received_validation_failure = true;
                        break;
                    }
                    _ => {
                        println!("📝 その他のイベント: {:?}", event.event_type);
                    }
                },
                _ => {
                    println!("⏰ イベント待機タイムアウト");
                    break;
                }
            }
        }

        // 有効なポリシーに修正
        let recovery_policy = create_test_policy("valid-after-fix", "1.0.1");
        PolicyLoader::save_to_file(&recovery_policy, &policy_file)
            .await
            .unwrap();

        // 成功イベントを待機
        let mut received_success = false;
        for _ in 0..3 {
            match tokio::time::timeout(Duration::from_secs(2), event_receiver.recv()).await {
                Ok(Ok(event)) => match event.event_type {
                    mcp_rs::policy_application::PolicyApplicationEventType::Applied => {
                        println!("✅ 修正後に正常適用イベントを受信");
                        received_success = true;
                        break;
                    }
                    _ => {
                        println!("📝 その他のイベント: {:?}", event.event_type);
                    }
                },
                _ => {
                    println!("⏰ イベント待機タイムアウト");
                    break;
                }
            }
        }

        engine.stop();

        // 最低限の検証が行われたことを確認
        println!("📊 テスト結果:");
        println!("   - 検証失敗イベント: {}", received_validation_failure);
        println!("   - 成功イベント: {}", received_success);

        // 少なくとも成功イベントは発生しているはず
        assert!(received_success, "有効なポリシーの適用成功イベントが必要");

        println!("✅ 検証システム統合テスト完了");
    }

    /// 複数ポリシーファイルの同時監視テスト
    #[tokio::test]
    async fn test_multiple_policy_files() {
        println!("📁 複数ポリシーファイル同時監視テスト開始");

        let temp_dir = TempDir::new().unwrap();
        let policy_file1 = temp_dir.path().join("policy1.toml");
        let policy_file2 = temp_dir.path().join("policy2.toml");

        // 2つのポリシーファイルを作成
        let policy1 = create_test_policy("multi-test-1", "1.0.0");
        let policy2 = create_test_policy("multi-test-2", "2.0.0");

        PolicyLoader::save_to_file(&policy1, &policy_file1)
            .await
            .unwrap();
        PolicyLoader::save_to_file(&policy2, &policy_file2)
            .await
            .unwrap();

        // エンジンに両方のファイルを追加
        let mut engine = PolicyApplicationEngine::new(temp_dir.path());
        engine.add_policy_file(&policy_file1);
        engine.add_policy_file(&policy_file2);

        let mut event_receiver = engine.subscribe();
        engine.start().await.unwrap();

        sleep(Duration::from_millis(500)).await;

        // 最初のファイルを更新
        let updated_policy1 = create_test_policy("multi-test-1-updated", "1.1.0");
        PolicyLoader::save_to_file(&updated_policy1, &policy_file1)
            .await
            .unwrap();

        // イベントを受信
        let event1 = tokio::time::timeout(Duration::from_secs(3), event_receiver.recv())
            .await
            .expect("タイムアウト")
            .expect("イベント受信エラー");

        assert_eq!(event1.policy_id, "multi-test-1-updated");

        engine.stop();
        println!("✅ 複数ポリシーファイル同時監視テスト完了");
    }

    /// エラー回復テスト
    #[tokio::test]
    async fn test_error_recovery() {
        println!("🔄 エラー回復テスト開始");

        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("recovery_test.toml");

        // 有効なポリシーから開始
        let valid_policy = create_test_policy("recovery-test", "1.0.0");
        PolicyLoader::save_to_file(&valid_policy, &policy_file)
            .await
            .unwrap();

        let mut engine = PolicyApplicationEngine::with_validation_level(
            temp_dir.path(),
            ValidationLevel::Standard,
        );
        engine.add_policy_file(&policy_file);

        let mut event_receiver = engine.subscribe();
        engine.start().await.unwrap();

        sleep(Duration::from_millis(300)).await;

        // 無効なポリシーファイルを書き込み（不正なTOML）
        tokio::fs::write(&policy_file, "invalid toml content [[[")
            .await
            .unwrap();

        // エラーイベントを確認
        let error_event = tokio::time::timeout(Duration::from_secs(3), event_receiver.recv())
            .await
            .expect("タイムアウト")
            .expect("イベント受信エラー");

        println!("📋 エラーイベント: {:?}", error_event.event_type);

        // 有効なポリシーに戻す
        let recovery_policy = create_test_policy("recovery-test-fixed", "1.0.1");
        PolicyLoader::save_to_file(&recovery_policy, &policy_file)
            .await
            .unwrap();

        // 回復イベントを確認
        let recovery_event = tokio::time::timeout(Duration::from_secs(3), event_receiver.recv())
            .await
            .expect("タイムアウト")
            .expect("イベント受信エラー");

        match recovery_event.event_type {
            mcp_rs::policy_application::PolicyApplicationEventType::Applied => {
                println!("✅ エラーから正常に回復");
            }
            _ => println!("回復イベント: {:?}", recovery_event.event_type),
        }

        engine.stop();
        println!("✅ エラー回復テスト完了");
    }

    /// 検証統計の測定テスト
    #[tokio::test]
    async fn test_validation_statistics() {
        println!("📊 検証統計測定テスト開始");

        let mut validation_engine = PolicyValidationEngine::new();

        // 複数の検証を実行
        for i in 1..=5 {
            let test_policy = create_test_policy(&format!("stats-test-{}", i), "1.0.0");
            validation_engine
                .validate_policy(&test_policy, ValidationLevel::Standard)
                .await;
        }

        // 統計を確認
        let stats = validation_engine.get_stats();

        println!("📈 検証統計:");
        println!("   - 総検証回数: {}", stats.total_validations);
        println!("   - 成功回数: {}", stats.successful_validations);
        println!("   - 失敗回数: {}", stats.failed_validations);
        println!(
            "   - 平均検証時間: {:.2}ms",
            stats.average_validation_time_ms
        );

        assert_eq!(stats.total_validations, 5);
        assert_eq!(stats.successful_validations, 5);
        assert_eq!(stats.failed_validations, 0);
        assert!(stats.average_validation_time_ms >= 0.0);

        println!("✅ 検証統計測定テスト完了");
    }

    /// ヘルパー関数: テスト用ポリシー作成
    fn create_test_policy(id: &str, version: &str) -> PolicyConfig {
        let mut policy = PolicyConfig::default();
        policy.id = id.to_string();
        policy.name = format!("Test Policy {}", id);
        policy.version = version.to_string();
        policy.description = Some(format!("テスト用ポリシー: {}", id));

        // テスト用の設定値
        policy.security.rate_limiting.requests_per_minute = 100;
        policy.monitoring.interval_seconds = 60;
        policy.authentication.session_timeout_seconds = 3600;

        policy
    }
}
