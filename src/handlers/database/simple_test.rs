//! Simple Database Types Test
//!
//! 基本的なタイプテストのみ

#[cfg(test)]
mod simple_tests {
    use crate::handlers::database::types::*;

    #[test]
    fn test_database_types() {
        // データベースタイプの基本テスト
        let pg = DatabaseType::PostgreSQL;
        let mysql = DatabaseType::MySQL;
        let sqlite = DatabaseType::SQLite;
        let mongodb = DatabaseType::MongoDB;

        // Display traitのテスト
        assert_eq!(format!("{}", pg), "postgresql");
        assert_eq!(format!("{}", mysql), "mysql");
        assert_eq!(format!("{}", sqlite), "sqlite");
        assert_eq!(format!("{}", mongodb), "mongodb");
    }

    #[test]
    fn test_value_creation() {
        // Value型の基本テスト
        let int_val = Value::from_i64(42);
        let string_val = Value::from_string("test".to_string());
        let bool_val = Value::from_bool(true);
        let float_val = Value::from_f64(std::f64::consts::PI);

        // パターンマッチングで確認
        match int_val {
            Value::Int(42) => {}
            _ => panic!("Expected Int(42)"),
        }

        match string_val {
            Value::String(ref s) if s == "test" => {}
            _ => panic!("Expected String(test)"),
        }

        match bool_val {
            Value::Bool(true) => {}
            _ => panic!("Expected Bool(true)"),
        }

        match float_val {
            Value::Float(f) if (f - std::f64::consts::PI).abs() < 0.001 => {}
            _ => panic!("Expected Float(PI)"),
        }
    }

    #[test]
    fn test_config_defaults() {
        // デフォルト設定のテスト
        let pool_config = PoolConfig::default();
        assert_eq!(pool_config.max_connections, 20);
        assert_eq!(pool_config.min_connections, 5);

        let security_config = SecurityConfig::default();
        assert!(security_config.enable_sql_injection_detection);
        assert!(security_config.enable_audit_logging);

        let feature_config = FeatureConfig::default();
        assert!(feature_config.enable_prepared_statements);
        assert!(feature_config.enable_transactions);
    }

    #[test]
    fn test_query_result() {
        // QueryResultの基本テスト
        use crate::handlers::database::types::ColumnInfo;
        let columns = vec![
            ColumnInfo {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                max_length: None,
            },
            ColumnInfo {
                name: "name".to_string(),
                data_type: "TEXT".to_string(),
                nullable: true,
                max_length: Some(255),
            },
        ];
        let rows = vec![
            vec![Value::from_i64(1), Value::from_string("Alice".to_string())],
            vec![Value::from_i64(2), Value::from_string("Bob".to_string())],
        ];

        let result = QueryResult {
            columns,
            rows,
            total_rows: Some(2),
            execution_time_ms: 100,
        };

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.total_rows, Some(2));
    }

    #[test]
    fn test_execute_result() {
        // ExecuteResultの基本テスト
        let result = ExecuteResult {
            rows_affected: 5,
            last_insert_id: Some(Value::from_i64(123)),
            execution_time_ms: 50,
        };

        assert_eq!(result.rows_affected, 5);

        match result.last_insert_id {
            Some(Value::Int(123)) => {}
            _ => panic!("Expected Some(Value::Int(123))"),
        }

        assert_eq!(result.execution_time_ms, 50);
    }
}
