//! 入力検証コアシステム
//!
//! この　モジュールは以下の機能を提供します：
//! - SQLインジェクション攻撃の検出と防止
//! - XSS攻撃の検出と防止  
//! - 入力データの形式検証
//! - データサニタイゼーション
//! - カスタム検証ルールのサポート

use crate::error::SecurityError;
use crate::security::sql_injection_protection::{
    SqlInjectionProtector, SqlProtectionConfig, ThreatLevel,
};
use ammonia::clean;
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use validator::{Validate, ValidationError};

/// 検証ルールの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValidationRuleType {
    /// SQLインジェクション検査
    SqlInjection,
    /// XSS攻撃検査
    XssAttack,
    /// URL形式検査
    UrlFormat,
    /// Email形式検査
    EmailFormat,
    /// HTMLタグ検査
    HtmlTags,
    /// 長さ制限検査
    LengthLimit,
    /// カスタムパターン検査
    CustomPattern,
}

/// 検証ルール定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    /// ルールの種類
    pub rule_type: ValidationRuleType,
    /// ルール名
    pub name: String,
    /// 検証パターン（正規表現）
    pub pattern: Option<String>,
    /// エラーメッセージ
    pub error_message: String,
    /// 最大長（LengthLimit用）
    pub max_length: Option<usize>,
    /// 許可されるHTMLタグ（HtmlTags用）
    pub allowed_tags: Option<Vec<String>>,
    /// 必須フラグ
    pub required: bool,
}

impl ValidationRule {
    /// 新しい検証ルールを作成
    pub fn new(rule_type: ValidationRuleType, name: String) -> Self {
        let (error_message, pattern) = match rule_type {
            ValidationRuleType::SqlInjection => {
                ("SQL injection attempt detected".to_string(), 
                 Some(r"(?i)(union|select|insert|update|delete|drop|create|alter|exec|execute|\-\-|\/\*|\*\/|xp_|sp_)".to_string()))
            },
            ValidationRuleType::XssAttack => {
                ("XSS attack attempt detected".to_string(),
                 Some(r"(?i)(<script|javascript:|onload|onerror|onclick|onmouseover|<iframe|<object|<embed)".to_string()))
            },
            ValidationRuleType::UrlFormat => {
                ("Invalid URL format".to_string(), None)
            },
            ValidationRuleType::EmailFormat => {
                ("Invalid email format".to_string(), None)
            },
            ValidationRuleType::HtmlTags => {
                ("HTML tags not allowed".to_string(),
                 Some(r"<[^>]*>".to_string()))
            },
            ValidationRuleType::LengthLimit => {
                ("Input exceeds maximum length".to_string(), None)
            },
            ValidationRuleType::CustomPattern => {
                ("Invalid input format".to_string(), None)
            },
        };

        Self {
            rule_type,
            name,
            pattern,
            error_message,
            max_length: None,
            allowed_tags: None,
            required: false,
        }
    }

    /// 長さ制限を設定
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// 許可するHTMLタグを設定
    pub fn with_allowed_tags(mut self, tags: Vec<String>) -> Self {
        self.allowed_tags = Some(tags);
        self
    }

    /// カスタムパターンを設定
    pub fn with_pattern(mut self, pattern: String) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// 必須フラグを設定
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// カスタムエラーメッセージを設定
    pub fn with_error_message(mut self, message: String) -> Self {
        self.error_message = message;
        self
    }
}

/// 検証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// 検証が成功したか
    pub is_valid: bool,
    /// エラーメッセージ（検証失敗時）
    pub errors: Vec<String>,
    /// サニタイズされた値（成功時）
    pub sanitized_value: Option<String>,
    /// 適用された検証ルール
    pub applied_rules: Vec<String>,
}

impl ValidationResult {
    /// 成功結果を作成
    pub fn success(sanitized_value: Option<String>, applied_rules: Vec<String>) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            sanitized_value,
            applied_rules,
        }
    }

    /// 失敗結果を作成
    pub fn failure(errors: Vec<String>, applied_rules: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            sanitized_value: None,
            applied_rules,
        }
    }
}

/// 入力検証システムのコア
#[derive(Debug)]
pub struct InputValidator {
    /// 登録された検証ルール
    rules: HashMap<String, ValidationRule>,
    /// 高度なSQLインジェクション保護エンジン
    sql_protector: Option<SqlInjectionProtector>,
}

impl InputValidator {
    /// 新しい入力検証システムを作成
    pub fn new() -> Self {
        let mut validator = Self {
            rules: HashMap::new(),
            sql_protector: None,
        };

        // デフォルトのセキュリティルールを追加
        validator.add_default_security_rules();

        // SQLインジェクション保護を初期化
        if let Ok(sql_protector) = SqlInjectionProtector::new(SqlProtectionConfig::default()) {
            validator.sql_protector = Some(sql_protector);
        }

        validator
    }

    /// デフォルトのセキュリティルールを追加
    fn add_default_security_rules(&mut self) {
        // SQLインジェクション検出ルール
        let sql_rule = ValidationRule::new(
            ValidationRuleType::SqlInjection,
            "sql_injection_protection".to_string(),
        );
        self.add_rule(sql_rule);

        // XSS攻撃検出ルール
        let xss_rule =
            ValidationRule::new(ValidationRuleType::XssAttack, "xss_protection".to_string());
        self.add_rule(xss_rule);

        //基本的な長さ制限ルール
        let length_rule = ValidationRule::new(
            ValidationRuleType::LengthLimit,
            "default_length_limit".to_string(),
        )
        .with_max_length(10000);
        self.add_rule(length_rule);
    }

    /// 検証ルールを追加
    pub fn add_rule(&mut self, rule: ValidationRule) {
        self.rules.insert(rule.name.clone(), rule);
    }

    /// 検証ルールを削除
    pub fn remove_rule(&mut self, rule_name: &str) -> Option<ValidationRule> {
        self.rules.remove(rule_name)
    }

    /// 指定されたルールで入力を検証
    pub fn validate_with_rules(
        &self,
        input: &str,
        rule_names: &[String],
    ) -> Result<ValidationResult, SecurityError> {
        let mut errors = Vec::new();
        let mut applied_rules = Vec::new();
        let mut sanitized_value = input.to_string();

        for rule_name in rule_names {
            if let Some(rule) = self.rules.get(rule_name) {
                applied_rules.push(rule_name.clone());

                match self.apply_rule(rule, &sanitized_value)? {
                    Some(result) => {
                        if !result.is_valid {
                            errors.extend(result.errors);
                        } else if let Some(new_value) = result.sanitized_value {
                            sanitized_value = new_value;
                        }
                    }
                    None => continue,
                }
            }
        }

        if errors.is_empty() {
            Ok(ValidationResult::success(
                Some(sanitized_value),
                applied_rules,
            ))
        } else {
            Ok(ValidationResult::failure(errors, applied_rules))
        }
    }

    /// すべてのセキュリティルールで検証
    pub fn validate_security(&self, input: &str) -> Result<ValidationResult, SecurityError> {
        let security_rules: Vec<String> = self
            .rules
            .iter()
            .filter(|(_, rule)| {
                matches!(
                    rule.rule_type,
                    ValidationRuleType::SqlInjection
                        | ValidationRuleType::XssAttack
                        | ValidationRuleType::LengthLimit
                )
            })
            .map(|(name, _)| name.clone())
            .collect();

        self.validate_with_rules(input, &security_rules)
    }

    /// 単一のルールを適用
    fn apply_rule(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        // 必須チェック
        if rule.required && input.trim().is_empty() {
            return Ok(Some(ValidationResult::failure(
                vec!["Required field cannot be empty".to_string()],
                vec![rule.name.clone()],
            )));
        }

        // 空の入力で必須でない場合はスキップ
        if input.trim().is_empty() && !rule.required {
            return Ok(None);
        }

        match rule.rule_type {
            ValidationRuleType::SqlInjection => self.check_sql_injection(rule, input),
            ValidationRuleType::XssAttack => self.check_xss_attack(rule, input),
            ValidationRuleType::UrlFormat => self.check_url_format(rule, input),
            ValidationRuleType::EmailFormat => self.check_email_format(rule, input),
            ValidationRuleType::HtmlTags => self.sanitize_html(rule, input),
            ValidationRuleType::LengthLimit => self.check_length_limit(rule, input),
            ValidationRuleType::CustomPattern => self.check_custom_pattern(rule, input),
        }
    }

    /// SQLインジェクション検査
    fn check_sql_injection(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        // 高度なSQLインジェクション保護エンジンを使用
        if let Some(ref _sql_protector) = self.sql_protector {
            // InputValidatorのsql_protectorは読み取り専用なので、一時的なプロテクターを作成
            let mut temp_protector = SqlInjectionProtector::new(SqlProtectionConfig::default())?;
            let detection = temp_protector.inspect_query(input)?;

            if detection.detected {
                let error_message = format!(
                    "{} - Detected patterns: {:?}, Threat level: {:?}, Suspicious keywords: {:?}",
                    rule.error_message,
                    detection.matched_patterns,
                    detection.max_threat_level,
                    detection.suspicious_keywords
                );

                return Ok(Some(ValidationResult::failure(
                    vec![error_message],
                    vec![rule.name.clone()],
                )));
            }
        }

        // フォールバック: 基本的な正規表現チェック
        if let Some(pattern) = &rule.pattern {
            let regex = Regex::new(pattern).map_err(|e| {
                SecurityError::ValidationError(format!("Invalid regex pattern: {}", e))
            })?;

            if regex
                .is_match(input)
                .map_err(|e| SecurityError::ValidationError(format!("Regex match error: {}", e)))?
            {
                return Ok(Some(ValidationResult::failure(
                    vec![rule.error_message.clone()],
                    vec![rule.name.clone()],
                )));
            }
        }

        Ok(Some(ValidationResult::success(
            Some(input.to_string()),
            vec![rule.name.clone()],
        )))
    }

    /// XSS攻撃検査
    fn check_xss_attack(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        if let Some(pattern) = &rule.pattern {
            let regex = Regex::new(pattern).map_err(|e| {
                SecurityError::ValidationError(format!("Invalid regex pattern: {}", e))
            })?;

            if regex
                .is_match(input)
                .map_err(|e| SecurityError::ValidationError(format!("Regex match error: {}", e)))?
            {
                return Ok(Some(ValidationResult::failure(
                    vec![rule.error_message.clone()],
                    vec![rule.name.clone()],
                )));
            }
        }

        Ok(Some(ValidationResult::success(
            Some(input.to_string()),
            vec![rule.name.clone()],
        )))
    }

    /// URL形式検査
    fn check_url_format(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        match url::Url::parse(input) {
            Ok(_) => Ok(Some(ValidationResult::success(
                Some(input.to_string()),
                vec![rule.name.clone()],
            ))),
            Err(_) => Ok(Some(ValidationResult::failure(
                vec![rule.error_message.clone()],
                vec![rule.name.clone()],
            ))),
        }
    }

    /// Email形式検査
    fn check_email_format(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        let email_regex = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
            .map_err(|e| SecurityError::ValidationError(format!("Email regex error: {}", e)))?;

        if email_regex
            .is_match(input)
            .map_err(|e| SecurityError::ValidationError(format!("Email validation error: {}", e)))?
        {
            Ok(Some(ValidationResult::success(
                Some(input.to_string()),
                vec![rule.name.clone()],
            )))
        } else {
            Ok(Some(ValidationResult::failure(
                vec![rule.error_message.clone()],
                vec![rule.name.clone()],
            )))
        }
    }

    /// HTMLサニタイゼーション
    fn sanitize_html(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        let sanitized = if let Some(allowed_tags) = &rule.allowed_tags {
            let mut builder = ammonia::Builder::default();
            builder.tags(allowed_tags.iter().map(|s| s.as_str()).collect());
            builder.clean(input).to_string()
        } else {
            clean(input)
        };

        Ok(Some(ValidationResult::success(
            Some(sanitized),
            vec![rule.name.clone()],
        )))
    }

    /// 長さ制限検査
    fn check_length_limit(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        if let Some(max_length) = rule.max_length {
            if input.len() > max_length {
                return Ok(Some(ValidationResult::failure(
                    vec![format!(
                        "{} (max: {}, actual: {})",
                        rule.error_message,
                        max_length,
                        input.len()
                    )],
                    vec![rule.name.clone()],
                )));
            }
        }

        Ok(Some(ValidationResult::success(
            Some(input.to_string()),
            vec![rule.name.clone()],
        )))
    }

    /// カスタムパターン検査
    fn check_custom_pattern(
        &self,
        rule: &ValidationRule,
        input: &str,
    ) -> Result<Option<ValidationResult>, SecurityError> {
        if let Some(pattern) = &rule.pattern {
            let regex = Regex::new(pattern).map_err(|e| {
                SecurityError::ValidationError(format!("Invalid custom pattern: {}", e))
            })?;

            if regex.is_match(input).map_err(|e| {
                SecurityError::ValidationError(format!("Custom pattern match error: {}", e))
            })? {
                Ok(Some(ValidationResult::success(
                    Some(input.to_string()),
                    vec![rule.name.clone()],
                )))
            } else {
                Ok(Some(ValidationResult::failure(
                    vec![rule.error_message.clone()],
                    vec![rule.name.clone()],
                )))
            }
        } else {
            Ok(Some(ValidationResult::success(
                Some(input.to_string()),
                vec![rule.name.clone()],
            )))
        }
    }

    /// 検証統計情報を取得
    pub fn get_validation_stats(&self) -> ValidationStats {
        ValidationStats {
            total_rules: self.rules.len(),
            security_rules: self
                .rules
                .iter()
                .filter(|(_, rule)| {
                    matches!(
                        rule.rule_type,
                        ValidationRuleType::SqlInjection | ValidationRuleType::XssAttack
                    )
                })
                .count(),
            custom_rules: self
                .rules
                .iter()
                .filter(|(_, rule)| matches!(rule.rule_type, ValidationRuleType::CustomPattern))
                .count(),
        }
    }

    /// SQLクエリを詳細分析（高度なSQLインジェクション保護エンジンを使用）
    pub fn analyze_sql_query(
        &self,
        query: &str,
    ) -> Result<Option<crate::security::sql_injection_protection::SqlQueryAnalysis>, SecurityError>
    {
        if let Some(ref _sql_protector) = self.sql_protector {
            // 読み取り専用のため、一時的なプロテクターを作成
            let temp_protector = SqlInjectionProtector::new(SqlProtectionConfig::default())?;
            let analysis = temp_protector.analyze_query(query)?;
            Ok(Some(analysis))
        } else {
            Ok(None)
        }
    }

    /// SQLインジェクション保護エンジンの設定を更新
    pub fn configure_sql_protection(
        &mut self,
        config: SqlProtectionConfig,
    ) -> Result<(), SecurityError> {
        self.sql_protector = Some(SqlInjectionProtector::new(config)?);
        Ok(())
    }

    /// SQLインジェクション保護の統計を取得
    pub fn get_sql_protection_stats(
        &self,
    ) -> Option<&crate::security::sql_injection_protection::SqlInjectionStats> {
        if let Some(ref _sql_protector) = self.sql_protector {
            // 現在の実装では統計を取得できないため、Noneを返す
            // 将来的にはmutableアクセスまたは設計変更が必要
            None
        } else {
            None
        }
    }
}

impl Default for InputValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// 検証統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationStats {
    /// 総ルール数
    pub total_rules: usize,
    /// セキュリティルール数
    pub security_rules: usize,
    /// カスタムルール数
    pub custom_rules: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_injection_detection() {
        let validator = InputValidator::new();

        // SQLインジェクションを含む入力
        let malicious_input = "'; DROP TABLE users; --";
        let result = validator.validate_security(malicious_input).unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_xss_attack_detection() {
        let validator = InputValidator::new();

        // XSS攻撃を含む入力
        let malicious_input = "<script>alert('XSS')</script>";
        let result = validator.validate_security(malicious_input).unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_valid_input() {
        let validator = InputValidator::new();

        // 正常な入力
        let safe_input = "Hello, World!";
        let result = validator.validate_security(safe_input).unwrap();

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.sanitized_value, Some(safe_input.to_string()));
    }

    #[test]
    fn test_length_limit() {
        let mut validator = InputValidator::new();

        // 短い長さ制限ルールを追加
        let length_rule =
            ValidationRule::new(ValidationRuleType::LengthLimit, "short_limit".to_string())
                .with_max_length(10);
        validator.add_rule(length_rule);

        // 長すぎる入力
        let long_input = "This is a very long input that exceeds the limit";
        let result = validator
            .validate_with_rules(long_input, &["short_limit".to_string()])
            .unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_html_sanitization() {
        let mut validator = InputValidator::new();

        // HTMLサニタイゼーションルールを追加
        let html_rule =
            ValidationRule::new(ValidationRuleType::HtmlTags, "html_sanitize".to_string())
                .with_allowed_tags(vec!["b".to_string(), "i".to_string()]);
        validator.add_rule(html_rule);

        // HTMLを含む入力
        let html_input = "<b>Bold</b> and <script>alert('bad')</script>";
        let result = validator
            .validate_with_rules(html_input, &["html_sanitize".to_string()])
            .unwrap();

        assert!(result.is_valid);
        let sanitized = result.sanitized_value.as_ref().unwrap();
        assert!(sanitized.contains("<b>Bold</b>"));
        assert!(!sanitized.contains("<script>"));
    }

    #[test]
    fn test_url_validation() {
        let mut validator = InputValidator::new();

        // URL検証ルールを追加
        let url_rule = ValidationRule::new(ValidationRuleType::UrlFormat, "url_check".to_string());
        validator.add_rule(url_rule);

        // 有効なURL
        let valid_url = "https://example.com";
        let result = validator
            .validate_with_rules(valid_url, &["url_check".to_string()])
            .unwrap();
        assert!(result.is_valid);

        // 無効なURL
        let invalid_url = "not-a-url";
        let result = validator
            .validate_with_rules(invalid_url, &["url_check".to_string()])
            .unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_email_validation() {
        let mut validator = InputValidator::new();

        // Email検証ルールを追加
        let email_rule =
            ValidationRule::new(ValidationRuleType::EmailFormat, "email_check".to_string());
        validator.add_rule(email_rule);

        // 有効なEmail
        let valid_email = "test@example.com";
        let result = validator
            .validate_with_rules(valid_email, &["email_check".to_string()])
            .unwrap();
        assert!(result.is_valid);

        // 無効なEmail
        let invalid_email = "not-an-email";
        let result = validator
            .validate_with_rules(invalid_email, &["email_check".to_string()])
            .unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_custom_pattern() {
        let mut validator = InputValidator::new();

        // カスタムパターンルール（数字のみ）を追加
        let custom_rule = ValidationRule::new(
            ValidationRuleType::CustomPattern,
            "numbers_only".to_string(),
        )
        .with_pattern(r"^\d+$".to_string());
        validator.add_rule(custom_rule);

        // 数字のみの入力
        let numbers_input = "12345";
        let result = validator
            .validate_with_rules(numbers_input, &["numbers_only".to_string()])
            .unwrap();
        assert!(result.is_valid);

        // 文字を含む入力
        let mixed_input = "123abc";
        let result = validator
            .validate_with_rules(mixed_input, &["numbers_only".to_string()])
            .unwrap();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_validation_stats() {
        let validator = InputValidator::new();
        let stats = validator.get_validation_stats();

        assert!(stats.total_rules >= stats.security_rules);
        assert!(stats.security_rules >= 2); // SQL + XSS ルール
    }

    #[test]
    fn test_advanced_sql_injection_detection() {
        let validator = InputValidator::new();

        // Union-based SQLインジェクション
        let union_attack =
            "SELECT * FROM users WHERE id = 1 UNION SELECT username, password FROM admin";
        let result = validator.validate_security(union_attack).unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Time-based SQLインジェクション
        let time_attack = "SELECT * FROM users WHERE id = 1; WAITFOR DELAY '00:00:05'";
        let result = validator.validate_security(time_attack).unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Boolean-based blind SQLインジェクション
        let boolean_attack = "SELECT * FROM users WHERE id = 1 AND 1=1";
        let result = validator.validate_security(boolean_attack).unwrap();

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_sql_query_analysis() {
        let validator = InputValidator::new();

        let query = "SELECT name, email FROM users WHERE id = ? AND active = 1 ORDER BY name";
        if let Ok(Some(analysis)) = validator.analyze_sql_query(query) {
            assert!(analysis.tables.contains(&"users".to_string()));
            assert!(analysis.columns.contains(&"name".to_string()));
            assert!(analysis.columns.contains(&"email".to_string()));
            assert_eq!(analysis.parameter_count, 1);
            assert!(analysis.prepared_statement_recommended);
        }
    }

    #[test]
    fn test_sql_protection_configuration() {
        let mut validator = InputValidator::new();

        let custom_config = crate::security::sql_injection_protection::SqlProtectionConfig {
            enabled: true,
            block_mode: true,
            min_threat_level: crate::security::sql_injection_protection::ThreatLevel::Medium,
            max_query_length: 1000,
            max_parameters: 10,
            whitelist_mode: true,
            real_time_monitoring: true,
        };

        assert!(validator.configure_sql_protection(custom_config).is_ok());
    }
}
