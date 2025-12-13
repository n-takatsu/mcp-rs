//! Threat Intelligence Manager
//!
//! 脅威インテリジェンスの統合管理とオーケストレーション

use crate::error::McpError;
use crate::threat_intelligence::providers::*;
use crate::threat_intelligence::types::*;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// 脅威インテリジェンス管理システムのメイン構造体
pub struct ThreatIntelligenceManager {
    /// プロバイダー管理
    provider_manager: Arc<RwLock<ProviderManager>>,
    /// キャッシュ
    cache: Arc<RwLock<ThreatCache>>,
    /// 設定
    config: Arc<RwLock<ThreatIntelligenceConfig>>,
    /// 統計情報
    stats: Arc<RwLock<ThreatDetectionStats>>,
}

/// 脅威インテリジェンス設定
#[derive(Debug, Clone)]
pub struct ThreatIntelligenceConfig {
    /// 有効/無効
    pub enabled: bool,
    /// 自動ブロック機能
    pub auto_block: bool,
    /// 信頼度閾値（この値以上で脅威と判定）
    pub confidence_threshold: f64,
    /// キャッシュサイズ上限
    pub max_cache_size: usize,
    /// キャッシュTTL（秒）
    pub cache_ttl_seconds: u64,
    /// 並列チェック数上限
    pub max_concurrent_checks: usize,
    /// デフォルトタイムアウト（秒）
    pub default_timeout_seconds: u32,
}

impl Default for ThreatIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_block: false,
            confidence_threshold: 0.7,
            max_cache_size: 10000,
            cache_ttl_seconds: 3600,
            max_concurrent_checks: 10,
            default_timeout_seconds: 30,
        }
    }
}

/// 脅威キャッシュ
pub struct ThreatCache {
    entries: HashMap<String, CacheEntry>,
    max_size: usize,
}

/// キャッシュエントリ
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub assessment: ThreatAssessment,
    pub created_at: DateTime<Utc>,
    pub ttl_seconds: u64,
}

impl ThreatCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size,
        }
    }

    /// キャッシュからエントリを取得
    pub fn get(&mut self, key: &str) -> Option<ThreatAssessment> {
        // 期限切れエントリをクリーンアップ
        self.cleanup_expired();

        if let Some(entry) = self.entries.get(key) {
            if !self.is_expired(entry) {
                return Some(entry.assessment.clone());
            } else {
                self.entries.remove(key);
            }
        }
        None
    }

    /// キャッシュにエントリを追加
    pub fn set(&mut self, key: String, assessment: ThreatAssessment, ttl_seconds: u64) {
        // キャッシュサイズ制限チェック
        if self.entries.len() >= self.max_size {
            self.evict_oldest();
        }

        let entry = CacheEntry {
            assessment,
            created_at: Utc::now(),
            ttl_seconds,
        };

        self.entries.insert(key, entry);
    }

    /// 期限切れエントリをクリーンアップ
    fn cleanup_expired(&mut self) {
        let now = Utc::now();
        self.entries.retain(|_, entry| {
            (now.timestamp() - entry.created_at.timestamp()) < entry.ttl_seconds as i64
        });
    }

    /// エントリが期限切れかどうかを確認
    fn is_expired(&self, entry: &CacheEntry) -> bool {
        let now = Utc::now();
        (now.timestamp() - entry.created_at.timestamp()) >= entry.ttl_seconds as i64
    }

    /// 最も古いエントリを削除
    fn evict_oldest(&mut self) {
        if let Some((oldest_key, _)) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(k, v)| (k.clone(), v.clone()))
        {
            self.entries.remove(&oldest_key);
        }
    }
}

impl ThreatIntelligenceManager {
    /// 新しい脅威インテリジェンス管理システムを作成
    pub fn new() -> Self {
        let config = ThreatIntelligenceConfig::default();
        let cache = ThreatCache::new(config.max_cache_size);

        Self {
            provider_manager: Arc::new(RwLock::new(ProviderManager::new())),
            cache: Arc::new(RwLock::new(cache)),
            config: Arc::new(RwLock::new(config)),
            stats: Arc::new(RwLock::new(ThreatDetectionStats::default())),
        }
    }

    /// プロバイダーを登録
    pub async fn register_provider(
        &self,
        provider: Box<dyn ThreatProvider>,
    ) -> Result<(), ThreatError> {
        let provider_name = provider.name().to_string();

        // プロバイダーの健全性チェック
        match provider.health_check().await {
            Ok(health) => {
                if health.status == HealthStatus::Healthy {
                    info!("Provider {} health check passed", provider_name);
                } else {
                    warn!(
                        "Provider {} health check warning: {:?}",
                        provider_name, health
                    );
                }
            }
            Err(e) => {
                error!("Provider {} health check failed: {}", provider_name, e);
                return Err(e);
            }
        }

        let mut manager = self.provider_manager.write().await;
        manager.add_provider(provider);

        info!("Registered threat intelligence provider: {}", provider_name);
        Ok(())
    }

    /// 単一指標の脅威チェック
    pub async fn check_threat(
        &self,
        indicator: ThreatIndicator,
    ) -> Result<ThreatAssessment, ThreatError> {
        let config = self.config.read().await;

        if !config.enabled {
            return Ok(self.create_safe_assessment(indicator));
        }

        let cache_key = self.generate_cache_key(&indicator);

        // キャッシュチェック
        if let Some(cached_result) = self.cache.write().await.get(&cache_key) {
            debug!("Cache hit for indicator: {}", indicator.value);
            return Ok(cached_result);
        }

        let start_time = std::time::Instant::now();

        // 全プロバイダーでチェック
        let threats = self.check_with_all_providers(&indicator).await?;

        let assessment_duration = start_time.elapsed().as_millis() as u64;

        // 評価結果を作成
        let assessment = self.create_assessment(
            indicator,
            threats,
            assessment_duration,
            false, // キャッシュからではない
        );

        // キャッシュに保存
        self.cache
            .write()
            .await
            .set(cache_key, assessment.clone(), config.cache_ttl_seconds);

        // 統計を更新
        self.update_stats(&assessment).await;

        Ok(assessment)
    }

    /// 複数指標のバッチチェック
    pub async fn batch_check_threats(
        &self,
        indicators: Vec<ThreatIndicator>,
    ) -> Result<Vec<ThreatAssessment>, ThreatError> {
        let config = self.config.read().await;
        let max_concurrent = config.max_concurrent_checks;
        drop(config);

        let mut results = Vec::new();

        // 並列処理でチェック
        let chunks: Vec<_> = indicators.chunks(max_concurrent).collect();

        for chunk in chunks {
            let mut tasks = Vec::new();

            for indicator in chunk {
                let indicator_clone = indicator.clone();
                let self_clone = self.clone();

                let task =
                    tokio::spawn(async move { self_clone.check_threat(indicator_clone).await });

                tasks.push(task);
            }

            // 結果を収集
            for task in tasks {
                match task.await {
                    Ok(Ok(assessment)) => results.push(assessment),
                    Ok(Err(e)) => return Err(e),
                    Err(e) => return Err(ThreatError::NetworkError(e.to_string())),
                }
            }
        }

        Ok(results)
    }

    /// 設定を更新
    pub async fn update_config(
        &self,
        new_config: ThreatIntelligenceConfig,
    ) -> Result<(), ThreatError> {
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Threat intelligence configuration updated");
        Ok(())
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> ThreatDetectionStats {
        self.stats.read().await.clone()
    }

    /// プロバイダーの健全性チェック
    pub async fn check_providers_health(&self) -> Result<Vec<ProviderHealth>, ThreatError> {
        let manager = self.provider_manager.read().await;
        let mut health_results = Vec::new();

        for provider in manager.get_all_providers().values() {
            match provider.health_check().await {
                Ok(health) => health_results.push(health),
                Err(e) => {
                    warn!(
                        "Health check failed for provider {}: {}",
                        provider.name(),
                        e
                    );
                    // 健全性チェック失敗も結果に含める
                    health_results.push(ProviderHealth {
                        provider_name: provider.name().to_string(),
                        status: HealthStatus::Error,
                        response_time_ms: 0,
                        last_check: Utc::now(),
                        error_message: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(health_results)
    }

    /// 全プロバイダーで指標をチェック
    async fn check_with_all_providers(
        &self,
        indicator: &ThreatIndicator,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let manager = self.provider_manager.read().await;
        let mut all_threats = Vec::new();

        for provider in manager.get_all_providers().values() {
            match provider.check_indicator(indicator).await {
                Ok(mut threats) => {
                    all_threats.append(&mut threats);
                }
                Err(e) => {
                    warn!(
                        "Provider {} failed to check indicator {}: {}",
                        provider.name(),
                        indicator.value,
                        e
                    );
                    // 個別プロバイダーの失敗は続行
                }
            }
        }

        Ok(all_threats)
    }

    /// 脅威評価結果を作成
    fn create_assessment(
        &self,
        indicator: ThreatIndicator,
        threats: Vec<ThreatIntelligence>,
        duration_ms: u64,
        from_cache: bool,
    ) -> ThreatAssessment {
        let is_threat = !threats.is_empty();

        // 最高の脅威レベルを取得
        let threat_level = threats
            .iter()
            .map(|t| &t.severity)
            .max()
            .cloned()
            .unwrap_or(SeverityLevel::Info);

        // 平均信頼度スコアを計算
        let confidence_score = if threats.is_empty() {
            0.0
        } else {
            threats.iter().map(|t| t.confidence_score).sum::<f64>() / threats.len() as f64
        };

        // 使用されたプロバイダーを収集
        let providers_used: Vec<String> = threats
            .iter()
            .map(|t| t.source.provider.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        ThreatAssessment {
            indicator,
            is_threat,
            threat_level,
            confidence_score,
            matched_threats: threats,
            assessed_at: Utc::now(),
            assessment_duration_ms: duration_ms,
            context: ThreatAssessmentContext {
                providers_used,
                from_cache,
                timing_breakdown: HashMap::new(),
                warnings: vec![],
            },
        }
    }

    /// 安全な評価結果を作成（脅威なし）
    fn create_safe_assessment(&self, indicator: ThreatIndicator) -> ThreatAssessment {
        ThreatAssessment {
            indicator,
            is_threat: false,
            threat_level: SeverityLevel::Info,
            confidence_score: 0.0,
            matched_threats: vec![],
            assessed_at: Utc::now(),
            assessment_duration_ms: 0,
            context: ThreatAssessmentContext {
                providers_used: vec![],
                from_cache: false,
                timing_breakdown: HashMap::new(),
                warnings: vec!["Threat intelligence is disabled".to_string()],
            },
        }
    }

    /// キャッシュキーを生成
    fn generate_cache_key(&self, indicator: &ThreatIndicator) -> String {
        format!("{}:{}", indicator.indicator_type as u8, &indicator.value)
    }

    /// 統計を更新
    async fn update_stats(&self, assessment: &ThreatAssessment) {
        let mut stats = self.stats.write().await;
        stats.total_checks += 1;

        if assessment.is_threat {
            stats.threats_detected += 1;
        }

        // キャッシュヒット率を更新
        if assessment.context.from_cache {
            stats.cache_hit_rate = (stats.cache_hit_rate * (stats.total_checks - 1) as f64 + 1.0)
                / stats.total_checks as f64;
        } else {
            stats.cache_hit_rate = (stats.cache_hit_rate * (stats.total_checks - 1) as f64)
                / stats.total_checks as f64;
        }

        // 平均応答時間を更新
        let total_time = stats.avg_response_time_ms * (stats.total_checks - 1) as f64
            + assessment.assessment_duration_ms as f64;
        stats.avg_response_time_ms = total_time / stats.total_checks as f64;
    }
}

// Clone トレイトの実装（Arc を使用しているため）
impl Clone for ThreatIntelligenceManager {
    fn clone(&self) -> Self {
        Self {
            provider_manager: self.provider_manager.clone(),
            cache: self.cache.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl Default for ThreatIntelligenceManager {
    fn default() -> Self {
        Self::new()
    }
}
