//! MITRE ATT&CK Provider Demo
//!
//! このデモは、MITRE ATT&CKプロバイダーの主要機能を示します：
//! - テクニックID検索
//! - キーワード検索
//! - ヘルスチェック
//! - キャッシュ機能

use mcp_rs::threat_intelligence::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MITRE ATT&CK Provider Demo ===\n");

    // プロバイダー設定
    let config = ProviderConfig {
        name: "MITRE-ATTACK".to_string(),
        enabled: true,
        api_key: String::new(), // MITRE ATT&CK APIキー不要（公開データ）
        base_url: "https://raw.githubusercontent.com/mitre/cti/master".to_string(),
        timeout_seconds: 30,
        rate_limit_per_minute: 60,
        reliability_factor: 0.95,
        provider_specific: HashMap::new(),
    };

    let provider = match ProviderFactory::create_provider(config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create MITRE ATT&CK provider: {}", e);
            return Err(e.into());
        }
    };

    // デモ1: ヘルスチェック
    demo_health_check(&*provider).await?;
    println!("\n{}\n", "=".repeat(80));

    // デモ2: Phishing テクニック検索
    demo_phishing_technique(&*provider).await?;
    println!("\n{}\n", "=".repeat(80));

    // デモ3: PowerShell テクニック検索
    demo_powershell_technique(&*provider).await?;
    println!("\n{}\n", "=".repeat(80));

    // デモ4: キーワード検索
    demo_keyword_search(&*provider).await?;
    println!("\n{}\n", "=".repeat(80));

    // デモ5: 複数テクニックのバッチ検索
    demo_batch_check(&*provider).await?;
    println!("\n{}\n", "=".repeat(80));

    // デモ6: レート制限ステータス
    demo_rate_limit_status(&*provider).await?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

/// デモ1: ヘルスチェック
async fn demo_health_check(
    provider: &dyn ThreatProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 1: Health Check");
    println!("{}", "-".repeat(40));

    match provider.health_check().await {
        Ok(health) => {
            println!("Provider: {}", health.provider_name);
            println!("Status: {:?}", health.status);
            println!("Response Time: {} ms", health.response_time_ms);
            println!("Last Check: {}", health.last_check);
            if let Some(error) = health.error_message {
                println!("Error: {}", error);
            }
        }
        Err(e) => {
            eprintln!("Health check failed: {}", e);
        }
    }

    Ok(())
}

/// デモ2: Phishing テクニック検索 (T1566)
async fn demo_phishing_technique(
    provider: &dyn ThreatProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 2: Phishing Technique (T1566)");
    println!("{}", "-".repeat(40));

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "T1566".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            println!("Found {} threat(s)", threats.len());
            for threat in threats {
                println!("\nThreat ID: {}", threat.id);
                println!("Type: {:?}", threat.threat_type);
                println!("Severity: {:?}", threat.severity);
                println!("Confidence: {:.2}", threat.confidence_score);

                if let Some(desc) = &threat.metadata.description {
                    println!("Description: {}", desc);
                }

                if !threat.metadata.mitre_attack_techniques.is_empty() {
                    println!("\nMITRE ATT&CK Techniques:");
                    for technique in &threat.metadata.mitre_attack_techniques {
                        println!("  - {} ({})", technique.name, technique.technique_id);
                        println!("    Tactics: {}", technique.tactics.join(", "));
                        println!("    Platforms: {}", technique.platforms.join(", "));
                        if !technique.data_sources.is_empty() {
                            println!("    Data Sources: {}", technique.data_sources.join(", "));
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to check Phishing technique: {}", e);
        }
    }

    Ok(())
}

/// デモ3: PowerShell テクニック検索 (T1059.001)
async fn demo_powershell_technique(
    provider: &dyn ThreatProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 3: PowerShell Technique (T1059.001)");
    println!("{}", "-".repeat(40));

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "T1059.001".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };

    match provider.check_indicator(&indicator).await {
        Ok(threats) => {
            println!("Found {} threat(s)", threats.len());
            for threat in threats {
                println!("\nTechnique: {:?}", threat.threat_type);
                println!("Severity: {:?}", threat.severity);

                if !threat.metadata.mitre_attack_techniques.is_empty() {
                    let technique = &threat.metadata.mitre_attack_techniques[0];
                    println!("Name: {}", technique.name);
                    println!("Tactics: {}", technique.tactics.join(", "));

                    if let Some(ref detection) = technique.detection {
                        println!("Detection: {}", detection);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to check PowerShell technique: {}", e);
        }
    }

    Ok(())
}

/// デモ4: キーワード検索
async fn demo_keyword_search(
    provider: &dyn ThreatProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 4: Keyword Search");
    println!("{}", "-".repeat(40));

    let keywords = vec!["phishing", "credential dumping", "lateral movement"];

    for keyword in keywords {
        println!("\nSearching for: '{}'", keyword);

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: keyword.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        match provider.check_indicator(&indicator).await {
            Ok(threats) => {
                if threats.is_empty() {
                    println!("  No techniques found");
                } else {
                    println!("  Found {} technique(s)", threats.len());
                    for threat in threats {
                        if !threat.metadata.mitre_attack_techniques.is_empty() {
                            let technique = &threat.metadata.mitre_attack_techniques[0];
                            println!("    - {} ({})", technique.name, technique.technique_id);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
            }
        }
    }

    Ok(())
}

/// デモ5: バッチテクニックチェック
async fn demo_batch_check(provider: &dyn ThreatProvider) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 5: Batch Technique Check");
    println!("{}", "-".repeat(40));

    let technique_ids = vec!["T1566", "T1003", "T1059.001"];
    let mut indicators = Vec::new();

    for id in &technique_ids {
        indicators.push(ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: id.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        });
    }

    println!("Checking {} techniques...", indicators.len());

    match provider.batch_check_indicators(&indicators).await {
        Ok(threats) => {
            println!("Total threats found: {}", threats.len());

            for threat in threats {
                if !threat.metadata.mitre_attack_techniques.is_empty() {
                    let technique = &threat.metadata.mitre_attack_techniques[0];
                    println!("\n  {} - {}", technique.technique_id, technique.name);
                    println!("    Severity: {:?}", threat.severity);
                    println!("    Tactics: {}", technique.tactics.join(", "));
                }
            }
        }
        Err(e) => {
            eprintln!("Batch check failed: {}", e);
        }
    }

    Ok(())
}

/// デモ6: レート制限ステータス
async fn demo_rate_limit_status(
    provider: &dyn ThreatProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Demo 6: Rate Limit Status");
    println!("{}", "-".repeat(40));

    match provider.get_rate_limit_status().await {
        Ok(status) => {
            println!("Limit per minute: {}", status.limit_per_minute);
            println!("Remaining requests: {}", status.remaining_requests);
            println!("Reset at: {}", status.reset_at);
            println!("Is limited: {}", status.is_limited);
        }
        Err(e) => {
            eprintln!("Failed to get rate limit status: {}", e);
        }
    }

    Ok(())
}
