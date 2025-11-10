/// ポリシー適用エンジンのリアルタイムデモンストレーション
///
/// ファイル監視システム + ポリシー設定管理 + 適用エンジンを統合し、
/// リアルタイムでのポリシー変更適用を実証します。
use mcp_rs::policy_application::{PolicyApplicationEngine, PolicyApplicationEventType};
use mcp_rs::policy_config::{PolicyConfig, PolicyLoader};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt::init();

    info!("🚀 ポリシー適用エンジンのリアルタイムデモを開始");

    // 1. 一時的なポリシーディレクトリを作成
    let temp_dir = tempfile::TempDir::new()?;
    let policy_dir = temp_dir.path();
    info!("📂 ポリシーディレクトリ: {:?}", policy_dir);

    // 2. ポリシー適用エンジンを作成・設定
    let mut engine = PolicyApplicationEngine::new(policy_dir);

    // テスト用ポリシーファイルのパスを追加
    let policy_file = policy_dir.join("runtime_policy.toml");
    engine.add_policy_file(&policy_file);

    // 3. 初期ポリシーファイルを作成
    let initial_policy = create_initial_policy();
    PolicyLoader::save_to_file(&initial_policy, &policy_file).await?;
    info!("📄 初期ポリシーファイルを作成: {:?}", policy_file);

    // 4. ポリシー適用エンジンを起動
    info!("\n=== ポリシー適用エンジン起動 ===");
    engine.start().await?;

    // 5. ポリシー変更イベントの監視を開始
    let mut event_receiver = engine.subscribe_policy_events();
    let event_monitor = tokio::spawn(async move {
        let mut event_count = 0;
        while let Ok(event) = event_receiver.recv().await {
            event_count += 1;
            match event.event_type {
                PolicyApplicationEventType::Loaded => {
                    info!(
                        "🔄 [Event {}] ポリシー読み込み: {}",
                        event_count, event.policy_id
                    );
                }
                PolicyApplicationEventType::Applied => {
                    info!(
                        "✅ [Event {}] ポリシー適用成功: {}",
                        event_count, event.policy_id
                    );
                    info!("   変更セクション: {:?}", event.changed_sections);
                }
                PolicyApplicationEventType::ApplicationFailed => {
                    error!(
                        "❌ [Event {}] ポリシー適用失敗: {}",
                        event_count, event.policy_id
                    );
                }
                PolicyApplicationEventType::ValidationFailed => {
                    warn!(
                        "⚠️ [Event {}] ポリシー検証失敗: {}",
                        event_count, event.policy_id
                    );
                }
            }

            if event_count >= 5 {
                info!("📊 イベント監視を終了 ({}個のイベントを処理)", event_count);
                break;
            }
        }
    });

    // 6. 段階的なポリシー変更をシミュレート
    info!("\n=== リアルタイムポリシー変更デモ ===");

    // Step 1: セキュリティポリシーの変更
    sleep(Duration::from_secs(1)).await;
    info!("🔧 Step 1: レート制限を厳格化...");
    let strict_policy = create_strict_security_policy();
    PolicyLoader::save_to_file(&strict_policy, &policy_file).await?;

    // Step 2: 監視設定の変更
    sleep(Duration::from_secs(2)).await;
    info!("🔧 Step 2: 監視間隔を短縮...");
    let monitoring_policy = create_enhanced_monitoring_policy();
    PolicyLoader::save_to_file(&monitoring_policy, &policy_file).await?;

    // Step 3: 認証設定の変更
    sleep(Duration::from_secs(2)).await;
    info!("🔧 Step 3: MFA認証を有効化...");
    let auth_policy = create_mfa_enabled_policy();
    PolicyLoader::save_to_file(&auth_policy, &policy_file).await?;

    // Step 4: カスタム設定の変更
    sleep(Duration::from_secs(2)).await;
    info!("🔧 Step 4: 環境設定を本番用に変更...");
    let production_policy = create_production_policy();
    PolicyLoader::save_to_file(&production_policy, &policy_file).await?;

    // 7. 現在のポリシー状態を表示
    sleep(Duration::from_secs(1)).await;
    info!("\n=== 最終ポリシー状態 ===");
    let final_policy = engine.get_current_policy().await;
    display_policy_summary(&final_policy);

    // 8. レート制限設定の確認
    if engine.has_rate_limiter("global").await {
        info!("✅ グローバルレート制限が設定されています");
    } else {
        info!("ℹ️ グローバルレート制限は設定されていません");
    }

    // 9. イベント監視の完了を待機
    let _ = tokio::time::timeout(Duration::from_secs(3), event_monitor).await;

    // 10. エンジン停止
    info!("\n=== エンジン停止 ===");
    engine.stop();

    info!("🎉 ポリシー適用エンジンのデモが完了しました！");
    Ok(())
}

fn create_initial_policy() -> PolicyConfig {
    let mut policy = PolicyConfig {
        name: "Initial Demo Policy".to_string(),
        description: Some("デモ用初期ポリシー".to_string()),
        ..Default::default()
    };
    policy.security.rate_limiting.requests_per_minute = 60;
    policy.monitoring.interval_seconds = 60;
    policy.authentication.require_mfa = false;
    policy
}

fn create_strict_security_policy() -> PolicyConfig {
    let mut policy = create_initial_policy();
    policy.name = "Strict Security Policy".to_string();
    policy.description = Some("厳格なセキュリティポリシー".to_string());
    policy.security.rate_limiting.requests_per_minute = 30; // 厳格化
    policy.security.rate_limiting.burst_size = 5; // バースト制限
    policy.security.input_validation.max_input_length = 256 * 1024; // 256KB制限
    policy.updated_at = chrono::Utc::now();
    policy
}

fn create_enhanced_monitoring_policy() -> PolicyConfig {
    let mut policy = create_strict_security_policy();
    policy.name = "Enhanced Monitoring Policy".to_string();
    policy.description = Some("強化された監視ポリシー".to_string());
    policy.monitoring.interval_seconds = 15; // 15秒間隔
    policy.monitoring.log_level = "debug".to_string();
    policy.monitoring.alerts_enabled = true;
    policy.monitoring.metrics.sampling_rate = 1.0; // 100%サンプリング
    policy.updated_at = chrono::Utc::now();
    policy
}

fn create_mfa_enabled_policy() -> PolicyConfig {
    let mut policy = create_enhanced_monitoring_policy();
    policy.name = "MFA Enabled Policy".to_string();
    policy.description = Some("MFA認証必須ポリシー".to_string());
    policy.authentication.require_mfa = true; // MFA有効化
    policy.authentication.session_timeout_seconds = 1800; // 30分
    policy.authentication.method = "oauth2".to_string();
    policy.updated_at = chrono::Utc::now();
    policy
}

fn create_production_policy() -> PolicyConfig {
    let mut policy = create_mfa_enabled_policy();
    policy.name = "Production Policy".to_string();
    policy.description = Some("本番環境用ポリシー".to_string());
    policy.updated_at = chrono::Utc::now();

    // カスタム設定を追加
    policy.custom.insert(
        "environment".to_string(),
        serde_json::Value::String("production".to_string()),
    );
    policy.custom.insert(
        "compliance_mode".to_string(),
        serde_json::Value::String("strict".to_string()),
    );
    policy
        .custom
        .insert("audit_enabled".to_string(), serde_json::Value::Bool(true));
    policy.custom.insert(
        "backup_retention_days".to_string(),
        serde_json::Value::Number(serde_json::Number::from(365)),
    );

    policy
}

fn display_policy_summary(policy: &PolicyConfig) {
    info!("📋 ポリシー概要:");
    info!("  名前: {}", policy.name);
    info!("  バージョン: {}", policy.version);
    info!(
        "  説明: {}",
        policy.description.as_ref().unwrap_or(&"なし".to_string())
    );
    info!("  最終更新: {}", policy.updated_at);

    info!("🔒 セキュリティ設定:");
    info!("  有効: {}", policy.security.enabled);
    info!("  暗号化: {}", policy.security.encryption.algorithm);
    info!(
        "  レート制限: {} req/min, burst: {}",
        policy.security.rate_limiting.requests_per_minute, policy.security.rate_limiting.burst_size
    );
    info!(
        "  最大入力長: {} bytes",
        policy.security.input_validation.max_input_length
    );

    info!("📊 監視設定:");
    info!("  監視間隔: {}秒", policy.monitoring.interval_seconds);
    info!("  ログレベル: {}", policy.monitoring.log_level);
    info!("  アラート: {}", policy.monitoring.alerts_enabled);

    info!("🔐 認証設定:");
    info!("  認証方式: {}", policy.authentication.method);
    info!("  MFA必須: {}", policy.authentication.require_mfa);
    info!(
        "  セッションタイムアウト: {}秒",
        policy.authentication.session_timeout_seconds
    );

    if !policy.custom.is_empty() {
        info!("⚙️ カスタム設定:");
        for (key, value) in &policy.custom {
            info!("  {}: {}", key, value);
        }
    }
}
