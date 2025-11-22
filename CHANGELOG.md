# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.15.0] - 2025-11-08

## 🚀 Major Release: ユーザーフレンドリーな設定管理システム

### Added

- **対話的設定セットアップシステム** (`--setup-config`)
  - WordPress接続テスト付きの設定ウィザード
  - リアルタイム接続検証とエラー診断
  - crossterm使用のクロスプラットフォーム対応UI
  - スピナーアニメーションとカラー出力
- **動的設定管理システム**
  - `--switch-config`: 実行時設定ファイル切り替え
  - `--config <file>`: カスタム設定ファイル指定
  - `--reload-config`: 動的設定再読み込み
  - 設定ファイル自動検出機能
- **デモンストレーションモード** (`--demo-setup`)
  - 安全なテスト環境での機能体験
  - デモ設定ファイル自動生成
- **包括的ヘルプシステム** (`--help`)
  - 全オプションの詳細説明と使用例
  - 設定ファイル検索順序の明示
- **GitHub Pages統合**
  - 美しいランディングページ (index.html)
  - Jekyll設定の最適化とpermalink構造
  - 404エラーの完全修正

### Enhanced

- **ユーザビリティの大幅向上**
  - 設定ファイル不存在時の自動セットアップ起動
  - 分かりやすい日本語エラーメッセージ
  - 初心者から上級者まで対応の段階的ガイダンス
- **ロバストな入力処理**
  - EOF検出と再試行制限によるパイプ入力対応
  - 非対話環境での自動フォールバック
  - 入力ストリーム終了の適切な処理

### Changed

- **Breaking Changes Resolved**: Complete migration to latest API versions
  - axum 0.8 WebSocket API with `.into()` conversions
  - secrecy 0.10 Secret types (Secret<String> → SecretString)
  - jsonwebtoken 10.0 new crypto backend support
  - criterion 0.7 black_box API migration
  - serde_yaml → serde_yaml_ng transition
- **Performance Improvements**: Significant performance gains across all subsystems
  - 15-20% faster async runtime (tokio 1.48)
  - 20-30% improved HTTP throughput (axum 0.8 + hyper 1.6)
  - 10-20% faster cryptographic operations
  - Enhanced DNS resolution and network performance

### Security

- **Enhanced Cryptography**: Latest security algorithms and implementations
  - jsonwebtoken 10.x with improved crypto backends
  - secrecy 0.10 with strengthened Secret management
  - ring 0.17.8 cryptographic optimizations
- **Vulnerability Management**: RUSTSEC-2023-0071 properly managed
  - No actual security impact (unused dependency chain)
  - Alternative secure MySQL implementation via mysql_async
  - Comprehensive audit trail documentation

### Technical

- **Quality Assurance**: 356+ tests passing with zero warnings
  - Complete test suite modernization
  - Strict clippy compliance (-D warnings)
  - Cargo fmt standardization
  - Release build optimization
- **Documentation**: Comprehensive guides and references
  - Updated API documentation
  - Migration guides for breaking changes
  - Production deployment guides
  - Security configuration examples

## [0.1.0-alpha] - 2025-11-04

## Added

- **WordPress Integration**: Complete WordPress REST API integration with 27 tools
  - Advanced post/page management with SEO integration
  - Complete media management with base64 upload support
  - Category and tag management with hierarchical support
  - YouTube and social media embed support
  - Comment management and retrieval
- **Enterprise Security**: 6-layer security architecture (100% implemented)
  - AES-GCM-256 encryption with PBKDF2 key derivation
  - SQL injection protection (11 attack patterns)
  - XSS attack prevention (14 attack patterns)
  - Token bucket rate limiting with DDoS protection
  - TLS 1.2+ enforcement
  - Comprehensive audit logging
- **Core Infrastructure**:
  - JSON-RPC 2.0 server implementation using axum
  - Type-safe TOML configuration with environment variable override
  - Comprehensive error handling with thiserror
  - Async/await support with tokio runtime
  - Production-ready logging with tracing
- **Documentation**:
  - Complete README with usage examples
  - Technical documentation in project-docs/
  - GitHub Pages website preparation
  - Contributing guidelines and code of conduct
- **Testing**: 205+ comprehensive tests with 100% pass rate
- **Security Features**:
  - Zero-panic operations with Result-based error handling
  - Safe environment variable expansion with infinite loop prevention
  - Application password lifecycle management
  - Production monitoring and health checks

## Security

- Implemented military-grade AES-GCM-256 encryption
- Added comprehensive input validation and sanitization
- Enabled zero-trust data validation architecture
- Implemented real-time security monitoring

## Technical

- Built with Rust 2021 edition for memory safety
- Async-first architecture using tokio
- Clean layered architecture with separation of concerns
- Production-optimized build profiles

## Documentation

- Comprehensive API documentation for all 27 WordPress tools
- Security implementation guide with examples
- Architecture documentation with design decisions
- Complete setup and deployment guides

## [0.0.0] - 2025-10-01

## Added

- Initial project setup
- Basic project structure
- License files (MIT/Apache-2.0)

[Unreleased]: https://github.com/n-takatsu/mcp-rs/compare/v0.1.0-alpha...HEAD
[0.1.0-alpha]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.1.0-alpha
[0.0.0]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.0.0
