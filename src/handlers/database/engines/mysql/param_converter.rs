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
            Value::DateTime(dt) => {
                let formatted = dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
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
    pub fn validate_param_count(sql: &str, param_count: usize) -> Result<(), DatabaseError> {
        let expected_count = sql.matches('?').count();
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
            mysql_async::Value::UInt(u) => Ok(Value::Int(u as i64)),
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
    /// Identifies parameter positions in SQL for validation and debugging
    pub fn extract_parameter_positions(sql: &str) -> Vec<usize> {
        sql.match_indices('?').map(|(pos, _)| pos).collect()
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
