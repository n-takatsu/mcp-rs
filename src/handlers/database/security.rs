//! Database Security Layer
//!
//! データベース操作のセキュリティ機能を提供

use super::types::{QueryContext, QueryType, SecurityConfig, SecurityError, ValidationResult};
// use crate::threat_intelligence::ThreatDetectionEngine;

// Temporary placeholder for ThreatDetectionEngine
pub struct ThreatDetectionEngine;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{info, warn};

/// データベースセキュリティマネージャー
pub struct DatabaseSecurity {
    config: SecurityConfig,
    sql_injection_detector: SqlInjectionDetector,
    query_whitelist: QueryWhitelist,
    audit_logger: AuditLogger,
    threat_intelligence: Option<Arc<ThreatDetectionEngine>>,
    rate_limiter: RateLimiter,
}

impl DatabaseSecurity {
    /// 新しいセキュリティインスタンスを作成
    pub fn new(
        config: SecurityConfig,
        threat_intelligence: Option<Arc<ThreatDetectionEngine>>,
    ) -> Self {
        Self {
            sql_injection_detector: SqlInjectionDetector::new(),
            query_whitelist: QueryWhitelist::new(),
            audit_logger: AuditLogger::new(),
            rate_limiter: RateLimiter::new(),
            config,
            threat_intelligence,
        }
    }

    /// クエリの安全性を検証
    pub async fn validate_query(
        &self,
        sql: &str,
        context: &QueryContext,
    ) -> Result<ValidationResult, SecurityError> {
        // 1. 基本的な前処理チェック
        self.validate_basic_constraints(sql, context)?;

        // 2. SQLインジェクション検知
        if self.config.enable_sql_injection_detection {
            self.sql_injection_detector.scan(sql, context)?;
        }

        // 3. クエリホワイトリストチェック
        if self.config.enable_query_whitelist {
            self.query_whitelist.validate(sql, context)?;
        }

        // 4. レート制限チェック
        self.rate_limiter.check_rate_limit(context).await?;

        // 5. 脅威インテリジェンス分析
        if self.config.threat_intelligence_enabled {
            if let Some(ti) = &self.threat_intelligence {
                self.analyze_with_threat_intelligence(sql, context, ti)
                    .await?;
            }
        }

        // 6. 監査ログ記録
        if self.config.enable_audit_logging {
            self.audit_logger
                .log_query_validation(sql, context, &ValidationResult::Approved)
                .await?;
        }

        Ok(ValidationResult::Approved)
    }

    /// 基本制約の検証
    fn validate_basic_constraints(
        &self,
        sql: &str,
        context: &QueryContext,
    ) -> Result<(), SecurityError> {
        // SQLの長さチェック
        if sql.len() > self.config.max_query_length {
            return Err(SecurityError::AccessDenied(format!(
                "Query too long: {} > {}",
                sql.len(),
                self.config.max_query_length
            )));
        }

        // 許可されていない操作タイプをチェック
        if !self.config.allowed_operations.contains(&context.query_type) {
            return Err(SecurityError::AccessDenied(format!(
                "Operation not allowed: {:?}",
                context.query_type
            )));
        }

        Ok(())
    }

    /// 脅威インテリジェンス分析
    async fn analyze_with_threat_intelligence(
        &self,
        sql: &str,
        _context: &QueryContext,
        _ti: &ThreatDetectionEngine,
    ) -> Result<(), SecurityError> {
        // SQLパターンを脅威インテリジェンスで分析
        let _query_hash = self.calculate_query_hash(sql);

        // 既知の悪意のあるクエリパターンをチェック
        // TODO: 実装が必要
        // if ti.is_malicious_pattern(&query_hash).await.unwrap_or(false) {
        //     return Err(SecurityError::ThreatDetected(
        //         "Malicious SQL pattern detected by threat intelligence".to_string()
        //     ));
        // }

        // ソースIPを脅威インテリジェンスで確認
        // TODO: 実装が必要
        // if let Some(source_ip) = &context.source_ip {
        //     if ti.is_malicious_ip(source_ip).await.unwrap_or(false) {
        //         return Err(SecurityError::ThreatDetected(
        //             format!("Malicious IP detected: {}", source_ip)
        //         ));
        //     }
        // }

        Ok(())
    }

    /// クエリハッシュを計算
    fn calculate_query_hash(&self, sql: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(sql.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// SQLインジェクション検知器
pub struct SqlInjectionDetector {
    dangerous_patterns: Vec<Regex>,
    suspicious_functions: HashSet<String>,
}

impl Default for SqlInjectionDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlInjectionDetector {
    pub fn new() -> Self {
        let dangerous_patterns = vec![
            // UNION攻撃
            Regex::new(r"(?i)\bunion\s+(all\s+)?select\b").unwrap(),
            // コメントアウト攻撃
            Regex::new(r"--\s*$").unwrap(),
            Regex::new(r"#\s*$").unwrap(), // MySQL hash comment
            Regex::new(r"/\*.*?\*/").unwrap(),
            Regex::new(r";\s*--").unwrap(), // Statement termination with comment
            // Stacked queries (複数クエリ実行)
            Regex::new(r"(?i);\s*(drop|delete|update|insert|alter|create)\b").unwrap(),
            // OR 1=1系の攻撃
            Regex::new(r"(?i)\bor\s+\d+\s*=\s*\d+").unwrap(),
            Regex::new(r"(?i)\bor\s+'[^']*'\s*=\s*'[^']*'").unwrap(),
            // AND 1=1系の攻撃
            Regex::new(r"(?i)\band\s+\d+\s*=\s*\d+").unwrap(),
            Regex::new(r"(?i)\band\s+'[^']*'\s*=\s*'[^']*'").unwrap(),
            // Information schema enumeration
            Regex::new(r"(?i)\binformation_schema\b").unwrap(),
            // Error-based injection functions
            Regex::new(r"(?i)\b(extractvalue|updatexml)\s*\(").unwrap(),
            Regex::new(r"(?i)\bgeometrycollection\s*\(").unwrap(),
            // Time-based functions (WAITFOR)
            Regex::new(r"(?i)\bwaitfor\s+delay\b").unwrap(),
            // Hexadecimal injection
            Regex::new(r"(?i)\b0x[0-9a-f]+\s*=\s*0x[0-9a-f]+").unwrap(),
            // CHAR/ASCII based obfuscation
            Regex::new(r"(?i)\b(char|ascii)\s*\([^)]+\)\s*=\s*(char|ascii)\s*\(").unwrap(),
            // Comment-based obfuscation
            Regex::new(r"(?i)(union|select|from|where)\s*/\*.*?\*/\s*(union|select|from|where)").unwrap(),
            // システム関数の呼び出し
            Regex::new(r"(?i)\b(exec|execute|sp_|xp_)\w*\s*\(").unwrap(),
            // ファイル操作
            Regex::new(r"(?i)\b(load_file|into\s+outfile|into\s+dumpfile)\b").unwrap(),
        ];

        let suspicious_functions = [
            "benchmark",
            "sleep",
            "pg_sleep",
            "waitfor",
            "load_file",
            // Note: char, ascii, substring removed to avoid false positives
            // They are caught by specific regex patterns instead
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            dangerous_patterns,
            suspicious_functions,
        }
    }

    pub fn scan(&self, sql: &str, _context: &QueryContext) -> Result<(), SecurityError> {
        // 危険なパターンをチェック
        for pattern in &self.dangerous_patterns {
            if pattern.is_match(sql) {
                return Err(SecurityError::SqlInjectionDetected(format!(
                    "Dangerous pattern detected: {}",
                    pattern.as_str()
                )));
            }
        }

        // 怪しい関数をチェック
        let sql_lower = sql.to_lowercase();
        for func in &self.suspicious_functions {
            if sql_lower.contains(func) {
                return Err(SecurityError::SqlInjectionDetected(format!(
                    "Suspicious function detected: {}",
                    func
                )));
            }
        }

        // バランスの取れていない引用符をチェック
        self.check_quote_balance(sql)?;

        Ok(())
    }

    fn check_quote_balance(&self, sql: &str) -> Result<(), SecurityError> {
        let mut single_quote_count = 0;
        let mut double_quote_count = 0;
        let mut in_escape = false;

        for ch in sql.chars() {
            if in_escape {
                in_escape = false;
                continue;
            }

            match ch {
                '\\' => in_escape = true,
                '\'' => single_quote_count += 1,
                '"' => double_quote_count += 1,
                _ => {}
            }
        }

        if single_quote_count % 2 != 0 || double_quote_count % 2 != 0 {
            return Err(SecurityError::SqlInjectionDetected(
                "Unbalanced quotes detected".to_string(),
            ));
        }

        Ok(())
    }
}

/// クエリホワイトリスト
pub struct QueryWhitelist {
    allowed_patterns: Vec<Regex>,
    allowed_tables: HashSet<String>,
    allowed_operations: HashMap<QueryType, Vec<String>>,
}

impl Default for QueryWhitelist {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryWhitelist {
    pub fn new() -> Self {
        Self {
            allowed_patterns: Vec::new(),
            allowed_tables: HashSet::new(),
            allowed_operations: HashMap::new(),
        }
    }

    /// 許可されたパターンを追加
    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.allowed_patterns.push(regex);
        Ok(())
    }

    /// 許可されたテーブルを追加
    pub fn add_table(&mut self, table_name: String) {
        self.allowed_tables.insert(table_name);
    }

    /// ホワイトリストに対してクエリを検証
    pub fn validate(&self, sql: &str, _context: &QueryContext) -> Result<(), SecurityError> {
        // パターンマッチングチェック
        if !self.allowed_patterns.is_empty() {
            let mut pattern_matched = false;
            for pattern in &self.allowed_patterns {
                if pattern.is_match(sql) {
                    pattern_matched = true;
                    break;
                }
            }

            if !pattern_matched {
                return Err(SecurityError::QueryNotWhitelisted(
                    "Query does not match any allowed pattern".to_string(),
                ));
            }
        }

        // テーブル名チェック
        if !self.allowed_tables.is_empty() {
            let referenced_tables = self.extract_table_names(sql);
            for table in referenced_tables {
                if !self.allowed_tables.contains(&table) {
                    return Err(SecurityError::QueryNotWhitelisted(format!(
                        "Table not in whitelist: {}",
                        table
                    )));
                }
            }
        }

        Ok(())
    }

    /// SQLからテーブル名を抽出（簡易版）
    fn extract_table_names(&self, sql: &str) -> Vec<String> {
        let table_pattern =
            Regex::new(r"(?i)\b(?:from|join|update|into)\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

        table_pattern
            .captures_iter(sql)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_lowercase())
            .collect()
    }
}

/// 監査ログ記録器
pub struct AuditLogger {
    // 実際の実装では外部ログシステムに接続
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLogger {
    pub fn new() -> Self {
        Self {}
    }

    /// クエリ検証の監査ログを記録
    pub async fn log_query_validation(
        &self,
        sql: &str,
        context: &QueryContext,
        result: &ValidationResult,
    ) -> Result<(), SecurityError> {
        let log_entry = AuditLogEntry {
            timestamp: Utc::now(),
            event_type: AuditEventType::QueryValidation,
            user_id: context.user_id.clone(),
            session_id: context.session_id.clone(),
            source_ip: context.source_ip.clone(),
            query_type: context.query_type.clone(),
            sql_hash: self.hash_sql(sql),
            result: match result {
                ValidationResult::Approved => "APPROVED".to_string(),
                ValidationResult::Denied(reason) => format!("DENIED: {}", reason),
                ValidationResult::Warning(reason) => format!("WARNING: {}", reason),
            },
            additional_info: None,
        };

        // 実際のログ出力（JSONフォーマット）
        info!(
            "AUDIT: {}",
            serde_json::to_string(&log_entry).unwrap_or_default()
        );

        Ok(())
    }

    /// クエリ実行の監査ログを記録
    pub async fn log_query_execution(
        &self,
        sql: &str,
        context: &QueryContext,
        execution_time_ms: u64,
        rows_affected: Option<u64>,
    ) -> Result<(), SecurityError> {
        let log_entry = AuditLogEntry {
            timestamp: Utc::now(),
            event_type: AuditEventType::QueryExecution,
            user_id: context.user_id.clone(),
            session_id: context.session_id.clone(),
            source_ip: context.source_ip.clone(),
            query_type: context.query_type.clone(),
            sql_hash: self.hash_sql(sql),
            result: "EXECUTED".to_string(),
            additional_info: Some(serde_json::json!({
                "execution_time_ms": execution_time_ms,
                "rows_affected": rows_affected
            })),
        };

        info!(
            "AUDIT: {}",
            serde_json::to_string(&log_entry).unwrap_or_default()
        );

        Ok(())
    }

    fn hash_sql(&self, sql: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(sql.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// 監査ログエントリ
#[derive(Debug, Serialize)]
struct AuditLogEntry {
    timestamp: DateTime<Utc>,
    event_type: AuditEventType,
    user_id: Option<String>,
    session_id: String,
    source_ip: Option<String>,
    query_type: QueryType,
    sql_hash: String,
    result: String,
    additional_info: Option<serde_json::Value>,
}

/// 監査イベントタイプ
#[derive(Debug, Serialize)]
enum AuditEventType {
    QueryValidation,
    QueryExecution,
    ConnectionAttempt,
    SecurityViolation,
}

/// レート制限器
pub struct RateLimiter {
    request_counts: Arc<tokio::sync::RwLock<HashMap<String, RequestCounter>>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        let limiter = Self {
            request_counts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        };

        // クリーンアップタスクを開始
        limiter.start_cleanup_task();

        limiter
    }

    /// レート制限をチェック
    pub async fn check_rate_limit(&self, context: &QueryContext) -> Result<(), SecurityError> {
        // セッション単位でレート制限
        let key = context.session_id.clone();

        let mut counts = self.request_counts.write().await;
        let counter = counts.entry(key).or_insert_with(RequestCounter::new);

        if counter.is_rate_limited() {
            return Err(SecurityError::RateLimitExceeded(
                "Too many requests in time window".to_string(),
            ));
        }

        counter.add_request();
        Ok(())
    }

    /// クリーンアップタスクを開始
    fn start_cleanup_task(&self) {
        let request_counts = self.request_counts.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

            loop {
                interval.tick().await;

                let mut counts = request_counts.write().await;
                counts.retain(|_, counter| !counter.is_expired());
            }
        });
    }
}

/// リクエストカウンター
struct RequestCounter {
    count: u32,
    window_start: DateTime<Utc>,
    max_requests: u32,
    window_duration_secs: i64,
}

impl RequestCounter {
    fn new() -> Self {
        Self {
            count: 0,
            window_start: Utc::now(),
            max_requests: 100, // 1分間に100リクエスト
            window_duration_secs: 60,
        }
    }

    fn add_request(&mut self) {
        if self.is_new_window() {
            self.reset_window();
        }
        self.count += 1;
    }

    fn is_rate_limited(&self) -> bool {
        !self.is_new_window() && self.count >= self.max_requests
    }

    fn is_new_window(&self) -> bool {
        (Utc::now() - self.window_start).num_seconds() >= self.window_duration_secs
    }

    fn is_expired(&self) -> bool {
        (Utc::now() - self.window_start).num_seconds() >= self.window_duration_secs * 2
    }

    fn reset_window(&mut self) {
        self.count = 0;
        self.window_start = Utc::now();
    }
}
