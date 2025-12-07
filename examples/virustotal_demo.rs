//! VirusTotal Provider Demo
//!
//! VirusTotalプロバイダーの完全な機能デモンストレーション
//!
//! # 実行方法
//!
//! ```bash
//! # 環境変数でAPIキーを設定
//! $env:VIRUSTOTAL_API_KEY="your_api_key_here"
//! cargo run --example virustotal_demo
//! ```

use mcp_rs::threat_intelligence::providers::{ThreatProvider, VirusTotalProvider};
use mcp_rs::threat_intelligence::types::{IndicatorType, ProviderConfig, ThreatIndicator};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング初期化
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== VirusTotal Provider Demo ===\n");

    // APIキーを環境変数から取得
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").unwrap_or_else(|_| {
        println!("⚠️  Warning: VIRUSTOTAL_API_KEY not set. Using dummy key for demo.");
        println!("   Set it with: $env:VIRUSTOTAL_API_KEY=\"your_key\"\n");
        "dummy_api_key_for_demo".to_string()
    });

    // VirusTotal設定
    let config = ProviderConfig {
        name: "VirusTotal".to_string(),
        enabled: true,
        api_key: api_key.clone(),
        base_url: "https://www.virustotal.com/api/v3".to_string(),
        timeout_seconds: 15,
        rate_limit_per_minute: 4, // 無料プランの制限
        reliability_factor: 0.98,
        provider_specific: HashMap::new(),
    };

    // プロバイダー初期化
    let provider = match VirusTotalProvider::new(config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Failed to initialize VirusTotal provider: {}", e);
            return Err(e.into());
        }
    };

    println!("✅ VirusTotal provider initialized");
    println!("   Provider: {}\n", provider.name());

    // Demo 1: Health Check
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
        Err(e) => println!("❌ Health check failed: {}", e),
    }
    println!();

    // Demo 2: Malicious Domain Check
    println!("📋 Demo 2: Malicious Domain Check");
    println!("─────────────────────────────────────────");
    let malicious_domain = "027.ru"; // 既知の悪意のあるドメイン
    println!("🔍 Checking domain: {}", malicious_domain);

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: malicious_domain.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Known malicious domain".to_string()),
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("   ✅ Domain is clean");
            } else {
                for threat in threats {
                    println!("   ⚠️  Threat detected:");
                    println!("      Type: {:?}", threat.threat_type);
                    println!("      Severity: {:?}", threat.severity);
                    println!("      Confidence: {:.1}%", threat.confidence_score * 100.0);
                    if let Some(desc) = &threat.metadata.description {
                        println!("      Description: {}", desc);
                    }
                }
            }
        }
        Err(e) => println!("   ❌ Error checking domain: {}", e),
    }
    println!();

    // Demo 3: Safe Domain Check
    println!("📋 Demo 3: Safe Domain Check");
    println!("─────────────────────────────────────────");
    let safe_domain = "google.com";
    println!("🔍 Checking domain: {}", safe_domain);

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: safe_domain.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: Some("Legitimate domain".to_string()),
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("   ✅ Domain is clean");
            } else {
                println!("   ⚠️  {} threat(s) detected", threats.len());
            }
        }
        Err(e) => println!("   ❌ Error checking domain: {}", e),
    }
    println!();

    // Demo 4: IP Address Check
    println!("📋 Demo 4: IP Address Check");
    println!("─────────────────────────────────────────");
    let test_ip = "1.1.1.1"; // Cloudflare DNS
    println!("🔍 Checking IP: {}", test_ip);

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: test_ip.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("   ✅ IP is clean");
            } else {
                println!("   ⚠️  {} threat(s) detected", threats.len());
            }
        }
        Err(e) => println!("   ❌ Error checking IP: {}", e),
    }
    println!();

    // Demo 5: URL Check
    println!("📋 Demo 5: URL Check");
    println!("─────────────────────────────────────────");
    let test_url = "https://example.com/";
    println!("🔍 Checking URL: {}", test_url);

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Url,
        value: test_url.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("   ✅ URL is clean");
            } else {
                println!("   ⚠️  {} threat(s) detected", threats.len());
            }
        }
        Err(e) => println!("   ❌ Error checking URL: {}", e),
    }
    println!();

    // Demo 6: File Hash Check
    println!("📋 Demo 6: File Hash Check");
    println!("─────────────────────────────────────────");
    let test_hash = "44d88612fea8a8f36de82e1278abb02f"; // EICAR test file MD5
    println!("🔍 Checking file hash: {}", test_hash);

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: test_hash.to_string(),
        pattern: None,
        tags: vec!["md5".to_string()],
        context: Some("Test file hash".to_string()),
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            if threats.is_empty() {
                println!("   ✅ File is clean");
            } else {
                for threat in threats {
                    println!("   ⚠️  Threat detected:");
                    println!("      Type: {:?}", threat.threat_type);
                    println!("      Severity: {:?}", threat.severity);
                }
            }
        }
        Err(e) => println!("   ❌ Error checking hash: {}", e),
    }
    println!();

    // Demo 7: Rate Limit Status
    println!("📋 Demo 7: Rate Limit Status");
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
        Err(e) => println!("❌ Error getting rate limit status: {}", e),
    }
    println!();

    println!("=== Demo Complete ===");

    Ok(())
}
