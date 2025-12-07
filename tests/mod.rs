//! MCP-RS Test Suite
//!
//! 構造化されたテストスイート - Phase 2 完了

// 単体テスト
pub mod unit;

// 統合テスト
pub mod integration;

// データベーステスト
pub mod database;

// WordPressテスト（将来実装予定）
pub mod wordpress;

// カナリーデプロイテスト
pub mod canary;

// パフォーマンステスト
pub mod performance;

// 脅威インテリジェンステスト
pub mod threat_intelligence;

// セキュリティテスト
pub mod security;

// テストデータとユーティリティ
pub mod fixtures;
