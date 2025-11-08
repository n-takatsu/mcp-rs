//! MCP-RS Test Suite
//!
//! 新しい構造化されたテストスイート

// 単体テスト
pub mod unit;

// 統合テスト  
pub mod integration;

// データベーステスト
pub mod database;

// WordPressテスト
pub mod wordpress;

// カナリーデプロイテスト
pub mod canary;

// パフォーマンステスト
pub mod performance;

// テストデータとユーティリティ
pub mod fixtures;

// 後方互換性のための既存テスト（段階的に移行予定）
pub mod session_current_tests;
