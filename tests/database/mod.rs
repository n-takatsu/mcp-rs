//! Database Tests
//!
//! データベースエンジンとハンドラーのテスト

pub mod security_tests;

// MySQL テスト
mod mysql_integration_tests;
mod mysql_phase1_basic_tests;
mod mysql_phase1_integration_complete_tests;
mod mysql_phase1_integration_tests;
mod mysql_phase1_tests;
mod mysql_postgres_compatibility_tests;
mod mysql_security_basic_tests;
mod mysql_security_tests;
mod mysql_security_test_runner;

// PostgreSQL テスト
mod postgresql_integration_tests;
mod postgres_database_integration_tests;
mod postgres_phase2_basic_tests;
mod postgres_phase2_integration_tests;

// その他のデータベース
mod mariadb_integration_tests;
mod mongodb_integration_tests;
mod sqlite_integration_tests;

// Redis
mod redis_integration_test;
mod redis_integration_tests;
