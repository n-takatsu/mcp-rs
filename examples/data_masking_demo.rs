//! データマスキングエンジンのデモ
//!
//! 5つのマスキングタイプを実演します。

use mcp_rs::handlers::database::{
    ColumnPattern, DataMaskingEngine, HashAlgorithm, MaskingContext, MaskingPolicy,
    MaskingPurpose, MaskingRule, MaskingType,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== データマスキングエンジン デモ ===\n");

    // エンジンを作成
    let engine = DataMaskingEngine::new();

    // ポリシーを定義
    let policy = MaskingPolicy {
        name: "demo_policy".to_string(),
        roles: vec![],
        permissions: vec![],
        time_constraints: None,
        network_constraints: None,
        rules: vec![
            // 1. 完全マスク (パスワード)
            MaskingRule {
                name: "password_full_mask".to_string(),
                description: Some("パスワードを完全マスク".to_string()),
                masking_type: MaskingType::FullMask,
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["password".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 100,
                enabled: true,
            },
            // 2. 部分マスク (クレジットカード)
            MaskingRule {
                name: "credit_card_partial_mask".to_string(),
                description: Some("クレジットカード番号を部分マスク".to_string()),
                masking_type: MaskingType::PartialMask {
                    prefix_visible: 0,
                    suffix_visible: 4,
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["credit_card".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 90,
                enabled: true,
            },
            // 3. ハッシュマスク (メール)
            MaskingRule {
                name: "email_hash_mask".to_string(),
                description: Some("メールアドレスをハッシュ化".to_string()),
                masking_type: MaskingType::HashMask {
                    algorithm: HashAlgorithm::Sha256,
                    display_length: 16,
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["email".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 80,
                enabled: true,
            },
            // 4. 形式保持マスク (電話番号)
            MaskingRule {
                name: "phone_format_preserving".to_string(),
                description: Some("電話番号を形式保持マスク".to_string()),
                masking_type: MaskingType::FormatPreserving {
                    format_pattern: "###-####-####".to_string(),
                    mask_char: '*',
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["phone".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 70,
                enabled: true,
            },
            // 5. トークンマスク (SSN)
            MaskingRule {
                name: "ssn_token_mask".to_string(),
                description: Some("SSNをトークン化".to_string()),
                masking_type: MaskingType::TokenMask {
                    prefix: "SSN_TOKEN".to_string(),
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["ssn".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 95,
                enabled: true,
            },
        ],
    };

    engine.add_policy(policy).await?;

    // テストデータ
    let mut data = json!({
        "id": 12345,
        "name": "John Doe",
        "email": "john.doe@example.com",
        "password": "SecretPassword123!",
        "credit_card": "1234-5678-9012-3456",
        "phone": "090-1234-5678",
        "ssn": "123-45-6789",
        "address": "123 Main Street"
    });

    println!("📋 元のデータ:");
    println!("{}\n", serde_json::to_string_pretty(&data)?);

    // マスキングコンテキスト
    let context = MaskingContext {
        roles: vec!["user".to_string()],
        permissions: vec!["read".to_string()],
        source_ip: Some("192.168.1.100".to_string()),
        timestamp: chrono::Utc::now(),
        purpose: MaskingPurpose::Normal,
    };

    // マスキング適用
    engine.mask_query_result(&mut data, &context).await?;

    println!("🔒 マスキング後のデータ:");
    println!("{}\n", serde_json::to_string_pretty(&data)?);

    // 統計情報
    let stats = engine.get_statistics().await;
    println!("📊 統計情報:");
    println!("  総マスキング数: {}", stats.total_maskings);
    println!("  ポリシー数: {}", stats.policy_count);
    println!("  キャッシュサイズ: {}", stats.cache_size);
    println!("\n  マスキングタイプ別:");
    for (mask_type, count) in stats.masking_type_counts {
        println!("    {}: {}", mask_type, count);
    }
    println!("\n  カラム別:");
    for (column, count) in stats.column_counts {
        println!("    {}: {}", column, count);
    }

    // 監査ログ
    println!("\n📝 監査ログ:");
    let audit_log = engine.get_audit_log(Some(10)).await;
    for (i, entry) in audit_log.iter().enumerate() {
        println!(
            "  {}. [{}] {} - ルール: {} (ロール: {:?})",
            i + 1,
            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.column_name,
            entry.rule_name,
            entry.user_roles
        );
    }

    Ok(())
}
