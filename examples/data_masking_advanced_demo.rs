//! データマスキングエンジンの拡張機能デモ
//!
//! カスタムマスカー、バッチ処理、結果キャッシュを実演します。

use mcp_rs::handlers::database::{
    ColumnPattern, CustomMasker, DataMaskingEngine, MaskingContext, MaskingPolicy,
    MaskingPurpose, MaskingRule, MaskingType,
};
use serde_json::json;
use std::sync::Arc;

/// カスタムマスカー: メールアドレスのドメイン部分のみ表示
struct EmailDomainMasker;

#[async_trait::async_trait]
impl CustomMasker for EmailDomainMasker {
    fn name(&self) -> &str {
        "email_domain_masker"
    }

    async fn mask(&self, value: &str, _context: &MaskingContext) -> anyhow::Result<String> {
        if let Some(at_pos) = value.find('@') {
            let domain = &value[at_pos..];
            Ok(format!("***{}", domain))
        } else {
            Ok("***".to_string())
        }
    }
}

/// カスタムマスカー: 日本の電話番号専用マスキング
struct JapanesePhoneMasker;

#[async_trait::async_trait]
impl CustomMasker for JapanesePhoneMasker {
    fn name(&self) -> &str {
        "japanese_phone_masker"
    }

    async fn mask(&self, value: &str, _context: &MaskingContext) -> anyhow::Result<String> {
        // 090-1234-5678 -> 090-****-5678
        let parts: Vec<&str> = value.split('-').collect();
        if parts.len() == 3 {
            Ok(format!("{}-****-{}", parts[0], parts[2]))
        } else {
            Ok("***-****-****".to_string())
        }
    }
}

/// カスタムマスカー: ロールベース可変マスキング
struct RoleBasedMasker;

#[async_trait::async_trait]
impl CustomMasker for RoleBasedMasker {
    fn name(&self) -> &str {
        "role_based_masker"
    }

    async fn mask(&self, value: &str, context: &MaskingContext) -> anyhow::Result<String> {
        if context.roles.contains(&"admin".to_string()) {
            // 管理者: 完全表示
            Ok(value.to_string())
        } else if context.roles.contains(&"manager".to_string()) {
            // マネージャー: 部分表示
            let len = value.len();
            if len > 4 {
                Ok(format!("{}***{}", &value[..2], &value[len - 2..]))
            } else {
                Ok("***".to_string())
            }
        } else {
            // 一般ユーザー: 完全マスク
            Ok("***".to_string())
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== データマスキングエンジン 拡張機能デモ ===\n");

    // エンジンを作成
    let mut engine = DataMaskingEngine::new();

    // カスタムマスカーを登録
    engine
        .register_custom_masker(Arc::new(EmailDomainMasker))
        .await?;
    engine
        .register_custom_masker(Arc::new(JapanesePhoneMasker))
        .await?;
    engine
        .register_custom_masker(Arc::new(RoleBasedMasker))
        .await?;

    println!("✅ カスタムマスカーを3つ登録:");
    println!("  1. EmailDomainMasker - ドメイン部分のみ表示");
    println!("  2. JapanesePhoneMasker - 日本の電話番号専用");
    println!("  3. RoleBasedMasker - ロールベース可変マスキング\n");

    // ポリシーを定義
    let policy = MaskingPolicy {
        name: "custom_masker_demo".to_string(),
        roles: vec![],
        permissions: vec![],
        time_constraints: None,
        network_constraints: None,
        rules: vec![
            MaskingRule {
                name: "email_custom".to_string(),
                description: Some("メールアドレスのカスタムマスキング".to_string()),
                masking_type: MaskingType::Custom {
                    name: "email_domain_masker".to_string(),
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["email".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 100,
                enabled: true,
            },
            MaskingRule {
                name: "phone_custom".to_string(),
                description: Some("電話番号のカスタムマスキング".to_string()),
                masking_type: MaskingType::Custom {
                    name: "japanese_phone_masker".to_string(),
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["phone".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 90,
                enabled: true,
            },
            MaskingRule {
                name: "salary_custom".to_string(),
                description: Some("給与のロールベースマスキング".to_string()),
                masking_type: MaskingType::Custom {
                    name: "role_based_masker".to_string(),
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["salary".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 80,
                enabled: true,
            },
        ],
    };

    engine.add_policy(policy).await?;

    // デモ1: カスタムマスカーのテスト
    println!("📋 デモ1: カスタムマスカー");
    println!("─────────────────────────────");

    let mut data = json!({
        "id": 1001,
        "name": "山田太郎",
        "email": "yamada.taro@example.com",
        "phone": "090-1234-5678",
        "salary": "5000000"
    });

    println!("元のデータ:");
    println!("{}\n", serde_json::to_string_pretty(&data)?);

    // 一般ユーザーのコンテキスト
    let user_context = MaskingContext {
        roles: vec!["user".to_string()],
        permissions: vec![],
        source_ip: Some("192.168.1.100".to_string()),
        timestamp: chrono::Utc::now(),
        purpose: MaskingPurpose::Normal,
    };

    let mut data_clone = data.clone();
    engine
        .mask_query_result(&mut data_clone, &user_context)
        .await?;

    println!("🔒 一般ユーザーとしてマスキング:");
    println!("{}\n", serde_json::to_string_pretty(&data_clone)?);

    // マネージャーのコンテキスト
    let manager_context = MaskingContext {
        roles: vec!["manager".to_string()],
        permissions: vec![],
        source_ip: Some("192.168.1.100".to_string()),
        timestamp: chrono::Utc::now(),
        purpose: MaskingPurpose::Normal,
    };

    let mut data_clone = data.clone();
    engine
        .mask_query_result(&mut data_clone, &manager_context)
        .await?;

    println!("👔 マネージャーとしてマスキング:");
    println!("{}\n", serde_json::to_string_pretty(&data_clone)?);

    // 管理者のコンテキスト
    let admin_context = MaskingContext {
        roles: vec!["admin".to_string()],
        permissions: vec![],
        source_ip: Some("192.168.1.100".to_string()),
        timestamp: chrono::Utc::now(),
        purpose: MaskingPurpose::Normal,
    };

    let mut data_clone = data.clone();
    engine
        .mask_query_result(&mut data_clone, &admin_context)
        .await?;

    println!("👑 管理者としてマスキング:");
    println!("{}\n", serde_json::to_string_pretty(&data_clone)?);

    // デモ2: バッチ処理
    println!("\n📦 デモ2: バッチ処理 (並列マスキング)");
    println!("─────────────────────────────");

    let mut batch_data = vec![
        json!({
            "id": 1,
            "email": "user1@example.com",
            "phone": "090-1111-2222",
            "salary": "4000000"
        }),
        json!({
            "id": 2,
            "email": "user2@example.com",
            "phone": "080-3333-4444",
            "salary": "4500000"
        }),
        json!({
            "id": 3,
            "email": "user3@example.com",
            "phone": "070-5555-6666",
            "salary": "5500000"
        }),
        json!({
            "id": 4,
            "email": "user4@example.com",
            "phone": "090-7777-8888",
            "salary": "6000000"
        }),
        json!({
            "id": 5,
            "email": "user5@example.com",
            "phone": "080-9999-0000",
            "salary": "5200000"
        }),
    ];

    println!("5件のレコードをバッチ処理でマスキング...\n");

    let start = std::time::Instant::now();
    engine.mask_batch(&mut batch_data, &user_context).await?;
    let duration = start.elapsed();

    println!("⚡ バッチ処理完了: {:?}", duration);
    println!("\nマスキング結果 (最初の2件):");
    for (i, data) in batch_data.iter().take(2).enumerate() {
        println!("  レコード {}:", i + 1);
        println!("    {}", serde_json::to_string(data)?);
    }

    // デモ3: 結果キャッシュ
    println!("\n\n💾 デモ3: 結果キャッシュ");
    println!("─────────────────────────────");

    let test_data = json!({
        "email": "test@example.com",
        "phone": "090-1234-5678",
        "salary": "5000000"
    });

    // キャッシュ有効時
    engine.enable_result_cache();
    let mut cached_data = test_data.clone();
    let start = std::time::Instant::now();
    engine
        .mask_query_result(&mut cached_data, &user_context)
        .await?;
    let cached_duration = start.elapsed();
    println!("✅ キャッシュ有効 (初回): {:?}", cached_duration);

    // 2回目 (キャッシュヒット)
    let mut cached_data = test_data.clone();
    let start = std::time::Instant::now();
    engine
        .mask_query_result(&mut cached_data, &user_context)
        .await?;
    let cached_duration2 = start.elapsed();
    println!("⚡ キャッシュ有効 (2回目): {:?}", cached_duration2);

    // キャッシュ無効時
    engine.disable_result_cache();
    let mut uncached_data = test_data.clone();
    let start = std::time::Instant::now();
    engine
        .mask_query_result(&mut uncached_data, &user_context)
        .await?;
    let uncached_duration = start.elapsed();
    println!("❌ キャッシュ無効: {:?}", uncached_duration);

    // 統計情報
    println!("\n\n📊 統計情報");
    println!("─────────────────────────────");
    let stats = engine.get_statistics().await;
    println!("総マスキング数: {}", stats.total_maskings);
    println!("ポリシー数: {}", stats.policy_count);
    println!("キャッシュサイズ: {}", stats.cache_size);

    println!("\n✨ デモ完了!");

    Ok(())
}
