//! 脅威インテリジェンス自動統合モジュール
//!
//! 外部脅威フィードを統合し、ポリシーを自動更新する機能を提供します。
//!
//! # 機能
//!
//! - **外部脅威フィード統合**: 複数の脅威情報ソースから情報を取得
//! - **脅威パターン自動更新**: 検出された脅威に基づきポリシーを自動調整
//! - **インテリジェンス検証システム**: フィード情報の信頼性を検証
//! - **脅威レベル自動調整**: 脅威の深刻度に応じてポリシーを動的に変更

use crate::error::Result;
use crate::policy::dynamic_updater::DynamicPolicyUpdater;
use crate::policy_config::PolicyConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::interval;

/// 脅威レベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThreatLevel {
    /// 情報提供レベル - 深刻度: 低
    Info = 1,
    /// 注意レベル - 深刻度: 中
    Warning = 2,
    /// 警告レベル - 深刻度: 高
    Alert = 3,
    /// 重大レベル - 深刻度: 最高
    Critical = 4,
}

/// 脅威タイプ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThreatType {
    /// 不正アクセス試行
    UnauthorizedAccess,
    /// SQLインジェクション
    SqlInjection,
    /// クロスサイトスクリプティング
    Xss,
    /// DDoS攻撃
    DDoS,
    /// マルウェア
    Malware,
    /// ゼロデイ脆弱性
    ZeroDay,
    /// ブルートフォース攻撃
    BruteForce,
    /// その他
    Other(String),
}

/// 脅威インテリジェンス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIntelligence {
    /// 脅威ID
    pub id: String,
    /// 脅威タイプ
    pub threat_type: ThreatType,
    /// 脅威レベル
    pub level: ThreatLevel,
    /// 脅威の説明
    pub description: String,
    /// 影響を受けるIPアドレス/範囲
    pub affected_ips: Vec<String>,
    /// 影響を受けるドメイン
    pub affected_domains: Vec<String>,
    /// 推奨される対応アクション
    pub recommended_actions: Vec<String>,
    /// 情報ソース
    pub source: String,
    /// 信頼スコア (0.0 - 1.0)
    pub confidence: f64,
    /// 検出時刻
    pub detected_at: SystemTime,
    /// 有効期限
    pub expires_at: Option<SystemTime>,
}

/// 脅威フィードソース
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatFeedSource {
    /// ソース名
    pub name: String,
    /// ソースURL
    pub url: String,
    /// 優先度 (1-10, 10が最高)
    pub priority: u8,
    /// 信頼性スコア (0.0 - 1.0)
    pub reliability: f64,
    /// 更新間隔
    pub update_interval: Duration,
    /// 最終更新時刻
    pub last_updated: Option<SystemTime>,
}

/// 脅威インテリジェンスマネージャー
pub struct ThreatIntelligenceManager {
    /// ポリシー更新マネージャー
    policy_updater: Arc<DynamicPolicyUpdater>,
    /// 脅威情報キャッシュ
    threat_cache: Arc<RwLock<HashMap<String, ThreatIntelligence>>>,
    /// フィードソース
    feed_sources: Arc<RwLock<Vec<ThreatFeedSource>>>,
    /// 自動更新有効化
    auto_update_enabled: Arc<RwLock<bool>>,
    /// 最小信頼スコア閾値
    min_confidence_threshold: f64,
}

impl ThreatIntelligenceManager {
    /// 新しい脅威インテリジェンスマネージャーを作成
    ///
    /// # 引数
    ///
    /// * `policy_updater` - ポリシー更新マネージャー
    /// * `min_confidence_threshold` - 最小信頼スコア閾値 (デフォルト: 0.7)
    pub fn new(
        policy_updater: Arc<DynamicPolicyUpdater>,
        min_confidence_threshold: Option<f64>,
    ) -> Self {
        Self {
            policy_updater,
            threat_cache: Arc::new(RwLock::new(HashMap::new())),
            feed_sources: Arc::new(RwLock::new(Vec::new())),
            auto_update_enabled: Arc::new(RwLock::new(false)),
            min_confidence_threshold: min_confidence_threshold.unwrap_or(0.7),
        }
    }

    /// 脅威フィードソースを追加
    ///
    /// # 引数
    ///
    /// * `source` - 追加するフィードソース
    pub async fn add_feed_source(&self, source: ThreatFeedSource) {
        let mut sources = self.feed_sources.write().await;
        sources.push(source);
        // 優先度順にソート
        sources.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// 脅威フィードソースを削除
    ///
    /// # 引数
    ///
    /// * `name` - 削除するソース名
    pub async fn remove_feed_source(&self, name: &str) -> bool {
        let mut sources = self.feed_sources.write().await;
        let initial_len = sources.len();
        sources.retain(|s| s.name != name);
        sources.len() < initial_len
    }

    /// 全フィードソースを取得
    pub async fn get_feed_sources(&self) -> Vec<ThreatFeedSource> {
        self.feed_sources.read().await.clone()
    }

    /// 脅威情報を手動で追加
    ///
    /// # 引数
    ///
    /// * `intelligence` - 追加する脅威情報
    pub async fn add_threat_intelligence(&self, intelligence: ThreatIntelligence) -> Result<()> {
        // 信頼スコア検証
        if intelligence.confidence < self.min_confidence_threshold {
            return Err(crate::error::Error::InvalidInput(format!(
                "信頼スコア {} が最小閾値 {} を下回っています",
                intelligence.confidence, self.min_confidence_threshold
            )));
        }

        // キャッシュに追加
        let mut cache = self.threat_cache.write().await;
        cache.insert(intelligence.id.clone(), intelligence.clone());

        // 自動更新が有効な場合、ポリシーを更新
        if *self.auto_update_enabled.read().await {
            self.apply_threat_to_policy(&intelligence).await?;
        }

        Ok(())
    }

    /// 脅威情報をキャッシュから取得
    ///
    /// # 引数
    ///
    /// * `id` - 脅威ID
    pub async fn get_threat_intelligence(&self, id: &str) -> Option<ThreatIntelligence> {
        self.threat_cache.read().await.get(id).cloned()
    }

    /// 全脅威情報を取得
    pub async fn get_all_threats(&self) -> Vec<ThreatIntelligence> {
        self.threat_cache.read().await.values().cloned().collect()
    }

    /// 脅威レベル別に脅威情報を取得
    ///
    /// # 引数
    ///
    /// * `level` - フィルター対象の脅威レベル
    pub async fn get_threats_by_level(&self, level: ThreatLevel) -> Vec<ThreatIntelligence> {
        self.threat_cache
            .read()
            .await
            .values()
            .filter(|t| t.level == level)
            .cloned()
            .collect()
    }

    /// 脅威タイプ別に脅威情報を取得
    ///
    /// # 引数
    ///
    /// * `threat_type` - フィルター対象の脅威タイプ
    pub async fn get_threats_by_type(&self, threat_type: &ThreatType) -> Vec<ThreatIntelligence> {
        self.threat_cache
            .read()
            .await
            .values()
            .filter(|t| &t.threat_type == threat_type)
            .cloned()
            .collect()
    }

    /// 期限切れの脅威情報を削除
    pub async fn cleanup_expired_threats(&self) -> usize {
        let mut cache = self.threat_cache.write().await;
        let now = SystemTime::now();
        let initial_size = cache.len();

        cache.retain(|_, threat| {
            threat
                .expires_at
                .map(|expires| expires > now)
                .unwrap_or(true)
        });

        initial_size - cache.len()
    }

    /// 自動更新を有効化
    pub async fn enable_auto_update(&self) {
        *self.auto_update_enabled.write().await = true;
    }

    /// 自動更新を無効化
    pub async fn disable_auto_update(&self) {
        *self.auto_update_enabled.write().await = false;
    }

    /// 自動更新の有効状態を取得
    pub async fn is_auto_update_enabled(&self) -> bool {
        *self.auto_update_enabled.read().await
    }

    /// 脅威情報をポリシーに適用
    ///
    /// # 引数
    ///
    /// * `intelligence` - 適用する脅威情報
    async fn apply_threat_to_policy(&self, intelligence: &ThreatIntelligence) -> Result<()> {
        // 現在のポリシーを取得
        let current_policy = self.policy_updater.get_active_policy().await;

        // 脅威情報に基づいてポリシーを調整
        let updated_policy = self
            .adjust_policy_for_threat(&current_policy, intelligence)
            .await?;

        // ポリシーを更新
        self.policy_updater.update_policy(updated_policy).await?;

        Ok(())
    }

    /// 脅威情報に基づいてポリシーを調整
    ///
    /// # 引数
    ///
    /// * `policy` - 現在のポリシー
    /// * `intelligence` - 脅威情報
    async fn adjust_policy_for_threat(
        &self,
        policy: &PolicyConfig,
        intelligence: &ThreatIntelligence,
    ) -> Result<PolicyConfig> {
        let mut updated_policy = policy.clone();

        // 脅威レベルに応じてポリシーを調整
        match intelligence.level {
            ThreatLevel::Critical => {
                // Critical: 厳格なポリシー適用
                updated_policy.security.rate_limiting.requests_per_minute =
                    updated_policy.security.rate_limiting.requests_per_minute / 2;
                updated_policy.security.encryption.algorithm = "AES-256-GCM".to_string();
            }
            ThreatLevel::Alert => {
                // Alert: 警戒モード
                updated_policy.security.rate_limiting.requests_per_minute =
                    (updated_policy.security.rate_limiting.requests_per_minute as f64 * 0.7) as u32;
            }
            ThreatLevel::Warning => {
                // Warning: 軽度の制限
                updated_policy.security.rate_limiting.requests_per_minute =
                    (updated_policy.security.rate_limiting.requests_per_minute as f64 * 0.9) as u32;
            }
            ThreatLevel::Info => {
                // Info: 通常運用
            }
        }

        // 脅威タイプに応じた調整
        match &intelligence.threat_type {
            ThreatType::DDoS | ThreatType::BruteForce => {
                // レート制限を強化
                updated_policy.security.rate_limiting.burst_size = updated_policy
                    .security
                    .rate_limiting
                    .burst_size
                    .saturating_sub(10);
            }
            ThreatType::SqlInjection => {
                // 入力検証を強化
                updated_policy
                    .security
                    .input_validation
                    .sql_injection_protection = true;
            }
            ThreatType::Xss => {
                // XSS保護を強化
                updated_policy.security.input_validation.xss_protection = true;
            }
            ThreatType::UnauthorizedAccess => {
                // 認証要件を強化（将来実装）
            }
            _ => {}
        }

        Ok(updated_policy)
    }

    /// フィードから脅威情報を取得（ダミー実装）
    ///
    /// 実際の実装では外部APIから情報を取得します
    ///
    /// # 引数
    ///
    /// * `source` - フィードソース
    async fn fetch_from_source(
        &self,
        _source: &ThreatFeedSource,
    ) -> Result<Vec<ThreatIntelligence>> {
        // TODO: 実際の外部API統合
        // 例: reqwest を使用してHTTPリクエスト
        Ok(Vec::new())
    }

    /// 全フィードソースから脅威情報を更新
    pub async fn update_from_all_sources(&self) -> Result<usize> {
        let sources = self.get_feed_sources().await;
        let mut total_updated = 0;

        for source in sources {
            match self.fetch_from_source(&source).await {
                Ok(threats) => {
                    for threat in threats {
                        if let Err(e) = self.add_threat_intelligence(threat).await {
                            eprintln!("脅威情報の追加に失敗: {}", e);
                        } else {
                            total_updated += 1;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("フィード {} からの取得に失敗: {}", source.name, e);
                }
            }
        }

        Ok(total_updated)
    }

    /// 自動更新ループを開始
    ///
    /// # 引数
    ///
    /// * `update_interval` - 更新間隔
    pub async fn start_auto_update_loop(self: Arc<Self>, update_interval: Duration) {
        let mut interval_timer = interval(update_interval);

        loop {
            interval_timer.tick().await;

            if !self.is_auto_update_enabled().await {
                continue;
            }

            // 期限切れ脅威情報のクリーンアップ
            let cleaned = self.cleanup_expired_threats().await;
            if cleaned > 0 {
                println!("期限切れ脅威情報 {} 件を削除しました", cleaned);
            }

            // フィードから更新
            match self.update_from_all_sources().await {
                Ok(count) => {
                    println!("脅威情報を {} 件更新しました", count);
                }
                Err(e) => {
                    eprintln!("脅威情報の更新に失敗: {}", e);
                }
            }
        }
    }

    /// 脅威統計情報を取得
    pub async fn get_threat_statistics(&self) -> ThreatStatistics {
        let cache = self.threat_cache.read().await;

        let mut stats = ThreatStatistics::default();
        stats.total_threats = cache.len();

        for threat in cache.values() {
            match threat.level {
                ThreatLevel::Info => stats.info_count += 1,
                ThreatLevel::Warning => stats.warning_count += 1,
                ThreatLevel::Alert => stats.alert_count += 1,
                ThreatLevel::Critical => stats.critical_count += 1,
            }
        }

        stats.sources_count = self.feed_sources.read().await.len();
        stats.auto_update_enabled = *self.auto_update_enabled.read().await;

        stats
    }
}

/// 脅威統計情報
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreatStatistics {
    /// 総脅威数
    pub total_threats: usize,
    /// Info レベル脅威数
    pub info_count: usize,
    /// Warning レベル脅威数
    pub warning_count: usize,
    /// Alert レベル脅威数
    pub alert_count: usize,
    /// Critical レベル脅威数
    pub critical_count: usize,
    /// フィードソース数
    pub sources_count: usize,
    /// 自動更新有効
    pub auto_update_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::dynamic_updater::UpdateConfig;
    use crate::policy_config::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_policy() -> PolicyConfig {
        PolicyConfig {
            id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test policy for threat intelligence".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            security: SecurityPolicyConfig {
                enabled: true,
                encryption: EncryptionConfig {
                    algorithm: "AES-128-GCM".to_string(),
                    key_size: 128,
                    pbkdf2_iterations: 10000,
                },
                tls: TlsConfig {
                    enforce: true,
                    min_version: "1.2".to_string(),
                    cipher_suites: vec![],
                },
                input_validation: InputValidationConfig {
                    enabled: true,
                    max_input_length: 1024,
                    sql_injection_protection: false,
                    xss_protection: false,
                },
                rate_limiting: RateLimitingConfig {
                    enabled: true,
                    requests_per_minute: 100,
                    burst_size: 20,
                },
            },
            monitoring: MonitoringPolicyConfig {
                interval_seconds: 60,
                alerts_enabled: true,
                log_level: "info".to_string(),
                metrics: MetricsConfig {
                    enabled: true,
                    sampling_rate: 1.0,
                    buffer_size: 1000,
                },
            },
            authentication: AuthenticationPolicyConfig {
                enabled: true,
                method: "token".to_string(),
                session_timeout_seconds: 3600,
                require_mfa: false,
            },
            custom: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_add_feed_source() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, None);

        let source = ThreatFeedSource {
            name: "Test Feed".to_string(),
            url: "https://example.com/feed".to_string(),
            priority: 8,
            reliability: 0.9,
            update_interval: Duration::from_secs(300),
            last_updated: None,
        };

        manager.add_feed_source(source).await;
        let sources = manager.get_feed_sources().await;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "Test Feed");
    }

    #[tokio::test]
    async fn test_add_threat_intelligence() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, Some(0.7));

        let threat = ThreatIntelligence {
            id: "THREAT-001".to_string(),
            threat_type: ThreatType::DDoS,
            level: ThreatLevel::Critical,
            description: "Large-scale DDoS attack detected".to_string(),
            affected_ips: vec!["192.168.1.0/24".to_string()],
            affected_domains: vec!["example.com".to_string()],
            recommended_actions: vec!["Block IP range".to_string()],
            source: "Test Source".to_string(),
            confidence: 0.95,
            detected_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
        };

        assert!(manager.add_threat_intelligence(threat).await.is_ok());
        let threats = manager.get_all_threats().await;
        assert_eq!(threats.len(), 1);
    }

    #[tokio::test]
    async fn test_threat_confidence_threshold() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, Some(0.8));

        let low_confidence_threat = ThreatIntelligence {
            id: "THREAT-002".to_string(),
            threat_type: ThreatType::Malware,
            level: ThreatLevel::Warning,
            description: "Suspicious activity".to_string(),
            affected_ips: vec![],
            affected_domains: vec![],
            recommended_actions: vec![],
            source: "Test".to_string(),
            confidence: 0.5,
            detected_at: SystemTime::now(),
            expires_at: None,
        };

        assert!(manager
            .add_threat_intelligence(low_confidence_threat)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_get_threats_by_level() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, None);

        // Critical threat
        let threat1 = ThreatIntelligence {
            id: "THREAT-003".to_string(),
            threat_type: ThreatType::ZeroDay,
            level: ThreatLevel::Critical,
            description: "Zero-day vulnerability".to_string(),
            affected_ips: vec![],
            affected_domains: vec![],
            recommended_actions: vec![],
            source: "Test".to_string(),
            confidence: 0.95,
            detected_at: SystemTime::now(),
            expires_at: None,
        };

        // Warning threat
        let threat2 = ThreatIntelligence {
            id: "THREAT-004".to_string(),
            threat_type: ThreatType::BruteForce,
            level: ThreatLevel::Warning,
            description: "Brute force attempt".to_string(),
            affected_ips: vec![],
            affected_domains: vec![],
            recommended_actions: vec![],
            source: "Test".to_string(),
            confidence: 0.8,
            detected_at: SystemTime::now(),
            expires_at: None,
        };

        manager.add_threat_intelligence(threat1).await.unwrap();
        manager.add_threat_intelligence(threat2).await.unwrap();

        let critical_threats = manager.get_threats_by_level(ThreatLevel::Critical).await;
        assert_eq!(critical_threats.len(), 1);
        assert_eq!(critical_threats[0].id, "THREAT-003");
    }

    #[tokio::test]
    async fn test_cleanup_expired_threats() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, None);

        // Already expired threat
        let expired_threat = ThreatIntelligence {
            id: "THREAT-005".to_string(),
            threat_type: ThreatType::Malware,
            level: ThreatLevel::Info,
            description: "Old malware signature".to_string(),
            affected_ips: vec![],
            affected_domains: vec![],
            recommended_actions: vec![],
            source: "Test".to_string(),
            confidence: 0.9,
            detected_at: SystemTime::now() - Duration::from_secs(7200),
            expires_at: Some(SystemTime::now() - Duration::from_secs(3600)),
        };

        manager
            .add_threat_intelligence(expired_threat)
            .await
            .unwrap();
        let cleaned = manager.cleanup_expired_threats().await;
        assert_eq!(cleaned, 1);
        assert_eq!(manager.get_all_threats().await.len(), 0);
    }

    #[tokio::test]
    async fn test_auto_update_toggle() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, None);

        assert!(!manager.is_auto_update_enabled().await);

        manager.enable_auto_update().await;
        assert!(manager.is_auto_update_enabled().await);

        manager.disable_auto_update().await;
        assert!(!manager.is_auto_update_enabled().await);
    }

    #[tokio::test]
    async fn test_threat_statistics() {
        let policy_updater = Arc::new(DynamicPolicyUpdater::new(
            create_test_policy(),
            UpdateConfig::default(),
        ));
        let manager = ThreatIntelligenceManager::new(policy_updater, None);

        // Add threats of different levels
        for (id, level) in [
            ("T1", ThreatLevel::Critical),
            ("T2", ThreatLevel::Alert),
            ("T3", ThreatLevel::Warning),
            ("T4", ThreatLevel::Info),
        ] {
            let threat = ThreatIntelligence {
                id: id.to_string(),
                threat_type: ThreatType::Other("Test".to_string()),
                level,
                description: "Test threat".to_string(),
                affected_ips: vec![],
                affected_domains: vec![],
                recommended_actions: vec![],
                source: "Test".to_string(),
                confidence: 0.9,
                detected_at: SystemTime::now(),
                expires_at: None,
            };
            manager.add_threat_intelligence(threat).await.unwrap();
        }

        let stats = manager.get_threat_statistics().await;
        assert_eq!(stats.total_threats, 4);
        assert_eq!(stats.critical_count, 1);
        assert_eq!(stats.alert_count, 1);
        assert_eq!(stats.warning_count, 1);
        assert_eq!(stats.info_count, 1);
    }
}
