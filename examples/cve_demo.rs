//! CVE Provider Demo
//!
//! CVEプロバイダーの完全な機能デモンストレーション
//!
//! # 実行方法
//!
//! ```bash
//! cargo run --example cve_demo
//! ```
//!
//! Note: NVD APIはAPIキー不要（レート制限あり: 5リクエスト/30秒）

use mcp_rs::threat_intelligence::providers::{CVEProvider, ThreatProvider};
use mcp_rs::threat_intelligence::types::{IndicatorType, ProviderConfig, ThreatIndicator};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング初期化
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== CVE Provider Demo ===\n");

    // CVE設定（NVD APIはAPIキー不要）
    let config = ProviderConfig {
        name: "CVE".to_string(),
        enabled: true,
        api_key: String::new(), // APIキー不要
        base_url: "https://services.nvd.nist.gov/rest/json".to_string(),
        timeout_seconds: 30, // NVDは応答が遅い場合があるため長めに設定
        rate_limit_per_minute: 10, // 5リクエスト/30秒 = 10リクエスト/分
        reliability_factor: 0.98, // NVDは信頼性が高い
        provider_specific: HashMap::new(),
    };

    // プロバイダー初期化
    let provider = match CVEProvider::new(config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Failed to initialize CVE provider: {}", e);
            return Err(e.into());
        }
    };

    println!("✅ CVE provider initialized");
    println!("   Provider: {}", provider.name());
    println!("   Base URL: https://services.nvd.nist.gov");
    println!();

    // デモ1: ヘルスチェック
    demo_health_check(&provider).await;

    // デモ2: 有名なCVEを検索（Log4Shell）
    demo_log4shell_cve(&provider).await;

    // デモ3: 別の重要なCVE（Heartbleed）
    demo_heartbleed_cve(&provider).await;

    // デモ4: キーワード検索（Apache）
    demo_keyword_search(&provider).await;

    // デモ5: 複数CVEのバッチチェック
    demo_batch_check(&provider).await;

    // デモ6: キャッシュ統計
    demo_cache_stats(&provider).await;

    // デモ7: 無効なCVE ID処理
    demo_invalid_cve(&provider).await;

    // デモ8: レート制限ステータス
    demo_rate_limit_status(&provider).await;

    println!("\n=== Demo Complete ===");

    Ok(())
}

/// デモ1: ヘルスチェック
async fn demo_health_check(provider: &CVEProvider) {
    println!("📋 Demo 1: Health Check");
    println!("─────────────────────────────────────────");

    match provider.health_check().await {
        Ok(health) => {
            println!("✅ Health check successful");
            println!("   Status: {:?}", health.status);
            println!("   Response time: {}ms", health.response_time_ms);
            println!("   Last check: {}", health.last_check);
            if let Some(error) = health.error_message {
                println!("   Error: {}", error);
            }
        }
        Err(e) => {
            println!("❌ Health check failed: {}", e);
        }
    }

    println!();
}

/// デモ2: Log4Shell CVE検索
async fn demo_log4shell_cve(provider: &CVEProvider) {
    println!("📋 Demo 2: Log4Shell CVE (CVE-2021-44228)");
    println!("─────────────────────────────────────────");

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "CVE-2021-44228".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Log4j RCE vulnerability".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Checking CVE: CVE-2021-44228 (Log4Shell)");

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("⚠️  No information found");
            } else {
                println!("✅ {} CVE record(s) found:", threats.len());
                for (i, threat) in threats.iter().enumerate() {
                    println!("\n   CVE #{}", i + 1);
                    println!("   ├─ Severity: {:?}", threat.severity);
                    println!(
                        "   ├─ Confidence: {:.1}%",
                        threat.confidence_score * 100.0
                    );
                    println!("   ├─ Published: {}", threat.first_seen);
                    println!("   ├─ Last Modified: {}", threat.last_seen);

                    if let Some(desc) = &threat.metadata.description {
                        let desc_preview = if desc.len() > 100 {
                            format!("{}...", &desc[..100])
                        } else {
                            desc.clone()
                        };
                        println!("   ├─ Description: {}", desc_preview);
                    }

                    if let Some(cvss) = threat.metadata.custom_attributes.get("cvss_score") {
                        println!("   ├─ CVSS Score: {}", cvss);
                    }

                    if let Some(vector) = threat.metadata.custom_attributes.get("cvss_vector") {
                        if !vector.is_empty() {
                            println!("   ├─ CVSS Vector: {}", vector);
                        }
                    }

                    if let Some(products) = threat
                        .metadata
                        .custom_attributes
                        .get("affected_products_count")
                    {
                        println!("   └─ Affected Products: {}", products);
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Error checking CVE: {}", e);
        }
    }

    println!();
}

/// デモ3: Heartbleed CVE検索
async fn demo_heartbleed_cve(provider: &CVEProvider) {
    println!("📋 Demo 3: Heartbleed CVE (CVE-2014-0160)");
    println!("─────────────────────────────────────────");

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "CVE-2014-0160".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("OpenSSL Heartbleed vulnerability".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Checking CVE: CVE-2014-0160 (Heartbleed)");

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("⚠️  No information found");
            } else {
                println!("✅ Found vulnerability information");
                for threat in &threats {
                    if let Some(cvss) = threat.metadata.custom_attributes.get("cvss_score") {
                        println!("   CVSS Score: {}", cvss);
                    }
                    println!("   Severity: {:?}", threat.severity);
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    println!();
}

/// デモ4: キーワード検索
async fn demo_keyword_search(provider: &CVEProvider) {
    println!("📋 Demo 4: Keyword Search (remote code execution)");
    println!("─────────────────────────────────────────");

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: "remote code execution".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Searching for RCE vulnerabilities".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Searching for: remote code execution");
    println!("   Note: This may return many results (limited to 10)");

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            println!("✅ Found {} CVE(s)", threats.len());
            for (i, threat) in threats.iter().take(5).enumerate() {
                println!(
                    "   {}. {} - Severity: {:?}",
                    i + 1,
                    threat.metadata.cve_references.first().unwrap_or(&String::from("Unknown")),
                    threat.severity
                );
            }
            if threats.len() > 5 {
                println!("   ... and {} more", threats.len() - 5);
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    println!();
}

/// デモ5: バッチチェック
async fn demo_batch_check(provider: &CVEProvider) {
    println!("📋 Demo 5: Batch CVE Check");
    println!("─────────────────────────────────────────");

    let cve_ids = [
        "CVE-2021-44228", // Log4Shell
        "CVE-2014-0160",  // Heartbleed
        "CVE-2017-5638",  // Apache Struts RCE
    ];

    let indicators: Vec<ThreatIndicator> = cve_ids
        .iter()
        .map(|cve| ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: cve.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: Some("Batch check".to_string()),
            first_seen: chrono::Utc::now(),
        })
        .collect();

    println!("🔍 Checking {} CVEs in batch...", cve_ids.len());

    let start = std::time::Instant::now();
    match provider.batch_check_indicators(&indicators).await {
        Ok(threats) => {
            let duration = start.elapsed();
            println!("✅ Batch check completed in {:.2}s", duration.as_secs_f64());
            println!("   Total CVEs found: {}", threats.len());

            for cve_id in &cve_ids {
                let cve_threats: Vec<_> = threats
                    .iter()
                    .filter(|t| {
                        t.metadata
                            .cve_references
                            .iter()
                            .any(|cve| cve == cve_id)
                    })
                    .collect();

                if cve_threats.is_empty() {
                    println!("   {} - ⚠️  Not found", cve_id);
                } else {
                    let severity = &cve_threats[0].severity;
                    println!("   {} - ✅ Severity: {:?}", cve_id, severity);
                }
            }
        }
        Err(e) => {
            println!("❌ Batch check failed: {}", e);
        }
    }

    println!();
}

/// デモ6: キャッシュ統計
async fn demo_cache_stats(provider: &CVEProvider) {
    println!("📋 Demo 6: Cache Statistics");
    println!("─────────────────────────────────────────");

    let cache_size = provider.cache_size().await;
    println!("📊 Cache Information:");
    println!("   Cached entries: {}", cache_size);
    println!("   Cache TTL: 24 hours");
    println!("   Cache benefit: Faster lookups, reduced API calls");

    println!();
}

/// デモ7: 無効なCVE ID処理
async fn demo_invalid_cve(provider: &CVEProvider) {
    println!("📋 Demo 7: Invalid CVE ID Handling");
    println!("─────────────────────────────────────────");

    let invalid_cves = ["CVE-INVALID", "not-a-cve", "CVE-99-1"];

    for invalid_cve in &invalid_cves {
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: invalid_cve.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        println!("🔍 Testing invalid CVE ID: {}", invalid_cve);

        match provider.check_indicator(&indicator).await {
            Ok(threats) => {
                if threats.is_empty() {
                    println!("   ⚠️  No results (treated as keyword search)");
                } else {
                    println!("   ✅ Found {} result(s) via keyword search", threats.len());
                }
            }
            Err(e) => {
                println!("   ✅ Correctly rejected: {}", e);
            }
        }
    }

    println!();
}

/// デモ8: レート制限ステータス
async fn demo_rate_limit_status(provider: &CVEProvider) {
    println!("📋 Demo 8: Rate Limit Status");
    println!("─────────────────────────────────────────");

    match provider.get_rate_limit_status().await {
        Ok(status) => {
            println!("📊 Rate Limit Information:");
            println!("   Limit per minute: {}", status.limit_per_minute);
            println!("   Remaining requests: {}", status.remaining_requests);
            println!("   Reset at: {}", status.reset_at);
            println!(
                "   Is limited: {}",
                if status.is_limited { "Yes" } else { "No" }
            );
            println!("\n   Note: NVD limit is 5 requests per 30 seconds");
        }
        Err(e) => {
            println!("❌ Failed to get rate limit status: {}", e);
        }
    }

    println!();
}
