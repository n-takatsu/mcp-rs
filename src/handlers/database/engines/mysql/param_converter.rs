//! MySQL Parameter Conversion Utilities
//!
//! Provides secure parameter conversion between mcp-rs Value types and mysql_async types
//! This module is critical for SQL injection prevention.

use crate::handlers::database::types::{DatabaseError, Value};
use mysql_async;

/// MySQL parameter conversion utility for secure query execution
pub struct MySqlParamConverter;

impl MySqlParamConverter {
    /// Convert mcp-rs Value to mysql_async::Value
    ///
    /// This function is the core of SQL injection prevention - it ensures
    /// all user input is properly escaped and typed.
    pub fn convert_value(value: &Value) -> Result<mysql_async::Value, DatabaseError> {
        match value {
            Value::Null => Ok(mysql_async::Value::NULL),
            Value::Bool(b) => Ok(mysql_async::Value::Int(*b as i64)),
            Value::Int(i) => Ok(mysql_async::Value::Int(*i)),
            Value::Float(f) => Ok(mysql_async::Value::Double(*f)),
            Value::String(s) => Ok(mysql_async::Value::Bytes(s.as_bytes().to_vec())),
            Value::Binary(b) => Ok(mysql_async::Value::Bytes(b.clone())),
            Value::Json(j) => Ok(mysql_async::Value::Bytes(j.to_string().into_bytes())),
            // マイクロ秒まで渡す（#768。以前は`%.3f`で、ミリ秒より下を切り
            // 捨てた）。MySQLのDATETIMEは時差を持たないので、UTCの時刻
            Value::DateTime(dt) => {
                let formatted = dt.naive_utc().format("%Y-%m-%d %H:%M:%S%.6f").to_string();
                Ok(mysql_async::Value::Bytes(formatted.into_bytes()))
            }
        }
    }

    /// Convert batch of parameters
    ///
    /// Processes multiple parameters at once, ensuring all conversions are successful
    /// before proceeding with query execution.
    pub fn convert_params(params: &[Value]) -> Result<Vec<mysql_async::Value>, DatabaseError> {
        params
            .iter()
            .map(Self::convert_value)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Validate parameter count against query placeholders
    ///
    /// Ensures the number of provided parameters matches the number of
    /// placeholders in the SQL query to prevent parameter injection attacks.
    ///
    /// This is an early check made without the server: the placeholders are
    /// counted by [`Self::extract_parameter_positions`], with the
    /// assumptions written there. The count can differ from the server's
    /// when the SQL has an executable comment (`/*! … */`, `/*M! … */`) that
    /// the server skips, so a statement this refuses may still be valid on
    /// that server. To check against the server's own count, prepare the
    /// statement (`MySqlPreparedStatement::new`) and use its
    /// `parameter_count()`.
    pub fn validate_param_count(sql: &str, param_count: usize) -> Result<(), DatabaseError> {
        let expected_count = Self::extract_parameter_positions(sql).len();
        if expected_count != param_count {
            return Err(DatabaseError::ValidationError(format!(
                "Parameter count mismatch: expected {}, provided {}",
                expected_count, param_count
            )));
        }
        Ok(())
    }

    /// Convert mysql_async::Value back to mcp-rs Value
    ///
    /// Used for converting query results back to our internal format
    pub fn convert_from_mysql_value(
        mysql_value: mysql_async::Value,
    ) -> Result<Value, DatabaseError> {
        match mysql_value {
            mysql_async::Value::NULL => Ok(Value::Null),
            mysql_async::Value::Int(i) => Ok(Value::Int(i)),
            // i64に入らない符号なしの値は、桁を落とさないよう文字列にする
            // （#768。以前は`as i64`で、`u64::MAX`が-1になった）
            mysql_async::Value::UInt(u) => Ok(match i64::try_from(u) {
                Ok(i) => Value::Int(i),
                Err(_) => Value::String(u.to_string()),
            }),
            mysql_async::Value::Float(f) => Ok(Value::Float(f as f64)),
            mysql_async::Value::Double(d) => Ok(Value::Float(d)),
            mysql_async::Value::Bytes(b) => match String::from_utf8(b.clone()) {
                Ok(s) => Ok(Value::String(s)),
                Err(_) => Ok(Value::Binary(b)),
            },
            mysql_async::Value::Date(year, month, day, hour, minute, second, microsecond) => {
                // Convert MySQL datetime to DateTime object
                let datetime_str = format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
                    year, month, day, hour, minute, second, microsecond
                );
                match chrono::NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S%.f") {
                    Ok(naive_dt) => Ok(Value::DateTime(naive_dt.and_utc())),
                    Err(_) => Ok(Value::String(datetime_str)),
                }
            }
            mysql_async::Value::Time(is_negative, days, hours, minutes, seconds, microseconds) => {
                let time_str = format!(
                    "{}{:02}:{:02}:{:02}.{:06}",
                    if is_negative { "-" } else { "" },
                    days * 24 + hours as u32,
                    minutes,
                    seconds,
                    microseconds
                );
                Ok(Value::String(time_str))
            }
        }
    }

    /// Validate parameter types for MySQL compatibility
    ///
    /// Ensures that parameter types are compatible with MySQL's type system
    pub fn validate_parameter_types(params: &[Value]) -> Result<(), DatabaseError> {
        for param in params.iter() {
            match param {
                Value::Null
                | Value::Bool(_)
                | Value::Int(_)
                | Value::Float(_)
                | Value::String(_)
                | Value::Binary(_)
                | Value::DateTime(_) => {
                    // These types are directly supported
                    continue;
                }
                Value::Json(_) => {
                    // JSON is supported as string in MySQL
                    continue;
                }
            }
        }
        Ok(())
    }

    /// Extract parameter placeholders from SQL
    ///
    /// Identifies parameter positions (byte offsets) in SQL for validation
    /// and debugging. A `?` inside a string, a quoted identifier, or a
    /// comment is not a placeholder (#768: it used to be counted).
    ///
    /// This reads the SQL without asking the server, so it assumes what the
    /// connections of this engine set up:
    ///
    /// - `\` escapes the next character inside `'…'` and `"…"`. The engine
    ///   removes `NO_BACKSLASH_ESCAPES` and `ANSI_QUOTES` from the session's
    ///   `sql_mode` (`session_setup_sql`), so this holds on its connections
    /// - Executable comments (`/*! … */`, `/*!80000 … */`, and MariaDB's
    ///   `/*M! … */`) are read as SQL, so a `?` inside one is counted. A
    ///   server that skips the comment (a version condition it does not
    ///   meet, or `/*M!` on MySQL) has fewer placeholders
    ///
    /// The server's own count is the one that decides:
    /// `MySqlPreparedStatement` uses it (`Statement::num_params`)
    pub fn extract_parameter_positions(sql: &str) -> Vec<usize> {
        let mut positions = Vec::new();
        // 実行されるコメント（`/*! … */`）の中にいるか
        let mut in_executable_comment = false;
        let mut chars = sql.char_indices().peekable();
        while let Some((position, c)) = chars.next() {
            match c {
                '?' => positions.push(position),
                // 文字列・引用符で囲んだ名前。`\`の次の文字と、重ねた引用符は
                // 終わりにしない（バッククォートの中では、`\`は普通の文字）
                '\'' | '"' | '`' => {
                    while let Some((_, inner)) = chars.next() {
                        if inner == '\\' && c != '`' {
                            chars.next();
                        } else if inner == c {
                            if chars.peek().is_some_and(|(_, next)| *next == c) {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                }
                // `#`と、`-- `（`--`の次が空白か終わり）は、行の終わりまでコメント
                '#' => {
                    for (_, inner) in chars.by_ref() {
                        if inner == '\n' {
                            break;
                        }
                    }
                }
                '-' if sql[position..].starts_with("--")
                    && sql[position + 2..]
                        .chars()
                        .next()
                        .is_none_or(char::is_whitespace) =>
                {
                    for (_, inner) in chars.by_ref() {
                        if inner == '\n' {
                            break;
                        }
                    }
                }
                // 実行されるコメントの終わりの`*/`。まとめて飛ばす（PR #804の
                // レビュー。飛ばさないと、`/*! ? */* ?`の`/`と次の`*`を、
                // 普通のコメントの始まりと読んでしまう）
                '*' if in_executable_comment
                    && chars.peek().is_some_and(|(_, next)| *next == '/') =>
                {
                    chars.next();
                    in_executable_comment = false;
                }
                // 実行されるコメント（`/*! … */`・`/*M! … */`）は、中身がSQL。
                // 始まりの印だけを飛ばし、中身は読む
                '/' if sql[position..].starts_with("/*!")
                    || sql[position..].starts_with("/*M!") =>
                {
                    let opener = if sql[position..].starts_with("/*!") {
                        2
                    } else {
                        3
                    };
                    for _ in 0..opener {
                        chars.next();
                    }
                    in_executable_comment = true;
                }
                // `/* … */`
                '/' if chars.peek().is_some_and(|(_, next)| *next == '*') => {
                    chars.next();
                    let mut previous = ' ';
                    for (_, inner) in chars.by_ref() {
                        if previous == '*' && inner == '/' {
                            break;
                        }
                        previous = inner;
                    }
                }
                _ => {}
            }
        }
        positions
    }

    /// Generate safe parameter summary for logging
    ///
    /// Creates a safe representation of parameters for audit logs
    /// without exposing sensitive data
    pub fn create_param_summary(params: &[Value]) -> String {
        params
            .iter()
            .enumerate()
            .map(|(i, param)| {
                let type_name = match param {
                    Value::Null => "NULL",
                    Value::Bool(_) => "BOOL",
                    Value::Int(_) => "INT",
                    Value::Float(_) => "FLOAT",
                    Value::String(s) => {
                        if s.len() > 50 {
                            "STRING(large)"
                        } else {
                            "STRING"
                        }
                    }
                    Value::Binary(b) => {
                        if b.len() > 1024 {
                            "BINARY(large)"
                        } else {
                            "BINARY"
                        }
                    }
                    Value::Json(_) => "JSON",
                    Value::DateTime(_) => "DATETIME",
                };
                format!("${}: {}", i + 1, type_name)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_parameter_count_validation() {
        let sql = "SELECT * FROM users WHERE id = ? AND name = ?";

        // Valid parameter count
        assert!(MySqlParamConverter::validate_param_count(sql, 2).is_ok());

        // Invalid parameter count
        assert!(MySqlParamConverter::validate_param_count(sql, 1).is_err());
        assert!(MySqlParamConverter::validate_param_count(sql, 3).is_err());
    }

    #[test]
    fn test_value_conversion() {
        // Test various value types
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(42),
            Value::Float(std::f64::consts::PI),
            Value::String("test".to_string()),
            Value::DateTime(Utc::now()),
        ];

        for value in values {
            let result = MySqlParamConverter::convert_value(&value);
            assert!(result.is_ok(), "Failed to convert value: {:?}", value);
        }
    }

    /// 文字列・引用符で囲んだ名前・コメントの中の`?`は、数えない（#768）
    #[test]
    fn test_placeholders_in_strings_and_comments_are_not_counted() {
        for (sql, count) in [
            ("SELECT * FROM t WHERE a = ? AND b = '?'", 1),
            ("SELECT '?', \"?\", `a?b` FROM t WHERE a = ?", 1),
            ("SELECT 'it''s ?', 'a\\'?' FROM t WHERE a = ?", 1),
            ("SELECT 1 /* ? */ FROM t WHERE a = ? -- ?", 1),
            ("SELECT 1 # ?\nFROM t WHERE a = ? AND b = ?", 2),
            ("SELECT a--? FROM t", 1),
            ("SELECT 'unclosed ?", 0),
            // 実行されるコメントの中身は、SQLとして読む
            ("SELECT 1 /*! + ? */", 1),
            ("SELECT 1 /*!80000 + ? */ + ?", 2),
            ("SELECT 1 /*M! + ? */ /* ? */", 1),
            ("SELECT 1 /*! + '?' */", 0),
            // 終わりの`*/`の後の`*`は、掛け算
            ("SELECT /*! ? */* ?", 2),
            ("SELECT /*! 2 */* ? /* ? */", 1),
            ("SELECT 2 */ 1 + ?", 1),
            ("SELECT ?, ?", 2),
        ] {
            assert_eq!(
                MySqlParamConverter::extract_parameter_positions(sql).len(),
                count,
                "{sql}"
            );
            assert!(MySqlParamConverter::validate_param_count(sql, count).is_ok());
        }
        assert_eq!(
            MySqlParamConverter::extract_parameter_positions("SELECT '?' WHERE a = ?"),
            vec![21]
        );
    }

    /// マイクロ秒まで渡す。i64に入らない符号なしの値は、文字列にする（#768）
    #[test]
    fn test_values_keep_their_precision() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-27T01:02:03.123456Z")
            .unwrap()
            .with_timezone(&Utc);
        let converted = MySqlParamConverter::convert_value(&Value::DateTime(at)).unwrap();
        assert_eq!(
            converted,
            mysql_async::Value::Bytes(b"2026-11-27 01:02:03.123456".to_vec())
        );

        let big = MySqlParamConverter::convert_from_mysql_value(mysql_async::Value::UInt(u64::MAX))
            .unwrap();
        assert!(matches!(big, Value::String(text) if text == u64::MAX.to_string()));
        let small =
            MySqlParamConverter::convert_from_mysql_value(mysql_async::Value::UInt(7)).unwrap();
        assert!(matches!(small, Value::Int(7)));
    }

    #[test]
    fn test_parameter_positions() {
        let sql = "SELECT * FROM users WHERE id = ? AND name = ? AND age > ?";
        let positions = MySqlParamConverter::extract_parameter_positions(sql);
        assert_eq!(positions.len(), 3);
    }

    #[test]
    fn test_param_summary() {
        let params = vec![
            Value::Int(1),
            Value::String("test_user".to_string()),
            Value::Bool(true),
        ];

        let summary = MySqlParamConverter::create_param_summary(&params);
        assert!(summary.contains("$1: INT"));
        assert!(summary.contains("$2: STRING"));
        assert!(summary.contains("$3: BOOL"));
    }

    #[test]
    fn test_parameter_type_validation() {
        let valid_params = vec![
            Value::Int(1),
            Value::String("test".to_string()),
            Value::Bool(true),
        ];

        assert!(MySqlParamConverter::validate_parameter_types(&valid_params).is_ok());
    }
}
