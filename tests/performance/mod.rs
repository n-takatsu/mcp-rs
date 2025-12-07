//! Performance Tests
//!
//! パフォーマンスとベンチマークテスト

pub mod memory_benchmarks;

// MFA パフォーマンステスト（mfaフィーチャーが有効な場合のみ）
#[cfg(feature = "mfa")]
mod mfa_performance_tests;
