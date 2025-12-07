//! Security Tests
//!
//! セキュリティ機能のテスト（攻撃パターン検出、MFA等）

// 攻撃パターンテスト
mod attack_pattern_tests;
mod simple_attack_pattern_tests;

// MFA統合テスト（mfaフィーチャーが有効な場合のみ）
#[cfg(feature = "mfa")]
mod mfa_integration_tests;
