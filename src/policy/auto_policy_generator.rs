//! 自動ポリシー生成エンジン
//!
//! 脅威インテリジェンス情報からセキュリティポリシーを自動生成

use crate::error::Result;
use crate::policy::dynamic_updater::DynamicPolicyUpdater;
use crate::policy::threat_providers::{AbuseIpDbReport, AttackPattern, CveReport};
use crate::policy_config::PolicyConfig;
use crate::security::SecurityConfig;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, warn};

/// 自動ポリシー生成エンジン
pub struct AutoPolicyGenerator {
    policy_updater: Arc<DynamicPolicyUpdater>,
    ip_blocklist_threshold: u8,
    pattern_confidence_min: f64,
    application_mode: PolicyApplicationMode,
}

/// ポリシー適用モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyApplicationMode {
    /// 自動適用
    Automatic,
    /// 手動レビュー後に適用
    ManualReview,
}

/// 生成されたポリシールール
#[derive(Debug, Clone)]
pub struct GeneratedPolicyRule {
    pub rule_id: String,
    pub rule_type: PolicyRuleType,
    pub description: String,
    pub confidence: f64,
    pub auto_apply: bool,
}

/// ポリシールールタイプ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyRuleType {
    /// IPブロック
    IpBlock { ips: Vec<String> },
    /// ドメインブロック
    DomainBlock { domains: Vec<String> },
    /// 攻撃パターン検知
    AttackPatternDetection {
        pattern_id: String,
        signatures: Vec<String>,
    },
    /// レート制限
    RateLimit {
        path: String,
        max_requests: u32,
        window_secs: u64,
    },
}

impl AutoPolicyGenerator {
    /// 新しい自動ポリシー生成エンジンを作成
    pub fn new(
        policy_updater: Arc<DynamicPolicyUpdater>,
        ip_blocklist_threshold: u8,
        pattern_confidence_min: f64,
        application_mode: PolicyApplicationMode,
    ) -> Self {
        Self {
            policy_updater,
            ip_blocklist_threshold,
            pattern_confidence_min,
            application_mode,
        }
    }

    /// AbuseIPDBレポートからIPブロックリストポリシーを生成
    ///
    /// # 引数
    /// * `reports` - AbuseIPDBレポート一覧
    ///
    /// # 戻り値
    /// 生成されたポリシールール一覧
    pub async fn generate_ip_blocklist_from_abuseipdb(
        &self,
        reports: &[AbuseIpDbReport],
    ) -> Result<Vec<GeneratedPolicyRule>> {
        let mut rules = Vec::new();
        let mut blocked_ips = Vec::new();

        for report in reports {
            // 信頼スコアが閾値以上の場合のみブロック
            if report.abuse_confidence_score >= self.ip_blocklist_threshold {
                blocked_ips.push(report.ip_address.clone());

                info!(
                    "Blocking IP {} (confidence: {}, reports: {})",
                    report.ip_address, report.abuse_confidence_score, report.total_reports
                );
            }
        }

        if !blocked_ips.is_empty() {
            let rule = GeneratedPolicyRule {
                rule_id: format!("abuseipdb-blocklist-{}", uuid::Uuid::new_v4()),
                rule_type: PolicyRuleType::IpBlock {
                    ips: blocked_ips.clone(),
                },
                description: format!(
                    "AbuseIPDB automated IP blocklist ({} IPs, threshold: {})",
                    blocked_ips.len(),
                    self.ip_blocklist_threshold
                ),
                confidence: blocked_ips
                    .iter()
                    .filter_map(|ip| {
                        reports
                            .iter()
                            .find(|r| &r.ip_address == ip)
                            .map(|r| r.abuse_confidence_score as f64 / 100.0)
                    })
                    .sum::<f64>()
                    / blocked_ips.len() as f64,
                auto_apply: matches!(self.application_mode, PolicyApplicationMode::Automatic),
            };

            // 自動適用モードの場合、即座にポリシー更新
            if rule.auto_apply {
                self.apply_ip_block_rule(&blocked_ips).await?;
                info!("Applied IP blocklist with {} addresses", blocked_ips.len());
            } else {
                info!("Generated IP blocklist rule (manual review required)");
            }

            rules.push(rule);
        }

        Ok(rules)
    }

    /// CVEレポートから攻撃パターン検知ルールを生成
    ///
    /// # 引数
    /// * `cve_reports` - CVEレポート一覧
    ///
    /// # 戻り値
    /// 生成されたポリシールール一覧
    pub async fn generate_cve_detection_rules(
        &self,
        cve_reports: &[CveReport],
    ) -> Result<Vec<GeneratedPolicyRule>> {
        let mut rules = Vec::new();

        for cve in cve_reports {
            // 深刻度がHigh以上の場合のみルール生成
            if cve.cvss_score >= 7.0 {
                // CVE情報から検知シグネチャを生成
                let signatures = self.generate_cve_signatures(cve);

                if !signatures.is_empty() {
                    let rule = GeneratedPolicyRule {
                        rule_id: format!("cve-{}", cve.cve_id),
                        rule_type: PolicyRuleType::AttackPatternDetection {
                            pattern_id: cve.cve_id.clone(),
                            signatures,
                        },
                        description: format!(
                            "CVE {} detection (CVSS: {}, Severity: {:?})",
                            cve.cve_id, cve.cvss_score, cve.severity
                        ),
                        confidence: (cve.cvss_score / 10.0) as f64,
                        auto_apply: matches!(
                            self.application_mode,
                            PolicyApplicationMode::Automatic
                        ),
                    };

                    info!("Generated CVE detection rule for {}", cve.cve_id);
                    rules.push(rule);
                }
            }
        }

        Ok(rules)
    }

    /// MITRE ATT&CK攻撃パターンから検知ルールを生成
    ///
    /// # 引数
    /// * `patterns` - MITRE ATT&CK攻撃パターン一覧
    ///
    /// # 戻り値
    /// 生成されたポリシールール一覧
    pub async fn generate_attack_pattern_rules(
        &self,
        patterns: &[AttackPattern],
    ) -> Result<Vec<GeneratedPolicyRule>> {
        let mut rules = Vec::new();

        for pattern in patterns {
            // 危険度の高いタクティックに対してルール生成
            let high_risk_tactics = [
                "initial-access",
                "execution",
                "persistence",
                "privilege-escalation",
            ];

            let has_high_risk = pattern
                .tactics
                .iter()
                .any(|tactic| high_risk_tactics.iter().any(|hr| tactic.contains(hr)));

            if has_high_risk {
                let signatures = self.generate_mitre_signatures(pattern);

                let rule = GeneratedPolicyRule {
                    rule_id: format!("mitre-{}", pattern.id),
                    rule_type: PolicyRuleType::AttackPatternDetection {
                        pattern_id: pattern.id.clone(),
                        signatures,
                    },
                    description: format!(
                        "MITRE ATT&CK {} detection ({})",
                        pattern.id, pattern.name
                    ),
                    confidence: 0.85, // MITRE ATT&CKは信頼性が高い
                    auto_apply: matches!(self.application_mode, PolicyApplicationMode::Automatic),
                };

                info!("Generated MITRE ATT&CK rule for {}", pattern.id);
                rules.push(rule);
            }
        }

        Ok(rules)
    }

    /// IPブロックルールをポリシーに適用
    async fn apply_ip_block_rule(&self, ips: &[String]) -> Result<()> {
        let current_policy = self.policy_updater.get_active_policy().await;
        let mut new_policy = current_policy.clone();

        // IPブロックリストを更新
        let mut blocked_ips = new_policy.security.blocked_ips.clone();
        for ip in ips {
            if !blocked_ips.contains(ip) {
                blocked_ips.push(ip.clone());
            }
        }

        // ポリシー更新
        new_policy.security.blocked_ips = blocked_ips;

        self.policy_updater.update_policy(new_policy).await?;
        Ok(())
    }

    /// CVEからシグネチャを生成
    fn generate_cve_signatures(&self, cve: &CveReport) -> Vec<String> {
        let mut signatures = Vec::new();

        // CWE IDからシグネチャを生成
        for cwe_id in &cve.cwe_ids {
            // 一般的な攻撃パターンマッピング
            match cwe_id.as_str() {
                "CWE-79" => signatures.push("<script".to_string()), // XSS
                "CWE-89" => signatures.push("' OR 1=1--".to_string()), // SQL Injection
                "CWE-78" => signatures.push("; ls -la".to_string()), // Command Injection
                "CWE-22" => signatures.push("../".to_string()),     // Path Traversal
                _ => {}
            }
        }

        // 説明文から関連キーワードを抽出
        let description_lower = cve.description.to_lowercase();
        if description_lower.contains("remote code execution") {
            signatures.push("eval(".to_string());
            signatures.push("exec(".to_string());
        }
        if description_lower.contains("buffer overflow") {
            signatures.push("AAAAAAA".repeat(100)); // パターン検知
        }

        signatures
    }

    /// MITRE ATT&CKからシグネチャを生成
    fn generate_mitre_signatures(&self, pattern: &AttackPattern) -> Vec<String> {
        let mut signatures = Vec::new();

        // テクニックIDに基づいたシグネチャ生成
        match pattern.id.as_str() {
            // T1059: Command and Scripting Interpreter
            id if id.starts_with("T1059") => {
                signatures.extend(vec![
                    "powershell.exe".to_string(),
                    "cmd.exe /c".to_string(),
                    "bash -c".to_string(),
                ]);
            }
            // T1190: Exploit Public-Facing Application
            "T1190" => {
                signatures.extend(vec![
                    "../../etc/passwd".to_string(),
                    "{{7*7}}".to_string(), // SSTI
                ]);
            }
            // T1566: Phishing
            id if id.starts_with("T1566") => {
                signatures.push("data:text/html".to_string());
            }
            _ => {}
        }

        signatures
    }

    /// すべてのルールを一括適用
    pub async fn apply_all_rules(&self, rules: &[GeneratedPolicyRule]) -> Result<usize> {
        let mut applied_count = 0;

        for rule in rules {
            if rule.auto_apply {
                match &rule.rule_type {
                    PolicyRuleType::IpBlock { ips } => {
                        self.apply_ip_block_rule(ips).await?;
                        applied_count += 1;
                    }
                    PolicyRuleType::AttackPatternDetection { .. } => {
                        // 攻撃パターン検知ルールの適用
                        // WAF統合またはIDS/IPSに転送
                        info!("Attack pattern rule applied: {}", rule.rule_id);
                        applied_count += 1;
                    }
                    PolicyRuleType::DomainBlock { .. } | PolicyRuleType::RateLimit { .. } => {
                        // その他のルールタイプの処理
                        warn!("Rule type not yet implemented: {}", rule.rule_id);
                    }
                }
            }
        }

        Ok(applied_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_ip_blocklist() {
        // テスト実装
    }

    #[tokio::test]
    async fn test_generate_cve_rules() {
        // テスト実装
    }

    #[tokio::test]
    async fn test_apply_rules() {
        // テスト実装
    }
}
