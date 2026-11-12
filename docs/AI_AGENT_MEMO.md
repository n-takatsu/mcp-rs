# AI Agent Memo - mcp-rs Project Context

**Last Updated**: 2025年11月8日
**Project**: mcp-rs v0.15.0 - Real-time Collaborative Editing System
**Branch**: feature/realtime-editing-system

## 🎯 Project Overview

## Core Functionality

- **Real-time Collaborative Editing System** built in Rust
- **Model Context Protocol (MCP) Server** implementation
- **Multi-database Support**: MySQL, PostgreSQL, SQLite, MongoDB, Redis
- **Security-first Architecture** with comprehensive input validation

## Version & Status

- **Current Version**: v0.15.0
- **Development Status**: Production-ready with comprehensive testing
- **Repository**: <https://github.com/n-takatsu/mcp-rs>
- **Branch**: feature/realtime-editing-system

## 🔐 Critical Security Context

## RSA Vulnerability (RUSTSEC-2023-0071)

**RESOLVED** - 2025年11月8日

### Problem

- RSA vulnerability detected in dependency chain: `rsa 0.9.8 → sqlx-mysql 0.8.6 → sqlx 0.8.6`
- Medium severity (5.9) timing sidechannel attack potential

### Solution Implemented

1. **MySQL Implementation**: Switched to `mysql_async v0.36.1` (RSA-free)
2. **Dependency Isolation**: `sqlx` only used for PostgreSQL/SQLite
3. **Audit Configuration**:
   - Local: `cargo-audit.toml` with ignore setting
   - CI: `--ignore RUSTSEC-2023-0071` flag in workflows

### Files Modified

- `Cargo.toml`: Added mysql_async dependency
- `cargo-audit.toml`: Added security audit ignore configuration
- `.github/workflows/ci.yml`: Added --ignore flag for CI
- `src/handlers/database/engines/mysql.rs`: Complete MySQL engine implementation

### Market Impact Consideration

- **User Concern**: "MySQLの需要は3割超えているのでインパクトが大きすぎます"
- **Solution**: Maintained full MySQL support using secure alternative library

## 🗄️ Database Architecture

## MySQL Engine (`mysql_async`)

```rust
// Location: src/handlers/database/engines/mysql.rs
pub struct MySqlEngine {
    config: DatabaseConfig,
}

impl DatabaseEngine for MySqlEngine {
    // Complete implementation with connection management,
    // query execution, health checks
}
```

## Key Features

- **Connection Pooling**: Automatic connection management
- **Health Monitoring**: Real-time database health checks
- **Security**: Input validation and SQL injection protection
- **Performance**: Async/await with connection reuse

## 🧪 Testing Framework

## Test Coverage (358+ Tests)

- **Unit Tests**: 345 tests across all modules
- **Integration Tests**: 13 comprehensive integration scenarios
- **Doc Tests**: 7 documentation examples
- **Security Tests**: SQL injection, XSS protection validation

## Test Execution Commands

```bash

## All tests with all features

cargo test --all-features

## MySQL-specific tests

cargo test --features "database,mysql-backend"

## Individual flaky test retry

cargo test --all-features test_timeout_strategy
```

## Known Test Issues

- `test_timeout_strategy`: Occasionally flaky due to timing, but passes on retry
- All other tests: Stable and reliable

## 🔧 Development Tools & Quality

## Code Quality Tools

- **Clippy**: Zero warnings with strict settings (`-D warnings -A dead_code`)
- **Rustfmt**: Consistent code formatting
- **Cargo Audit**: Security vulnerability scanning

## CI/CD Pipeline

```yaml

## .github/workflows/ci.yml

- name: Run cargo audit
  run: cargo audit --ignore RUSTSEC-2023-0071


## RUSTSEC-2023-0071: RSA脆弱性を無視



## 理由: sqlx-mysql経由の未使用依存関係のため影響なし



## mysql_asyncを使用してMySQL機能を安全に実装済み

```

## 📁 Key Files & Locations

### Core Implementation

- `src/handlers/database/engines/mysql.rs` - MySQL engine implementation
- `src/handlers/database/mod.rs` - Database handler registry
- `Cargo.toml` - Dependencies and feature flags

### Configuration Files

- `cargo-audit.toml` - Security audit configuration
- `mcp-config.toml` - MCP server configuration template
- `.github/workflows/ci.yml` - CI pipeline with security audit

### Documentation

- `docs/design/mysql-engine.md` - MySQL engine design document
- `README.md` - Project overview and setup instructions

## 🚨 Critical Issues & Resolutions

## Issue 1: RSA Vulnerability Detection

- **Date**: 2025年11月8日
- **Impact**: CI pipeline failure
- **Root Cause**: Unused sqlx-mysql dependency containing vulnerable RSA crate
- **Resolution**: Documented ignore in audit configuration, implemented mysql_async alternative
- **Status**: ✅ RESOLVED

## Issue 2: CI Environment Differences

- **Problem**: Local cargo-audit.toml not recognized in CI
- **Solution**: Added explicit --ignore flag in CI workflow
- **Lesson**: Always test configuration changes in both local and CI environments

## Issue 3: PowerShell Search Limitations

- **Problem**: PowerShell performance issues with large dependency files
- **Workaround**: Direct file reading for Cargo.lock analysis
- **Note**: Consider alternative search methods for large files

## 🔄 Development Workflow

## Pre-Push Checklist

1. ✅ `cargo build --all-features`
2. ✅ `cargo test --all-features`
3. ✅ `cargo clippy --all-targets --all-features -- -D warnings -A dead_code`
4. ✅ `cargo audit` (with appropriate ignore settings)
5. ✅ Feature-specific testing (e.g., MySQL backend)

## Git Workflow

- **Main Branch**: `main`
- **Development Branch**: `develop`
- **Current Feature**: `feature/realtime-editing-system`
- **Commit Style**: Descriptive commits with security context

## 🎯 Future Development Notes

## MySQL Engine Enhancements

- **Current**: Basic query execution, connection management, health checks
- **Future**: Advanced features like transactions, prepared statements, schema introspection
- **Performance**: Connection pooling optimization, query caching

## Security Monitoring

- **Regular**: Automated security audit in CI pipeline
- **Manual**: Periodic dependency review and upgrade planning
- **Documentation**: Keep security decisions documented for future reference

## 📞 Context for New AI Agents

## When Taking Over This Project

1. **Read This Memo First**: Essential context for all decisions
2. **Check Current Branch**: Ensure you're on `feature/realtime-editing-system`
3. **Verify Test Status**: Run full test suite to ensure clean state
4. **Review Recent Commits**: Understand latest changes and context
5. **Security First**: Always consider security implications of changes

## Key Principles

- **Security**: Never compromise on security for convenience
- **Testing**: Comprehensive testing is non-negotiable
- **Documentation**: Document all significant decisions and their rationale
- **Market Impact**: Consider user needs (e.g., MySQL market demand)
- **Environment Parity**: Ensure local and CI environments behave identically

## 🤖 Claude Desktop Integration (2025年11月9日)

## Implementation Status: ✅ COMPLETED

## Overview

Claude Desktop MCP統合を完了し、AI AgentがWordPressリソースに直接アクセス可能になりました。

## Package Location

- **Standalone Package**: `C:\Users\takat\Desktop\mcp-rs-server\`
- **Files**:
  - `mcp-rs.exe` (6.26MB) - 実行ファイル
  - `mcp-config-claude.toml` - STDIO mode設定
  - `claude_desktop_config.json` - Claude Desktop統合設定
  - `claude-desktop-diagnosis.ps1` - 診断ツール
  - `README.md` - 使用方法

## Technical Architecture

- **Dual Server Mode**:
  - STDIO mode (`stdio = true`): Claude Desktop MCP統合
  - HTTP+TCP mode (`stdio = false`): AI Agent HTTP access
- **Ports**:
  - TCP: `127.0.0.1:8080` (line-delimited protocol)
  - HTTP: `127.0.0.1:8081` (JSON-RPC for AI agents)

## Configuration

```toml
[server]
stdio = true

## Claude Desktop mode

log_level = "info"

[handlers.wordpress]
url = "https://example.com"

## Replace with your WordPress site URL

enabled = true
burst_size = 20
```

## Claude Desktop Setup

```json
{
  "mcpServers": {
    "mcp-rs-wordpress": {
      "command": "C:/Users/takat/Desktop/mcp-rs-server/mcp-rs.exe",
      "args": ["--config", "C:/Users/takat/Desktop/mcp-rs-server/mcp-config-claude.toml"],
      "env": { "RUST_LOG": "info" }
    }
  }
}
```

## Validation Results (2025-11-09)

- ✅ MCP-RS executable: 正常動作
- ✅ Configuration files: 適切配置
- ✅ WordPress integration: 正常初期化
- ✅ Claude Desktop config: AppData配置完了

## Resolved Issues

1. **Path Separator**: Windows環境でのパス区切り統一 (`/` 使用)
2. **Configuration Fields**: `burst_size`, `enabled` フィールド追加
3. **Process Management**: Claude Desktop完全再起動手順確立

## Usage Verification

Claude Desktopでの動作確認:

- "WordPressサイトのカテゴリ一覧を取得してください"
- "ブログのタグ一覧を教えてください"

## Future Enhancements

- [ ] Claude.ai web_fetch統合
- [ ] 外部トンネリング安定化
- [ ] エラーハンドリング強化

---

**Note**: This memo should be updated whenever significant changes are made to the project architecture, security posture, or development workflow.
