//! Issue #211 Phase 1実装デモ
//!
//! 脅威インテリジェンス統合 (AbuseIPDB, CVE, MITRE ATT&CK)
//!
//! 実行方法:
//! ```bash
//! export ABUSEIPDB_API_KEY="your_api_key"
//! cargo run --example issue_211_phase1_demo
//! ```

use mcp_rs::policy::{DynamicPolicyUpdater, ThreatIntelligenceConfig, UpdateConfig};
use mcp_rs::policy_config::PolicyConfig;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング初期化
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("🚀 Issue #211 Phase 1 Demo - Threat Intelligence Integration");
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Step 1: 設定読み込み
    log::info!("📂 Loading threat intelligence configuration");
    let threat_config = match ThreatIntelligenceConfig::load_default() {
        Ok(config) => {
            log::info!("✅ Configuration loaded from configs/threat-intelligence.toml");
            config
        }
        Err(e) => {
            log::warn!("⚠️  Config file not found: {}", e);
            log::info!("Using default configuration");
            ThreatIntelligenceConfig {
                abuseipdb: Some(Default::default()),
                cve_database: Some(Default::default()),
                mitre_attack: Some(Default::default()),
                auto_policy_generator: Some(Default::default()),
            }
        }
    };

    // Step 2: ポリシー更新システム初期化
    log::info!("🔧 Initializing Policy Update System");
    let policy_config = PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(
        policy_config,
        UpdateConfig::default(),
    ));
    log::info!("✅ Policy updater ready");

    // Step 3: 脅威インテリジェンスマネージャー作成
    log::info!("🛡️  Creating Threat Intelligence Manager");
    let manager = match threat_config.create_manager(policy_updater.clone()) {
        Ok(mgr) => {
            log::info!("✅ Manager initialized with external providers");
            mgr
        }
        Err(e) => {
            log::error!("❌ Failed to create manager: {}", e);
            log::error!("💡 Set environment variable: export ABUSEIPDB_API_KEY=\"your_key\"");
            return Err(e);
        }
    };

    manager.enable_auto_update().await;
    log::info!("✅ Auto-update enabled");

    // Step 4: 自動ポリシー生成器作成
    log::info!("⚙️  Creating Auto Policy Generator");
    let generator = threat_config.create_policy_generator(policy_updater.clone())?;
    log::info!("✅ Generator ready");

    // Step 5: サンプルIP脅威チェック
    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log::info!("🔍 Checking sample IPs for threats");

    let sample_ips = vec![
        "118.25.6.39".to_string(), // 既知の悪意あるIP
        "8.8.8.8".to_string(),     // Google DNS (正常)
    ];

    match manager.fetch_bulk_from_abuseipdb(&sample_ips, 30).await {
        Ok(reports) => {
            log::info!("✅ Fetched {} reports", reports.len());

            for report in &reports {
                log::info!(
                    "  • {} - Confidence: {}% - Reports: {}",
                    report.ip_address,
                    report.abuse_confidence_score,
                    report.total_reports
                );
            }

            // Step 6: ポリシールール生成
            log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            log::info!("🤖 Generating security policy rules");

            let rules = generator
                .generate_ip_blocklist_from_abuseipdb(&reports)
                .await?;
            log::info!("✅ Generated {} rules", rules.len());

            for (i, rule) in rules.iter().enumerate() {
                log::info!(
                    "  Rule #{}: {} ({})",
                    i + 1,
                    rule.rule_id,
                    if rule.auto_apply { "AUTO" } else { "MANUAL" }
                );
            }

            // Step 7: ルール適用
            log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            if threat_config
                .auto_policy_generator
                .as_ref()
                .map(|c| c.application_mode == "automatic")
                .unwrap_or(false)
            {
                log::info!("🔐 Applying rules (Automatic mode)");
                let applied = generator.apply_all_rules(&rules).await?;
                log::info!("✅ Applied {} rules", applied);
            } else {
                log::info!("📋 Rules ready for manual review");
            }
        }
        Err(e) => {
            log::error!("❌ Failed: {}", e);
        }
    }

    log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    log::info!("✅ Demo completed!");

    Ok(())
}
