//! Threat Intelligence Providers
//!
//! 外部脅威インテリジェンスプロバイダーとの統合を抽象化

use crate::threat_intelligence::types::*;
use async_trait::async_trait;
use base64::Engine;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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

/// AbuseIPDB プロバイダー
pub struct AbuseIPDBProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
}

impl AbuseIPDBProvider {
    /// 新しいAbuseIPDBプロバイダーを作成
    pub fn new(config: ProviderConfig) -> Result<Self, ThreatError> {
        // APIキーの検証
        if config.api_key.is_empty() {
            return Err(ThreatError::ConfigurationError(
                "AbuseIPDB API key is required".to_string(),
            ));
        }

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
        // IP形式の基本的な検証
        if !Self::is_valid_ip(ip) {
            return Err(ThreatError::ConfigurationError(format!(
                "Invalid IP address format: {}",
                ip
            )));
        }

        let url = format!("{}/api/v2/check", self.config.base_url);

        // レート制限チェック
        {
            let limiter = self.rate_limiter.lock().await;
            if !limiter.allow_request().await {
                return Err(ThreatError::RateLimitExceeded(self.name().to_string()));
            }
        }

        let response = self
            .client
            .get(&url)
            .header("Key", &self.config.api_key)
            .header("Accept", "application/json")
            .query(&[("ipAddress", ip), ("maxAgeInDays", "90"), ("verbose", "")])
            .send()
            .await
            .map_err(|e| ThreatError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ThreatError::ProviderError(format!(
                "AbuseIPDB API error: {} - {}",
                status, error_text
            )));
        }

        let json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ThreatError::ParsingError(e.to_string()))?;

        self.parse_abuseipdb_response(ip, json_response).await
    }

    /// AbuseIPDBレスポンスをパース
    async fn parse_abuseipdb_response(
        &self,
        ip: &str,
        response: serde_json::Value,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let data = response["data"]
            .as_object()
            .ok_or_else(|| ThreatError::ParsingError("Missing data field".to_string()))?;

        let abuse_confidence_score =
            data["abuseConfidenceScore"].as_u64().unwrap_or(0) as f64 / 100.0;

        // スコアが低い場合は脅威なしと判断
        if abuse_confidence_score < 0.1 {
            return Ok(vec![]);
        }

        let total_reports = data["totalReports"].as_u64().unwrap_or(0);
        let country_code = data["countryCode"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();
        let country_name = data["countryName"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();
        let is_public = data["isPublic"].as_bool().unwrap_or(true);
        let is_whitelisted = data["isWhitelisted"].as_bool().unwrap_or(false);

        // ホワイトリスト登録されている場合は脅威なしと判断
        if is_whitelisted {
            return Ok(vec![]);
        }

        // 深刻度をスコアに基づいて決定
        let severity = if abuse_confidence_score >= 0.8 {
            SeverityLevel::Critical
        } else if abuse_confidence_score >= 0.6 {
            SeverityLevel::High
        } else if abuse_confidence_score >= 0.4 {
            SeverityLevel::Medium
        } else if abuse_confidence_score >= 0.2 {
            SeverityLevel::Low
        } else {
            SeverityLevel::Info
        };

        // 脅威タイプを判定
        let mut threat_types = Vec::new();
        let usage_type = data["usageType"].as_str().unwrap_or("");

        if let Some(reports) = data["reports"].as_array() {
            for report in reports {
                if let Some(categories) = report["categories"].as_array() {
                    for category in categories {
                        if let Some(cat_num) = category.as_u64() {
                            threat_types.push(Self::category_to_threat_type(cat_num));
                        }
                    }
                }
            }
        }

        // デフォルトの脅威タイプ
        let threat_type = if !threat_types.is_empty() {
            threat_types[0].clone()
        } else if usage_type.contains("Data Center") {
            ThreatType::Other("Datacenter/Proxy".to_string())
        } else {
            ThreatType::MaliciousIp
        };

        // 脅威指標の作成
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: ip.to_string(),
            pattern: None,
            tags: vec![
                format!("abuse_score:{:.0}", abuse_confidence_score * 100.0),
                format!("reports:{}", total_reports),
                country_code.clone(),
            ],
            context: Some(format!(
                "Public: {}, Reports: {}, Country: {}",
                is_public, total_reports, country_name
            )),
            first_seen: chrono::Utc::now(),
        };

        // 地理情報の作成
        let geolocation = if country_code != "Unknown" {
            Some(GeolocationInfo {
                country_code: country_code.clone(),
                country_name: country_name.clone(),
                region: None,
                city: None,
                latitude: None,
                longitude: None,
            })
        } else {
            None
        };

        // メタデータの作成
        let mut custom_attributes = HashMap::new();
        custom_attributes.insert("is_public".to_string(), is_public.to_string());
        custom_attributes.insert("usage_type".to_string(), usage_type.to_string());
        custom_attributes.insert("total_reports".to_string(), total_reports.to_string());
        custom_attributes.insert(
            "abuse_confidence_score".to_string(),
            format!("{:.2}", abuse_confidence_score),
        );

        if let Some(isp) = data["isp"].as_str() {
            custom_attributes.insert("isp".to_string(), isp.to_string());
        }
        if let Some(domain) = data["domain"].as_str() {
            custom_attributes.insert("domain".to_string(), domain.to_string());
        }

        let metadata = ThreatMetadata {
            description: Some(format!(
                "IP {} reported {} times with {:.0}% abuse confidence score",
                ip,
                total_reports,
                abuse_confidence_score * 100.0
            )),
            attack_techniques: Vec::new(),
            mitre_attack_techniques: Vec::new(),
            cve_references: Vec::new(),
            malware_families: Vec::new(),
            geolocation,
            custom_attributes,
        };

        // 脅威ソースの作成
        let source = ThreatSource {
            provider: self.name().to_string(),
            feed_name: "AbuseIPDB Reports".to_string(),
            reliability: self.config.reliability_factor,
            last_updated: chrono::Utc::now(),
        };

        // 脅威インテリジェンスの作成
        let threat = ThreatIntelligence {
            id: uuid::Uuid::new_v4().to_string(),
            threat_type,
            severity,
            indicators: vec![indicator],
            source,
            confidence_score: abuse_confidence_score * self.config.reliability_factor,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            expiration: Some(chrono::Utc::now() + chrono::Duration::days(30)),
            metadata,
        };

        Ok(vec![threat])
    }

    /// AbuseIPDBカテゴリー番号を脅威タイプに変換
    fn category_to_threat_type(category: u64) -> ThreatType {
        match category {
            3..=11 => ThreatType::Malware,
            12 | 13 => ThreatType::Phishing,
            14 => ThreatType::Spam,
            15..=17 => ThreatType::CommandAndControl,
            18..=20 => ThreatType::Botnet,
            21 => ThreatType::Exploit,
            _ => ThreatType::MaliciousIp,
        }
    }

    /// IP形式の検証
    fn is_valid_ip(ip: &str) -> bool {
        // 簡易的なIPv4/IPv6検証
        ip.parse::<std::net::IpAddr>().is_ok()
    }
}

#[async_trait]
impl ThreatProvider for AbuseIPDBProvider {
    fn name(&self) -> &str {
        "AbuseIPDB"
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
            _ => Err(ThreatError::ConfigurationError(format!(
                "AbuseIPDB only supports IP address lookups, got: {:?}",
                indicator.indicator_type
            ))),
        }
    }

    async fn health_check(&self) -> Result<ProviderHealth, ThreatError> {
        let start_time = std::time::Instant::now();

        // 既知の安全なIPでテスト（8.8.8.8 - Google DNS）
        let test_indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "8.8.8.8".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        let result = self.check_indicator(&test_indicator).await;
        let duration = start_time.elapsed();
        let response_time_ms = duration.as_millis() as u64;

        let (status, error_message) = match result {
            Ok(_) => (HealthStatus::Healthy, None),
            Err(ThreatError::RateLimitExceeded(_)) => (
                HealthStatus::Warning,
                Some("Rate limit reached".to_string()),
            ),
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

/// CVEキャッシュエントリ型
type CveCache = HashMap<String, (Vec<ThreatIntelligence>, DateTime<Utc>)>;

/// CVE (Common Vulnerabilities and Exposures) プロバイダー
pub struct CVEProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
    cache: Arc<RwLock<CveCache>>,
}

impl CVEProvider {
    /// 新しいCVEプロバイダーを作成
    pub fn new(config: ProviderConfig) -> Result<Self, ThreatError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.timeout_seconds as u64,
            ))
            .user_agent("mcp-rs-threat-intelligence/0.15.0")
            .build()
            .map_err(|e| ThreatError::ConfigurationError(e.to_string()))?;

        let rate_limiter = RateLimiter::new(config.rate_limit_per_minute);

        Ok(Self {
            config,
            client,
            rate_limiter: Arc::new(tokio::sync::Mutex::new(rate_limiter)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// CVE IDで脆弱性情報を検索
    async fn check_cve_id(&self, cve_id: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        // CVE ID形式の検証 (CVE-YYYY-NNNNN)
        if !Self::is_valid_cve_id(cve_id) {
            return Err(ThreatError::ConfigurationError(format!(
                "Invalid CVE ID format: {}. Expected format: CVE-YYYY-NNNNN",
                cve_id
            )));
        }

        // キャッシュチェック（24時間有効）
        {
            let cache = self.cache.read().await;
            if let Some((cached_threats, cached_at)) = cache.get(cve_id) {
                let age = Utc::now() - *cached_at;
                if age < chrono::Duration::hours(24) {
                    return Ok(cached_threats.clone());
                }
            }
        }

        let url = format!(
            "{}/rest/json/cves/2.0?cveId={}",
            self.config.base_url, cve_id
        );

        // レート制限チェック
        {
            let limiter = self.rate_limiter.lock().await;
            if !limiter.allow_request().await {
                return Err(ThreatError::RateLimitExceeded(self.name().to_string()));
            }
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ThreatError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ThreatError::ProviderError(format!(
                "NVD API error: {} - {}",
                status, error_text
            )));
        }

        let json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ThreatError::ParsingError(e.to_string()))?;

        let threats = self.parse_cve_response(cve_id, json_response).await?;

        // キャッシュ更新
        {
            let mut cache = self.cache.write().await;
            cache.insert(cve_id.to_string(), (threats.clone(), Utc::now()));
        }

        Ok(threats)
    }

    /// キーワードでCVEを検索
    async fn search_cve_by_keyword(
        &self,
        keyword: &str,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let url = format!(
            "{}/rest/json/cves/2.0?keywordSearch={}",
            self.config.base_url,
            urlencoding::encode(keyword)
        );

        // レート制限チェック
        {
            let limiter = self.rate_limiter.lock().await;
            if !limiter.allow_request().await {
                return Err(ThreatError::RateLimitExceeded(self.name().to_string()));
            }
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ThreatError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ThreatError::ProviderError(format!(
                "NVD API error: {} - {}",
                status, error_text
            )));
        }

        let json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ThreatError::ParsingError(e.to_string()))?;

        self.parse_cve_search_response(json_response).await
    }

    /// CVEレスポンスをパース
    async fn parse_cve_response(
        &self,
        cve_id: &str,
        response: serde_json::Value,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let vulnerabilities = response["vulnerabilities"].as_array().ok_or_else(|| {
            ThreatError::ParsingError("Missing vulnerabilities field".to_string())
        })?;

        if vulnerabilities.is_empty() {
            return Ok(vec![]);
        }

        let mut threats = Vec::new();

        for vuln in vulnerabilities {
            let cve = &vuln["cve"];

            // 基本情報
            let id = cve["id"].as_str().unwrap_or(cve_id).to_string();
            let published = cve["published"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            let last_modified = cve["lastModified"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            // 説明
            let descriptions = &cve["descriptions"];
            let description = descriptions
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|desc| desc["value"].as_str())
                .unwrap_or("No description available")
                .to_string();

            // CVSS スコア
            let metrics = &cve["metrics"];
            let (severity, cvss_score, cvss_vector) = Self::extract_cvss_info(metrics);

            // CPE (影響を受ける製品)
            let mut affected_products = Vec::new();
            if let Some(configurations) = cve["configurations"].as_array() {
                for config in configurations {
                    if let Some(nodes) = config["nodes"].as_array() {
                        for node in nodes {
                            if let Some(cpe_match) = node["cpeMatch"].as_array() {
                                for cpe in cpe_match {
                                    if let Some(criteria) = cpe["criteria"].as_str() {
                                        affected_products.push(criteria.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 参照リンク
            let mut references = Vec::new();
            if let Some(refs) = cve["references"].as_array() {
                for ref_item in refs {
                    if let Some(url) = ref_item["url"].as_str() {
                        references.push(url.to_string());
                    }
                }
            }

            // 脅威指標の作成
            let indicator = ThreatIndicator {
                indicator_type: IndicatorType::FileHash, // CVEは一般的な脆弱性として扱う
                value: id.clone(),
                pattern: None,
                tags: vec![
                    format!("cvss:{:.1}", cvss_score),
                    format!("severity:{:?}", severity),
                ],
                context: Some(description.clone()),
                first_seen: published,
            };

            // メタデータ
            let mut custom_attributes = HashMap::new();
            custom_attributes.insert("cvss_score".to_string(), format!("{:.1}", cvss_score));
            custom_attributes.insert("cvss_vector".to_string(), cvss_vector.clone());
            custom_attributes.insert(
                "affected_products_count".to_string(),
                affected_products.len().to_string(),
            );
            custom_attributes.insert("references_count".to_string(), references.len().to_string());

            if !affected_products.is_empty() {
                custom_attributes.insert(
                    "sample_affected_product".to_string(),
                    affected_products[0].clone(),
                );
            }

            let metadata = ThreatMetadata {
                description: Some(description),
                attack_techniques: Vec::new(),
                mitre_attack_techniques: Vec::new(),
                cve_references: vec![id.clone()],
                malware_families: Vec::new(),
                geolocation: None,
                custom_attributes,
            };

            // 脅威ソース
            let source = ThreatSource {
                provider: self.name().to_string(),
                feed_name: "NVD CVE Database".to_string(),
                reliability: self.config.reliability_factor,
                last_updated: last_modified,
            };

            // 脅威インテリジェンス
            let threat = ThreatIntelligence {
                id: uuid::Uuid::new_v4().to_string(),
                threat_type: ThreatType::Exploit,
                severity,
                indicators: vec![indicator],
                source,
                confidence_score: self.config.reliability_factor,
                first_seen: published,
                last_seen: last_modified,
                expiration: None, // CVEは期限切れにならない
                metadata,
            };

            threats.push(threat);
        }

        Ok(threats)
    }

    /// CVE検索レスポンスをパース
    async fn parse_cve_search_response(
        &self,
        response: serde_json::Value,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let vulnerabilities = response["vulnerabilities"].as_array().ok_or_else(|| {
            ThreatError::ParsingError("Missing vulnerabilities field".to_string())
        })?;

        let mut all_threats = Vec::new();

        // 最大10件まで処理（検索結果が多すぎる場合）
        for vuln in vulnerabilities.iter().take(10) {
            let cve_id = vuln["cve"]["id"].as_str().unwrap_or("UNKNOWN").to_string();

            if let Ok(threats) = self
                .parse_cve_response(
                    &cve_id,
                    serde_json::json!({
                        "vulnerabilities": [vuln]
                    }),
                )
                .await
            {
                all_threats.extend(threats);
            }
        }

        Ok(all_threats)
    }

    /// CVSSメトリクスから情報を抽出
    fn extract_cvss_info(metrics: &serde_json::Value) -> (SeverityLevel, f64, String) {
        // CVSS v3.1を優先、次にv3.0、最後にv2.0
        if let Some(cvss31) = metrics["cvssMetricV31"]
            .as_array()
            .and_then(|arr| arr.first())
        {
            let score = cvss31["cvssData"]["baseScore"].as_f64().unwrap_or(0.0);
            let severity = cvss31["cvssData"]["baseSeverity"]
                .as_str()
                .and_then(SeverityLevel::from_string)
                .unwrap_or(SeverityLevel::Medium);
            let vector = cvss31["cvssData"]["vectorString"]
                .as_str()
                .unwrap_or("")
                .to_string();
            return (severity, score, vector);
        }

        if let Some(cvss30) = metrics["cvssMetricV30"]
            .as_array()
            .and_then(|arr| arr.first())
        {
            let score = cvss30["cvssData"]["baseScore"].as_f64().unwrap_or(0.0);
            let severity = cvss30["cvssData"]["baseSeverity"]
                .as_str()
                .and_then(SeverityLevel::from_string)
                .unwrap_or(SeverityLevel::Medium);
            let vector = cvss30["cvssData"]["vectorString"]
                .as_str()
                .unwrap_or("")
                .to_string();
            return (severity, score, vector);
        }

        if let Some(cvss2) = metrics["cvssMetricV2"]
            .as_array()
            .and_then(|arr| arr.first())
        {
            let score = cvss2["cvssData"]["baseScore"].as_f64().unwrap_or(0.0);
            let severity = Self::cvss_score_to_severity(score);
            let vector = cvss2["cvssData"]["vectorString"]
                .as_str()
                .unwrap_or("")
                .to_string();
            return (severity, score, vector);
        }

        (SeverityLevel::Medium, 0.0, String::new())
    }

    /// CVSSスコアから深刻度を計算
    fn cvss_score_to_severity(score: f64) -> SeverityLevel {
        match score {
            s if s >= 9.0 => SeverityLevel::Critical,
            s if s >= 7.0 => SeverityLevel::High,
            s if s >= 4.0 => SeverityLevel::Medium,
            s if s > 0.0 => SeverityLevel::Low,
            _ => SeverityLevel::Info,
        }
    }

    /// CVE ID形式の検証
    fn is_valid_cve_id(cve_id: &str) -> bool {
        // CVE-YYYY-NNNNN 形式（YYYYは4桁、NNNNNは4桁以上）
        let pattern = regex::Regex::new(r"^CVE-\d{4}-\d{4,}$").unwrap();
        pattern.is_match(cve_id)
    }

    /// キャッシュサイズを取得
    pub async fn cache_size(&self) -> usize {
        self.cache.read().await.len()
    }

    /// キャッシュをクリア
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}

#[async_trait]
impl ThreatProvider for CVEProvider {
    fn name(&self) -> &str {
        "CVE"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn check_indicator(
        &self,
        indicator: &ThreatIndicator,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        match indicator.indicator_type {
            IndicatorType::FileHash => {
                // CVE ID として扱う
                if Self::is_valid_cve_id(&indicator.value) {
                    self.check_cve_id(&indicator.value).await
                } else {
                    // キーワード検索として扱う
                    self.search_cve_by_keyword(&indicator.value).await
                }
            }
            _ => {
                // その他の指標タイプはキーワード検索
                self.search_cve_by_keyword(&indicator.value).await
            }
        }
    }

    async fn health_check(&self) -> Result<ProviderHealth, ThreatError> {
        let start_time = std::time::Instant::now();

        // 既知のCVEでテスト（CVE-2021-44228 - Log4Shell）
        let test_indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "CVE-2021-44228".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        let result = self.check_indicator(&test_indicator).await;
        let duration = start_time.elapsed();
        let response_time_ms = duration.as_millis() as u64;

        let (status, error_message) = match result {
            Ok(_) => (HealthStatus::Healthy, None),
            Err(ThreatError::RateLimitExceeded(_)) => (
                HealthStatus::Warning,
                Some("Rate limit reached".to_string()),
            ),
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
            "AbuseIPDB" => {
                let provider = AbuseIPDBProvider::new(config)?;
                Ok(Box::new(provider))
            }
            "CVE" => {
                let provider = CVEProvider::new(config)?;
                Ok(Box::new(provider))
            }
            "MITRE" | "MITRE-ATTACK" => {
                let provider = MitreAttackProvider::new(config)?;
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

/// MITRE ATT&CK キャッシュエントリ型
type MitreCache = HashMap<String, (Vec<MitreAttackTechnique>, DateTime<Utc>)>;
type MitreGroupCache = HashMap<String, (MitreAttackGroup, DateTime<Utc>)>;

/// MITRE ATT&CK プロバイダー
pub struct MitreAttackProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
    techniques_cache: Arc<RwLock<MitreCache>>,
    groups_cache: Arc<RwLock<MitreGroupCache>>,
}

impl MitreAttackProvider {
    /// 新しいMITRE ATT&CKプロバイダーを作成
    pub fn new(config: ProviderConfig) -> Result<Self, ThreatError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.timeout_seconds as u64,
            ))
            .user_agent("mcp-rs-threat-intelligence/0.15.0")
            .build()
            .map_err(|e| ThreatError::ConfigurationError(e.to_string()))?;

        let rate_limiter = RateLimiter::new(config.rate_limit_per_minute);

        Ok(Self {
            config,
            client,
            rate_limiter: Arc::new(tokio::sync::Mutex::new(rate_limiter)),
            techniques_cache: Arc::new(RwLock::new(HashMap::new())),
            groups_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// テクニックIDでATT&CK情報を検索
    async fn search_technique(
        &self,
        technique_id: &str,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        // テクニックID形式の検証 (T1234 または T1234.001)
        if !Self::is_valid_technique_id(technique_id) {
            return Err(ThreatError::ConfigurationError(format!(
                "Invalid MITRE ATT&CK technique ID format: {}. Expected format: TXXXX or TXXXX.XXX",
                technique_id
            )));
        }

        // キャッシュチェック（7日間有効）
        {
            let cache = self.techniques_cache.read().await;
            if let Some((cached_techniques, cached_at)) = cache.get(technique_id) {
                let age = Utc::now() - *cached_at;
                if age < chrono::Duration::days(7) {
                    return self
                        .build_threat_intelligence_from_techniques(cached_techniques.clone())
                        .await;
                }
            }
        }

        // MITRE ATT&CK STIX APIエンドポイント
        let url = format!(
            "{}/attack-pattern/attack-pattern--{}",
            self.config.base_url,
            self.technique_id_to_uuid(technique_id)
        );

        // レート制限チェック
        {
            let limiter = self.rate_limiter.lock().await;
            if !limiter.allow_request().await {
                return Err(ThreatError::RateLimitExceeded(self.name().to_string()));
            }
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ThreatError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ThreatError::ProviderError(format!(
                "MITRE ATT&CK API error: {} - {}",
                status, error_text
            )));
        }

        let json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ThreatError::ParsingError(e.to_string()))?;

        let techniques = self
            .parse_technique_response(technique_id, json_response)
            .await?;

        // キャッシュ更新
        {
            let mut cache = self.techniques_cache.write().await;
            cache.insert(technique_id.to_string(), (techniques.clone(), Utc::now()));
        }

        self.build_threat_intelligence_from_techniques(techniques)
            .await
    }

    /// キーワードでテクニックを検索
    async fn search_by_keyword(
        &self,
        keyword: &str,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        // 簡略化: ローカルマッピングを使用
        let techniques = self.search_techniques_by_keyword(keyword).await?;
        self.build_threat_intelligence_from_techniques(techniques)
            .await
    }

    /// テクニック情報をパース
    async fn parse_technique_response(
        &self,
        technique_id: &str,
        response: serde_json::Value,
    ) -> Result<Vec<MitreAttackTechnique>, ThreatError> {
        let mut techniques = Vec::new();

        // STIX 2.0/2.1 形式のレスポンスをパース
        if let Some(objects) = response["objects"].as_array() {
            for obj in objects {
                if obj["type"].as_str() == Some("attack-pattern") {
                    let name = obj["name"].as_str().unwrap_or("Unknown").to_string();
                    let description = obj["description"].as_str().map(|s| s.to_string());

                    // 戦術 (Tactics) の抽出
                    let mut tactics = Vec::new();
                    if let Some(kill_chain_phases) = obj["kill_chain_phases"].as_array() {
                        for phase in kill_chain_phases {
                            if let Some(phase_name) = phase["phase_name"].as_str() {
                                tactics.push(phase_name.to_string());
                            }
                        }
                    }

                    // プラットフォームの抽出
                    let mut platforms = Vec::new();
                    if let Some(platform_array) = obj["x_mitre_platforms"].as_array() {
                        for platform in platform_array {
                            if let Some(p) = platform.as_str() {
                                platforms.push(p.to_string());
                            }
                        }
                    }

                    // データソースの抽出
                    let mut data_sources = Vec::new();
                    if let Some(ds_array) = obj["x_mitre_data_sources"].as_array() {
                        for ds in ds_array {
                            if let Some(d) = ds.as_str() {
                                data_sources.push(d.to_string());
                            }
                        }
                    }

                    // 検出方法
                    let detection = obj["x_mitre_detection"].as_str().map(|s| s.to_string());

                    // 緩和策（別のAPIコールが必要な場合があるため、ここでは空）
                    let mitigation = Vec::new();

                    let technique = MitreAttackTechnique {
                        technique_id: technique_id.to_string(),
                        sub_technique_id: if technique_id.contains('.') {
                            Some(technique_id.to_string())
                        } else {
                            None
                        },
                        name,
                        tactics,
                        platforms,
                        data_sources,
                        description,
                        detection,
                        mitigation,
                    };

                    techniques.push(technique);
                }
            }
        }

        Ok(techniques)
    }

    /// キーワードでテクニックを検索（ローカルマッピング使用）
    async fn search_techniques_by_keyword(
        &self,
        keyword: &str,
    ) -> Result<Vec<MitreAttackTechnique>, ThreatError> {
        // 一般的な攻撃手法のマッピング
        let keyword_lower = keyword.to_lowercase();
        let technique_id = match keyword_lower.as_str() {
            "phishing" | "spearphishing" => "T1566",
            "credential dumping" | "credentials" => "T1003",
            "powershell" => "T1059.001",
            "command and control" | "c2" | "c&c" => "T1071",
            "remote desktop" | "rdp" => "T1021.001",
            "lateral movement" => "T1021",
            "privilege escalation" => "T1068",
            "persistence" => "T1547",
            "defense evasion" => "T1562",
            "exfiltration" => "T1041",
            _ => {
                // キーワードに一致しない場合は空の結果を返す
                return Ok(Vec::new());
            }
        };

        // テクニックIDで検索
        self.search_technique(technique_id).await?;

        // キャッシュから取得
        let cache = self.techniques_cache.read().await;
        if let Some((techniques, _)) = cache.get(technique_id) {
            Ok(techniques.clone())
        } else {
            Ok(Vec::new())
        }
    }

    /// テクニック情報から脅威インテリジェンスを構築
    async fn build_threat_intelligence_from_techniques(
        &self,
        techniques: Vec<MitreAttackTechnique>,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        let mut threats = Vec::new();

        for technique in techniques {
            // 脅威指標の作成
            let indicator = ThreatIndicator {
                indicator_type: IndicatorType::FileHash, // ATT&CKは一般的な攻撃パターンとして扱う
                value: technique.technique_id.clone(),
                pattern: None,
                tags: vec![
                    format!("mitre:{}", technique.technique_id),
                    format!("tactics:{}", technique.tactics.join(",")),
                ],
                context: technique.description.clone(),
                first_seen: Utc::now(),
            };

            // 深刻度の決定（戦術に基づく）
            let severity = Self::determine_severity(&technique.tactics);

            // メタデータ
            let mut custom_attributes = HashMap::new();
            custom_attributes.insert("technique_id".to_string(), technique.technique_id.clone());
            custom_attributes.insert("technique_name".to_string(), technique.name.clone());
            custom_attributes.insert("tactics".to_string(), technique.tactics.join(", "));
            custom_attributes.insert("platforms".to_string(), technique.platforms.join(", "));
            custom_attributes.insert(
                "data_sources".to_string(),
                technique.data_sources.join(", "),
            );

            if let Some(ref detection) = technique.detection {
                custom_attributes.insert("detection".to_string(), detection.clone());
            }

            let metadata = ThreatMetadata {
                description: technique.description.clone(),
                attack_techniques: vec![technique.technique_id.clone()],
                mitre_attack_techniques: vec![technique.clone()],
                cve_references: Vec::new(),
                malware_families: Vec::new(),
                geolocation: None,
                custom_attributes,
            };

            // 脅威ソース
            let source = ThreatSource {
                provider: self.name().to_string(),
                feed_name: "MITRE ATT&CK Framework".to_string(),
                reliability: self.config.reliability_factor,
                last_updated: Utc::now(),
            };

            // 脅威インテリジェンス
            let threat = ThreatIntelligence {
                id: uuid::Uuid::new_v4().to_string(),
                threat_type: ThreatType::Other(format!("ATT&CK: {}", technique.tactics.join(", "))),
                severity,
                indicators: vec![indicator],
                source,
                confidence_score: self.config.reliability_factor,
                first_seen: Utc::now(),
                last_seen: Utc::now(),
                expiration: None, // ATT&CKテクニックは期限切れにならない
                metadata,
            };

            threats.push(threat);
        }

        Ok(threats)
    }

    /// 戦術に基づいて深刻度を決定
    fn determine_severity(tactics: &[String]) -> SeverityLevel {
        // 高リスクの戦術
        let high_risk_tactics = ["impact", "exfiltration", "lateral-movement"];
        let medium_risk_tactics = [
            "privilege-escalation",
            "credential-access",
            "defense-evasion",
        ];

        for tactic in tactics {
            let tactic_lower = tactic.to_lowercase();
            if high_risk_tactics.iter().any(|&t| tactic_lower.contains(t)) {
                return SeverityLevel::High;
            }
        }

        for tactic in tactics {
            let tactic_lower = tactic.to_lowercase();
            if medium_risk_tactics
                .iter()
                .any(|&t| tactic_lower.contains(t))
            {
                return SeverityLevel::Medium;
            }
        }

        SeverityLevel::Low
    }

    /// テクニックID形式の検証
    fn is_valid_technique_id(technique_id: &str) -> bool {
        // T1234 または T1234.001 形式
        let pattern = regex::Regex::new(r"^T\d{4}(\.\d{3})?$").unwrap();
        pattern.is_match(technique_id)
    }

    /// テクニックIDをUUIDに変換（STIX ID用）
    fn technique_id_to_uuid(&self, _technique_id: &str) -> String {
        // 実際の実装ではMITRE ATT&CKのSTIX IDマッピングを使用
        // ここでは簡略化のためダミーUUIDを返す
        uuid::Uuid::new_v4().to_string()
    }

    /// キャッシュサイズを取得
    pub async fn techniques_cache_size(&self) -> usize {
        self.techniques_cache.read().await.len()
    }

    /// キャッシュをクリア
    pub async fn clear_cache(&self) {
        self.techniques_cache.write().await.clear();
        self.groups_cache.write().await.clear();
    }
}

#[async_trait]
impl ThreatProvider for MitreAttackProvider {
    fn name(&self) -> &str {
        "MITRE-ATTACK"
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    async fn check_indicator(
        &self,
        indicator: &ThreatIndicator,
    ) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        // テクニックIDとして扱う
        if Self::is_valid_technique_id(&indicator.value) {
            self.search_technique(&indicator.value).await
        } else {
            // キーワード検索として扱う
            self.search_by_keyword(&indicator.value).await
        }
    }

    async fn health_check(&self) -> Result<ProviderHealth, ThreatError> {
        let start_time = std::time::Instant::now();

        // 既知のテクニックでテスト（T1566 - Phishing）
        let test_indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "T1566".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        let result = self.check_indicator(&test_indicator).await;
        let duration = start_time.elapsed();
        let response_time_ms = duration.as_millis() as u64;

        let (status, error_message) = match result {
            Ok(_) => (HealthStatus::Healthy, None),
            Err(ThreatError::RateLimitExceeded(_)) => (
                HealthStatus::Warning,
                Some("Rate limit reached".to_string()),
            ),
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
