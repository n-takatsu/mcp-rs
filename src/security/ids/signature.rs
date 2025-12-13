//! Signature-Based Detection
//!
//! 既知の攻撃パターンをマッチングして検知します。

use super::{DetectionType, RequestData, Severity};
use crate::error::McpError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// シグネチャ検知器
pub struct SignatureDetector {
    /// 攻撃パターンデータベース
    patterns: Vec<AttackPattern>,
    /// カテゴリ別パターンインデックス
    pattern_index: HashMap<DetectionType, Vec<usize>>,
    /// カスタムルール
    custom_rules: Vec<CustomRule>,
}

/// 攻撃パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    /// パターンID
    pub id: String,
    /// パターン名
    pub name: String,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 正規表現パターン
    pub regex_pattern: String,
    /// コンパイル済み正規表現（シリアライズ不可）
    #[serde(skip)]
    pub compiled_regex: Option<Regex>,
    /// 深刻度
    pub severity: Severity,
    /// 説明
    pub description: String,
    /// CVE ID
    pub cve_ids: Vec<String>,
    /// 有効/無効
    pub enabled: bool,
}

/// カスタムルール
#[derive(Debug, Clone)]
pub struct CustomRule {
    /// ルールID
    pub id: String,
    /// ルール名
    pub name: String,
    /// マッチング関数
    pub matcher: fn(&RequestData) -> bool,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 深刻度
    pub severity: Severity,
}

/// 検知結果
#[derive(Debug, Clone)]
pub struct SignatureDetectionResult {
    /// マッチフラグ
    pub matched: bool,
    /// 信頼度
    pub confidence: f64,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// マッチしたパターン名
    pub pattern_names: Vec<String>,
    /// 深刻度
    pub severity: Severity,
}

impl SignatureDetector {
    /// 新しいシグネチャ検知器を作成
    pub async fn new() -> Result<Self, McpError> {
        let patterns = Self::load_default_patterns()?;
        let pattern_index = Self::build_index(&patterns);

        Ok(Self {
            patterns,
            pattern_index,
            custom_rules: Vec::new(),
        })
    }

    /// デフォルトパターンをロード（50+パターン）
    fn load_default_patterns() -> Result<Vec<AttackPattern>, McpError> {
        let mut patterns = Vec::new();

        // SQL Injection パターン (15パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "SQL-001",
                "UNION SELECT Attack",
                DetectionType::SqlInjection,
                r"(?i)union\s+(all\s+)?select",
                Severity::Critical,
                "UNION-based SQL injection attempt",
                vec!["CVE-2019-0708"],
            )?,
            AttackPattern::new(
                "SQL-002",
                "Boolean-based Blind SQL Injection",
                DetectionType::SqlInjection,
                r"(?i)('|\s+)(and|or)('|\s+)('.+'='.+|\d+\s*=\s*\d+)",
                Severity::High,
                "Boolean-based blind SQL injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-003",
                "Time-based Blind SQL Injection",
                DetectionType::SqlInjection,
                r"(?i)(sleep|benchmark|waitfor)\s*\(",
                Severity::High,
                "Time-based blind SQL injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-004",
                "Comment Injection",
                DetectionType::SqlInjection,
                r"(/\*|\*/|--|#|;%00)",
                Severity::Medium,
                "SQL comment injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-005",
                "Information Schema Access",
                DetectionType::SqlInjection,
                r"(?i)information_schema",
                Severity::High,
                "Access to information_schema",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-006",
                "SQL Stacked Queries",
                DetectionType::SqlInjection,
                r";\s*(drop|delete|update|insert)\s+",
                Severity::Critical,
                "SQL stacked queries attack",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-007",
                "SQL Hex Encoding",
                DetectionType::SqlInjection,
                r"0x[0-9a-fA-F]{2,}",
                Severity::Medium,
                "SQL hex encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-008",
                "SQL Char Function",
                DetectionType::SqlInjection,
                r"(?i)char\s*\(\s*\d+",
                Severity::Medium,
                "SQL CHAR function obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-009",
                "SQL Concat Function",
                DetectionType::SqlInjection,
                r"(?i)concat\s*\(",
                Severity::Low,
                "SQL CONCAT function usage",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-010",
                "SQL Load File",
                DetectionType::SqlInjection,
                r"(?i)load_file\s*\(",
                Severity::Critical,
                "SQL LOAD_FILE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-011",
                "SQL Into Outfile",
                DetectionType::SqlInjection,
                r"(?i)into\s+(out|dump)file",
                Severity::Critical,
                "SQL INTO OUTFILE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-012",
                "SQL Database Names",
                DetectionType::SqlInjection,
                r"(?i)(mysql|mssql|postgres|oracle)\.(sys|user|dbo)",
                Severity::High,
                "SQL database system access",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-013",
                "SQL Execute Command",
                DetectionType::SqlInjection,
                r"(?i)(exec|execute)\s+(sp_|xp_)",
                Severity::Critical,
                "SQL stored procedure execution",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-014",
                "SQL Truncate",
                DetectionType::SqlInjection,
                r"(?i)truncate\s+table",
                Severity::Critical,
                "SQL TRUNCATE TABLE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-015",
                "SQL Alter Table",
                DetectionType::SqlInjection,
                r"(?i)alter\s+table",
                Severity::Critical,
                "SQL ALTER TABLE attempt",
                vec![],
            )?,
        ]);

        // XSS パターン (15パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "XSS-001",
                "Script Tag Injection",
                DetectionType::XssAttack,
                r"<script[^>]*>",
                Severity::High,
                "XSS script tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-002",
                "JavaScript Event Handler",
                DetectionType::XssAttack,
                r"(?i)on(load|error|click|mouseover)\s*=",
                Severity::High,
                "XSS via event handler",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-003",
                "JavaScript Protocol",
                DetectionType::XssAttack,
                r"javascript\s*:",
                Severity::High,
                "XSS via javascript: protocol",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-004",
                "Data URI Scheme",
                DetectionType::XssAttack,
                r"data\s*:\s*text/html",
                Severity::Medium,
                "XSS via data URI",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-005",
                "Iframe Injection",
                DetectionType::XssAttack,
                r"<iframe[^>]*>",
                Severity::High,
                "Iframe injection attack",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-006",
                "Object Tag Injection",
                DetectionType::XssAttack,
                r"<object[^>]*>",
                Severity::Medium,
                "Object tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-007",
                "Embed Tag Injection",
                DetectionType::XssAttack,
                r"<embed[^>]*>",
                Severity::Medium,
                "Embed tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-008",
                "SVG Script Injection",
                DetectionType::XssAttack,
                r"<svg[^>]*>.*<script",
                Severity::High,
                "SVG-based XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-009",
                "Style Expression",
                DetectionType::XssAttack,
                r"(?i)expression\s*\(",
                Severity::Medium,
                "CSS expression XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-010",
                "Import Stylesheet",
                DetectionType::XssAttack,
                r"@import",
                Severity::Low,
                "CSS import injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-011",
                "Base64 Encoded Script",
                DetectionType::XssAttack,
                r"(?i)base64",
                Severity::High,
                "Base64 encoded XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-012",
                "Meta Redirect",
                DetectionType::XssAttack,
                r"<meta[^>]*http-equiv",
                Severity::Medium,
                "Meta redirect attack",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-013",
                "Link Stylesheet Injection",
                DetectionType::XssAttack,
                r"<link[^>]*stylesheet",
                Severity::Low,
                "Link stylesheet injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-014",
                "Vbscript Protocol",
                DetectionType::XssAttack,
                r"vbscript:",
                Severity::High,
                "VBScript protocol XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-015",
                "HTML Entity Obfuscation",
                DetectionType::XssAttack,
                r"&#x?[0-9a-fA-F]{2,};.*<script",
                Severity::Medium,
                "HTML entity obfuscated XSS",
                vec![],
            )?,
        ]);

        // Path Traversal パターン (10パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "PATH-001",
                "Directory Traversal",
                DetectionType::UnauthorizedAccess,
                r"(\.\./|\.\.\\)",
                Severity::High,
                "Directory traversal attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-002",
                "Absolute Path Access",
                DetectionType::UnauthorizedAccess,
                r"(/etc/(passwd|shadow)|C:\\Windows\\System32)",
                Severity::Critical,
                "Absolute path access attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-003",
                "Null Byte Injection",
                DetectionType::UnauthorizedAccess,
                r"%00",
                Severity::High,
                "Null byte injection",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-004",
                "URL Encoded Traversal",
                DetectionType::UnauthorizedAccess,
                r"(%2e%2e|%252e)",
                Severity::High,
                "URL encoded directory traversal",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-005",
                "Double Encoding",
                DetectionType::UnauthorizedAccess,
                r"%25(2e|5c|2f)",
                Severity::Medium,
                "Double URL encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-006",
                "Unicode Encoding",
                DetectionType::UnauthorizedAccess,
                r"(%u002e|%uff0e)",
                Severity::Medium,
                "Unicode encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-007",
                "Windows UNC Path",
                DetectionType::UnauthorizedAccess,
                r"(\\\\[a-zA-Z0-9_.-]+\\)",
                Severity::High,
                "Windows UNC path access",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-008",
                "Config File Access",
                DetectionType::UnauthorizedAccess,
                r"\.(env|config|ini|conf)",
                Severity::High,
                "Configuration file access",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-009",
                "Backup File Access",
                DetectionType::UnauthorizedAccess,
                r"\.(bak|backup|old|~)$",
                Severity::Medium,
                "Backup file access attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-010",
                "Source Code Access",
                DetectionType::UnauthorizedAccess,
                r"\.(php|asp|aspx|jsp|java)\.txt$",
                Severity::Medium,
                "Source code disclosure attempt",
                vec![],
            )?,
        ]);

        // Command Injection パターン (10パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "CMD-001",
                "Shell Command Injection",
                DetectionType::Other,
                r"[;&|$<>]",
                Severity::Critical,
                "Shell command injection",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-002",
                "Backtick Command",
                DetectionType::Other,
                r"`[\w\s]+`",
                Severity::Critical,
                "Backtick command execution",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-003",
                "Dollar Parenthesis",
                DetectionType::Other,
                r"\$\([^)]+\)",
                Severity::Critical,
                "Dollar parenthesis command",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-004",
                "Pipe Command",
                DetectionType::Other,
                r"\|\s*(cat|ls|pwd|whoami|id)",
                Severity::High,
                "Piped command execution",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-005",
                "Semicolon Command",
                DetectionType::Other,
                r";\s*(rm|del|format|kill)",
                Severity::Critical,
                "Semicolon command chaining",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-006",
                "Ampersand Command",
                DetectionType::Other,
                r"&&\s*(wget|curl|nc|netcat)",
                Severity::Critical,
                "Ampersand command chaining",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-007",
                "Redirection Attack",
                DetectionType::Other,
                r">\s*/dev/",
                Severity::Medium,
                "Output redirection",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-008",
                "Eval Function",
                DetectionType::Other,
                r"(?i)eval\s*\(",
                Severity::High,
                "Eval function usage",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-009",
                "System Function",
                DetectionType::Other,
                r"(?i)system\s*\(",
                Severity::Critical,
                "System function call",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-010",
                "Exec Function",
                DetectionType::Other,
                r"(?i)exec\s*\(",
                Severity::Critical,
                "Exec function call",
                vec![],
            )?,
        ]);

        Ok(patterns)
    }

    /// パターンインデックスを構築
    fn build_index(patterns: &[AttackPattern]) -> HashMap<DetectionType, Vec<usize>> {
        let mut index: HashMap<DetectionType, Vec<usize>> = HashMap::new();

        for (i, pattern) in patterns.iter().enumerate() {
            index.entry(pattern.detection_type).or_default().push(i);
        }

        index
    }

    /// リクエストを検知
    pub async fn detect(
        &self,
        request: &RequestData,
    ) -> Result<SignatureDetectionResult, McpError> {
        debug!(
            "Running signature detection on request: {}",
            request.request_id
        );

        let mut matched_patterns = Vec::new();
        let mut max_severity = Severity::Low;

        // リクエスト全体を検査対象文字列に変換
        let check_strings = self.extract_check_strings(request);

        // 全パターンをチェック
        for pattern in &self.patterns {
            if !pattern.enabled {
                continue;
            }

            if let Some(regex) = &pattern.compiled_regex {
                for check_str in &check_strings {
                    if regex.is_match(check_str) {
                        matched_patterns.push(pattern.name.clone());
                        max_severity = max_severity.max(pattern.severity);

                        warn!(
                            "Attack pattern matched: {} ({}), severity: {:?}",
                            pattern.name, pattern.id, pattern.severity
                        );
                        break;
                    }
                }
            }
        }

        // カスタムルールをチェック
        for rule in &self.custom_rules {
            if (rule.matcher)(request) {
                matched_patterns.push(rule.name.clone());
                max_severity = max_severity.max(rule.severity);
            }
        }

        let matched = !matched_patterns.is_empty();
        let confidence = if matched {
            match max_severity {
                Severity::Critical => 0.95,
                Severity::High => 0.85,
                Severity::Medium => 0.70,
                Severity::Low => 0.50,
            }
        } else {
            0.0
        };

        Ok(SignatureDetectionResult {
            matched,
            confidence,
            detection_type: if matched {
                self.patterns
                    .iter()
                    .find(|p| matched_patterns.contains(&p.name))
                    .map(|p| p.detection_type)
                    .unwrap_or(DetectionType::Other)
            } else {
                DetectionType::Other
            },
            pattern_names: matched_patterns,
            severity: max_severity,
        })
    }

    /// カスタムルールを追加
    pub fn add_custom_rule(&mut self, rule: CustomRule) {
        self.custom_rules.push(rule);
    }

    /// 検査対象文字列を抽出
    fn extract_check_strings(&self, request: &RequestData) -> Vec<String> {
        let mut strings = Vec::new();

        // URL パス
        strings.push(request.path.clone());

        // クエリパラメータ
        for (key, value) in &request.query_params {
            strings.push(format!("{}={}", key, value));
            strings.push(value.clone());
        }

        // ヘッダー
        for (key, value) in &request.headers {
            let key_lower = key.to_lowercase();
            if key_lower.contains("cookie")
                || key_lower.contains("referer")
                || key_lower.contains("user")
            {
                strings.push(value.clone());
            }
        }

        // ボディ
        if let Some(body) = &request.body {
            if let Ok(body_str) = String::from_utf8(body.clone()) {
                strings.push(body_str);
            }
        }

        strings
    }
}

impl AttackPattern {
    /// 新しい攻撃パターンを作成
    pub fn new(
        id: &str,
        name: &str,
        detection_type: DetectionType,
        regex_pattern: &str,
        severity: Severity,
        description: &str,
        cve_ids: Vec<&str>,
    ) -> Result<Self, McpError> {
        let compiled_regex = Regex::new(regex_pattern)
            .map_err(|e| McpError::Config(format!("Invalid regex pattern: {}", e)))?;

        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            detection_type,
            regex_pattern: regex_pattern.to_string(),
            compiled_regex: Some(compiled_regex),
            severity,
            description: description.to_string(),
            cve_ids: cve_ids.iter().map(|s| s.to_string()).collect(),
            enabled: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_signature_detector_initialization() {
        let detector = SignatureDetector::new().await;
        assert!(detector.is_ok());

        let detector = detector.unwrap();
        assert!(detector.patterns.len() >= 50);
    }

    #[tokio::test]
    async fn test_sql_injection_detection() {
        let detector = SignatureDetector::new().await.unwrap();

        let mut query_params = HashMap::new();
        query_params.insert("id".to_string(), "1 UNION SELECT FROM users".to_string());

        let request = RequestData {
            request_id: "test-001".to_string(),
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            query_params,
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(result.matched);
        assert_eq!(result.detection_type, DetectionType::SqlInjection);
        assert!(result.confidence > 0.8);
    }

    #[tokio::test]
    async fn test_xss_detection() {
        let detector = SignatureDetector::new().await.unwrap();

        let request = RequestData {
            request_id: "test-002".to_string(),
            method: "POST".to_string(),
            path: "/api/comments".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: Some(b"<script>alert(XSS)</script>".to_vec()),
            source_ip: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(result.matched);
        assert_eq!(result.detection_type, DetectionType::XssAttack);
    }

    #[tokio::test]
    async fn test_benign_request() {
        let detector = SignatureDetector::new().await.unwrap();

        let mut query_params = HashMap::new();
        query_params.insert("category".to_string(), "electronics".to_string());

        let request = RequestData {
            request_id: "test-003".to_string(),
            method: "GET".to_string(),
            path: "/api/products".to_string(),
            query_params,
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(!result.matched);
        assert_eq!(result.confidence, 0.0);
    }
}
