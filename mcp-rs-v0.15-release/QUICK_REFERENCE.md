# Quick Reference - mcp-rs Project

## 🚀 Essential Commands
```bash
# Full validation suite
cargo build --all-features
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings -A dead_code
cargo audit --ignore RUSTSEC-2023-0071

# MySQL-specific testing
cargo test --features "database,mysql-backend"
```

## 🔐 Security Status
- **RSA Vulnerability (RUSTSEC-2023-0071)**: ✅ RESOLVED
- **MySQL Implementation**: `mysql_async v0.36.1` (secure)
- **Audit Configuration**: Properly ignored in both local and CI

## 📊 Test Status
- **Total Tests**: 358+
- **Status**: All passing (1 occasionally flaky timeout test)
- **Coverage**: Comprehensive unit, integration, and doc tests

## 🗄️ Database Support
- **MySQL**: ✅ `mysql_async` (secure implementation)
- **PostgreSQL**: ✅ `sqlx` 
- **SQLite**: ✅ `sqlx`
- **MongoDB**: ✅ Native driver
- **Redis**: ✅ Native driver

## 🔧 Key Files
- MySQL Engine: `src/handlers/database/engines/mysql.rs`
- Dependencies: `Cargo.toml`
- Security Config: `cargo-audit.toml`
- CI Config: `.github/workflows/ci.yml`
- Full Context: `docs/AI_AGENT_MEMO.md`

## ⚠️ Known Issues
- Timeout test occasionally flaky (retryable)
- PowerShell search performance issues with large files
- CI environment requires explicit audit ignore flags

## 🎯 Current State
- **Version**: v0.15.0
- **Branch**: feature/realtime-editing-system
- **Status**: Production-ready, security-audited
- **Last Push**: 2025年11月8日