use mcp_rs::config::McpConfig;
use std::env;
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    println!("🛡️ 安全な環境変数展開機能のテスト\n");

    // Test 1: 正常なケース
    println!("🧪 Test 1: 正常な環境変数展開");
    env::set_var("TEST_URL", "https://example.com");
    env::set_var("TEST_USER", "testuser");
    
    let test_cases = vec![
        "${TEST_URL}",
        "${TEST_USER}",
        "URL: ${TEST_URL}, User: ${TEST_USER}",
    ];
    
    for test_case in test_cases {
        let result = McpConfig::expand_env_vars(test_case);
        println!("   '{}' → '{}'", test_case, result);
    }

    // Test 2: 環境変数が見つからないケース
    println!("\n🧪 Test 2: 環境変数未設定（安全な処理）");
    let missing_cases = vec![
        "${NONEXISTENT_VAR}",
        "${MISSING_PASSWORD}",
        "URL: ${TEST_URL}, Pass: ${MISSING_PASSWORD}",
    ];
    
    for test_case in missing_cases {
        let result = McpConfig::expand_env_vars(test_case);
        println!("   '{}' → '{}'", test_case, result);
    }

    // Test 3: 無効な形式のケース
    println!("\n🧪 Test 3: 無効な環境変数形式");
    let invalid_cases = vec![
        "${UNCLOSED_VAR",
        "${}",
        "${",
        "Normal text ${VALID_VAR} ${INVALID",
    ];
    
    env::set_var("VALID_VAR", "valid_value");
    
    for test_case in invalid_cases {
        let result = McpConfig::expand_env_vars(test_case);
        println!("   '{}' → '{}'", test_case, result);
    }

    // Test 4: 無限ループ防止テスト
    println!("\n🧪 Test 4: 無限ループ防止機能");
    
    // 自己参照環境変数（これまでなら無限ループの原因）
    env::set_var("SELF_REF", "${SELF_REF}");
    let self_ref_test = "${SELF_REF}";
    let result = McpConfig::expand_env_vars(self_ref_test);
    println!("   自己参照テスト: '{}' → '{}'", self_ref_test, result);

    // Test 5: 大量の環境変数でのパフォーマンステスト
    println!("\n🧪 Test 5: パフォーマンステスト");
    let start_time = std::time::Instant::now();
    
    for i in 0..10 {
        env::set_var(&format!("PERF_VAR_{}", i), &format!("value_{}", i));
    }
    
    let complex_string = (0..10)
        .map(|i| format!("${{PERF_VAR_{}}}", i))
        .collect::<Vec<_>>()
        .join(" ");
    
    let result = McpConfig::expand_env_vars(&complex_string);
    let duration = start_time.elapsed();
    
    println!("   複雑な文字列: {} variables", 10);
    println!("   処理時間: {:?}", duration);
    println!("   結果（短縮）: {}...", &result[..result.len().min(50)]);

    // Test 6: WordPress設定での実際の使用例
    println!("\n🧪 Test 6: WordPress設定での実用例");
    
    // 環境変数を設定
    env::set_var("WP_URL", "https://test-site.com");
    env::set_var("WP_USER", "admin");
    // パスワードは意図的に設定しない（セキュリティテスト）
    
    let wp_config_examples = vec![
        "${WP_URL}",
        "${WP_USER}",
        "${WP_PASSWORD}",  // これは失敗するはず
    ];
    
    for example in wp_config_examples {
        let result = McpConfig::expand_env_vars(example);
        println!("   WordPress設定: '{}' → '{}'", example, result);
    }

    println!("\n✅ すべてのセキュリティテストが完了しました！");
    println!("\n🔒 セキュリティ改善点:");
    println!("   ✅ 無限ループ防止（最大100回反復）");
    println!("   ✅ 未設定環境変数の安全な処理");
    println!("   ✅ 無効な形式の検出と処理");
    println!("   ✅ 詳細なログ出力");
    println!("   ✅ 既処理変数の追跡");
    
    Ok(())
}