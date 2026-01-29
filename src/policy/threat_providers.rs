//! 外部脅威インテリジェンスプロバイダー統合
//!
//! AbuseIPDB, CVE Database, MITRE ATT&CKなどの外部APIクライアント

use crate::error::{McpError, Result};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// ==================== AbuseIPDB ====================

/// AbuseIPDBクライアント
#[derive(Debug, Clone)]
pub struct AbuseIpDbClient {
    api_key: String,
    client: reqwest::Client,
    base_url: String,
}

/// AbuseIPDB脅威レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbuseIpDbReport {
    pub ip_address: String,
    pub abuse_confidence_score: u8,
    pub country_code: Option<String>,
    pub usage_type: Option<String>,
    pub isp: Option<String>,
    pub domain: Option<String>,
    pub total_reports: u32,
    pub last_reported_at: Option<SystemTime>,
    pub is_whitelisted: bool,
    pub is_tor: bool,
}

impl AbuseIpDbClient {
    /// 新しいAbuseIPDBクライアントを作成
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
            base_url: "https://api.abuseipdb.com/api/v2".to_string(),
        }
    }

    /// IPアドレスの脅威情報を取得
    ///
    /// # 引数
    /// * `ip` - 調査対象のIPアドレス
    /// * `max_age_days` - 最大何日前までのレポートを含めるか
    pub async fn check_ip(&self, ip: &str, max_age_days: u32) -> Result<AbuseIpDbReport> {
        let url = format!("{}/check", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&[
                ("ipAddress", ip),
                ("maxAgeInDays", &max_age_days.to_string()),
                ("verbose", ""),
            ])
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("AbuseIPDB request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(McpError::ExternalApi(format!(
                "AbuseIPDB API error {}: {}",
                status, error_text
            )));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse response: {}", e)))?;

        // データ変換
        let data = &data["data"];
        Ok(AbuseIpDbReport {
            ip_address: data["ipAddress"].as_str().unwrap_or(ip).to_string(),
            abuse_confidence_score: data["abuseConfidenceScore"].as_u64().unwrap_or(0) as u8,
            country_code: data["countryCode"].as_str().map(String::from),
            usage_type: data["usageType"].as_str().map(String::from),
            isp: data["isp"].as_str().map(String::from),
            domain: data["domain"].as_str().map(String::from),
            total_reports: data["totalReports"].as_u64().unwrap_or(0) as u32,
            last_reported_at: data["lastReportedAt"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
                }),
            is_whitelisted: data["isWhitelisted"].as_bool().unwrap_or(false),
            is_tor: data["isTor"].as_bool().unwrap_or(false),
        })
    }

    /// 複数のIPアドレスをバルクチェック
    pub async fn check_bulk(
        &self,
        ips: &[String],
        max_age_days: u32,
    ) -> Result<Vec<AbuseIpDbReport>> {
        let mut results = Vec::new();

        for ip in ips {
            match self.check_ip(ip, max_age_days).await {
                Ok(report) => results.push(report),
                Err(e) => {
                    tracing::warn!("Failed to check IP {}: {}", ip, e);
                    continue;
                }
            }
            // レート制限対策: 1秒待機
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        Ok(results)
    }
}

// ==================== CVE Database ====================

/// CVE Databaseクライアント
#[derive(Debug, Clone)]
pub struct CveDbClient {
    client: reqwest::Client,
    base_url: String,
    nvd_api_key: Option<String>,
}

/// CVE脆弱性レポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveReport {
    pub cve_id: String,
    pub description: String,
    pub severity: CveSeverity,
    pub cvss_score: f32,
    pub published_date: Option<SystemTime>,
    pub last_modified: Option<SystemTime>,
    pub affected_products: Vec<String>,
    pub references: Vec<String>,
    pub cwe_ids: Vec<String>,
}

/// CVE深刻度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CveSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl CveDbClient {
    /// 新しいCVE Databaseクライアントを作成
    pub fn new(nvd_api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://cve.circl.lu/api".to_string(),
            nvd_api_key,
        }
    }

    /// CVE IDから脆弱性情報を取得
    pub async fn fetch_cve(&self, cve_id: &str) -> Result<CveReport> {
        let url = format!("{}/cve/{}", self.base_url, cve_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("CVE DB request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!("CVE not found: {}", cve_id)));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse response: {}", e)))?;

        // CVSS score取得
        let cvss_score = data["cvss"].as_f64().unwrap_or(0.0) as f32;
        let severity = Self::score_to_severity(cvss_score);

        Ok(CveReport {
            cve_id: cve_id.to_string(),
            description: data["summary"].as_str().unwrap_or("").to_string(),
            severity,
            cvss_score,
            published_date: data["Published"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
                }),
            last_modified: data["Modified"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
                }),
            affected_products: data["vulnerable_product"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            references: data["references"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            cwe_ids: data["cwe"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// CVSS scoreから深刻度を判定
    fn score_to_severity(score: f32) -> CveSeverity {
        match score {
            s if s >= 9.0 => CveSeverity::Critical,
            s if s >= 7.0 => CveSeverity::High,
            s if s >= 4.0 => CveSeverity::Medium,
            _ => CveSeverity::Low,
        }
    }

    /// 最新のCVEを取得
    pub async fn fetch_recent_cves(&self, limit: usize) -> Result<Vec<CveReport>> {
        let url = format!("{}/last", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("CVE DB request failed: {}", e)))?;

        let cve_ids: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse response: {}", e)))?;

        let mut results = Vec::new();
        for (i, cve_data) in cve_ids.iter().take(limit).enumerate() {
            if let Some(cve_id) = cve_data.as_str() {
                match self.fetch_cve(cve_id).await {
                    Ok(report) => results.push(report),
                    Err(e) => {
                        tracing::warn!("Failed to fetch CVE {}: {}", cve_id, e);
                        continue;
                    }
                }

                // レート制限対策
                if (i + 1) % 5 == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }

        Ok(results)
    }
}

// ==================== MITRE ATT&CK ====================

/// MITRE ATT&CKクライアント
#[derive(Debug, Clone)]
pub struct MitreAttackClient {
    client: reqwest::Client,
    base_url: String,
    framework_version: String,
}

/// MITRE ATT&CK攻撃パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tactics: Vec<String>,
    pub techniques: Vec<String>,
    pub mitigation: Vec<String>,
    pub detection: Vec<String>,
}

impl MitreAttackClient {
    /// 新しいMITRE ATT&CKクライアントを作成
    pub fn new(framework_version: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://raw.githubusercontent.com/mitre/cti/master".to_string(),
            framework_version,
        }
    }

    /// 攻撃テクニックIDから情報を取得
    pub async fn fetch_technique(&self, technique_id: &str) -> Result<AttackPattern> {
        // 簡略化実装: 実際にはSTIXフォーマットのJSONをパース
        let url = format!("{}/enterprise-attack/enterprise-attack.json", self.base_url);

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                McpError::ExternalApi(format!("MITRE ATT&CK request failed: {}", e))
            })?;

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse response: {}", e)))?;

        // STIXオブジェクトから該当テクニックを検索
        if let Some(objects) = data["objects"].as_array() {
            for obj in objects {
                if obj["type"] == "attack-pattern" {
                    if let Some(external_refs) = obj["external_references"].as_array() {
                        for ext_ref in external_refs {
                            if ext_ref["external_id"].as_str() == Some(technique_id) {
                                return Ok(AttackPattern {
                                    id: technique_id.to_string(),
                                    name: obj["name"].as_str().unwrap_or("").to_string(),
                                    description: obj["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    tactics: obj["kill_chain_phases"]
                                        .as_array()
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| {
                                                    v["phase_name"].as_str().map(String::from)
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    techniques: vec![technique_id.to_string()],
                                    mitigation: Vec::new(), // 別途取得が必要
                                    detection: Vec::new(),  // 別途取得が必要
                                });
                            }
                        }
                    }
                }
            }
        }

        Err(McpError::ExternalApi(format!(
            "Technique not found: {}",
            technique_id
        )))
    }

    /// すべての攻撃テクニックを取得
    pub async fn fetch_all_techniques(&self) -> Result<Vec<AttackPattern>> {
        let url = format!("{}/enterprise-attack/enterprise-attack.json", self.base_url);

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                McpError::ExternalApi(format!("MITRE ATT&CK request failed: {}", e))
            })?;

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse response: {}", e)))?;

        let mut patterns = Vec::new();

        if let Some(objects) = data["objects"].as_array() {
            for obj in objects {
                if obj["type"] == "attack-pattern" {
                    if let Some(external_refs) = obj["external_references"].as_array() {
                        for ext_ref in external_refs {
                            if let Some(technique_id) = ext_ref["external_id"].as_str() {
                                if technique_id.starts_with('T') {
                                    patterns.push(AttackPattern {
                                        id: technique_id.to_string(),
                                        name: obj["name"].as_str().unwrap_or("").to_string(),
                                        description: obj["description"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        tactics: obj["kill_chain_phases"]
                                            .as_array()
                                            .map(|arr| {
                                                arr.iter()
                                                    .filter_map(|v| {
                                                        v["phase_name"].as_str().map(String::from)
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default(),
                                        techniques: vec![technique_id.to_string()],
                                        mitigation: Vec::new(),
                                        detection: Vec::new(),
                                    });
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(patterns)
    }
}
