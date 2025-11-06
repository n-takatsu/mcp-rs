//! セキュリティ強化の使用例とテストケース

use super::*;
use crate::handlers::database::{
    AdvancedSecurityConfig, IntegratedSecurityManager, SecurityCheckResult,
    types::{QueryContext, SecurityError},
};
use tokio;

/// セキュリティ強化の実用例
pub struct SecurityEnhancementDemo {
    security_manager: IntegratedSecurityManager,
}

impl SecurityEnhancementDemo {
    /// 新しいデモインスタンスを作成
    pub fn new() -> Self {
        // 高度なセキュリティ設定を構成
        let config = AdvancedSecurityConfig {
            mfa: super::security_config::MfaConfig {
                required: true,
                totp: super::security_config::TotpConfig {
                    enabled: true,
                    secret_length: 32,
                    time_window: 30,
                    algorithm: "SHA256".to_string(),
                },
                backup_codes: super::security_config::BackupCodeConfig {
                    enabled: true,
                    code_count: 8,
                    code_length: 12,
                    single_use: true,
                },
                device_trust: super::security_config::DeviceTrustConfig {
                    enabled: true,
                    trust_threshold: 0.8,
                    learning_period_days: 14,
                    auto_trust_known_devices: true,
                },
                exceptions: std::collections::HashSet::new(),
            },
            rbac: super::security_config::RbacConfig {
                enabled: true,
                default_role: "employee".to_string(),
                role_hierarchy: {
                    let mut hierarchy = std::collections::HashMap::new();
                    hierarchy.insert("admin".to_string(), vec!["manager".to_string(), "employee".to_string()]);
                    hierarchy.insert("manager".to_string(), vec!["employee".to_string()]);
                    hierarchy
                },
                resource_policies: std::collections::HashMap::new(),
                time_based_access: super::security_config::TimeBasedAccessConfig::default(),
                ip_restrictions: super::security_config::IpRestrictionConfig::default(),
            },
            anomaly_detection: super::security_config::AnomalyDetectionConfig {
                enabled: true,
                ml_config: super::security_config::MachineLearningConfig {
                    model_type: "neural_network".to_string(),
                    training_data_retention_days: 180,
                    retrain_interval_hours: 12,
                    feature_selection: super::security_config::FeatureSelectionConfig {
                        query_timing: true,
                        query_complexity: true,
                        data_volume: true,
                        access_patterns: true,
                        user_behavior: true,
                        network_patterns: true,
                    },
                },
                baseline_learning: super::security_config::BaselineLearningConfig {
                    learning_period_days: 21,
                    minimum_samples: 500,
                    update_frequency_hours: 2,
                    seasonal_adjustment: true,
                },
                thresholds: super::security_config::AnomalyThresholds {
                    low_risk: 0.25,
                    medium_risk: 0.55,
                    high_risk: 0.75,
                    critical_risk: 0.90,
                },
                real_time_monitoring: super::security_config::RealTimeMonitoringConfig {
                    enabled: true,
                    monitoring_interval_seconds: 10,
                    alert_delay_seconds: 5,
                    batch_processing: true,
                },
            },
            ..Default::default()
        };

        let security_manager = IntegratedSecurityManager::new(config);

        Self { security_manager }
    }

    /// シナリオ1: 正常なユーザーアクセス
    pub async fn scenario_normal_access(&self) -> Result<(), SecurityError> {
        println!("=== シナリオ1: 正常なユーザーアクセス ===");

        let context = QueryContext {
            user_id: Some("employee001".to_string()),
            source_ip: Some("192.168.1.100".to_string()),
            session_id: Some("SESSION-ABC123".to_string()),
            timestamp: chrono::Utc::now(),
            query_type: super::types::QueryType::Select,
            client_info: Some("Browser/1.0".to_string()),
        };

        let sql = "SELECT first_name, last_name, department FROM employees WHERE department = 'Engineering'";

        match self.security_manager.comprehensive_security_check(sql, &context).await? {
            SecurityCheckResult { allowed: true, context: Some(sec_context), .. } => {
                println!("✅ アクセス許可");
                println!("   異常スコア: {:.3}", sec_context.anomaly_score);
                println!("   信頼レベル: {:.3}", sec_context.trust_level);
                println!("   アクセスレベル: {:?}", sec_context.access_level);
            },
            SecurityCheckResult { allowed: false, reason, .. } => {
                println!("❌ アクセス拒否: {}", reason);
            },
            _ => println!("⚠️ 予期しない結果"),
        }

        Ok(())
    }

    /// シナリオ2: SQLインジェクション攻撃の検知
    pub async fn scenario_sql_injection_detection(&self) -> Result<(), SecurityError> {
        println!("\n=== シナリオ2: SQLインジェクション攻撃の検知 ===");

        let context = QueryContext {
            user_id: Some("external_user".to_string()),
            source_ip: Some("203.0.113.42".to_string()),
            session_id: Some("SESSION-SUSPICIOUS".to_string()),
            timestamp: chrono::Utc::now(),
            query_type: super::types::QueryType::Select,
            client_info: Some("curl/7.0".to_string()),
        };

        let malicious_sql = "SELECT * FROM users WHERE id = 1; DROP TABLE users; --";

        match self.security_manager.comprehensive_security_check(malicious_sql, &context).await? {
            SecurityCheckResult { allowed: false, reason, .. } => {
                println!("🛡️ 攻撃を阻止: {}", reason);
                println!("   悪意のあるSQL: {}", malicious_sql);
            },
            SecurityCheckResult { allowed: true, .. } => {
                println!("⚠️ 警告: 攻撃が検知されませんでした");
            },
            _ => println!("⚠️ 予期しない結果"),
        }

        Ok(())
    }

    /// シナリオ3: 異常なアクセスパターンの検知
    pub async fn scenario_anomaly_detection(&self) -> Result<(), SecurityError> {
        println!("\n=== シナリオ3: 異常なアクセスパターンの検知 ===");

        let context = QueryContext {
            user_id: Some("employee002".to_string()),
            source_ip: Some("192.168.1.200".to_string()),
            session_id: Some("SESSION-LATE-NIGHT".to_string()),
            timestamp: chrono::Utc::now().with_hour(3).unwrap(), // 深夜3時のアクセス
            query_type: super::types::QueryType::Select,
            client_info: Some("EmployeeApp/2.0".to_string()),
        };

        // 通常とは異なる大量データアクセス
        let suspicious_sql = "SELECT employee_id, salary, ssn, bank_account FROM salary_data ORDER BY salary DESC LIMIT 10000";

        match self.security_manager.comprehensive_security_check(suspicious_sql, &context).await? {
            SecurityCheckResult { allowed: false, reason, .. } => {
                println!("🔍 異常なパターンを検知: {}", reason);
                println!("   時間: 深夜3時");
                println!("   対象: 機密給与データ");
            },
            SecurityCheckResult { allowed: true, context: Some(sec_context), .. } => {
                println!("⚠️ アクセスは許可されましたが、監視中");
                println!("   異常スコア: {:.3}", sec_context.anomaly_score);
                if sec_context.anomaly_score > 0.6 {
                    println!("   🚨 高リスクレベル");
                }
            },
            _ => println!("⚠️ 予期しない結果"),
        }

        Ok(())
    }

    /// シナリオ4: 多要素認証が必要なケース
    pub async fn scenario_mfa_required(&self) -> Result<(), SecurityError> {
        println!("\n=== シナリオ4: 多要素認証が必要なケース ===");

        let context_without_mfa = QueryContext {
            user_id: Some("admin001".to_string()),
            source_ip: Some("10.0.0.50".to_string()),
            session_id: Some("SESSION-ADMIN".to_string()),
            timestamp: chrono::Utc::now(),
            query_type: super::types::QueryType::Update,
            client_info: Some("AdminPanel/1.5".to_string()),
        };

        let admin_sql = "UPDATE system_settings SET maintenance_mode = true";

        match self.security_manager.comprehensive_security_check(admin_sql, &context_without_mfa).await? {
            SecurityCheckResult { allowed: false, reason, .. } => {
                println!("🔐 多要素認証が必要: {}", reason);
                println!("   管理者操作にはMFAが必須です");
            },
            SecurityCheckResult { allowed: true, .. } => {
                println!("⚠️ 警告: MFAなしでアクセスが許可されました");
            },
            _ => println!("⚠️ 予期しない結果"),
        }

        // MFAありの場合（簡略化 - 実際の実装では別の方法でMFA状態を管理）
        let context_with_mfa = QueryContext {
            client_info: Some("AdminPanel/1.5 (MFA-Verified)".to_string()),
            ..context_without_mfa
        };

        match self.security_manager.comprehensive_security_check(admin_sql, &context_with_mfa).await? {
            SecurityCheckResult { allowed: true, context: Some(sec_context), .. } => {
                println!("✅ MFA認証成功 - アクセス許可");
                println!("   信頼レベル: {:.3}", sec_context.trust_level);
            },
            SecurityCheckResult { allowed: false, reason, .. } => {
                println!("❌ MFA認証失敗: {}", reason);
            },
            _ => println!("⚠️ 予期しない結果"),
        }

        Ok(())
    }

    /// シナリオ5: セキュリティダッシュボードの生成
    pub async fn scenario_security_dashboard(&self) -> Result<(), SecurityError> {
        println!("\n=== シナリオ5: セキュリティダッシュボード ===");

        let dashboard = self.security_manager.generate_security_dashboard().await?;

        println!("📊 セキュリティダッシュボード ({}):", dashboard.timestamp.format("%Y-%m-%d %H:%M:%S"));
        println!("   総イベント数: {}", dashboard.event_summary.total_events);
        println!("   重要イベント数: {}", dashboard.event_summary.critical_events);
        println!("   異常検知総数: {}", dashboard.anomaly_summary.total_anomalies);
        println!("   平均異常スコア: {:.3}", dashboard.anomaly_summary.average_score);
        println!("   高リスク異常: {}", dashboard.anomaly_summary.high_risk_anomalies);

        if !dashboard.top_risk_users.is_empty() {
            println!("   🚨 上位リスクユーザー:");
            for (i, (user, score)) in dashboard.top_risk_users.iter().take(3).enumerate() {
                println!("     {}. {} (リスクスコア: {:.3})", i + 1, user, score);
            }
        }

        println!("   脅威インテリジェンス:");
        println!("     最終更新: {}", dashboard.threat_intelligence_status.last_update.format("%Y-%m-%d %H:%M:%S"));
        println!("     アクティブ脅威: {}", dashboard.threat_intelligence_status.active_threats);
        println!("     ブロック済みIP: {}", dashboard.threat_intelligence_status.blocked_ips);

        println!("   システム健全性:");
        for (component, health) in &dashboard.system_health.security_components_status {
            println!("     {}: {:?}", component, health);
        }

        let metrics = &dashboard.system_health.performance_metrics;
        println!("   パフォーマンス:");
        println!("     平均応答時間: {:.1}ms", metrics.average_response_time_ms);
        println!("     スループット: {:.0}/秒", metrics.throughput_per_second);
        println!("     エラー率: {:.2}%", metrics.error_rate_percent);
        println!("     メモリ使用量: {:.0}MB", metrics.memory_usage_mb);

        Ok(())
    }

    /// 全シナリオの実行
    pub async fn run_all_scenarios(&self) -> Result<(), SecurityError> {
        println!("🔒 データベースセキュリティ強化デモンストレーション");
        println!("==================================================");

        self.scenario_normal_access().await?;
        self.scenario_sql_injection_detection().await?;
        self.scenario_anomaly_detection().await?;
        self.scenario_mfa_required().await?;
        self.scenario_security_dashboard().await?;

        println!("\n✅ 全シナリオ完了");
        println!("==================================================");

        Ok(())
    }
}

/// セキュリティパフォーマンステスト
pub struct SecurityPerformanceTest {
    security_manager: IntegratedSecurityManager,
}

impl SecurityPerformanceTest {
    pub fn new() -> Self {
        let config = AdvancedSecurityConfig::default();
        let security_manager = IntegratedSecurityManager::new(config);
        Self { security_manager }
    }

    /// セキュリティチェックのパフォーマンステスト
    pub async fn benchmark_security_checks(&self, iterations: usize) -> Result<(), SecurityError> {
        println!("\n⏱️ セキュリティチェック パフォーマンステスト");
        println!("反復回数: {}", iterations);

        let context = QueryContext {
            user_id: Some("test_user".to_string()),
            source_ip: Some("192.168.1.1".to_string()),
            session_id: Some("SESSION-TEST".to_string()),
            timestamp: chrono::Utc::now(),
            query_type: super::types::QueryType::Select,
            client_info: Some("TestClient/1.0".to_string()),
        };

        let test_queries = vec![
            "SELECT * FROM users WHERE active = true",
            "INSERT INTO logs (message, level) VALUES ('test', 'info')",
            "UPDATE user_preferences SET theme = 'dark' WHERE user_id = 123",
            "DELETE FROM temp_data WHERE created_at < NOW() - INTERVAL 1 DAY",
        ];

        let start_time = std::time::Instant::now();

        for i in 0..iterations {
            let sql = &test_queries[i % test_queries.len()];
            let _result = self.security_manager.comprehensive_security_check(sql, &context).await?;
        }

        let elapsed = start_time.elapsed();
        let avg_time = elapsed.as_micros() as f64 / iterations as f64;

        println!("総実行時間: {:?}", elapsed);
        println!("平均実行時間: {:.2}μs/チェック", avg_time);
        println!("スループット: {:.0}チェック/秒", 1_000_000.0 / avg_time);

        // メモリ使用量の概算
        let memory_estimate = iterations * 1024; // 簡易推定
        println!("推定メモリ使用量: {}KB", memory_estimate / 1024);

        Ok(())
    }
}

/// 統合テスト
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_enhancement_scenarios() {
        let demo = SecurityEnhancementDemo::new();
        
        // 各シナリオのテスト
        assert!(demo.scenario_normal_access().await.is_ok());
        assert!(demo.scenario_sql_injection_detection().await.is_ok());
        assert!(demo.scenario_anomaly_detection().await.is_ok());
        assert!(demo.scenario_mfa_required().await.is_ok());
        assert!(demo.scenario_security_dashboard().await.is_ok());
    }

    #[tokio::test]
    async fn test_performance_benchmarks() {
        let benchmark = SecurityPerformanceTest::new();
        assert!(benchmark.benchmark_security_checks(100).await.is_ok());
    }

    #[tokio::test]
    async fn test_security_config_validation() {
        let config = AdvancedSecurityConfig::default();
        
        // デフォルト設定の検証
        assert!(config.anomaly_detection.enabled);
        assert!(config.rbac.enabled);
        assert_eq!(config.rbac.default_role, "user");
        assert!(config.audit.detailed_logging.log_all_queries);
    }

    #[tokio::test]
    async fn test_integrated_security_manager() {
        let config = AdvancedSecurityConfig::default();
        let manager = IntegratedSecurityManager::new(config);

        let context = QueryContext {
            user_id: Some("test_user".to_string()),
            source_ip: Some("127.0.0.1".to_string()),
            session_id: Some("test_session".to_string()),
            timestamp: chrono::Utc::now(),
            query_type: super::types::QueryType::Select,
            client_info: Some("TestRunner/1.0".to_string()),
        };

        let result = manager.comprehensive_security_check(
            "SELECT 1",
            &context
        ).await;

        assert!(result.is_ok());
    }
}

/// 使用例のメイン関数
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ初期化
    tracing_subscriber::fmt::init();

    // セキュリティ強化デモの実行
    let demo = SecurityEnhancementDemo::new();
    demo.run_all_scenarios().await?;

    // パフォーマンステストの実行
    let benchmark = SecurityPerformanceTest::new();
    benchmark.benchmark_security_checks(1000).await?;

    Ok(())
}