//! Threat Detection Engine
//!
//! 脅威の検出とリアルタイム分析

use crate::error::McpError;
use crate::threat_intelligence::manager::ThreatIntelligenceManager;
use crate::threat_intelligence::types::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

/// 脅威検出エンジン
pub struct ThreatDetectionEngine {
    /// 脅威インテリジェンス管理システム
    threat_manager: Arc<ThreatIntelligenceManager>,
    /// 検出ルール
    detection_rules: Arc<RwLock<DetectionRules>>,
    /// 検出設定
    config: Arc<RwLock<DetectionConfig>>,
    /// 検出統計
    stats: Arc<RwLock<DetectionStats>>,
}

/// 検出設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    /// IP アドレス検出の有効/無効
    pub ip_detection_enabled: bool,
    /// URL 検出の有効/無効
    pub url_detection_enabled: bool,
    /// ファイルハッシュ検出の有効/無効
    pub file_hash_detection_enabled: bool,
    /// メール検出の有効/無効
    pub email_detection_enabled: bool,
    /// 最大並列検出数
    pub max_concurrent_detections: usize,
    /// 検出タイムアウト（秒）
    pub detection_timeout_seconds: u32,
    /// 自動抽出の有効/無効
    pub auto_extraction_enabled: bool,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            ip_detection_enabled: true,
            url_detection_enabled: true,
            file_hash_detection_enabled: true,
            email_detection_enabled: true,
            max_concurrent_detections: 20,
            detection_timeout_seconds: 60,
            auto_extraction_enabled: true,
        }
    }
}

/// 検出ルール
pub struct DetectionRules {
    /// IP アドレス検出のための正規表現
    ip_regex: Regex,
    /// URL 検出のための正規表現
    url_regex: Regex,
    /// MD5 ハッシュ検出のための正規表現
    md5_regex: Regex,
    /// SHA1 ハッシュ検出のための正規表現
    sha1_regex: Regex,
    /// SHA256 ハッシュ検出のための正規表現
    sha256_regex: Regex,
    /// メールアドレス検出のための正規表現
    email_regex: Regex,
    /// 除外パターン
    exclusion_patterns: Vec<Regex>,
}

impl DetectionRules {
    /// 新しい検出ルールを作成
    pub fn new() -> Result<Self, ThreatError> {
        Ok(Self {
            ip_regex: Regex::new(
                r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b"
            ).map_err(|e| ThreatError::ConfigurationError(format!("Invalid IP regex: {}", e)))?,

            url_regex: Regex::new(
                r"https?://(?:[-\w.])+(?:[:\d]+)?(?:/(?:[\w/_.])*(?:\?(?:[\w&=%.])*)?(?:#(?:[\w.])*)?)?",
            ).map_err(|e| ThreatError::ConfigurationError(format!("Invalid URL regex: {}", e)))?,

            md5_regex: Regex::new(r"\b[a-fA-F0-9]{32}\b")
                .map_err(|e| ThreatError::ConfigurationError(format!("Invalid MD5 regex: {}", e)))?,

            sha1_regex: Regex::new(r"\b[a-fA-F0-9]{40}\b")
                .map_err(|e| ThreatError::ConfigurationError(format!("Invalid SHA1 regex: {}", e)))?,

            sha256_regex: Regex::new(r"\b[a-fA-F0-9]{64}\b")
                .map_err(|e| ThreatError::ConfigurationError(format!("Invalid SHA256 regex: {}", e)))?,

            email_regex: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b")
                .map_err(|e| ThreatError::ConfigurationError(format!("Invalid email regex: {}", e)))?,

            exclusion_patterns: vec![
                // プライベートIPアドレスを除外
                Regex::new(r"^10\.").unwrap(),
                Regex::new(r"^192\.168\.").unwrap(),
                Regex::new(r"^172\.(1[6-9]|2[0-9]|3[0-1])\.").unwrap(),
                Regex::new(r"^127\.").unwrap(),
                // ローカルホストURL を除外
                Regex::new(r"https?://localhost").unwrap(),
                Regex::new(r"https?://127\.0\.0\.1").unwrap(),
            ],
        })
    }
}

impl Default for DetectionRules {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

/// 検出統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectionStats {
    /// 総検出試行数
    pub total_detections: u64,
    /// 成功した検出数
    pub successful_detections: u64,
    /// 失敗した検出数
    pub failed_detections: u64,
    /// 抽出された指標数
    pub indicators_extracted: u64,
    /// 脅威として分類された指標数
    pub threats_identified: u64,
    /// 平均検出時間（ミリ秒）
    pub avg_detection_time_ms: f64,
    /// 指標タイプ別統計
    pub indicator_type_stats: HashMap<IndicatorType, TypeStats>,
}

/// 指標タイプ別統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeStats {
    /// 検出数
    pub detected: u64,
    /// 脅威数
    pub threats: u64,
    /// 平均信頼度
    pub avg_confidence: f64,
}

/// 検出結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// 検出された指標
    pub indicators: Vec<ThreatIndicator>,
    /// 脅威評価結果
    pub assessments: Vec<ThreatAssessment>,
    /// 検出メタデータ
    pub metadata: DetectionMetadata,
}

/// 検出メタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionMetadata {
    /// 検出開始時刻
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// 検出完了時刻
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// 検出時間（ミリ秒）
    pub duration_ms: u64,
    /// 抽出された指標数
    pub indicators_extracted: usize,
    /// 評価済み指標数
    pub indicators_assessed: usize,
    /// 脅威として識別された指標数
    pub threats_identified: usize,
    /// エラー
    pub errors: Vec<String>,
    /// 警告
    pub warnings: Vec<String>,
}

impl ThreatDetectionEngine {
    /// 新しい脅威検出エンジンを作成
    pub fn new(threat_manager: Arc<ThreatIntelligenceManager>) -> Result<Self, ThreatError> {
        Ok(Self {
            threat_manager,
            detection_rules: Arc::new(RwLock::new(DetectionRules::new()?)),
            config: Arc::new(RwLock::new(DetectionConfig::default())),
            stats: Arc::new(RwLock::new(DetectionStats::default())),
        })
    }

    /// テキストから脅威指標を検出・評価
    pub async fn detect_threats(&self, input: &str) -> Result<DetectionResult, ThreatError> {
        let start_time = chrono::Utc::now();
        let mut errors = Vec::new();
        let warnings = Vec::new();

        debug!(
            "Starting threat detection for input length: {}",
            input.len()
        );

        // 1. 指標を抽出
        let indicators = match self.extract_indicators(input).await {
            Ok(indicators) => indicators,
            Err(e) => {
                errors.push(format!("Indicator extraction failed: {}", e));
                Vec::new()
            }
        };

        debug!("Extracted {} indicators", indicators.len());

        // 2. 各指標を評価
        let assessments = match self.assess_indicators(indicators.clone()).await {
            Ok(assessments) => assessments,
            Err(e) => {
                errors.push(format!("Indicator assessment failed: {}", e));
                Vec::new()
            }
        };

        let threats_count = assessments.iter().filter(|a| a.is_threat).count();

        debug!(
            "Assessment completed: {} threats identified out of {} indicators",
            threats_count,
            indicators.len()
        );

        let end_time = chrono::Utc::now();
        let duration_ms = (end_time.timestamp_millis() - start_time.timestamp_millis()) as u64;

        // 統計を更新
        self.update_detection_stats(&indicators, &assessments, duration_ms)
            .await;

        Ok(DetectionResult {
            indicators: indicators.clone(),
            assessments,
            metadata: DetectionMetadata {
                started_at: start_time,
                completed_at: end_time,
                duration_ms,
                indicators_extracted: indicators.len(),
                indicators_assessed: indicators.len(), // TODO: 実際に評価された数を追跡
                threats_identified: threats_count,
                errors,
                warnings,
            },
        })
    }

    /// 指定された指標タイプのみを検出
    pub async fn detect_specific_threats(
        &self,
        input: &str,
        indicator_types: &[IndicatorType],
    ) -> Result<DetectionResult, ThreatError> {
        let start_time = chrono::Utc::now();

        // 指定されたタイプの指標のみを抽出
        let mut all_indicators = self.extract_indicators(input).await?;
        all_indicators.retain(|indicator| indicator_types.contains(&indicator.indicator_type));

        // 評価
        let assessments = self.assess_indicators(all_indicators.clone()).await?;
        let threats_count = assessments.iter().filter(|a| a.is_threat).count();

        let end_time = chrono::Utc::now();
        let duration_ms = (end_time.timestamp_millis() - start_time.timestamp_millis()) as u64;

        Ok(DetectionResult {
            indicators: all_indicators.clone(),
            assessments,
            metadata: DetectionMetadata {
                started_at: start_time,
                completed_at: end_time,
                duration_ms,
                indicators_extracted: all_indicators.len(),
                indicators_assessed: all_indicators.len(),
                threats_identified: threats_count,
                errors: Vec::new(),
                warnings: Vec::new(),
            },
        })
    }

    /// 入力から指標を抽出
    async fn extract_indicators(&self, input: &str) -> Result<Vec<ThreatIndicator>, ThreatError> {
        let rules = self.detection_rules.read().await;
        let config = self.config.read().await;
        let mut indicators = Vec::new();

        // IP アドレスを抽出
        if config.ip_detection_enabled {
            for cap in rules.ip_regex.find_iter(input) {
                let ip_str = cap.as_str();

                // 除外パターンをチェック
                if !self.should_exclude(ip_str, &rules) {
                    if let Ok(ip) = ip_str.parse::<IpAddr>() {
                        indicators.push(ThreatIndicator {
                            indicator_type: IndicatorType::IpAddress,
                            value: ip.to_string(),
                            pattern: None,
                            tags: vec!["auto-extracted".to_string()],
                            context: Some(format!(
                                "Extracted from input at position {}",
                                cap.start()
                            )),
                            first_seen: chrono::Utc::now(),
                        });
                    }
                }
            }
        }

        // URL を抽出
        if config.url_detection_enabled {
            for cap in rules.url_regex.find_iter(input) {
                let url_str = cap.as_str();

                if !self.should_exclude(url_str, &rules) && Url::parse(url_str).is_ok() {
                    indicators.push(ThreatIndicator {
                        indicator_type: IndicatorType::Url,
                        value: url_str.to_string(),
                        pattern: None,
                        tags: vec!["auto-extracted".to_string()],
                        context: Some(format!("Extracted from input at position {}", cap.start())),
                        first_seen: chrono::Utc::now(),
                    });
                }
            }
        }

        // ファイルハッシュを抽出
        if config.file_hash_detection_enabled {
            // MD5
            for cap in rules.md5_regex.find_iter(input) {
                indicators.push(ThreatIndicator {
                    indicator_type: IndicatorType::FileHash,
                    value: cap.as_str().to_lowercase(),
                    pattern: None,
                    tags: vec!["auto-extracted".to_string(), "md5".to_string()],
                    context: Some(format!("MD5 hash extracted at position {}", cap.start())),
                    first_seen: chrono::Utc::now(),
                });
            }

            // SHA1
            for cap in rules.sha1_regex.find_iter(input) {
                indicators.push(ThreatIndicator {
                    indicator_type: IndicatorType::FileHash,
                    value: cap.as_str().to_lowercase(),
                    pattern: None,
                    tags: vec!["auto-extracted".to_string(), "sha1".to_string()],
                    context: Some(format!("SHA1 hash extracted at position {}", cap.start())),
                    first_seen: chrono::Utc::now(),
                });
            }

            // SHA256
            for cap in rules.sha256_regex.find_iter(input) {
                indicators.push(ThreatIndicator {
                    indicator_type: IndicatorType::FileHash,
                    value: cap.as_str().to_lowercase(),
                    pattern: None,
                    tags: vec!["auto-extracted".to_string(), "sha256".to_string()],
                    context: Some(format!("SHA256 hash extracted at position {}", cap.start())),
                    first_seen: chrono::Utc::now(),
                });
            }
        }

        // メールアドレスを抽出
        if config.email_detection_enabled {
            for cap in rules.email_regex.find_iter(input) {
                let email_str = cap.as_str();

                if !self.should_exclude(email_str, &rules) {
                    indicators.push(ThreatIndicator {
                        indicator_type: IndicatorType::Email,
                        value: email_str.to_lowercase(),
                        pattern: None,
                        tags: vec!["auto-extracted".to_string()],
                        context: Some(format!("Email extracted at position {}", cap.start())),
                        first_seen: chrono::Utc::now(),
                    });
                }
            }
        }

        // 重複を除去
        indicators.sort_by(|a, b| {
            a.indicator_type
                .cmp(&b.indicator_type)
                .then(a.value.cmp(&b.value))
        });
        indicators.dedup_by(|a, b| a.indicator_type == b.indicator_type && a.value == b.value);

        debug!("Extracted {} unique indicators", indicators.len());
        Ok(indicators)
    }

    /// 指標を評価
    async fn assess_indicators(
        &self,
        indicators: Vec<ThreatIndicator>,
    ) -> Result<Vec<ThreatAssessment>, ThreatError> {
        if indicators.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Assessing {} indicators", indicators.len());

        // バッチ評価を実行
        match self.threat_manager.batch_check_threats(indicators).await {
            Ok(assessments) => {
                info!(
                    "Successfully assessed {} indicators, {} identified as threats",
                    assessments.len(),
                    assessments.iter().filter(|a| a.is_threat).count()
                );
                Ok(assessments)
            }
            Err(e) => {
                error!("Batch threat assessment failed: {}", e);
                Err(e)
            }
        }
    }

    /// 除外すべき指標かどうかを判定
    fn should_exclude(&self, value: &str, rules: &DetectionRules) -> bool {
        for pattern in &rules.exclusion_patterns {
            if pattern.is_match(value) {
                return true;
            }
        }
        false
    }

    /// 検出統計を更新
    async fn update_detection_stats(
        &self,
        indicators: &[ThreatIndicator],
        assessments: &[ThreatAssessment],
        duration_ms: u64,
    ) {
        let mut stats = self.stats.write().await;

        stats.total_detections += 1;
        stats.indicators_extracted += indicators.len() as u64;
        stats.threats_identified += assessments.iter().filter(|a| a.is_threat).count() as u64;

        // 平均検出時間を更新
        let total_time =
            stats.avg_detection_time_ms * (stats.total_detections - 1) as f64 + duration_ms as f64;
        stats.avg_detection_time_ms = total_time / stats.total_detections as f64;

        // 指標タイプ別統計を更新
        for assessment in assessments {
            let indicator_type = assessment.indicator.indicator_type;
            let type_stats = stats
                .indicator_type_stats
                .entry(indicator_type)
                .or_insert_with(TypeStats::default);

            type_stats.detected += 1;
            if assessment.is_threat {
                type_stats.threats += 1;
            }

            // 平均信頼度を更新
            let total_confidence = type_stats.avg_confidence * (type_stats.detected - 1) as f64
                + assessment.confidence_score;
            type_stats.avg_confidence = total_confidence / type_stats.detected as f64;
        }

        if !assessments.is_empty() {
            stats.successful_detections += 1;
        }
    }

    /// 設定を更新
    pub async fn update_config(&self, new_config: DetectionConfig) -> Result<(), ThreatError> {
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Detection engine configuration updated");
        Ok(())
    }

    /// 統計を取得
    pub async fn get_stats(&self) -> DetectionStats {
        self.stats.read().await.clone()
    }

    /// 検出ルールを更新（高度な使用例）
    pub async fn update_rules(&self, new_rules: DetectionRules) -> Result<(), ThreatError> {
        let mut rules = self.detection_rules.write().await;
        *rules = new_rules;
        info!("Detection rules updated");
        Ok(())
    }
}

impl Clone for ThreatDetectionEngine {
    fn clone(&self) -> Self {
        Self {
            threat_manager: self.threat_manager.clone(),
            detection_rules: self.detection_rules.clone(),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}
