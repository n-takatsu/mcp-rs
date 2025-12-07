//! AbuseIPDB Provider Demo
//!
//! AbuseIPDBプロバイダーの完全な機能デモンストレーション
//!
//! # 実行方法
//!
//! ```bash
//! # 環境変数でAPIキーを設定
//! $env:ABUSEIPDB_API_KEY="your_api_key_here"
//! cargo run --example abuseipdb_demo
//! ```

use mcp_rs::threat_intelligence::providers::{AbuseIPDBProvider, ThreatProvider};
use mcp_rs::threat_intelligence::types::{IndicatorType, ProviderConfig, ThreatIndicator};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング初期化
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== AbuseIPDB Provider Demo ===\n");

    // APIキーを環境変数から取得
    let api_key = std::env::var("ABUSEIPDB_API_KEY").unwrap_or_else(|_| {
        println!("⚠️  Warning: ABUSEIPDB_API_KEY not set. Using dummy key for demo.");
        println!("   Set it with: $env:ABUSEIPDB_API_KEY=\"your_key\"\n");
        "dummy_api_key_for_demo".to_string()
    });

    // AbuseIPDB設定
    let config = ProviderConfig {
        name: "AbuseIPDB".to_string(),
        enabled: true,
        api_key: api_key.clone(),
        base_url: "https://api.abuseipdb.com".to_string(),
        timeout_seconds: 10,
        rate_limit_per_minute: 60, // 無料プランの制限
        reliability_factor: 0.95,
        provider_specific: HashMap::new(),
    };

    // プロバイダー初期化
    let provider = match AbuseIPDBProvider::new(config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Failed to initialize AbuseIPDB provider: {}", e);
            return Err(e.into());
        }
    };

    println!("✅ AbuseIPDB provider initialized");
    println!("   Provider: {}", provider.name());
    println!();

    // デモ1: ヘルスチェック
    demo_health_check(&provider).await;

    // デモ2: 既知の悪意あるIPチェック（例: ブルートフォース攻撃で報告されているIP）
    demo_malicious_ip_check(&provider).await;

    // デモ3: 安全なIPチェック（例: Google DNS）
    demo_safe_ip_check(&provider).await;

    // デモ4: 複数IPのバッチチェック
    demo_batch_check(&provider).await;

    // デモ5: レート制限ステータス確認
    demo_rate_limit_status(&provider).await;

    // デモ6: 無効なIP形式のエラーハンドリング
    demo_invalid_ip_handling(&provider).await;

    // デモ7: IPv6アドレスのチェック
    demo_ipv6_check(&provider).await;

    println!("\n=== Demo Complete ===");

    Ok(())
}

/// デモ1: ヘルスチェック
async fn demo_health_check(provider: &AbuseIPDBProvider) {
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

/// デモ2: 既知の悪意あるIPチェック
async fn demo_malicious_ip_check(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 2: Malicious IP Check");
    println!("─────────────────────────────────────────");

    // 注意: これは例示用のIPアドレスです。実際の悪意あるIPは時間とともに変化します
    let malicious_ip = "118.25.6.39"; // 中国のIPで過去に報告があったもの

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: malicious_ip.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Testing known malicious IP".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Checking IP: {}", malicious_ip);

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("✅ No threats found for this IP (may be clean or not reported)");
            } else {
                println!("⚠️  {} threat(s) detected:", threats.len());
                for (i, threat) in threats.iter().enumerate() {
                    println!("\n   Threat #{}", i + 1);
                    println!("   ├─ Type: {:?}", threat.threat_type);
                    println!("   ├─ Severity: {:?}", threat.severity);
                    println!("   ├─ Confidence: {:.1}%", threat.confidence_score * 100.0);
                    if let Some(desc) = &threat.metadata.description {
                        println!("   ├─ Description: {}", desc);
                    }
                    if let Some(geo) = &threat.metadata.geolocation {
                        println!("   ├─ Location: {}, {}", geo.country_name, geo.country_code);
                    }
                    if let Some(reports) = threat.metadata.custom_attributes.get("total_reports") {
                        println!("   ├─ Total Reports: {}", reports);
                    }
                    if let Some(score) = threat
                        .metadata
                        .custom_attributes
                        .get("abuse_confidence_score")
                    {
                        println!("   └─ Abuse Score: {}", score);
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Error checking IP: {}", e);
        }
    }

    println!();
}

/// デモ3: 安全なIPチェック
async fn demo_safe_ip_check(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 3: Safe IP Check");
    println!("─────────────────────────────────────────");

    let safe_ip = "8.8.8.8"; // Google Public DNS

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: safe_ip.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Testing known safe IP".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Checking IP: {} (Google DNS)", safe_ip);

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("✅ IP is clean - no threats detected");
            } else {
                println!("⚠️  Unexpected: {} threat(s) found", threats.len());
            }
        }
        Err(e) => {
            println!("❌ Error checking IP: {}", e);
        }
    }

    println!();
}

/// デモ4: 複数IPのバッチチェック
async fn demo_batch_check(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 4: Batch IP Check");
    println!("─────────────────────────────────────────");

    let ips = [
        "1.1.1.1",       // Cloudflare DNS
        "8.8.4.4",       // Google DNS
        "198.51.100.42", // Test IP
    ];

    let indicators: Vec<ThreatIndicator> = ips
        .iter()
        .map(|ip| ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: ip.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: Some("Batch check".to_string()),
            first_seen: chrono::Utc::now(),
        })
        .collect();

    println!("🔍 Checking {} IPs in batch...", ips.len());

    let start = std::time::Instant::now();
    match provider.batch_check_indicators(&indicators).await {
        Ok(threats) => {
            let duration = start.elapsed();
            println!("✅ Batch check completed in {:.2}s", duration.as_secs_f64());
            println!("   Total threats detected: {}", threats.len());

            for ip in &ips {
                let ip_threats: Vec<_> = threats
                    .iter()
                    .filter(|t| t.indicators.iter().any(|ind| ind.value == *ip))
                    .collect();

                if ip_threats.is_empty() {
                    println!("   {} - ✅ Clean", ip);
                } else {
                    println!("   {} - ⚠️  {} threat(s)", ip, ip_threats.len());
                }
            }
        }
        Err(e) => {
            println!("❌ Batch check failed: {}", e);
        }
    }

    println!();
}

/// デモ5: レート制限ステータス確認
async fn demo_rate_limit_status(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 5: Rate Limit Status");
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
        }
        Err(e) => {
            println!("❌ Failed to get rate limit status: {}", e);
        }
    }

    println!();
}

/// デモ6: 無効なIP形式のエラーハンドリング
async fn demo_invalid_ip_handling(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 6: Invalid IP Format Handling");
    println!("─────────────────────────────────────────");

    let invalid_ips = ["not-an-ip", "999.999.999.999", "malformed.ip.address"];

    for invalid_ip in &invalid_ips {
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: invalid_ip.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        println!("🔍 Testing invalid IP: {}", invalid_ip);

        match provider.check_indicator(&indicator).await {
            Ok(_) => {
                println!("   ⚠️  Unexpected success");
            }
            Err(e) => {
                println!("   ✅ Correctly rejected: {}", e);
            }
        }
    }

    println!();
}

/// デモ7: IPv6アドレスのチェック
async fn demo_ipv6_check(provider: &AbuseIPDBProvider) {
    println!("📋 Demo 7: IPv6 Address Check");
    println!("─────────────────────────────────────────");

    let ipv6 = "2001:4860:4860::8888"; // Google DNS IPv6

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: ipv6.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Testing IPv6 support".to_string()),
        first_seen: chrono::Utc::now(),
    };

    println!("🔍 Checking IPv6: {}", ipv6);

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("✅ IPv6 address is clean");
            } else {
                println!("⚠️  {} threat(s) detected", threats.len());
            }
        }
        Err(e) => {
            println!("❌ Error checking IPv6: {}", e);
        }
    }

    println!();
}
