//! XSS（Cross-Site Scripting）攻撃対策システム
//!
//! このモジュールは包括的なXSS攻撃防御機能を提供します：
//! - 14種類のXSS攻撃パターン検出
//! - HTML/JavaScript サニタイゼーション
//! - コンテキスト別出力エンコーディング
//! - CSP（Content Security Policy）ヘッダー生成
//! - DOM-based XSS検出
//! - Mutation XSS（mXSS）対策

use crate::error::SecurityError;
use ammonia::{clean, clean_text, Builder};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// XSS攻撃の種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum XssAttackType {
    /// Reflected XSS - 入力データが即座に出力される攻撃
    Reflected,
    /// Stored XSS - 永続化されたスクリプト攻撃
    Stored,
    /// DOM-based XSS - クライアントサイドDOM操作攻撃
    DomBased,
    /// Event-based XSS - HTMLイベントハンドラーを悪用した攻撃
    EventBased,
    /// Attribute-based XSS - HTML属性を悪用した攻撃
    AttributeBased,
    /// CSS-based XSS - CSSを悪用した攻撃
    CssBased,
    /// SVG-based XSS - SVG要素を悪用した攻撃
    SvgBased,
    /// Data URI XSS - データURIを悪用した攻撃
    DataUri,
    /// JavaScript Protocol XSS - javascript:プロトコル悪用攻撃
    JavascriptProtocol,
    /// Mutation XSS (mXSS) - HTML解析過程での変異攻撃
    Mutation,
    /// Filter Bypass XSS - フィルター回避攻撃
    FilterBypass,
    /// Template Injection XSS - テンプレートインジェクション攻撃
    TemplateInjection,
    /// Unicode XSS - Unicode文字を悪用した攻撃
    Unicode,
    /// Polyglot XSS - 複数形式攻撃
    Polyglot,
}

/// XSS脅威レベル
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum XssThreatLevel {
    /// 情報 - 軽微な検出
    Info = 1,
    /// 低 - 低リスク攻撃
    Low = 2,
    /// 中 - 中リスク攻撃
    Medium = 3,
    /// 高 - 高リスク攻撃
    High = 4,
    /// 緊急 - 緊急対応必要
    Critical = 5,
}

/// XSS攻撃検出パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XssPattern {
    /// パターン名
    pub name: String,
    /// 攻撃タイプ
    pub attack_type: XssAttackType,
    /// 検出用正規表現パターン
    pub pattern: String,
    /// 脅威レベル
    pub threat_level: XssThreatLevel,
    /// 攻撃の説明
    pub description: String,
    /// 検出回数
    pub detection_count: u64,
}

/// XSS保護設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XssProtectionConfig {
    /// 検出モードの有効化
    pub detection_enabled: bool,
    /// サニタイゼーションの有効化
    pub sanitization_enabled: bool,
    /// CSPヘッダーの有効化
    pub csp_enabled: bool,
    /// 厳格モード（すべての検出で拒否）
    pub strict_mode: bool,
    /// 許可するHTMLタグ
    pub allowed_tags: HashSet<String>,
    /// 許可するHTML属性
    pub allowed_attributes: HashMap<String, HashSet<String>>,
    /// ログ記録の有効化
    pub logging_enabled: bool,
    /// 最大入力長
    pub max_input_length: usize,
}

/// XSS検出結果
#[derive(Debug, Clone)]
pub struct XssDetectionResult {
    /// 攻撃が検出されたか
    pub is_attack_detected: bool,
    /// 検出された攻撃タイプ
    pub detected_attacks: Vec<XssAttackType>,
    /// 最高脅威レベル
    pub max_threat_level: XssThreatLevel,
    /// サニタイズ済みコンテンツ
    pub sanitized_content: String,
    /// 検出されたパターン詳細
    pub detection_details: Vec<String>,
    /// 検出時刻
    pub detection_time: Instant,
    /// 処理時間（マイクロ秒）
    pub processing_time_us: u64,
}

/// XSS統計情報
#[derive(Debug, Clone)]
pub struct XssStatistics {
    /// 総検査回数
    pub total_scans: u64,
    /// 攻撃検出回数
    pub attacks_detected: u64,
    /// 攻撃タイプ別統計
    pub attacks_by_type: HashMap<XssAttackType, u64>,
    /// 脅威レベル別統計
    pub threats_by_level: HashMap<XssThreatLevel, u64>,
    /// 平均処理時間（マイクロ秒）
    pub avg_processing_time_us: f64,
    /// 最後のリセット時刻
    pub last_reset: Instant,
}

/// XSS保護システム
#[derive(Debug)]
pub struct XssProtector {
    /// 設定
    config: XssProtectionConfig,
    /// 攻撃パターン
    patterns: Vec<XssPattern>,
    /// 統計情報
    statistics: XssStatistics,
    /// コンパイル済み正規表現キャッシュ
    regex_cache: HashMap<String, Arc<Regex>>,
}

impl Default for XssProtectionConfig {
    fn default() -> Self {
        let mut allowed_tags = HashSet::new();
        allowed_tags.extend(
            [
                "p",
                "br",
                "strong",
                "em",
                "u",
                "i",
                "b",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
                "ul",
                "ol",
                "li",
                "blockquote",
                "code",
                "pre",
            ]
            .iter()
            .map(|s| s.to_string()),
        );

        let mut allowed_attributes = HashMap::new();
        let mut a_attrs = HashSet::new();
        a_attrs.insert("href".to_string());
        allowed_attributes.insert("a".to_string(), a_attrs);

        Self {
            detection_enabled: true,
            sanitization_enabled: true,
            csp_enabled: true,
            strict_mode: false,
            allowed_tags,
            allowed_attributes,
            logging_enabled: true,
            max_input_length: 1_000_000, // 1MB
        }
    }
}

impl XssProtector {
    /// 新しいXSS保護システムを作成
    pub fn new(config: XssProtectionConfig) -> Result<Self, SecurityError> {
        let patterns = Self::initialize_patterns();
        let statistics = XssStatistics {
            total_scans: 0,
            attacks_detected: 0,
            attacks_by_type: HashMap::new(),
            threats_by_level: HashMap::new(),
            avg_processing_time_us: 0.0,
            last_reset: Instant::now(),
        };

        Ok(Self {
            config,
            patterns,
            statistics,
            regex_cache: HashMap::new(),
        })
    }

    /// デフォルト設定でXSS保護システムを作成
    pub fn with_defaults() -> Result<Self, SecurityError> {
        Self::new(XssProtectionConfig::default())
    }

    /// 入力を検査してXSS攻撃を検出
    pub fn scan_input(&mut self, input: &str) -> Result<XssDetectionResult, SecurityError> {
        let start_time = Instant::now();

        // 入力長チェック
        if input.len() > self.config.max_input_length {
            return Err(SecurityError::ValidationError(format!(
                "Input too long: {} bytes (max: {})",
                input.len(),
                self.config.max_input_length
            )));
        }

        self.statistics.total_scans += 1;

        let mut detected_attacks = Vec::new();
        let mut max_threat_level = XssThreatLevel::Info;
        let mut detection_details = Vec::new();

        // パターンマッチング検出
        if self.config.detection_enabled {
            // パターンを先に収集
            let patterns_copy = self.patterns.clone();

            for pattern in &patterns_copy {
                if let Ok(regex) = self.get_compiled_regex(&pattern.pattern) {
                    if let Ok(is_match) = regex.is_match(input) {
                        if is_match {
                            detected_attacks.push(pattern.attack_type.clone());
                            if pattern.threat_level > max_threat_level {
                                max_threat_level = pattern.threat_level.clone();
                            }
                            detection_details.push(format!(
                                "{} ({:?}): {}",
                                pattern.name, pattern.threat_level, pattern.description
                            ));

                            // 統計更新
                            *self
                                .statistics
                                .attacks_by_type
                                .entry(pattern.attack_type.clone())
                                .or_insert(0) += 1;
                            *self
                                .statistics
                                .threats_by_level
                                .entry(pattern.threat_level.clone())
                                .or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        let is_attack_detected = !detected_attacks.is_empty();
        if is_attack_detected {
            self.statistics.attacks_detected += 1;
        }

        // サニタイゼーション実行
        let sanitized_content = if self.config.sanitization_enabled {
            self.sanitize_with_config(input)
        } else {
            input.to_string()
        };

        let processing_time = start_time.elapsed();
        let processing_time_us = processing_time.as_micros() as u64;

        // 平均処理時間更新
        self.statistics.avg_processing_time_us = (self.statistics.avg_processing_time_us
            * (self.statistics.total_scans - 1) as f64
            + processing_time_us as f64)
            / self.statistics.total_scans as f64;

        Ok(XssDetectionResult {
            is_attack_detected,
            detected_attacks,
            max_threat_level,
            sanitized_content,
            detection_details,
            detection_time: start_time,
            processing_time_us,
        })
    }

    /// HTML コンテンツを安全にサニタイズ
    pub fn sanitize_html(&self, html: &str) -> String {
        self.sanitize_with_config(html)
    }

    /// プレーンテキストを安全にエスケープ
    pub fn escape_text(&self, text: &str) -> String {
        clean_text(text)
    }

    /// 設定に基づいてサニタイズ
    fn sanitize_with_config(&self, input: &str) -> String {
        let mut builder = Builder::new();

        let allowed_tags_refs: HashSet<&str> = self
            .config
            .allowed_tags
            .iter()
            .map(|s| s.as_str())
            .collect();
        builder
            .tags(allowed_tags_refs)
            .clean_content_tags(HashSet::new())
            .strip_comments(true)
            .link_rel(Some("noopener noreferrer"));

        // 属性設定
        if !self.config.allowed_attributes.is_empty() {
            let mut tag_attributes = HashMap::new();
            for (tag, attrs) in &self.config.allowed_attributes {
                let attrs_refs: HashSet<&str> = attrs.iter().map(|s| s.as_str()).collect();
                tag_attributes.insert(tag.as_str(), attrs_refs);
            }
            builder.tag_attributes(tag_attributes);
        }

        builder.clean(input).to_string()
    }

    /// コンテキスト別エンコーディング
    pub fn encode_for_context(&self, input: &str, context: &str) -> Result<String, SecurityError> {
        match context.to_lowercase().as_str() {
            "html" => Ok(self.sanitize_html(input)),
            "text" => Ok(self.escape_text(input)),
            "js" | "javascript" => Ok(self.encode_for_javascript(input)),
            "css" => Ok(self.encode_for_css(input)),
            "url" => Ok(self.encode_for_url(input)),
            "attribute" => Ok(self.encode_for_attribute(input)),
            _ => Err(SecurityError::ValidationError(format!(
                "Unknown encoding context: {}",
                context
            ))),
        }
    }

    /// JavaScript用エンコーディング
    fn encode_for_javascript(&self, input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                '<' => "\\u003C".to_string(),
                '>' => "\\u003E".to_string(),
                '"' => "\\u0022".to_string(),
                '\'' => "\\u0027".to_string(),
                '&' => "\\u0026".to_string(),
                '\\' => "\\\\".to_string(),
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                c if c.is_control() => format!("\\u{:04X}", c as u32),
                c => c.to_string(),
            })
            .collect()
    }

    /// CSS用エンコーディング
    fn encode_for_css(&self, input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                '<' | '>' | '"' | '\'' | '&' | '\\' | '(' | ')' | ';' | ':' => {
                    format!("\\{:06X}", c as u32)
                }
                c if c.is_control() => format!("\\{:06X}", c as u32),
                c => c.to_string(),
            })
            .collect()
    }

    /// URL用エンコーディング
    fn encode_for_url(&self, input: &str) -> String {
        url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
    }

    /// HTML属性用エンコーディング
    fn encode_for_attribute(&self, input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '"' => "&quot;".to_string(),
                '\'' => "&#x27;".to_string(),
                '&' => "&amp;".to_string(),
                c => c.to_string(),
            })
            .collect()
    }

    /// CSP（Content Security Policy）ヘッダーを生成
    pub fn generate_csp_header(&self) -> String {
        let directives = [
            "default-src 'self'",
            "script-src 'self' 'unsafe-inline'",
            "style-src 'self' 'unsafe-inline'",
            "img-src 'self' data:",
            "font-src 'self'",
            "connect-src 'self'",
            "media-src 'self'",
            "object-src 'none'",
            "frame-src 'none'",
            "base-uri 'self'",
            "form-action 'self'",
            "upgrade-insecure-requests",
        ];

        directives.join("; ")
    }

    /// 統計情報を取得
    pub fn get_statistics(&self) -> &XssStatistics {
        &self.statistics
    }

    /// 統計情報をリセット
    pub fn reset_statistics(&mut self) {
        self.statistics = XssStatistics {
            total_scans: 0,
            attacks_detected: 0,
            attacks_by_type: HashMap::new(),
            threats_by_level: HashMap::new(),
            avg_processing_time_us: 0.0,
            last_reset: Instant::now(),
        };
    }

    /// 正規表現をコンパイルしてキャッシュ
    fn get_compiled_regex(&mut self, pattern: &str) -> Result<Arc<Regex>, SecurityError> {
        if let Some(regex) = self.regex_cache.get(pattern) {
            Ok(Arc::clone(regex))
        } else {
            let regex = Regex::new(pattern).map_err(|e| {
                SecurityError::ValidationError(format!(
                    "Invalid regex pattern '{}': {}",
                    pattern, e
                ))
            })?;
            let arc_regex = Arc::new(regex);
            self.regex_cache
                .insert(pattern.to_string(), Arc::clone(&arc_regex));
            Ok(arc_regex)
        }
    }

    /// XSS攻撃パターンを初期化
    fn initialize_patterns() -> Vec<XssPattern> {
        vec![
            // 1. Basic Script Tags
            XssPattern {
                name: "Basic Script Tag".to_string(),
                attack_type: XssAttackType::Reflected,
                pattern: r"(?i)<\s*script[^>]*>".to_string(),
                threat_level: XssThreatLevel::High,
                description: "Basic script tag injection".to_string(),
                detection_count: 0,
            },
            // 2. Event Handlers
            XssPattern {
                name: "Event Handler Injection".to_string(),
                attack_type: XssAttackType::EventBased,
                pattern: r#"(?i)on\w+\s*=\s*["']?[^"']*(?:javascript|script|alert|confirm|prompt)"#
                    .to_string(),
                threat_level: XssThreatLevel::High,
                description: "HTML event handler with JavaScript".to_string(),
                detection_count: 0,
            },
            // 3. JavaScript Protocol
            XssPattern {
                name: "JavaScript Protocol".to_string(),
                attack_type: XssAttackType::JavascriptProtocol,
                pattern: r"(?i)javascript\s*:".to_string(),
                threat_level: XssThreatLevel::Medium,
                description: "JavaScript protocol in URLs".to_string(),
                detection_count: 0,
            },
            // 4. Data URI with Script
            XssPattern {
                name: "Data URI Script".to_string(),
                attack_type: XssAttackType::DataUri,
                pattern: r"(?i)data\s*:\s*[^;]*;[^,]*(?:javascript|script|html)".to_string(),
                threat_level: XssThreatLevel::High,
                description: "Data URI containing executable content".to_string(),
                detection_count: 0,
            },
            // 5. SVG Script Injection
            XssPattern {
                name: "SVG Script Injection".to_string(),
                attack_type: XssAttackType::SvgBased,
                pattern: r"(?i)<\s*svg[^>]*>.*<\s*script".to_string(),
                threat_level: XssThreatLevel::High,
                description: "Script injection via SVG elements".to_string(),
                detection_count: 0,
            },
            // 6. CSS Expression
            XssPattern {
                name: "CSS Expression".to_string(),
                attack_type: XssAttackType::CssBased,
                pattern: r"(?i)expression\s*\(\s*".to_string(),
                threat_level: XssThreatLevel::Medium,
                description: "CSS expression injection (IE)".to_string(),
                detection_count: 0,
            },
            // 7. Template Injection
            XssPattern {
                name: "Template Injection".to_string(),
                attack_type: XssAttackType::TemplateInjection,
                pattern: r"\{\{.*(?:constructor|prototype|__proto__).*\}\}".to_string(),
                threat_level: XssThreatLevel::High,
                description: "Template engine injection".to_string(),
                detection_count: 0,
            },
            // 8. Unicode Bypass
            XssPattern {
                name: "Unicode Bypass".to_string(),
                attack_type: XssAttackType::Unicode,
                pattern: r"\\u[0-9a-fA-F]{4}".to_string(),
                threat_level: XssThreatLevel::Medium,
                description: "Unicode encoding bypass attempt".to_string(),
                detection_count: 0,
            },
            // 9. HTML Entity Bypass
            XssPattern {
                name: "HTML Entity Bypass".to_string(),
                attack_type: XssAttackType::FilterBypass,
                pattern: r"&#x?[0-9a-fA-F]+;".to_string(),
                threat_level: XssThreatLevel::Low,
                description: "HTML entity encoding bypass".to_string(),
                detection_count: 0,
            },
            // 10. Base64 Encoded Script
            XssPattern {
                name: "Base64 Script".to_string(),
                attack_type: XssAttackType::FilterBypass,
                pattern: r"(?i)(?:atob|btoa)\s*\(|data:.*base64".to_string(),
                threat_level: XssThreatLevel::Medium,
                description: "Base64 encoded script content".to_string(),
                detection_count: 0,
            },
            // 11. DOM-based XSS Patterns
            XssPattern {
                name: "DOM Manipulation".to_string(),
                attack_type: XssAttackType::DomBased,
                pattern: r"(?i)(?:document\.write|innerHTML|outerHTML|insertAdjacentHTML)"
                    .to_string(),
                threat_level: XssThreatLevel::High,
                description: "DOM manipulation methods".to_string(),
                detection_count: 0,
            },
            // 12. Comment-based Injection
            XssPattern {
                name: "Comment Injection".to_string(),
                attack_type: XssAttackType::FilterBypass,
                pattern: r"<!--.*(?:script|javascript|alert).*-->".to_string(),
                threat_level: XssThreatLevel::Medium,
                description: "Script injection via HTML comments".to_string(),
                detection_count: 0,
            },
            // 13. Polyglot XSS
            XssPattern {
                name: "Polyglot XSS".to_string(),
                attack_type: XssAttackType::Polyglot,
                pattern: r#"(?i)jaVasCript:.*(?:|"|').*<.*>.*\("#.to_string(),
                threat_level: XssThreatLevel::Critical,
                description: "Multi-context polyglot XSS".to_string(),
                detection_count: 0,
            },
            // 14. Mutation XSS (mXSS)
            XssPattern {
                name: "Mutation XSS".to_string(),
                attack_type: XssAttackType::Mutation,
                pattern: r#"(?i)<\s*\w+\s+[^>]*\s+\w+\s*=\s*["']?[^"'>]*<"#.to_string(),
                threat_level: XssThreatLevel::Critical,
                description: "HTML mutation during parsing".to_string(),
                detection_count: 0,
            },
        ]
    }
}

// エラー型の拡張
impl From<fancy_regex::Error> for SecurityError {
    fn from(err: fancy_regex::Error) -> Self {
        SecurityError::ValidationError(format!("Regex error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_xss_detection() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        // Basic script tag attack
        let malicious_input = "<script>alert('XSS')</script>";
        let result = protector.scan_input(malicious_input).expect("Scan failed");

        assert!(result.is_attack_detected);
        assert!(result.detected_attacks.contains(&XssAttackType::Reflected));
        assert_eq!(result.max_threat_level, XssThreatLevel::High);
        assert!(!result.sanitized_content.contains("<script>"));
    }

    #[tokio::test]
    async fn test_event_handler_xss() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let malicious_input = r#"<img src="x" onerror="alert('XSS')">"#;
        let result = protector.scan_input(malicious_input).expect("Scan failed");

        assert!(result.is_attack_detected);
        assert!(result.detected_attacks.contains(&XssAttackType::EventBased));
    }

    #[tokio::test]
    async fn test_javascript_protocol() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let malicious_input = r#"<a href="javascript:alert('XSS')">Click</a>"#;
        let result = protector.scan_input(malicious_input).expect("Scan failed");

        assert!(result.is_attack_detected);
        assert!(result
            .detected_attacks
            .contains(&XssAttackType::JavascriptProtocol));
    }

    #[tokio::test]
    async fn test_svg_xss() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let malicious_input = r#"<svg><script>alert('XSS')</script></svg>"#;
        let result = protector.scan_input(malicious_input).expect("Scan failed");

        assert!(result.is_attack_detected);
        assert!(result.detected_attacks.contains(&XssAttackType::SvgBased));
    }

    #[tokio::test]
    async fn test_clean_input() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let clean_input = "<p>This is safe content</p>";
        let result = protector.scan_input(clean_input).expect("Scan failed");

        assert!(!result.is_attack_detected);
        assert!(result.detected_attacks.is_empty());
        assert_eq!(result.max_threat_level, XssThreatLevel::Info);
    }

    #[test]
    fn test_context_encoding() {
        let protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let input = r#"<script>alert("XSS")</script>"#;

        // HTML context
        let html_encoded = protector
            .encode_for_context(input, "html")
            .expect("HTML encoding failed");
        assert!(!html_encoded.contains("<script>"));

        // JavaScript context
        let js_encoded = protector
            .encode_for_context(input, "javascript")
            .expect("JS encoding failed");
        assert!(js_encoded.contains("\\u003C"));

        // CSS context
        let css_encoded = protector
            .encode_for_context(input, "css")
            .expect("CSS encoding failed");
        assert!(css_encoded.contains("\\"));
    }

    #[test]
    fn test_csp_header_generation() {
        let protector = XssProtector::with_defaults().expect("Failed to create XSS protector");
        let csp = protector.generate_csp_header();

        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("object-src 'none'"));
    }

    #[tokio::test]
    async fn test_statistics() {
        let mut protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        // Test multiple attacks
        let attacks = vec![
            "<script>alert('1')</script>",
            r#"<img onerror="alert('2')">"#,
            "javascript:alert('3')",
        ];

        for attack in attacks {
            protector.scan_input(attack).expect("Scan failed");
        }

        let stats = protector.get_statistics();
        assert_eq!(stats.total_scans, 3);
        assert_eq!(stats.attacks_detected, 3);
        assert!(stats.avg_processing_time_us > 0.0);
    }

    #[test]
    fn test_sanitization() {
        let protector = XssProtector::with_defaults().expect("Failed to create XSS protector");

        let dirty_html =
            r#"<p>Good content</p><script>alert('bad')</script><strong>More good</strong>"#;
        let clean_html = protector.sanitize_html(dirty_html);

        assert!(clean_html.contains("<p>Good content</p>"));
        assert!(clean_html.contains("<strong>More good</strong>"));
        assert!(!clean_html.contains("<script>"));
    }
}
