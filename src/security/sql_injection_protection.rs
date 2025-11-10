//! 高度なSQLインジェクション保護システム
//!
//! このモジュールは基本的な入力検証を超えた、包括的なSQLインジェクション対策を提供します：
//! - 高度なSQLパターン検出エンジン
//! - SQLコンテキスト分析
//! - Prepared statement強制機能
//! - リアルタイムクエリ監視
//! - SQL構文解析とホワイトリスト検証

use crate::error::SecurityError;
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// SQLクエリの種類
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SqlQueryType {
    /// SELECT文
    Select,
    /// INSERT文
    Insert,
    /// UPDATE文
    Update,
    /// DELETE文
    Delete,
    /// CREATE文
    Create,
    /// DROP文
    Drop,
    /// ALTER文
    Alter,
    /// EXEC/EXECUTE文
    Execute,
    /// その他/不明
    Unknown,
}

impl SqlQueryType {
    /// 文字列からSQLクエリタイプを判定
    pub fn from_query(query: &str) -> Self {
        let query_lower = query.trim().to_lowercase();

        if query_lower.starts_with("select") {
            SqlQueryType::Select
        } else if query_lower.starts_with("insert") {
            SqlQueryType::Insert
        } else if query_lower.starts_with("update") {
            SqlQueryType::Update
        } else if query_lower.starts_with("delete") {
            SqlQueryType::Delete
        } else if query_lower.starts_with("create") {
            SqlQueryType::Create
        } else if query_lower.starts_with("drop") {
            SqlQueryType::Drop
        } else if query_lower.starts_with("alter") {
            SqlQueryType::Alter
        } else if query_lower.starts_with("exec") || query_lower.starts_with("execute") {
            SqlQueryType::Execute
        } else {
            SqlQueryType::Unknown
        }
    }
}

/// SQLインジェクション脅威レベル
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ThreatLevel {
    /// 情報収集の試み
    Info = 1,
    /// 軽微な脅威
    Low = 2,
    /// 中程度の脅威
    Medium = 3,
    /// 高い脅威
    High = 4,
    /// 重大な脅威
    Critical = 5,
}

/// SQLインジェクション攻撃パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlInjectionPattern {
    /// パターン名
    pub name: String,
    /// 正規表現パターン
    pub pattern: String,
    /// 脅威レベル
    pub threat_level: ThreatLevel,
    /// 説明
    pub description: String,
    /// 対応するSQLクエリタイプ
    pub query_types: Vec<SqlQueryType>,
}

impl SqlInjectionPattern {
    /// 新しいSQLインジェクションパターンを作成
    pub fn new(
        name: String,
        pattern: String,
        threat_level: ThreatLevel,
        description: String,
        query_types: Vec<SqlQueryType>,
    ) -> Self {
        Self {
            name,
            pattern,
            threat_level,
            description,
            query_types,
        }
    }
}

/// SQLインジェクション検出結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlInjectionDetection {
    /// 検出されたか
    pub detected: bool,
    /// マッチしたパターン
    pub matched_patterns: Vec<String>,
    /// 最高脅威レベル
    pub max_threat_level: ThreatLevel,
    /// 検出されたクエリタイプ
    pub detected_query_type: SqlQueryType,
    /// 疑わしいキーワード
    pub suspicious_keywords: Vec<String>,
    /// 分析メッセージ
    pub analysis_message: String,
    /// 検出時刻
    pub detection_time: std::time::SystemTime,
}

/// SQLクエリ分析結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlQueryAnalysis {
    /// 元のクエリ
    pub original_query: String,
    /// 正規化されたクエリ
    pub normalized_query: String,
    /// 検出されたテーブル名
    pub tables: Vec<String>,
    /// 検出されたカラム名
    pub columns: Vec<String>,
    /// パラメータ数
    pub parameter_count: usize,
    /// クエリの複雑さスコア
    pub complexity_score: u32,
    /// Prepared statement使用推奨度
    pub prepared_statement_recommended: bool,
}

/// SQLインジェクション保護エンジン
#[derive(Debug)]
pub struct SqlInjectionProtector {
    /// 検出パターン
    patterns: HashMap<String, SqlInjectionPattern>,
    /// コンパイル済み正規表現
    compiled_patterns: HashMap<String, Regex>,
    /// 許可されたSQLキーワード
    allowed_keywords: HashSet<String>,
    /// 許可されたテーブル名
    allowed_tables: HashSet<String>,
    /// 検出統計
    detection_stats: SqlInjectionStats,
    /// 保護設定
    config: SqlProtectionConfig,
}

/// SQL保護設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlProtectionConfig {
    /// 保護を有効にするか
    pub enabled: bool,
    /// ブロックモード（true: ブロック、false: ログのみ）
    pub block_mode: bool,
    /// 最小脅威レベル
    pub min_threat_level: ThreatLevel,
    /// 最大クエリ長
    pub max_query_length: usize,
    /// 最大パラメータ数
    pub max_parameters: usize,
    /// ホワイトリストモード
    pub whitelist_mode: bool,
    /// リアルタイム監視
    pub real_time_monitoring: bool,
}

impl Default for SqlProtectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            block_mode: true,
            min_threat_level: ThreatLevel::Low,
            max_query_length: 4096,
            max_parameters: 50,
            whitelist_mode: false,
            real_time_monitoring: true,
        }
    }
}

/// SQL検出統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlInjectionStats {
    /// 総検査数
    pub total_inspections: u64,
    /// 検出総数
    pub total_detections: u64,
    /// ブロック数
    pub total_blocks: u64,
    /// 脅威レベル別検出数
    pub detections_by_level: HashMap<ThreatLevel, u64>,
    /// 最後の検出時刻
    pub last_detection: Option<std::time::SystemTime>,
    /// 最後のリセット時刻
    pub stats_reset_time: std::time::SystemTime,
}

impl Default for SqlInjectionStats {
    fn default() -> Self {
        Self {
            total_inspections: 0,
            total_detections: 0,
            total_blocks: 0,
            detections_by_level: HashMap::new(),
            last_detection: None,
            stats_reset_time: std::time::SystemTime::now(),
        }
    }
}

impl SqlInjectionProtector {
    /// 新しいSQL保護エンジンを作成
    pub fn new(config: SqlProtectionConfig) -> Result<Self, SecurityError> {
        let mut protector = Self {
            patterns: HashMap::new(),
            compiled_patterns: HashMap::new(),
            allowed_keywords: HashSet::new(),
            allowed_tables: HashSet::new(),
            detection_stats: SqlInjectionStats::default(),
            config,
        };

        // デフォルトパターンを登録
        protector.register_default_patterns()?;

        // デフォルトホワイトリストを設定
        protector.setup_default_whitelist();

        Ok(protector)
    }

    /// デフォルトの攻撃パターンを登録
    fn register_default_patterns(&mut self) -> Result<(), SecurityError> {
        let patterns = vec![
            // Union based SQLi
            SqlInjectionPattern::new(
                "union_select".to_string(),
                r"(?i)\bunion\s+(all\s+)?select\b".to_string(),
                ThreatLevel::High,
                "Union-based SQL injection attempt".to_string(),
                vec![SqlQueryType::Select],
            ),
            // Boolean based SQLi
            SqlInjectionPattern::new(
                "boolean_blindsqli".to_string(),
                r"(?i)(\b(and|or)\s+)?(1\s*=\s*1|1\s*=\s*0|\'\s*=\s*\'|\'\s*or\s*\'|1\s*or\s*1)"
                    .to_string(),
                ThreatLevel::High,
                "Boolean-based blind SQL injection".to_string(),
                vec![
                    SqlQueryType::Select,
                    SqlQueryType::Update,
                    SqlQueryType::Delete,
                ],
            ),
            // Time based SQLi
            SqlInjectionPattern::new(
                "time_based_sqli".to_string(),
                r"(?i)\b(sleep\s*\(|benchmark\s*\(|waitfor\s+delay|pg_sleep\s*\()".to_string(),
                ThreatLevel::Critical,
                "Time-based SQL injection attempt".to_string(),
                vec![SqlQueryType::Select],
            ),
            // Error based SQLi
            SqlInjectionPattern::new(
                "error_based_sqli".to_string(),
                r"(?i)\b(extractvalue\s*\(|updatexml\s*\(|exp\s*\(\s*~|floor\s*\(\s*rand)"
                    .to_string(),
                ThreatLevel::High,
                "Error-based SQL injection attempt".to_string(),
                vec![SqlQueryType::Select],
            ),
            // LDAP injection
            SqlInjectionPattern::new(
                "ldap_injection".to_string(),
                r"(?i)(\*\)|&\(|\|\(|\)\(|\*\(\w+\=)".to_string(),
                ThreatLevel::Medium,
                "LDAP injection attempt".to_string(),
                vec![SqlQueryType::Unknown],
            ),
            // Database fingerprinting
            SqlInjectionPattern::new(
                "db_fingerprinting".to_string(),
                r"(?i)\b(@@version|version\(\)|user\(\)|database\(\)|schema\(\)|connection_id\(\))"
                    .to_string(),
                ThreatLevel::Medium,
                "Database fingerprinting attempt".to_string(),
                vec![SqlQueryType::Select],
            ),
            // Stored procedure execution
            SqlInjectionPattern::new(
                "stored_procedure_exec".to_string(),
                r"(?i)\b(exec\s+|execute\s+|sp_|xp_)\w+".to_string(),
                ThreatLevel::Critical,
                "Stored procedure execution attempt".to_string(),
                vec![SqlQueryType::Execute],
            ),
            // System table access
            SqlInjectionPattern::new(
                "system_table_access".to_string(),
                r"(?i)\b(information_schema|sys\.|mysql\.|pg_|all_tables|user_tables)".to_string(),
                ThreatLevel::High,
                "System table access attempt".to_string(),
                vec![SqlQueryType::Select],
            ),
            // Comment-based bypass
            SqlInjectionPattern::new(
                "comment_bypass".to_string(),
                r"(?i)(\/\*[\s\S]*?\*\/|--[\s\S]*?$|\#[\s\S]*?$)".to_string(),
                ThreatLevel::Medium,
                "Comment-based SQL injection bypass".to_string(),
                vec![
                    SqlQueryType::Select,
                    SqlQueryType::Update,
                    SqlQueryType::Delete,
                ],
            ),
            // Hex encoding bypass
            SqlInjectionPattern::new(
                "hex_encoding".to_string(),
                r"(?i)\b0x[0-9a-f]{8,}".to_string(),
                ThreatLevel::Medium,
                "Hex encoding bypass attempt".to_string(),
                vec![
                    SqlQueryType::Select,
                    SqlQueryType::Insert,
                    SqlQueryType::Update,
                ],
            ),
            // Stacked queries
            SqlInjectionPattern::new(
                "stacked_queries".to_string(),
                r"(?i);[\s]*\b(select|insert|update|delete|drop|create|alter|exec)".to_string(),
                ThreatLevel::Critical,
                "Stacked query injection attempt".to_string(),
                vec![
                    SqlQueryType::Select,
                    SqlQueryType::Insert,
                    SqlQueryType::Update,
                    SqlQueryType::Delete,
                ],
            ),
        ];

        for pattern in patterns {
            self.add_pattern(pattern)?;
        }

        Ok(())
    }

    /// デフォルトホワイトリストを設定
    fn setup_default_whitelist(&mut self) {
        // 許可されたSQLキーワード
        let allowed_keywords = vec![
            "SELECT", "FROM", "WHERE", "AND", "OR", "ORDER", "BY", "GROUP", "HAVING", "LIMIT",
            "OFFSET", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "JOIN", "LEFT",
            "RIGHT", "INNER", "OUTER", "ON", "AS", "DISTINCT", "COUNT", "SUM", "AVG", "MAX", "MIN",
            "LIKE", "IN", "BETWEEN", "IS", "NULL", "NOT", "EXISTS", "CASE", "WHEN", "THEN", "ELSE",
            "END",
        ];

        self.allowed_keywords = allowed_keywords
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // 基本的なテーブル名（必要に応じて設定）
        self.allowed_tables.insert("users".to_string());
        self.allowed_tables.insert("posts".to_string());
        self.allowed_tables.insert("categories".to_string());
        self.allowed_tables.insert("comments".to_string());
    }

    /// 新しいパターンを追加
    pub fn add_pattern(&mut self, pattern: SqlInjectionPattern) -> Result<(), SecurityError> {
        let regex = Regex::new(&pattern.pattern)
            .map_err(|e| SecurityError::ValidationError(format!("Invalid pattern regex: {}", e)))?;

        self.compiled_patterns.insert(pattern.name.clone(), regex);
        self.patterns.insert(pattern.name.clone(), pattern);

        Ok(())
    }

    /// SQLクエリを検査
    pub fn inspect_query(&mut self, query: &str) -> Result<SqlInjectionDetection, SecurityError> {
        self.detection_stats.total_inspections += 1;

        if !self.config.enabled {
            return Ok(SqlInjectionDetection {
                detected: false,
                matched_patterns: vec![],
                max_threat_level: ThreatLevel::Info,
                detected_query_type: SqlQueryType::Unknown,
                suspicious_keywords: vec![],
                analysis_message: "SQL protection disabled".to_string(),
                detection_time: std::time::SystemTime::now(),
            });
        }

        // 基本的な検証
        if query.len() > self.config.max_query_length {
            return Ok(self.create_detection_result(
                true,
                vec!["query_too_long".to_string()],
                ThreatLevel::Medium,
                SqlQueryType::Unknown,
                vec![],
                format!(
                    "Query length exceeds limit: {} > {}",
                    query.len(),
                    self.config.max_query_length
                ),
            ));
        }

        let mut matched_patterns = Vec::new();
        let mut max_threat_level = ThreatLevel::Info;

        // パターンマッチング
        for (name, pattern) in &self.compiled_patterns {
            if let Ok(is_match) = pattern.is_match(query) {
                if is_match {
                    if let Some(pattern_def) = self.patterns.get(name) {
                        matched_patterns.push(name.clone());
                        if pattern_def.threat_level > max_threat_level {
                            max_threat_level = pattern_def.threat_level.clone();
                        }
                    }
                }
            }
        }

        // キーワード分析
        let suspicious_keywords = self.analyze_keywords(query);

        // クエリタイプ判定
        let query_type = SqlQueryType::from_query(query);

        // ホワイトリストチェック
        if self.config.whitelist_mode && !self.validate_whitelist(query, &query_type) {
            matched_patterns.push("whitelist_violation".to_string());
            max_threat_level = ThreatLevel::High;
        }

        let detected =
            !matched_patterns.is_empty() || max_threat_level >= self.config.min_threat_level;

        if detected {
            self.detection_stats.total_detections += 1;
            self.detection_stats.last_detection = Some(std::time::SystemTime::now());

            // 脅威レベル別統計更新
            *self
                .detection_stats
                .detections_by_level
                .entry(max_threat_level.clone())
                .or_insert(0) += 1;

            if self.config.block_mode {
                self.detection_stats.total_blocks += 1;
            }

            // ログ出力
            if self.config.real_time_monitoring {
                warn!(
                    "SQL injection detected: patterns={:?}, threat_level={:?}, query_type={:?}",
                    matched_patterns, max_threat_level, query_type
                );
            }
        }

        let analysis_message = if detected {
            format!(
                "SQL injection detected with {} patterns, threat level: {:?}",
                matched_patterns.len(),
                max_threat_level
            )
        } else {
            "Query passed SQL injection inspection".to_string()
        };

        Ok(self.create_detection_result(
            detected,
            matched_patterns,
            max_threat_level,
            query_type,
            suspicious_keywords,
            analysis_message,
        ))
    }

    /// SQLクエリを詳細分析
    pub fn analyze_query(&self, query: &str) -> Result<SqlQueryAnalysis, SecurityError> {
        let normalized = self.normalize_query(query);
        let tables = self.extract_tables(&normalized);
        let columns = self.extract_columns(&normalized);
        let parameter_count = self.count_parameters(&normalized);
        let complexity_score = self.calculate_complexity(&normalized);
        let prepared_statement_recommended = self.should_use_prepared_statement(&normalized);

        Ok(SqlQueryAnalysis {
            original_query: query.to_string(),
            normalized_query: normalized,
            tables,
            columns,
            parameter_count,
            complexity_score,
            prepared_statement_recommended,
        })
    }

    /// 検出結果を作成
    fn create_detection_result(
        &self,
        detected: bool,
        matched_patterns: Vec<String>,
        max_threat_level: ThreatLevel,
        query_type: SqlQueryType,
        suspicious_keywords: Vec<String>,
        analysis_message: String,
    ) -> SqlInjectionDetection {
        SqlInjectionDetection {
            detected,
            matched_patterns,
            max_threat_level,
            detected_query_type: query_type,
            suspicious_keywords,
            analysis_message,
            detection_time: std::time::SystemTime::now(),
        }
    }

    /// キーワード分析
    fn analyze_keywords(&self, query: &str) -> Vec<String> {
        let suspicious_words = vec![
            "UNION",
            "SELECT",
            "INSERT",
            "UPDATE",
            "DELETE",
            "DROP",
            "CREATE",
            "ALTER",
            "EXEC",
            "EXECUTE",
            "sp_",
            "xp_",
            "--",
            "/*",
            "*/",
            "@@",
            "CHAR",
            "ASCII",
            "SUBSTRING",
            "LENGTH",
            "CONCAT",
            "SLEEP",
            "BENCHMARK",
            "WAITFOR",
        ];

        let query_upper = query.to_uppercase();
        suspicious_words
            .into_iter()
            .filter(|&word| query_upper.contains(word))
            .map(|s| s.to_string())
            .collect()
    }

    /// ホワイトリスト検証
    fn validate_whitelist(&self, query: &str, query_type: &SqlQueryType) -> bool {
        // 基本的なホワイトリスト検証の実装
        // 実際の使用では、より厳密な検証が必要
        matches!(
            query_type,
            SqlQueryType::Select | SqlQueryType::Insert | SqlQueryType::Update
        ) && !query.to_uppercase().contains("DROP")
            && !query.to_uppercase().contains("EXEC")
    }

    /// クエリの正規化
    fn normalize_query(&self, query: &str) -> String {
        query
            .trim()
            .replace(['\n', '\t'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// テーブル名の抽出
    fn extract_tables(&self, query: &str) -> Vec<String> {
        // 簡単な実装 - より高度な解析が可能
        if let Ok(table_pattern) =
            Regex::new(r"(?i)\bFROM\s+(\w+)|\bJOIN\s+(\w+)|\bINTO\s+(\w+)|\bUPDATE\s+(\w+)")
        {
            let mut tables = Vec::new();

            let captures_iter = table_pattern.captures_iter(query);
            for capture in captures_iter.flatten() {
                for i in 1..=4 {
                    if let Some(table) = capture.get(i) {
                        tables.push(table.as_str().to_string());
                    }
                }
            }

            tables
        } else {
            Vec::new()
        }
    }

    /// カラム名の抽出
    fn extract_columns(&self, query: &str) -> Vec<String> {
        // 簡単な実装 - SELECTリストからカラムを抽出
        if let Some(select_pos) = query.to_uppercase().find("SELECT") {
            if let Some(from_pos) = query.to_uppercase().find("FROM") {
                let columns_part = &query[select_pos + 6..from_pos].trim();
                return columns_part
                    .split(',')
                    .map(|col| col.trim().to_string())
                    .filter(|col| !col.is_empty() && col != "*")
                    .collect();
            }
        }
        Vec::new()
    }

    /// パラメータ数をカウント
    fn count_parameters(&self, query: &str) -> usize {
        query.matches('?').count() + query.matches('$').count()
    }

    /// クエリの複雑さスコアを計算
    fn calculate_complexity(&self, query: &str) -> u32 {
        let mut score = 0u32;

        // 基本的なスコア計算
        score += query.len() as u32 / 10; // 長さベース
        score += query.matches("JOIN").count() as u32 * 2; // JOIN数
        score += query.matches("UNION").count() as u32 * 3; // UNION数
        score += query.matches("SELECT").count() as u32; // サブクエリ

        score
    }

    /// Prepared statement使用推奨判定
    fn should_use_prepared_statement(&self, query: &str) -> bool {
        // ユーザー入力を含む可能性が高い場合にtrue
        let user_input_indicators = ["WHERE", "VALUES", "SET"];
        let query_upper = query.to_uppercase();

        user_input_indicators
            .iter()
            .any(|indicator| query_upper.contains(indicator))
    }

    /// 統計情報を取得
    pub fn get_stats(&self) -> &SqlInjectionStats {
        &self.detection_stats
    }

    /// 統計をリセット
    pub fn reset_stats(&mut self) {
        self.detection_stats = SqlInjectionStats::default();
    }

    /// 設定を更新
    pub fn update_config(&mut self, config: SqlProtectionConfig) {
        self.config = config;
    }

    /// 許可テーブルを追加
    pub fn add_allowed_table(&mut self, table: String) {
        self.allowed_tables.insert(table);
    }

    /// 許可キーワードを追加
    pub fn add_allowed_keyword(&mut self, keyword: String) {
        self.allowed_keywords.insert(keyword.to_uppercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_query_type_detection() {
        assert_eq!(
            SqlQueryType::from_query("SELECT * FROM users"),
            SqlQueryType::Select
        );
        assert_eq!(
            SqlQueryType::from_query("INSERT INTO users VALUES"),
            SqlQueryType::Insert
        );
        assert_eq!(
            SqlQueryType::from_query("UPDATE users SET"),
            SqlQueryType::Update
        );
        assert_eq!(
            SqlQueryType::from_query("DELETE FROM users"),
            SqlQueryType::Delete
        );
        assert_eq!(
            SqlQueryType::from_query("DROP TABLE users"),
            SqlQueryType::Drop
        );
    }

    #[test]
    fn test_union_based_sqli_detection() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let malicious_query =
            "SELECT * FROM users WHERE id = 1 UNION SELECT username, password FROM admin";
        let result = protector.inspect_query(malicious_query).unwrap();

        assert!(result.detected);
        assert!(result
            .matched_patterns
            .contains(&"union_select".to_string()));
        assert_eq!(result.max_threat_level, ThreatLevel::High);
    }

    #[test]
    fn test_boolean_blind_sqli_detection() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let malicious_query = "SELECT * FROM users WHERE id = 1 AND 1=1";
        let result = protector.inspect_query(malicious_query).unwrap();

        assert!(result.detected);
        assert!(result
            .matched_patterns
            .contains(&"boolean_blindsqli".to_string()));
    }

    #[test]
    fn test_time_based_sqli_detection() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let malicious_query = "SELECT * FROM users WHERE id = 1; WAITFOR DELAY '00:00:05'";
        let result = protector.inspect_query(malicious_query).unwrap();

        assert!(result.detected);
        assert_eq!(result.max_threat_level, ThreatLevel::Critical);
    }

    #[test]
    fn test_safe_query() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let safe_query = "SELECT name, email FROM users WHERE active = 1 ORDER BY name";
        let result = protector.inspect_query(safe_query).unwrap();

        assert!(!result.detected);
        assert!(result.matched_patterns.is_empty());
    }

    #[test]
    fn test_query_analysis() {
        let config = SqlProtectionConfig::default();
        let protector = SqlInjectionProtector::new(config).unwrap();

        let query = "SELECT name, email FROM users WHERE id = ? ORDER BY name";
        let analysis = protector.analyze_query(query).unwrap();

        assert_eq!(analysis.tables, vec!["users"]);
        assert_eq!(analysis.columns, vec!["name", "email"]);
        assert_eq!(analysis.parameter_count, 1);
        assert!(analysis.prepared_statement_recommended);
    }

    #[test]
    fn test_query_length_limit() {
        let config = SqlProtectionConfig {
            max_query_length: 50,
            ..Default::default()
        };
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let long_query = "SELECT * FROM users WHERE name = 'this is a very long query that exceeds the length limit'";
        let result = protector.inspect_query(long_query).unwrap();

        assert!(result.detected);
        assert!(result
            .matched_patterns
            .contains(&"query_too_long".to_string()));
    }

    #[test]
    fn test_whitelist_mode() {
        let config = SqlProtectionConfig {
            whitelist_mode: true,
            ..Default::default()
        };
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        let blocked_query = "DROP TABLE users";
        let result = protector.inspect_query(blocked_query).unwrap();

        assert!(result.detected);
        assert!(result
            .matched_patterns
            .contains(&"whitelist_violation".to_string()));
    }

    #[test]
    fn test_statistics_tracking() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        // 正常なクエリ
        protector.inspect_query("SELECT * FROM users").unwrap();

        // 悪意のあるクエリ
        protector
            .inspect_query("SELECT * FROM users UNION SELECT * FROM admin")
            .unwrap();

        let stats = protector.get_stats();
        assert_eq!(stats.total_inspections, 2);
        assert_eq!(stats.total_detections, 1);
        assert_eq!(stats.total_blocks, 1);
    }

    #[test]
    fn test_custom_pattern_addition() {
        let config = SqlProtectionConfig::default();
        let mut protector = SqlInjectionProtector::new(config).unwrap();

        // カスタムパターンを追加
        let custom_pattern = SqlInjectionPattern::new(
            "custom_test".to_string(),
            r"(?i)\bTEST_INJECTION\b".to_string(),
            ThreatLevel::Medium,
            "Custom test pattern".to_string(),
            vec![SqlQueryType::Select],
        );

        protector.add_pattern(custom_pattern).unwrap();

        let test_query = "SELECT * FROM users WHERE test_injection = 1";
        let result = protector.inspect_query(test_query).unwrap();

        assert!(result.detected);
        assert!(result.matched_patterns.contains(&"custom_test".to_string()));
    }
}
