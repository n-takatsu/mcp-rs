//! Threat Intelligence Providers
//!
//! 外部脅威インテリジェンスプロバイダーとの統合を抽象化

use crate::threat_intelligence::types::*;
use async_trait::async_trait;
use base64::Engine;
use std::collections::HashMap;
use std::sync::Arc;

/// 脅威インテリジェンスプロバイダーの共通インターフェース
#[async_trait]
pub trait ThreatProvider: Send + Sync {
    /// プロバイダー名を取得
    fn name(&self) -> &str;

    /// プロバイダーの設定を取得
    fn config(&self) -> &ProviderConfig;

    /// 単一指標の脅威チェック
    async fn check_indicator(
        &self,
        indicator: &ThreatIndicator,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError>;

    /// 複数指標のバッチチェック
    async fn batch_check_indicators(
        &self,
        indicators: &[ThreatIndicator],
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let mut results = Vec::new();
        for indicator in indicators {
            match self.check_indicator(indicator).await {
                Ok(mut threats) => results.append(&mut threats),
                Err(e) => return Err(e),
            }
        }
        Ok(results)
    }

    /// プロバイダーの健全性チェック
    async fn health_check(&self) -> Result<ProviderHealth, ThreatError>;

    /// レート制限の状態を取得
    async fn get_rate_limit_status(&self) -> Result<RateLimitStatus, ThreatError>;
}

/// プロバイダーの健全性情報
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    /// プロバイダー名
    pub provider_name: String,
    /// 健全性状態
    pub status: HealthStatus,
    /// 応答時間（ミリ秒）
    pub response_time_ms: u64,
    /// 最終チェック時刻
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// エラーメッセージ（エラー時）
    pub error_message: Option<String>,
}

/// 健全性状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// 正常
    Healthy,
    /// 警告
    Warning,
    /// エラー
    Error,
    /// 不明
    Unknown,
}

/// レート制限状態
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    /// 制限値（1分あたり）
    pub limit_per_minute: u32,
    /// 残りリクエスト数
    pub remaining_requests: u32,
    /// リセット時刻
    pub reset_at: chrono::DateTime<chrono::Utc>,
    /// 制限に達しているかどうか
    pub is_limited: bool,
}

/// VirusTotal プロバイダー
pub struct VirusTotalProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
}

impl VirusTotalProvider {
    /// 新しいVirusTotalプロバイダーを作成
    pub fn new(config: ProviderConfig) -> Result<Self, ThreatError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.timeout_seconds as u64,
            ))
            .build()
            .map_err(|e| ThreatError::ConfigurationError(e.to_string()))?;

        let rate_limiter = RateLimiter::new(config.rate_limit_per_minute);

        Ok(Self {
            config,
            client,
            rate_limiter: Arc::new(tokio::sync::Mutex::new(rate_limiter)),
        })
    }

    /// IPアドレスをチェック
    async fn check_ip_address(&self, ip: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let url = format!("{}/ip-addresses/{}", self.config.base_url, ip);
        let response = self.make_request(&url).await?;
        self.parse_virustotal_response(response).await
    }

    /// ドメインをチェック
    async fn check_domain(&self, domain: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let url = format!("{}/domains/{}", self.config.base_url, domain);
        let response = self.make_request(&url).await?;
        self.parse_virustotal_response(response).await
    }

    /// URLをチェック
    async fn check_url(&self, url_to_check: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let encoded_url = base64::engine::general_purpose::STANDARD.encode(url_to_check);
        let url = format!("{}/urls/{}", self.config.base_url, encoded_url);
        let response = self.make_request(&url).await?;
        self.parse_virustotal_response(response).await
    }

    /// ファイルハッシュをチェック
    async fn check_file_hash(&self, hash: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let url = format!("{}/files/{}", self.config.base_url, hash);
        let response = self.make_request(&url).await?;
        self.parse_virustotal_response(response).await
    }

    /// API リクエストを実行
    async fn make_request(&self, url: &str) -> Result<reqwest::Response, ThreatError> {
        // レート制限チェック
        {
            let limiter = self.rate_limiter.lock().await;
            if !limiter.allow_request().await {
                return Err(ThreatError::RateLimitExceeded(self.name().to_string()));
            }
        }

        let response = self
            .client
            .get(url)
            .header("x-apikey", &self.config.api_key)
            .send()
            .await
            .map_err(|e| ThreatError::NetworkError(e.to_string()))?;

        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            Err(ThreatError::ProviderError(format!(
                "VirusTotal API error: {} - {}",
                status, error_text
            )))
        }
    }

    /// VirusTotalレスポンスをパース
    async fn parse_virustotal_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let _json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ThreatError::ParsingError(e.to_string()))?;

        // VirusTotalのレスポンス構造に基づいてパース
        // 実装は省略（実際のAPIレスポンス形式に依存）
        Ok(vec![])
    }
}

#[async_trait]
impl ThreatProvider for VirusTotalProvider {
    fn name(&self) -> &str {
        "VirusTotal"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn check_indicator(
        &self,
        indicator: &ThreatIndicator,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        match indicator.indicator_type {
            IndicatorType::IpAddress => self.check_ip_address(&indicator.value).await,
            IndicatorType::Domain => self.check_domain(&indicator.value).await,
            IndicatorType::Url => self.check_url(&indicator.value).await,
            IndicatorType::FileHash => self.check_file_hash(&indicator.value).await,
            _ => Err(ThreatError::ConfigurationError(format!(
                "Unsupported indicator type: {:?}",
                indicator.indicator_type
            ))),
        }
    }

    async fn health_check(&self) -> Result<ProviderHealth, ThreatError> {
        let start_time = std::time::Instant::now();

        // 簡単なテストクエリを実行
        let test_url = format!("{}/domains/example.com", self.config.base_url);
        let result = self.make_request(&test_url).await;

        let duration = start_time.elapsed();
        let response_time_ms = duration.as_millis() as u64;

        let (status, error_message) = match result {
            Ok(_) => (HealthStatus::Healthy, None),
            Err(e) => (HealthStatus::Error, Some(e.to_string())),
        };

        Ok(ProviderHealth {
            provider_name: self.name().to_string(),
            status,
            response_time_ms,
            last_check: chrono::Utc::now(),
            error_message,
        })
    }

    async fn get_rate_limit_status(&self) -> Result<RateLimitStatus, ThreatError> {
        let limiter = self.rate_limiter.lock().await;
        Ok(limiter.get_status())
    }
}

/// シンプルなレート制限器
pub struct RateLimiter {
    limit_per_minute: u32,
    requests: tokio::sync::Mutex<Vec<chrono::DateTime<chrono::Utc>>>,
}

impl RateLimiter {
    pub fn new(limit_per_minute: u32) -> Self {
        Self {
            limit_per_minute,
            requests: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    pub async fn allow_request(&self) -> bool {
        let mut requests = self.requests.lock().await;
        let now = chrono::Utc::now();
        let one_minute_ago = now - chrono::Duration::minutes(1);

        // 1分以内のリクエストをフィルタリング
        requests.retain(|&req_time| req_time > one_minute_ago);

        if requests.len() < self.limit_per_minute as usize {
            requests.push(now);
            true
        } else {
            false
        }
    }

    pub fn get_status(&self) -> RateLimitStatus {
        // 簡略化された実装
        RateLimitStatus {
            limit_per_minute: self.limit_per_minute,
            remaining_requests: self.limit_per_minute, // 実際の実装では正確な値を計算
            reset_at: chrono::Utc::now() + chrono::Duration::minutes(1),
            is_limited: false,
        }
    }
}

/// プロバイダーファクトリー
pub struct ProviderFactory;

impl ProviderFactory {
    /// 設定に基づいてプロバイダーを作成
    pub fn create_provider(config: ProviderConfig) -> Result<Box<dyn ThreatProvider>, ThreatError> {
        match config.name.as_str() {
            "VirusTotal" => {
                let provider = VirusTotalProvider::new(config)?;
                Ok(Box::new(provider))
            }
            // 他のプロバイダーもここに追加
            _ => Err(ThreatError::ConfigurationError(format!(
                "Unknown provider: {}",
                config.name
            ))),
        }
    }
}

/// プロバイダー管理
pub struct ProviderManager {
    providers: HashMap<String, Box<dyn ThreatProvider>>,
}

impl ProviderManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// プロバイダーを追加
    pub fn add_provider(&mut self, provider: Box<dyn ThreatProvider>) {
        let name = provider.name().to_string();
        self.providers.insert(name, provider);
    }

    /// プロバイダーを取得
    pub fn get_provider(&self, name: &str) -> Option<&dyn ThreatProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    /// すべてのプロバイダーを取得
    pub fn get_all_providers(&self) -> &HashMap<String, Box<dyn ThreatProvider>> {
        &self.providers
    }

    /// プロバイダーを削除
    pub fn remove_provider(&mut self, name: &str) -> Option<Box<dyn ThreatProvider>> {
        self.providers.remove(name)
    }
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}
