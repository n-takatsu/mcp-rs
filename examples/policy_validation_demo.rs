use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing_subscriber;

use mcp_rs::policy_application::{PolicyApplicationEngine, PolicyApplicationEvent};
use mcp_rs::policy_config::PolicyConfig;
use mcp_rs::policy_validation::{PolicyValidationEngine, ValidationLevel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    println!("🔍 Policy Validation System Demo");
    println!("================================");
    println!();

    // テスト用ディレクトリの作成
    let test_dir = "test_policies_validation";
    if Path::new(test_dir).exists() {
        std::fs::remove_dir_all(test_dir)?;
    }
    std::fs::create_dir_all(test_dir)?;

    // 1. 有効なポリシーファイルを作成
    create_valid_policy_file(&format!("{}/valid_policy.toml", test_dir)).await?;

    // 2. 無効なポリシーファイルを作成
    create_invalid_policy_file(&format!("{}/invalid_policy.toml", test_dir)).await?;

    // 3. 警告付きポリシーファイルを作成
    create_warning_policy_file(&format!("{}/warning_policy.toml", test_dir)).await?;

    // 4. 本番環境向け厳格ポリシーを作成
    create_production_policy_file(&format!("{}/production_policy.toml", test_dir)).await?;

    // ポリシー適用エンジンを作成（厳格な検証レベル）
    let mut engine =
        PolicyApplicationEngine::with_validation_level(test_dir, ValidationLevel::Custom);

    // ポリシーファイルを追加
    engine.add_policy_file(&format!("{}/valid_policy.toml", test_dir));
    engine.add_policy_file(&format!("{}/invalid_policy.toml", test_dir));
    engine.add_policy_file(&format!("{}/warning_policy.toml", test_dir));
    engine.add_policy_file(&format!("{}/production_policy.toml", test_dir));

    // イベント監視を開始
    let mut event_receiver = engine.subscribe();
    let event_counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let event_counter_clone = event_counter.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            let count = event_counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            print_policy_event(&event, count);
        }
    });

    println!("📋 各ポリシーファイルの検証テストを実行中...");
    println!();

    // エンジンを起動（初期ポリシー読み込み）
    engine.start().await?;

    sleep(Duration::from_millis(2000)).await;

    // 検証統計を表示
    let validation_stats = engine.get_validation_stats().await;
    print_validation_stats(&validation_stats);

    println!();
    println!("🔧 ライブ検証テスト: 無効なポリシーファイルを修正");
    println!("─────────────────────────────────────────────");

    // 無効なポリシーファイルを修正（ライブ検証テスト）
    fix_invalid_policy_file(&format!("{}/invalid_policy.toml", test_dir)).await?;

    sleep(Duration::from_millis(2000)).await;

    println!();
    println!("📊 個別検証エンジンテスト");
    println!("─────────────────────────");

    // 個別の検証エンジンテスト
    test_individual_validation_engine().await?;

    sleep(Duration::from_millis(1000)).await;

    // エンジンを停止
    engine.stop();

    // クリーンアップ
    std::fs::remove_dir_all(test_dir)?;

    println!();
    let total_events = event_counter.load(std::sync::atomic::Ordering::SeqCst);
    println!("✅ Policy Validation System Demo 完了!");
    println!("   - 処理されたイベント数: {} 個", total_events);
    println!("   - すべての検証テストが正常に実行されました");

    Ok(())
}

/// 有効なポリシーファイルを作成
async fn create_valid_policy_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let policy_content = format!(
        r#"
id = "valid-policy-001"
name = "Valid Test Policy"
version = "1.0.0"
description = "完全に有効なテストポリシー"
created_at = "{}"
updated_at = "{}"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 256
pbkdf2_iterations = 100000

[security.tls]
enforce = true
min_version = "TLSv1.3"
cipher_suites = ["TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]

[security.rate_limiting]
enabled = true
requests_per_minute = 60
burst_size = 10
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 1048576
sql_injection_protection = true
xss_protection = true

[monitoring]
enabled = true
interval_seconds = 30
log_level = "info"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 1.0
buffer_size = 1000

[authentication]
enabled = true
method = "oauth2"
require_mfa = true
session_timeout_seconds = 3600

[custom]
environment = "production"
compliance_mode = "strict"
"#,
        now.to_rfc3339(),
        now.to_rfc3339()
    );

    tokio::fs::write(path, policy_content).await?;
    Ok(())
}

/// 無効なポリシーファイルを作成
async fn create_invalid_policy_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let policy_content = format!(
        r#"
id = ""
name = ""
version = "invalid"
description = "意図的に無効なテストポリシー"
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
requests_per_minute = 2000
burst_size = 5000
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 20971520
sql_injection_protection = false
xss_protection = false

[monitoring]
enabled = true
interval_seconds = 1200
log_level = "debug"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 0.0
buffer_size = 100

[authentication]
enabled = false
method = "none"
require_mfa = false
session_timeout_seconds = 120

[custom]
environment = "unknown_env"
compliance_mode = "invalid_mode"
"#,
        now.to_rfc3339(),
        now.to_rfc3339()
    );

    tokio::fs::write(path, policy_content).await?;
    Ok(())
}

/// 警告付きポリシーファイルを作成
async fn create_warning_policy_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let policy_content = format!(
        r#"
id = "warning-policy-001"
name = "Warning Test Policy"
version = "1.2"
description = "警告が発生するテストポリシー"
created_at = "{}"
updated_at = "{}"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 256
pbkdf2_iterations = 50000

[security.tls]
enforce = true
min_version = "TLSv1.2"
cipher_suites = ["TLS_AES_256_GCM_SHA384"]

[security.rate_limiting]
enabled = true
requests_per_minute = 500
burst_size = 300
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 5242880
sql_injection_protection = true
xss_protection = true

[monitoring]
enabled = true
interval_seconds = 600
log_level = "warn"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 0.5
buffer_size = 500

[authentication]
enabled = true
method = "basic"
require_mfa = false
session_timeout_seconds = 7200

[custom]
environment = "staging"
compliance_mode = "standard"
"#,
        now.to_rfc3339(),
        now.to_rfc3339()
    );

    tokio::fs::write(path, policy_content).await?;
    Ok(())
}

/// 本番環境向け厳格ポリシーを作成
async fn create_production_policy_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let policy_content = format!(
        r#"
id = "production-policy-001"
name = "Production Security Policy"
version = "2.1.0"
description = "本番環境向け厳格セキュリティポリシー"
created_at = "{}"
updated_at = "{}"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 256
pbkdf2_iterations = 150000

[security.tls]
enforce = true
min_version = "TLSv1.3"
cipher_suites = [
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
    "TLS_AES_128_GCM_SHA256"
]

[security.rate_limiting]
enabled = true
requests_per_minute = 100
burst_size = 20
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 524288
sql_injection_protection = true
xss_protection = true

[monitoring]
enabled = true
interval_seconds = 60
log_level = "warn"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 1.0
buffer_size = 2000

[authentication]
enabled = true
method = "saml"
require_mfa = true
session_timeout_seconds = 1800

[custom]
environment = "production"
compliance_mode = "strict"
"#,
        now.to_rfc3339(),
        now.to_rfc3339()
    );

    tokio::fs::write(path, policy_content).await?;
    Ok(())
}

/// 無効なポリシーファイルを修正
async fn fix_invalid_policy_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let fixed_policy_content = format!(
        r#"
id = "fixed-policy-001"
name = "Fixed Test Policy"
version = "1.0.1"
description = "修正されたテストポリシー"
created_at = "{}"
updated_at = "{}"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 256
pbkdf2_iterations = 100000

[security.tls]
enforce = true
min_version = "TLSv1.3"
cipher_suites = ["TLS_AES_256_GCM_SHA384"]

[security.rate_limiting]
enabled = true
requests_per_minute = 120
burst_size = 25
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
buffer_size = 1500

[authentication]
enabled = true
method = "jwt"
require_mfa = true
session_timeout_seconds = 3600

[custom]
environment = "development"
compliance_mode = "standard"
"#,
        now.to_rfc3339(),
        now.to_rfc3339()
    );

    tokio::fs::write(path, fixed_policy_content).await?;
    Ok(())
}

/// 個別検証エンジンテスト
async fn test_individual_validation_engine() -> Result<(), Box<dyn std::error::Error>> {
    println!("📝 個別PolicyValidationEngineテスト:");

    let mut validation_engine = PolicyValidationEngine::new();

    // テストポリシーを作成
    let mut test_policy = PolicyConfig::default();
    test_policy.id = "test-validation-001".to_string();
    test_policy.name = "Individual Validation Test".to_string();
    test_policy.version = "1.0.0".to_string();

    // 各検証レベルでテスト
    let levels = vec![
        (ValidationLevel::Basic, "Basic"),
        (ValidationLevel::Standard, "Standard"),
        (ValidationLevel::Strict, "Strict"),
        (ValidationLevel::Custom, "Custom"),
    ];

    for (level, level_name) in levels {
        let result = validation_engine.validate_policy(&test_policy, level).await;

        println!("  🔸 {} 検証:", level_name);
        println!(
            "     - 結果: {}",
            if result.is_valid {
                "✅ 有効"
            } else {
                "❌ 無効"
            }
        );
        println!("     - エラー数: {}", result.errors.len());
        println!("     - 警告数: {}", result.warnings.len());
        println!("     - 推奨事項数: {}", result.recommendations.len());
        println!("     - 検証時間: {}ms", result.validation_time_ms);
    }

    let stats = validation_engine.get_stats();
    println!();
    println!("  📊 検証エンジン統計:");
    println!("     - 総検証回数: {}", stats.total_validations);
    println!("     - 成功回数: {}", stats.successful_validations);
    println!("     - 失敗回数: {}", stats.failed_validations);
    println!(
        "     - 平均検証時間: {:.2}ms",
        stats.average_validation_time_ms
    );

    Ok(())
}

/// ポリシーイベントを表示
fn print_policy_event(event: &PolicyApplicationEvent, count: u32) {
    let event_type_str = match &event.event_type {
        mcp_rs::policy_application::PolicyApplicationEventType::Loaded => "📥 ポリシー読み込み",
        mcp_rs::policy_application::PolicyApplicationEventType::Applied => "✅ ポリシー適用",
        mcp_rs::policy_application::PolicyApplicationEventType::ApplicationFailed => "❌ 適用失敗",
        mcp_rs::policy_application::PolicyApplicationEventType::ValidationFailed => "🚫 検証失敗",
    };

    let result_str = match &event.result {
        mcp_rs::policy_application::PolicyApplicationResult::Success => "成功".to_string(),
        mcp_rs::policy_application::PolicyApplicationResult::SuccessWithWarnings(warnings) => {
            format!("成功（警告{}個）", warnings.len())
        }
        mcp_rs::policy_application::PolicyApplicationResult::Failed(err) => {
            format!("失敗: {}", err)
        }
    };

    println!(
        "📨 イベント #{}: {} - {} ({})",
        count, event_type_str, event.policy_id, result_str
    );

    if !event.changed_sections.is_empty() {
        println!("   変更セクション: {:?}", event.changed_sections);
    }
}

/// 検証統計を表示
fn print_validation_stats(stats: &mcp_rs::policy_validation::ValidationStats) {
    println!("📊 ポリシー検証統計:");
    println!("─────────────────");
    println!("   - 総検証回数: {}", stats.total_validations);
    println!("   - 成功回数: {}", stats.successful_validations);
    println!("   - 失敗回数: {}", stats.failed_validations);
    println!(
        "   - 平均検証時間: {:.2}ms",
        stats.average_validation_time_ms
    );

    if let Some(last_time) = &stats.last_validation_time {
        println!("   - 最後の検証: {}", last_time.format("%H:%M:%S"));
    }
}
